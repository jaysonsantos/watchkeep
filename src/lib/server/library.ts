import type { EpisodeRow, MediaKind, MediaRow, PlayRow, PlayState, ProgressRow, Queryable, TargetKind, WatchlistRow } from "./db.ts";
import type { ExternalIds } from "./plex/payload.ts";

export interface MovieInput {
  title: string;
  year: number | null;
  ids: ExternalIds;
  durationMs: number | null;
  summary: string | null;
  posterPath?: string | null;
}

export interface ShowInput {
  title: string;
  year: number | null;
  ids: ExternalIds;
  summary?: string | null;
  posterPath?: string | null;
}

export interface EpisodeInput {
  season: number;
  number: number;
  title: string | null;
  ids: ExternalIds;
  durationMs: number | null;
  airedAt: string | null;
}

export interface Clock {
  now(): Date;
}

export const systemClock: Clock = { now: () => new Date() };

function coalesce<T>(next: T | null | undefined, current: T | null): T | null {
  return next === null || next === undefined ? current : next;
}

function tmdbNumber(value: string | null): number | null {
  if (value === null) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

/**
 * Data access for the watch library. Every method runs one or a few statements
 * on the given connection. Callers that need atomicity pass a transaction client.
 */
export class Library {
  constructor(
    private readonly db: Queryable,
    private readonly clock: Clock = systemClock,
  ) {}

  private now(): string {
    return this.clock.now().toISOString();
  }

  private async one<T extends object>(text: string, values: unknown[]): Promise<T | null> {
    const { rows } = await this.db.query<T & Record<string, unknown>>(text, values);
    return rows[0] ?? null;
  }

  // --- media ---------------------------------------------------------------

  async findMedia(kind: "movie" | "show", title: string, year: number | null, ids: ExternalIds): Promise<MediaRow | null> {
    const byColumn = async (column: string, value: string | number | null): Promise<MediaRow | null> => {
      if (value === null || value === "") return null;
      return this.one<MediaRow>(`SELECT * FROM media WHERE kind = $1 AND ${column} = $2`, [kind, value]);
    };
    const found =
      (await byColumn("plex_guid", ids.plexGuid)) ??
      (await byColumn("tmdb_id", tmdbNumber(ids.tmdb))) ??
      (await byColumn("tvdb_id", ids.tvdb)) ??
      (await byColumn("imdb_id", ids.imdb));
    if (found) return found;
    return this.one<MediaRow>(
      `SELECT * FROM media WHERE kind = $1 AND lower(title) = lower($2)
         AND (year IS NULL OR $3::int IS NULL OR year = $3)
       ORDER BY (year = $3) DESC NULLS LAST, id LIMIT 1`,
      [kind, title, year],
    );
  }

  getMedia(id: number): Promise<MediaRow | null> {
    return this.one<MediaRow>("SELECT * FROM media WHERE id = $1", [id]);
  }

  private async updateMedia(current: MediaRow, input: ShowInput | MovieInput): Promise<MediaRow> {
    const row = await this.one<MediaRow>(
      `UPDATE media SET title = $1, year = $2, plex_guid = $3, imdb_id = $4, tmdb_id = $5, tvdb_id = $6,
         duration_ms = $7, summary = $8, poster_path = $9, updated_at = $10 WHERE id = $11 RETURNING *`,
      [
        input.title,
        coalesce(input.year, current.year),
        coalesce(input.ids.plexGuid, current.plex_guid),
        coalesce(input.ids.imdb, current.imdb_id),
        coalesce(tmdbNumber(input.ids.tmdb), current.tmdb_id),
        coalesce(input.ids.tvdb, current.tvdb_id),
        coalesce("durationMs" in input ? input.durationMs : null, current.duration_ms),
        coalesce(input.summary, current.summary),
        coalesce(input.posterPath, current.poster_path),
        this.now(),
        current.id,
      ],
    );
    return row!;
  }

  private async insertMedia(kind: "movie" | "show", input: ShowInput | MovieInput): Promise<MediaRow> {
    const now = this.now();
    const row = await this.one<MediaRow>(
      `INSERT INTO media (kind, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, created_at, updated_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11) RETURNING *`,
      [
        kind,
        input.title,
        input.year,
        input.ids.plexGuid,
        input.ids.imdb,
        tmdbNumber(input.ids.tmdb),
        input.ids.tvdb,
        "durationMs" in input ? input.durationMs : null,
        input.summary ?? null,
        input.posterPath ?? null,
        now,
      ],
    );
    return row!;
  }

  async upsertMovie(input: MovieInput): Promise<MediaRow> {
    const current = await this.findMedia("movie", input.title, input.year, input.ids);
    return current ? this.updateMedia(current, input) : this.insertMedia("movie", input);
  }

  async upsertShow(input: ShowInput): Promise<MediaRow> {
    const current = await this.findMedia("show", input.title, input.year, input.ids);
    return current ? this.updateMedia(current, input) : this.insertMedia("show", input);
  }

  // --- episodes ------------------------------------------------------------

  getEpisode(id: number): Promise<EpisodeRow | null> {
    return this.one<EpisodeRow>("SELECT * FROM episodes WHERE id = $1", [id]);
  }

  findEpisode(showId: number, season: number, number: number): Promise<EpisodeRow | null> {
    return this.one<EpisodeRow>("SELECT * FROM episodes WHERE show_id = $1 AND season = $2 AND number = $3", [
      showId,
      season,
      number,
    ]);
  }

  async upsertEpisode(showId: number, input: EpisodeInput): Promise<EpisodeRow> {
    const byGuid = input.ids.plexGuid
      ? await this.one<EpisodeRow>("SELECT * FROM episodes WHERE plex_guid = $1", [input.ids.plexGuid])
      : null;
    const current = byGuid ?? (await this.findEpisode(showId, input.season, input.number));
    const now = this.now();
    if (current) {
      const row = await this.one<EpisodeRow>(
        `UPDATE episodes SET show_id = $1, season = $2, number = $3, title = $4, plex_guid = $5, imdb_id = $6, tmdb_id = $7,
           tvdb_id = $8, duration_ms = $9, aired_at = $10, updated_at = $11 WHERE id = $12 RETURNING *`,
        [
          showId,
          input.season,
          input.number,
          coalesce(input.title, current.title),
          coalesce(input.ids.plexGuid, current.plex_guid),
          coalesce(input.ids.imdb, current.imdb_id),
          coalesce(tmdbNumber(input.ids.tmdb), current.tmdb_id),
          coalesce(input.ids.tvdb, current.tvdb_id),
          coalesce(input.durationMs, current.duration_ms),
          coalesce(input.airedAt, current.aired_at),
          now,
          current.id,
        ],
      );
      return row!;
    }
    const row = await this.one<EpisodeRow>(
      `INSERT INTO episodes (show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11) RETURNING *`,
      [
        showId,
        input.season,
        input.number,
        input.title,
        input.ids.plexGuid,
        input.ids.imdb,
        tmdbNumber(input.ids.tmdb),
        input.ids.tvdb,
        input.durationMs,
        input.airedAt,
        now,
      ],
    );
    return row!;
  }

  async listEpisodes(showId: number): Promise<EpisodeRow[]> {
    const { rows } = await this.db.query<EpisodeRow>("SELECT * FROM episodes WHERE show_id = $1 ORDER BY season, number", [
      showId,
    ]);
    return rows;
  }

  // --- plays ---------------------------------------------------------------

  lastPlay(kind: TargetKind, id: number): Promise<PlayRow | null> {
    return this.one<PlayRow>(
      "SELECT * FROM plays WHERE target_kind = $1 AND target_id = $2 ORDER BY watched_at DESC LIMIT 1",
      [kind, id],
    );
  }

  async playCount(kind: TargetKind, id: number): Promise<number> {
    const row = await this.one<{ count: number }>(
      "SELECT COUNT(*)::int AS count FROM plays WHERE target_kind = $1 AND target_id = $2",
      [kind, id],
    );
    return row?.count ?? 0;
  }

  /**
   * Insert a play. With `externalId`, a play that already exists for the same
   * source and id is skipped and `null` is returned.
   */
  async recordPlay(input: {
    kind: TargetKind;
    id: number;
    watchedAt?: string;
    source: string;
    account?: string | null;
    player?: string | null;
    externalId?: string | null;
  }): Promise<PlayRow | null> {
    return this.one<PlayRow>(
      `INSERT INTO plays (target_kind, target_id, watched_at, source, account, player, external_id)
       VALUES ($1, $2, $3, $4, $5, $6, $7)
       ON CONFLICT (source, external_id) WHERE external_id IS NOT NULL DO NOTHING
       RETURNING *`,
      [
        input.kind,
        input.id,
        input.watchedAt ?? this.now(),
        input.source,
        input.account ?? null,
        input.player ?? null,
        input.externalId ?? null,
      ],
    );
  }

  async removePlays(kind: TargetKind, id: number): Promise<number> {
    const result = await this.db.query("DELETE FROM plays WHERE target_kind = $1 AND target_id = $2", [kind, id]);
    return result.rowCount ?? 0;
  }

  async removePlay(playId: number): Promise<boolean> {
    const result = await this.db.query("DELETE FROM plays WHERE id = $1", [playId]);
    return (result.rowCount ?? 0) > 0;
  }

  // --- progress ------------------------------------------------------------

  getProgress(kind: TargetKind, id: number): Promise<ProgressRow | null> {
    return this.one<ProgressRow>("SELECT * FROM progress WHERE target_kind = $1 AND target_id = $2", [kind, id]);
  }

  async setProgress(input: {
    kind: TargetKind;
    id: number;
    positionMs: number;
    durationMs: number | null;
    state: PlayState;
    account?: string | null;
    player?: string | null;
    updatedAt?: string;
  }): Promise<ProgressRow> {
    const row = await this.one<ProgressRow>(
      `INSERT INTO progress (target_kind, target_id, position_ms, duration_ms, state, account, player, updated_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
       ON CONFLICT (target_kind, target_id) DO UPDATE SET
         position_ms = EXCLUDED.position_ms,
         duration_ms = COALESCE(EXCLUDED.duration_ms, progress.duration_ms),
         state = EXCLUDED.state,
         account = EXCLUDED.account,
         player = EXCLUDED.player,
         updated_at = EXCLUDED.updated_at
       RETURNING *`,
      [
        input.kind,
        input.id,
        input.positionMs,
        input.durationMs,
        input.state,
        input.account ?? null,
        input.player ?? null,
        input.updatedAt ?? this.now(),
      ],
    );
    return row!;
  }

  async clearProgress(kind: TargetKind, id: number): Promise<void> {
    await this.db.query("DELETE FROM progress WHERE target_kind = $1 AND target_id = $2", [kind, id]);
  }

  // --- ratings -------------------------------------------------------------

  async setRating(kind: "movie" | "show" | "episode", id: number, rating: number, ratedAt?: string): Promise<void> {
    await this.db.query(
      `INSERT INTO ratings (target_kind, target_id, rating, rated_at) VALUES ($1, $2, $3, $4)
       ON CONFLICT (target_kind, target_id) DO UPDATE SET rating = EXCLUDED.rating, rated_at = EXCLUDED.rated_at`,
      [kind, id, rating, ratedAt ?? this.now()],
    );
  }

  // --- watchlist and hidden -----------------------------------------------

  async addToWatchlist(kind: MediaKind, id: number, listedAt?: string, rank: number | null = null): Promise<void> {
    await this.db.query(
      `INSERT INTO watchlist (target_kind, target_id, listed_at, rank) VALUES ($1, $2, $3, $4)
       ON CONFLICT (target_kind, target_id) DO UPDATE SET rank = COALESCE(EXCLUDED.rank, watchlist.rank)`,
      [kind, id, listedAt ?? this.now(), rank],
    );
  }

  async removeFromWatchlist(kind: MediaKind, id: number): Promise<boolean> {
    const result = await this.db.query("DELETE FROM watchlist WHERE target_kind = $1 AND target_id = $2", [kind, id]);
    return (result.rowCount ?? 0) > 0;
  }

  async listWatchlist(): Promise<WatchlistRow[]> {
    const { rows } = await this.db.query<WatchlistRow>("SELECT * FROM watchlist ORDER BY rank NULLS LAST, listed_at");
    return rows;
  }

  async setHidden(id: number, hidden: boolean, at?: string): Promise<void> {
    await this.db.query("UPDATE media SET hidden_at = $1, updated_at = $2 WHERE id = $3", [
      hidden ? (at ?? this.now()) : null,
      this.now(),
      id,
    ]);
  }

  async getRating(kind: "movie" | "show" | "episode", id: number): Promise<number | null> {
    const row = await this.one<{ rating: number }>("SELECT rating FROM ratings WHERE target_kind = $1 AND target_id = $2", [
      kind,
      id,
    ]);
    return row?.rating ?? null;
  }

  // --- webhook log ---------------------------------------------------------

  async logWebhook(input: {
    event: string | null;
    account: string | null;
    player: string | null;
    mediaType: string | null;
    title: string | null;
    outcome: string;
    payload: string;
  }): Promise<void> {
    await this.db.query(
      `INSERT INTO webhook_events (received_at, event, account, player, media_type, title, outcome, payload)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
      [this.now(), input.event ?? "unknown", input.account, input.player, input.mediaType, input.title, input.outcome, input.payload],
    );
  }

  async pruneWebhookLog(retentionDays: number): Promise<number> {
    if (retentionDays <= 0) return 0;
    const cutoff = new Date(this.clock.now().getTime() - retentionDays * 86_400_000).toISOString();
    const result = await this.db.query("DELETE FROM webhook_events WHERE received_at < $1", [cutoff]);
    return result.rowCount ?? 0;
  }
}
