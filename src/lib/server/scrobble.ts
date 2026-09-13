import type { Catalog, CatalogEpisode, CatalogMovie, CatalogShow } from "./catalog/catalog.ts";
import type { Config } from "./config.ts";
import { transaction, type Pool, type Queryable, type TargetKind } from "./db.ts";
import { Library, type Clock, systemClock } from "./library.ts";
import type { EpisodeRef, ExternalIds, MediaRef, MovieRef, PlexEvent } from "./plex/payload.ts";

export interface ScrobbleResult {
  action: "progress" | "play" | "duplicate-play" | "rating" | "ignored-account";
  targetKind: TargetKind;
  targetId: number;
  title: string;
  positionMs?: number;
  percent?: number;
}

export interface ResolvedTarget {
  kind: TargetKind;
  id: number;
  title: string;
  durationMs: number | null;
  showId: number | null;
}

function withTmdb(ids: ExternalIds, tmdbId: number | null, imdb?: string | null, tvdb?: string | null): ExternalIds {
  return {
    plexGuid: ids.plexGuid,
    tmdb: ids.tmdb ?? (tmdbId === null ? null : String(tmdbId)),
    imdb: ids.imdb ?? imdb ?? null,
    tvdb: ids.tvdb ?? tvdb ?? null,
  };
}

/** Merge catalog data into a movie reference. Plex values win, the catalog fills gaps. */
export function movieWithCatalog(media: MovieRef, match: CatalogMovie | null): MovieRef {
  if (!match) return media;
  return {
    ...media,
    ids: withTmdb(media.ids, match.tmdbId, match.imdbId),
    year: media.year ?? match.year,
    durationMs: media.durationMs ?? (match.runtimeMin ? match.runtimeMin * 60_000 : null),
    summary: media.summary ?? match.overview,
    posterPath: match.posterPath,
  };
}

export function showWithCatalog(show: ShowRef, match: CatalogShow | null): ShowRef {
  if (!match) return show;
  return {
    title: show.title,
    year: show.year ?? match.year,
    ids: withTmdb(show.ids, match.tmdbId, match.imdbId, match.tvdbId),
    posterPath: match.posterPath,
    summary: match.overview,
  };
}

export function episodeWithCatalog(media: EpisodeRef, show: ShowRef, episode: CatalogEpisode | null): EpisodeRef {
  return {
    ...media,
    title: media.title ?? episode?.title ?? null,
    ids: withTmdb(media.ids, episode?.tmdbId ?? null),
    durationMs: media.durationMs ?? (episode?.runtimeMin ? episode.runtimeMin * 60_000 : null),
    airedAt: media.airedAt ?? episode?.airDate ?? null,
    show,
  };
}

/** Fill in TMDB ids, posters, and runtimes from the catalog when it knows the item. */
export async function enrichMovie(catalog: Catalog | null, media: MovieRef): Promise<MovieRef> {
  if (!catalog) return media;
  const tmdbId = Number(media.ids.tmdb);
  const match =
    (Number.isInteger(tmdbId) && tmdbId > 0 ? await catalog.movieByTmdbId(tmdbId) : null) ??
    (media.ids.imdb ? await catalog.movieByImdbId(media.ids.imdb) : null) ??
    (await catalog.movieByTitle(media.title, media.year));
  return movieWithCatalog(media, match);
}

export type ShowRef = EpisodeRef["show"];

/** Find the catalog show for a show reference, or for the episode TMDB id when given. */
export async function enrichShow(catalog: Catalog | null, show: ShowRef, episodeTmdbId: string | null = null): Promise<ShowRef> {
  if (!catalog) return show;
  const showTmdb = Number(show.ids.tmdb);
  const episodeTmdb = Number(episodeTmdbId);
  let match: CatalogShow | null = null;
  if (Number.isInteger(showTmdb) && showTmdb > 0) match = await catalog.showByTmdbId(showTmdb);
  if (!match && Number.isInteger(episodeTmdb) && episodeTmdb > 0) match = (await catalog.showForEpisode(episodeTmdb))?.show ?? null;
  if (!match && show.ids.tvdb) match = await catalog.showByTvdbId(show.ids.tvdb);
  if (!match && show.ids.imdb) match = await catalog.showByImdbId(show.ids.imdb);
  if (!match) match = await catalog.showByName(show.title, show.year);
  return showWithCatalog(show, match);
}

export async function enrichEpisode(catalog: Catalog | null, media: EpisodeRef): Promise<EpisodeRef> {
  if (!catalog) return media;
  const show = await enrichShow(catalog, media.show, media.ids.tmdb);
  const showTmdb = Number(show.ids.tmdb);
  if (!Number.isInteger(showTmdb) || showTmdb <= 0) return { ...media, show };
  return episodeWithCatalog(media, show, await catalog.episode(showTmdb, media.season, media.number));
}

/** Create or update the movie, show, and episode rows for a media reference. */
export async function resolveTarget(library: Library, catalog: Catalog | null, media: MediaRef): Promise<ResolvedTarget> {
  if (media.type === "movie") {
    const movie = await library.upsertMovie(await enrichMovie(catalog, media));
    const label = movie.year ? `${movie.title} (${movie.year})` : movie.title;
    return { kind: "movie", id: movie.id, title: label, durationMs: movie.duration_ms, showId: null };
  }
  const enriched = await enrichEpisode(catalog, media);
  const show = await library.upsertShow(enriched.show);
  const episode = await library.upsertEpisode(show.id, enriched);
  const code = `S${String(episode.season).padStart(2, "0")}E${String(episode.number).padStart(2, "0")}`;
  const label = episode.title ? `${show.title} ${code} ${episode.title}` : `${show.title} ${code}`;
  return { kind: "episode", id: episode.id, title: label, durationMs: episode.duration_ms, showId: show.id };
}

function accountAllowed(config: Config, event: PlexEvent): boolean {
  if (config.plexAccounts.length === 0) return true;
  return config.plexAccounts.some((allowed) => allowed === event.account || allowed === event.accountId);
}

export class Scrobbler {
  constructor(
    private readonly pool: Pool,
    private readonly config: Config,
    private readonly catalog: Catalog | null,
    private readonly clock: Clock = systemClock,
  ) {}

  async apply(event: PlexEvent): Promise<ScrobbleResult> {
    if (!accountAllowed(this.config, event)) {
      return { action: "ignored-account", targetKind: event.media.type, targetId: 0, title: event.media.type };
    }
    return transaction(this.pool, async (client) => {
      const library = new Library(client, this.clock);
      const target = await resolveTarget(library, this.catalog, event.media);
      switch (event.event) {
        case "media.rate":
          return this.rate(library, event, target);
        case "media.scrobble":
          return this.recordPlay(library, event, target, "plex-scrobble");
        case "media.stop":
          return this.stop(library, event, target);
        default:
          return this.progress(library, event, target, event.event === "media.pause" ? "paused" : "playing");
      }
    });
  }

  private async rate(library: Library, event: PlexEvent, target: ResolvedTarget): Promise<ScrobbleResult> {
    if (event.rating !== null) await library.setRating(target.kind, target.id, event.rating);
    return { action: "rating", targetKind: target.kind, targetId: target.id, title: target.title };
  }

  private async progress(
    library: Library,
    event: PlexEvent,
    target: ResolvedTarget,
    state: "playing" | "paused" | "stopped",
  ): Promise<ScrobbleResult> {
    const existing = await library.getProgress(target.kind, target.id);
    const positionMs = event.viewOffsetMs ?? existing?.position_ms ?? 0;
    const durationMs = target.durationMs ?? existing?.duration_ms ?? null;
    await library.setProgress({
      kind: target.kind,
      id: target.id,
      positionMs,
      durationMs,
      state,
      account: event.account,
      player: event.player,
    });
    return {
      action: "progress",
      targetKind: target.kind,
      targetId: target.id,
      title: target.title,
      positionMs,
      percent: percentOf(positionMs, durationMs),
    };
  }

  private async stop(library: Library, event: PlexEvent, target: ResolvedTarget): Promise<ScrobbleResult> {
    const existing = await library.getProgress(target.kind, target.id);
    const positionMs = event.viewOffsetMs ?? existing?.position_ms ?? 0;
    const durationMs = target.durationMs ?? existing?.duration_ms ?? null;
    const percent = percentOf(positionMs, durationMs);
    if (percent !== undefined && percent >= this.config.watchedThresholdPercent) {
      return this.recordPlay(library, event, target, "plex-stop");
    }
    return this.progress(library, event, target, "stopped");
  }

  private async recordPlay(library: Library, event: PlexEvent, target: ResolvedTarget, source: string): Promise<ScrobbleResult> {
    const last = await library.lastPlay(target.kind, target.id);
    const windowMs = this.config.rewatchWindowMinutes * 60_000;
    const now = this.clock.now().getTime();
    await library.clearProgress(target.kind, target.id);
    if (last && now - Date.parse(last.watched_at) < windowMs) {
      return { action: "duplicate-play", targetKind: target.kind, targetId: target.id, title: target.title };
    }
    await library.recordPlay({ kind: target.kind, id: target.id, source, account: event.account, player: event.player });
    return { action: "play", targetKind: target.kind, targetId: target.id, title: target.title, percent: 100 };
  }
}

export function percentOf(positionMs: number, durationMs: number | null): number | undefined {
  if (!durationMs || durationMs <= 0) return undefined;
  return Math.min(100, Math.round((positionMs / durationMs) * 1000) / 10);
}

export type { Queryable };
