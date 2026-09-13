/**
 * Read models that combine Watchkeep data with the TMDB catalog.
 * The catalog supplies the full episode list, so unwatched episodes show up
 * even when Plex never reported them.
 */
import type { Catalog, CatalogEpisode } from "./catalog/catalog.ts";
import type { Clock } from "./library.ts";
import type { EpisodeView, Queries, ShowView, SortOrder, WatchFilter } from "./queries.ts";

export interface ShowListItem extends ShowView {
  /** Episodes known to the catalog (aired, regular seasons) or, without a catalog, episodes seen locally. */
  total_episodes: number;
}

export interface MergedEpisode {
  /** Local episode id, or null when the episode exists only in the catalog. */
  id: number | null;
  season: number;
  number: number;
  title: string | null;
  aired_at: string | null;
  duration_ms: number | null;
  play_count: number;
  last_watched_at: string | null;
  position_ms: number | null;
  progress_state: string | null;
  tmdb_id: number | null;
}

export class Views {
  constructor(
    private readonly queries: Queries,
    private readonly catalog: Catalog | null,
    private readonly clock: Clock,
  ) {}

  private today(): string {
    return this.clock.now().toISOString().slice(0, 10);
  }

  /** Filters in memory, because the watched state needs the catalog episode counts. */
  async shows(filter: WatchFilter = "all", search = "", sort: SortOrder = "recent"): Promise<ShowListItem[]> {
    const rows = await this.queries.shows(search, sort);
    const tmdbIds = rows.map((row) => row.tmdb_id).filter((id): id is number => id !== null);
    const totals = this.catalog ? await this.catalog.airedEpisodeCounts(tmdbIds, this.today()) : new Map<number, number>();
    const items = rows.map((row) => ({
      ...row,
      total_episodes: Math.max(row.episode_count, row.tmdb_id === null ? 0 : (totals.get(row.tmdb_id) ?? 0)),
    }));
    if (filter === "watched") return items.filter((item) => item.total_episodes > 0 && item.watched_count >= item.total_episodes);
    if (filter === "unwatched") {
      return items.filter((item) => item.hidden_at === null && (item.watched_count < item.total_episodes || item.total_episodes === 0));
    }
    return items;
  }

  async show(id: number): Promise<{ show: ShowListItem; episodes: MergedEpisode[] } | null> {
    const show = await this.queries.show(id);
    if (!show) return null;
    const local = await this.queries.episodes(id);
    const fromCatalog = show.tmdb_id && this.catalog ? await this.catalog.episodes(show.tmdb_id) : [];
    const episodes = mergeEpisodes(local, fromCatalog);
    const today = this.today();
    const total = fromCatalog.length
      ? fromCatalog.filter((episode) => episode.season > 0 && (!episode.airDate || episode.airDate <= today)).length
      : local.length;
    return { show: { ...show, total_episodes: Math.max(total, local.length) }, episodes };
  }
}

export function mergeEpisodes(local: EpisodeView[], fromCatalog: CatalogEpisode[]): MergedEpisode[] {
  const byNumber = new Map<string, MergedEpisode>();
  const key = (season: number, number: number) => `${season}:${number}`;
  for (const episode of fromCatalog) {
    byNumber.set(key(episode.season, episode.number), {
      id: null,
      season: episode.season,
      number: episode.number,
      title: episode.title,
      aired_at: episode.airDate,
      duration_ms: episode.runtimeMin ? episode.runtimeMin * 60_000 : null,
      play_count: 0,
      last_watched_at: null,
      position_ms: null,
      progress_state: null,
      tmdb_id: episode.tmdbId,
    });
  }
  for (const episode of local) {
    const existing = byNumber.get(key(episode.season, episode.number));
    byNumber.set(key(episode.season, episode.number), {
      id: episode.id,
      season: episode.season,
      number: episode.number,
      title: episode.title ?? existing?.title ?? null,
      aired_at: episode.aired_at ?? existing?.aired_at ?? null,
      duration_ms: episode.duration_ms ?? existing?.duration_ms ?? null,
      play_count: episode.play_count,
      last_watched_at: episode.last_watched_at,
      position_ms: episode.position_ms,
      progress_state: episode.progress_state,
      tmdb_id: episode.tmdb_id ?? existing?.tmdb_id ?? null,
    });
  }
  return [...byNumber.values()].sort((a, b) => a.season - b.season || a.number - b.number);
}
