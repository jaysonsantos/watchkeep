import type { Catalog } from "./catalog/catalog.ts";
import { transaction, type Pool, type TargetKind } from "./db.ts";
import { Library, type Clock, systemClock, type EpisodeInput } from "./library.ts";

/** Manual changes that the UI and the JSON API both expose. */
export class Actions {
  private readonly library: Library;

  constructor(
    private readonly pool: Pool,
    private readonly catalog: Catalog | null,
    private readonly clock: Clock = systemClock,
  ) {
    this.library = new Library(pool, clock);
  }

  async markWatched(kind: TargetKind, id: number, watchedAt?: string): Promise<boolean> {
    if (!(await this.exists(kind, id))) return false;
    await transaction(this.pool, async (client) => {
      const library = new Library(client, this.clock);
      await library.recordPlay({ kind, id, source: "manual", watchedAt });
      await library.clearProgress(kind, id);
    });
    return true;
  }

  async markUnwatched(kind: TargetKind, id: number): Promise<boolean> {
    if (!(await this.exists(kind, id))) return false;
    await transaction(this.pool, async (client) => {
      const library = new Library(client, this.clock);
      await library.removePlays(kind, id);
      await library.clearProgress(kind, id);
    });
    return true;
  }

  /**
   * Mark an episode by season and number. Creates the local episode row when
   * the episode is known only through the catalog.
   */
  async markEpisodeByNumber(showId: number, season: number, number: number, watched: boolean): Promise<boolean> {
    const show = await this.library.getMedia(showId);
    if (!show || show.kind !== "show") return false;
    const episode = await transaction(this.pool, async (client) => {
      const library = new Library(client, this.clock);
      const current = await library.findEpisode(showId, season, number);
      if (current) return current;
      const detail = show.tmdb_id && this.catalog ? await this.catalog.episode(show.tmdb_id, season, number) : null;
      const input: EpisodeInput = {
        season,
        number,
        title: detail?.title ?? null,
        ids: { plexGuid: null, imdb: null, tmdb: detail ? String(detail.tmdbId) : null, tvdb: null },
        durationMs: detail?.runtimeMin ? detail.runtimeMin * 60_000 : null,
        airedAt: detail?.airDate ?? null,
      };
      return library.upsertEpisode(showId, input);
    });
    return watched ? this.markWatched("episode", episode.id) : this.markUnwatched("episode", episode.id);
  }

  /** Add one play to every known episode of the show that has no play yet. */
  async markShowWatched(showId: number, today?: string): Promise<number> {
    const show = await this.library.getMedia(showId);
    if (!show || show.kind !== "show") return -1;
    return transaction(this.pool, async (client) => {
      const library = new Library(client, this.clock);
      let changed = 0;
      if (show.tmdb_id && this.catalog) {
        const cutoff = today ?? this.clock.now().toISOString().slice(0, 10);
        for (const detail of await this.catalog.episodes(show.tmdb_id)) {
          if (detail.season === 0 || (detail.airDate && detail.airDate > cutoff)) continue;
          await library.upsertEpisode(showId, {
            season: detail.season,
            number: detail.number,
            title: detail.title,
            ids: { plexGuid: null, imdb: null, tmdb: String(detail.tmdbId), tvdb: null },
            durationMs: detail.runtimeMin ? detail.runtimeMin * 60_000 : null,
            airedAt: detail.airDate,
          });
        }
      }
      for (const episode of await library.listEpisodes(showId)) {
        if ((await library.playCount("episode", episode.id)) > 0) continue;
        await library.recordPlay({ kind: "episode", id: episode.id, source: "manual" });
        await library.clearProgress("episode", episode.id);
        changed += 1;
      }
      return changed;
    });
  }

  async markShowUnwatched(showId: number): Promise<number> {
    const show = await this.library.getMedia(showId);
    if (!show || show.kind !== "show") return -1;
    return transaction(this.pool, async (client) => {
      const library = new Library(client, this.clock);
      let changed = 0;
      for (const episode of await library.listEpisodes(showId)) {
        changed += await library.removePlays("episode", episode.id);
        await library.clearProgress("episode", episode.id);
      }
      return changed;
    });
  }

  removePlay(playId: number): Promise<boolean> {
    return this.library.removePlay(playId);
  }

  clearProgress(kind: TargetKind, id: number): Promise<void> {
    return this.library.clearProgress(kind, id);
  }

  private async exists(kind: TargetKind, id: number): Promise<boolean> {
    if (kind === "movie") return (await this.library.getMedia(id))?.kind === "movie";
    return (await this.library.getEpisode(id)) !== null;
  }
}
