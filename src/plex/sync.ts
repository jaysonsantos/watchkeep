/**
 * Import the Plex library so that Watchkeep knows which movies and episodes
 * exist and which of them Plex already marks as watched.
 * Uses the Plex Media Server HTTP API with `X-Plex-Token`.
 */
import type { Catalog } from "../catalog/catalog.ts";
import { transaction, type Pool } from "../db.ts";
import { Library, type Clock, systemClock } from "../library.ts";
import { parseIds } from "./payload.ts";
import { enrichEpisode, enrichMovie } from "../scrobble.ts";

export interface SyncOptions {
  plexUrl: string;
  plexToken: string;
  fetchImpl?: typeof fetch;
  pageSize?: number;
  log?: (message: string) => void;
}

export interface SyncReport {
  sections: number;
  movies: number;
  shows: number;
  episodes: number;
  playsImported: number;
  progressImported: number;
}

interface PlexContainer<T> {
  MediaContainer?: {
    size?: number;
    totalSize?: number;
    Directory?: T[];
    Metadata?: T[];
  };
}

interface PlexSection {
  key: string;
  type: string;
  title: string;
}

interface PlexItem {
  ratingKey?: string;
  guid?: string;
  Guid?: Array<{ id?: string }>;
  type?: string;
  title?: string;
  year?: number;
  duration?: number;
  summary?: string;
  viewCount?: number;
  lastViewedAt?: number;
  viewOffset?: number;
  index?: number;
  parentIndex?: number;
  grandparentGuid?: string;
  grandparentRatingKey?: string;
  grandparentTitle?: string;
  originallyAvailableAt?: string;
}

export class PlexClient {
  private readonly fetchImpl: typeof fetch;
  private readonly pageSize: number;

  constructor(private readonly options: SyncOptions) {
    this.fetchImpl = options.fetchImpl ?? fetch;
    this.pageSize = options.pageSize ?? 500;
  }

  private async get<T>(path: string, start = 0): Promise<PlexContainer<T>> {
    const response = await this.fetchImpl(`${this.options.plexUrl}${path}`, {
      headers: {
        Accept: "application/json",
        "X-Plex-Token": this.options.plexToken,
        "X-Plex-Container-Start": String(start),
        "X-Plex-Container-Size": String(this.pageSize),
      },
    });
    if (!response.ok) throw new Error(`Plex request failed: ${response.status} ${path}`);
    return (await response.json()) as PlexContainer<T>;
  }

  async sections(): Promise<PlexSection[]> {
    const body = await this.get<PlexSection>("/library/sections");
    return body.MediaContainer?.Directory ?? [];
  }

  /** Fetch every item of one Plex type inside a section, page by page. */
  async items(sectionKey: string, type: 1 | 2 | 4): Promise<PlexItem[]> {
    const items: PlexItem[] = [];
    let start = 0;
    for (;;) {
      const body = await this.get<PlexItem>(`/library/sections/${sectionKey}/all?type=${type}&includeGuids=1`, start);
      const page = body.MediaContainer?.Metadata ?? [];
      items.push(...page);
      const total = body.MediaContainer?.totalSize ?? page.length;
      start += page.length;
      if (page.length === 0 || start >= total) break;
    }
    return items;
  }
}

function isoFromUnix(seconds: number | undefined, fallback: string): string {
  return seconds ? new Date(seconds * 1000).toISOString() : fallback;
}

export async function syncPlexLibrary(
  pool: Pool,
  catalog: Catalog | null,
  options: SyncOptions,
  clock: Clock = systemClock,
): Promise<SyncReport> {
  if (!options.plexUrl || !options.plexToken) {
    throw new Error("Plex sync needs WATCHKEEP_PLEX_URL and WATCHKEEP_PLEX_TOKEN");
  }
  const log = options.log ?? (() => {});
  const client = new PlexClient(options);
  const report: SyncReport = { sections: 0, movies: 0, shows: 0, episodes: 0, playsImported: 0, progressImported: 0 };

  const importWatchState = async (
    library: Library,
    kind: "movie" | "episode",
    id: number,
    item: PlexItem,
    durationMs: number | null,
  ) => {
    if ((item.viewCount ?? 0) > 0 && (await library.playCount(kind, id)) === 0) {
      await library.recordPlay({
        kind,
        id,
        watchedAt: isoFromUnix(item.lastViewedAt, clock.now().toISOString()),
        source: "plex-sync",
      });
      report.playsImported += 1;
    }
    if ((item.viewOffset ?? 0) > 0 && !(await library.getProgress(kind, id))) {
      await library.setProgress({ kind, id, positionMs: item.viewOffset!, durationMs, state: "stopped" });
      report.progressImported += 1;
    }
  };

  for (const section of await client.sections()) {
    if (section.type !== "movie" && section.type !== "show") continue;
    report.sections += 1;
    log(`sync: section "${section.title}" (${section.type})`);

    if (section.type === "movie") {
      const movies = await client.items(section.key, 1);
      await transaction(pool, async (tx) => {
        const library = new Library(tx, clock);
        for (const item of movies) {
          if (!item.title) continue;
          const movie = await library.upsertMovie(
            await enrichMovie(catalog, {
              type: "movie",
              title: item.title,
              year: item.year ?? null,
              ids: parseIds(item.guid, item.Guid),
              durationMs: item.duration ?? null,
              summary: item.summary ?? null,
            }),
          );
          report.movies += 1;
          await importWatchState(library, "movie", movie.id, item, movie.duration_ms);
        }
      });
      continue;
    }

    const shows = await client.items(section.key, 2);
    const episodes = await client.items(section.key, 4);
    await transaction(pool, async (tx) => {
      const library = new Library(tx, clock);
      const showByKey = new Map<string, { id: number; title: string; year: number | null; ids: ReturnType<typeof parseIds> }>();
      for (const item of shows) {
        if (!item.title) continue;
        const ids = parseIds(item.guid, item.Guid);
        const show = await library.upsertShow({ title: item.title, year: item.year ?? null, ids, summary: item.summary ?? null });
        report.shows += 1;
        const entry = { id: show.id, title: item.title, year: item.year ?? null, ids };
        if (item.ratingKey) showByKey.set(item.ratingKey, entry);
        if (item.guid) showByKey.set(item.guid, entry);
      }
      for (const item of episodes) {
        if (item.parentIndex === undefined || item.index === undefined) continue;
        const parent =
          (item.grandparentRatingKey && showByKey.get(item.grandparentRatingKey)) ||
          (item.grandparentGuid && showByKey.get(item.grandparentGuid)) ||
          null;
        const showTitle = parent?.title ?? item.grandparentTitle;
        if (!showTitle) continue;
        const enriched = await enrichEpisode(catalog, {
          type: "episode",
          title: item.title ?? null,
          season: item.parentIndex,
          number: item.index,
          ids: parseIds(item.guid, item.Guid),
          durationMs: item.duration ?? null,
          airedAt: item.originallyAvailableAt ?? null,
          show: {
            title: showTitle,
            year: parent?.year ?? null,
            ids: parent?.ids ?? parseIds(item.grandparentGuid, undefined),
          },
        });
        const show = await library.upsertShow(enriched.show);
        const episode = await library.upsertEpisode(show.id, enriched);
        report.episodes += 1;
        await importWatchState(library, "episode", episode.id, item, episode.duration_ms);
      }
    });
  }
  log(
    `sync: ${report.movies} movies, ${report.shows} shows, ${report.episodes} episodes, ` +
      `${report.playsImported} plays and ${report.progressImported} positions imported`,
  );
  return report;
}
