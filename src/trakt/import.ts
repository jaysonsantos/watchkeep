/**
 * Import a Trakt data export (the ZIP from trakt.tv settings). Reads watch
 * history, ratings, playback positions, the watchlist, and hidden shows.
 * Every play carries the Trakt history id, so a second import adds nothing.
 *
 * The import collects every unique movie, show, and episode first and then
 * writes in bulk, so it needs a few dozen queries instead of one per record.
 */
import { readFileSync } from "node:fs";
import { bulkRecordPlays, bulkSetRatings, bulkUpsertEpisodes, bulkUpsertMedia, type PlayInsert, type RatingInsert } from "../bulk.ts";
import type { Catalog, CatalogMovie, CatalogShow } from "../catalog/catalog.ts";
import type { EpisodeRow, MediaRow, Pool } from "../db.ts";
import { Library, type Clock, systemClock } from "../library.ts";
import type { EpisodeRef, ExternalIds, MovieRef } from "../plex/payload.ts";
import { episodeWithCatalog, movieWithCatalog, showWithCatalog, type ShowRef } from "../scrobble.ts";
import { readZip, type ZipEntry } from "./zip.ts";

export const TRAKT_SOURCE = "trakt";

export interface TraktImportReport {
  files: number;
  plays: number;
  playsSkipped: number;
  movies: number;
  shows: number;
  episodes: number;
  ratings: number;
  playback: number;
  playbackSkipped: number;
  watchlist: number;
  hidden: number;
  ignoredFiles: string[];
  exportErrors: Array<{ endpoint?: string; error?: string }>;
  dryRun: boolean;
}

interface TraktIds {
  trakt?: number;
  slug?: string;
  imdb?: string | null;
  tmdb?: number | null;
  tvdb?: number | null;
  plex?: { guid?: string; slug?: string } | null;
}

interface TraktMovie {
  title?: string;
  year?: number | null;
  ids?: TraktIds;
}

type TraktShow = TraktMovie;

interface TraktEpisode {
  season?: number;
  number?: number;
  title?: string | null;
  ids?: TraktIds;
}

interface TraktRecord {
  id?: number;
  type?: string;
  action?: string;
  watched_at?: string;
  rated_at?: string;
  rating?: number;
  progress?: number;
  paused_at?: string;
  listed_at?: string;
  hidden_at?: string;
  rank?: number;
  movie?: TraktMovie;
  show?: TraktShow;
  episode?: TraktEpisode;
}

type Section = "history" | "ratings" | "playback" | "watchlist" | "hidden";

const SECTIONS: Array<[RegExp, Section]> = [
  [/^watched-history(-\d+)?\.json$/, "history"],
  [/^ratings-(movies|shows|episodes)(-\d+)?\.json$/, "ratings"],
  [/^watched-playback(-\d+)?\.json$/, "playback"],
  [/^lists-watchlist(-\d+)?\.json$/, "watchlist"],
  [/^hidden-progress-watched(-\d+)?\.json$/, "hidden"],
];

export function sectionOf(name: string): Section | null {
  const base = name.split("/").pop() ?? name;
  for (const [pattern, section] of SECTIONS) if (pattern.test(base)) return section;
  return null;
}

function ids(source: TraktIds | undefined, plexKind: "movie" | "show" | "episode"): ExternalIds {
  const plexGuid = source?.plex?.guid;
  return {
    plexGuid: plexGuid ? `plex://${plexKind}/${plexGuid}` : null,
    imdb: source?.imdb ?? null,
    tmdb: source?.tmdb ? String(source.tmdb) : null,
    tvdb: source?.tvdb ? String(source.tvdb) : null,
  };
}

function movieKey(movie: TraktMovie): string | null {
  if (!movie.title) return null;
  return movie.ids?.trakt ? `trakt:${movie.ids.trakt}` : `title:${movie.title.toLowerCase()}:${movie.year ?? ""}`;
}

function episodeKey(episode: TraktEpisode, showKey: string): string | null {
  if (episode.season === undefined || episode.number === undefined) return null;
  return episode.ids?.trakt ? `trakt:${episode.ids.trakt}` : `${showKey}:${episode.season}:${episode.number}`;
}

/** Sort `name-1.json`, `name-2.json`, … numerically. */
function sortEntries(entries: ZipEntry[]): ZipEntry[] {
  const key = (name: string) => {
    const match = /^(.*?)(?:-(\d+))?\.json$/.exec(name);
    return [match?.[1] ?? name, Number(match?.[2] ?? 0)] as const;
  };
  return [...entries].sort((a, b) => {
    const [aName, aNumber] = key(a.name);
    const [bName, bNumber] = key(b.name);
    return aName.localeCompare(bName) || aNumber - bNumber;
  });
}

function positive(value: string | null): number | null {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

/** Collected unique items from every file, keyed by Trakt id. */
class Collector {
  readonly movies = new Map<string, MovieRef>();
  readonly shows = new Map<string, ShowRef>();
  readonly episodes = new Map<string, { showKey: string; ref: EpisodeRef }>();

  movie(movie: TraktMovie | undefined): string | null {
    if (!movie?.title) return null;
    const key = movieKey(movie)!;
    if (!this.movies.has(key)) {
      this.movies.set(key, { type: "movie", title: movie.title, year: movie.year ?? null, ids: ids(movie.ids, "movie"), durationMs: null, summary: null });
    }
    return key;
  }

  show(show: TraktShow | undefined): string | null {
    if (!show?.title) return null;
    const key = movieKey(show)!;
    if (!this.shows.has(key)) this.shows.set(key, { title: show.title, year: show.year ?? null, ids: ids(show.ids, "show") });
    return key;
  }

  episode(episode: TraktEpisode | undefined, show: TraktShow | undefined): string | null {
    const showKey = this.show(show);
    if (!showKey || !episode) return null;
    const key = episodeKey(episode, showKey);
    if (!key) return null;
    if (!this.episodes.has(key)) {
      this.episodes.set(key, {
        showKey,
        ref: {
          type: "episode",
          title: episode.title ?? null,
          season: episode.season!,
          number: episode.number!,
          ids: ids(episode.ids, "episode"),
          durationMs: null,
          airedAt: null,
          show: this.shows.get(showKey)!,
        },
      });
    }
    return key;
  }

  target(record: TraktRecord): { kind: "movie" | "episode"; key: string } | null {
    if (record.type === "movie") {
      const key = this.movie(record.movie);
      return key ? { kind: "movie", key } : null;
    }
    if (record.type === "episode") {
      const key = this.episode(record.episode, record.show);
      return key ? { kind: "episode", key } : null;
    }
    return null;
  }
}

async function enrichMovies(catalog: Catalog | null, movies: Map<string, MovieRef>): Promise<void> {
  if (!catalog) return;
  const refs = [...movies.entries()];
  const byTmdb = await catalog.moviesByTmdbIds(refs.map(([, ref]) => positive(ref.ids.tmdb)).filter((id): id is number => id !== null));
  const byImdb = await catalog.moviesByImdbIds(refs.filter(([, ref]) => !byTmdb.has(positive(ref.ids.tmdb) ?? -1) && ref.ids.imdb).map(([, ref]) => ref.ids.imdb!));
  for (const [key, ref] of refs) {
    let match: CatalogMovie | null = byTmdb.get(positive(ref.ids.tmdb) ?? -1) ?? (ref.ids.imdb ? byImdb.get(ref.ids.imdb) : undefined) ?? null;
    if (!match && !ref.ids.tmdb && !ref.ids.imdb) match = await catalog.movieByTitle(ref.title, ref.year);
    movies.set(key, movieWithCatalog(ref, match));
  }
}

async function enrichShows(catalog: Catalog | null, shows: Map<string, ShowRef>): Promise<void> {
  if (!catalog) return;
  const refs = [...shows.entries()];
  const byTmdb = await catalog.showsByTmdbIds(refs.map(([, ref]) => positive(ref.ids.tmdb)).filter((id): id is number => id !== null));
  const unmatched = refs.filter(([, ref]) => !byTmdb.has(positive(ref.ids.tmdb) ?? -1));
  const byTvdb = await catalog.showsByTvdbIds(unmatched.map(([, ref]) => ref.ids.tvdb).filter((id): id is string => id !== null));
  const byImdb = await catalog.showsByImdbIds(unmatched.map(([, ref]) => ref.ids.imdb).filter((id): id is string => id !== null));
  for (const [key, ref] of refs) {
    let match: CatalogShow | null =
      byTmdb.get(positive(ref.ids.tmdb) ?? -1) ??
      (ref.ids.tvdb ? byTvdb.get(ref.ids.tvdb) : undefined) ??
      (ref.ids.imdb ? byImdb.get(ref.ids.imdb) : undefined) ??
      null;
    if (!match && !ref.ids.tmdb && !ref.ids.tvdb && !ref.ids.imdb) match = await catalog.showByName(ref.title, ref.year);
    shows.set(key, showWithCatalog(ref, match));
  }
}

async function enrichEpisodes(
  catalog: Catalog | null,
  shows: Map<string, ShowRef>,
  episodes: Map<string, { showKey: string; ref: EpisodeRef }>,
): Promise<void> {
  const showTmdbIds = [...new Set([...shows.values()].map((show) => positive(show.ids.tmdb)).filter((id): id is number => id !== null))];
  const known = catalog ? await catalog.episodesForShows(showTmdbIds) : new Map();
  for (const [key, entry] of episodes) {
    const show = shows.get(entry.showKey)!;
    const showTmdb = positive(show.ids.tmdb);
    const match = showTmdb !== null ? (known.get(`${showTmdb}:${entry.ref.season}:${entry.ref.number}`) ?? null) : null;
    episodes.set(key, { showKey: entry.showKey, ref: episodeWithCatalog(entry.ref, show, match) });
  }
}

export async function importTraktExport(
  pool: Pool,
  catalog: Catalog | null,
  zipPath: string,
  options: { clock?: Clock; log?: (message: string) => void; dryRun?: boolean } = {},
): Promise<TraktImportReport> {
  const clock = options.clock ?? systemClock;
  const log = options.log ?? (() => {});
  const dryRun = options.dryRun ?? false;
  const now = clock.now().toISOString();
  const entries = sortEntries(readZip(readFileSync(zipPath)));
  const report: TraktImportReport = {
    files: 0,
    plays: 0,
    playsSkipped: 0,
    movies: 0,
    shows: 0,
    episodes: 0,
    ratings: 0,
    playback: 0,
    playbackSkipped: 0,
    watchlist: 0,
    hidden: 0,
    ignoredFiles: [],
    exportErrors: [],
    dryRun,
  };

  // Phase 1: read every file and collect unique items.
  const records: Record<Section, TraktRecord[]> = { history: [], ratings: [], playback: [], watchlist: [], hidden: [] };
  for (const entry of entries) {
    const base = entry.name.split("/").pop() ?? entry.name;
    if (!base.endsWith(".json")) continue;
    const parsed = JSON.parse(entry.read().toString("utf8")) as unknown;
    if (base === "_errors.json") {
      if (Array.isArray(parsed)) report.exportErrors = parsed as TraktImportReport["exportErrors"];
      continue;
    }
    const section = sectionOf(base);
    if (!section || !Array.isArray(parsed)) {
      report.ignoredFiles.push(base);
      continue;
    }
    report.files += 1;
    records[section].push(...(parsed as TraktRecord[]));
    log(`${base}: ${parsed.length} records`);
  }

  const collector = new Collector();
  for (const record of records.history) collector.target(record);
  for (const record of records.ratings) record.type === "show" ? collector.show(record.show) : collector.target(record);
  for (const record of records.playback) collector.target(record);
  for (const record of [...records.watchlist, ...records.hidden]) {
    if (record.type === "show") collector.show(record.show);
    else if (record.type === "movie") collector.movie(record.movie);
  }
  log(`${collector.movies.size} movies, ${collector.shows.size} shows, ${collector.episodes.size} episodes to resolve`);

  // Phase 2: catalog enrichment in bulk.
  await enrichMovies(catalog, collector.movies);
  await enrichShows(catalog, collector.shows);
  await enrichEpisodes(catalog, collector.shows, collector.episodes);

  // Phase 3: write everything in one transaction.
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const library = new Library(client, clock);

    const movieKeys = [...collector.movies.keys()];
    const movieRows = new Map<string, MediaRow>();
    (await bulkUpsertMedia(client, "movie", movieKeys.map((key) => collector.movies.get(key)!), now)).forEach((row, index) =>
      movieRows.set(movieKeys[index]!, row),
    );
    report.movies = new Set([...movieRows.values()].map((row) => row.id)).size;

    const showKeys = [...collector.shows.keys()];
    const showRows = new Map<string, MediaRow>();
    (await bulkUpsertMedia(client, "show", showKeys.map((key) => collector.shows.get(key)!), now)).forEach((row, index) =>
      showRows.set(showKeys[index]!, row),
    );
    report.shows = new Set([...showRows.values()].map((row) => row.id)).size;

    const episodeKeys = [...collector.episodes.keys()];
    const episodeRows = new Map<string, EpisodeRow>();
    (
      await bulkUpsertEpisodes(
        client,
        episodeKeys.map((key) => {
          const entry = collector.episodes.get(key)!;
          return { ...entry.ref, showId: showRows.get(entry.showKey)!.id };
        }),
        now,
      )
    ).forEach((row, index) => episodeRows.set(episodeKeys[index]!, row));
    report.episodes = new Set([...episodeRows.values()].map((row) => row.id)).size;

    const targetOf = (record: TraktRecord): { kind: "movie" | "episode"; id: number; durationMs: number | null } | null => {
      const target = collector.target(record);
      if (!target) return null;
      if (target.kind === "movie") {
        const row = movieRows.get(target.key);
        return row ? { kind: "movie", id: row.id, durationMs: row.duration_ms } : null;
      }
      const row = episodeRows.get(target.key);
      return row ? { kind: "episode", id: row.id, durationMs: row.duration_ms } : null;
    };

    // Plays.
    const plays: PlayInsert[] = [];
    const seen = new Set<string>();
    for (const record of records.history) {
      const target = targetOf(record);
      const externalId = record.id ? String(record.id) : null;
      if (!target || !record.watched_at || (externalId && seen.has(externalId))) {
        report.playsSkipped += 1;
        continue;
      }
      if (externalId) seen.add(externalId);
      plays.push({ kind: target.kind, id: target.id, watchedAt: record.watched_at, source: TRAKT_SOURCE, externalId });
    }
    report.plays = await bulkRecordPlays(client, plays);
    report.playsSkipped += plays.length - report.plays;

    // Ratings: the latest rating per item wins.
    const ratings = new Map<string, RatingInsert>();
    for (const record of records.ratings) {
      if (typeof record.rating !== "number") continue;
      let target: { kind: "movie" | "show" | "episode"; id: number } | null = null;
      if (record.type === "show") {
        const key = collector.show(record.show);
        const row = key ? showRows.get(key) : undefined;
        target = row ? { kind: "show", id: row.id } : null;
      } else {
        target = targetOf(record);
      }
      if (!target) continue;
      const key = `${target.kind}:${target.id}`;
      const ratedAt = record.rated_at ?? now;
      const current = ratings.get(key);
      if (!current || current.ratedAt < ratedAt) ratings.set(key, { kind: target.kind, id: target.id, rating: record.rating, ratedAt });
    }
    await bulkSetRatings(client, [...ratings.values()]);
    report.ratings = ratings.size;

    // Playback positions: need a runtime, and never overwrite a newer position.
    for (const record of records.playback) {
      const target = targetOf(record);
      if (!target || typeof record.progress !== "number" || !target.durationMs) {
        report.playbackSkipped += 1;
        continue;
      }
      const pausedAt = record.paused_at ?? now;
      const existing = await library.getProgress(target.kind, target.id);
      if (existing && existing.updated_at >= pausedAt) {
        report.playbackSkipped += 1;
        continue;
      }
      await library.setProgress({
        kind: target.kind,
        id: target.id,
        positionMs: Math.round((record.progress / 100) * target.durationMs),
        durationMs: target.durationMs,
        state: "stopped",
        player: "Trakt",
        updatedAt: pausedAt,
      });
      report.playback += 1;
    }

    // Watchlist and hidden items.
    const mediaOf = (record: TraktRecord): { kind: "movie" | "show"; id: number } | null => {
      if (record.type === "movie") {
        const row = movieRows.get(collector.movie(record.movie) ?? "");
        return row ? { kind: "movie", id: row.id } : null;
      }
      if (record.type === "show") {
        const row = showRows.get(collector.show(record.show) ?? "");
        return row ? { kind: "show", id: row.id } : null;
      }
      return null;
    };
    for (const record of records.watchlist) {
      const media = mediaOf(record);
      if (!media) continue;
      await library.addToWatchlist(media.kind, media.id, record.listed_at, record.rank ?? null);
      report.watchlist += 1;
    }
    for (const record of records.hidden) {
      const media = mediaOf(record);
      if (!media) continue;
      await library.setHidden(media.id, true, record.hidden_at);
      report.hidden += 1;
    }

    await client.query(dryRun ? "ROLLBACK" : "COMMIT");
  } catch (error) {
    await client.query("ROLLBACK").catch(() => undefined);
    throw error;
  } finally {
    client.release();
  }
  log(
    `${dryRun ? "dry run: " : ""}${report.plays} plays (${report.playsSkipped} skipped), ${report.ratings} ratings, ` +
      `${report.playback} positions (${report.playbackSkipped} skipped), ${report.watchlist} watchlist items, ${report.hidden} hidden`,
  );
  return report;
}
