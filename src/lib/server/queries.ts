import type { SortOrder, WatchFilter } from "../lists.ts";
import type { EpisodeRow, MediaRow, PlayRow, ProgressRow, Queryable } from "./db.ts";

export type { SortOrder, WatchFilter };

export interface MovieView extends MediaRow {
  play_count: number;
  last_watched_at: string | null;
  rating: number | null;
}

export interface ShowView extends MediaRow {
  episode_count: number;
  watched_count: number;
  last_watched_at: string | null;
  rating: number | null;
}

export interface EpisodeView extends EpisodeRow {
  play_count: number;
  last_watched_at: string | null;
  position_ms: number | null;
  progress_state: string | null;
}

export interface HistoryEntry extends PlayRow {
  title: string;
  show_id: number | null;
  show_title: string | null;
  season: number | null;
  number: number | null;
  year: number | null;
  poster_path: string | null;
}

export interface ProgressView extends ProgressRow {
  title: string;
  show_id: number | null;
  show_title: string | null;
  season: number | null;
  number: number | null;
  poster_path: string | null;
}

export interface WatchlistItem {
  kind: "movie" | "show";
  id: number;
  title: string;
  year: number | null;
  poster_path: string | null;
  listed_at: string;
  rank: number | null;
  /** Plays for a movie, watched episodes for a show. */
  watched_count: number;
}

export interface Stats {
  movies: number;
  movies_watched: number;
  shows: number;
  episodes: number;
  episodes_watched: number;
  plays: number;
}

export interface WebhookLogEntry {
  id: number;
  received_at: string;
  event: string;
  account: string | null;
  player: string | null;
  media_type: string | null;
  title: string | null;
  outcome: string;
}

const HISTORY_SELECT = `
  SELECT p.*,
    CASE p.target_kind WHEN 'movie' THEN m.title ELSE e.title END AS title,
    CASE p.target_kind WHEN 'movie' THEN NULL ELSE s.id END AS show_id,
    CASE p.target_kind WHEN 'movie' THEN NULL ELSE s.title END AS show_title,
    e.season AS season, e.number AS number,
    CASE p.target_kind WHEN 'movie' THEN m.year ELSE s.year END AS year,
    CASE p.target_kind WHEN 'movie' THEN m.poster_path ELSE s.poster_path END AS poster_path
  FROM plays p
  LEFT JOIN media m ON p.target_kind = 'movie' AND m.id = p.target_id
  LEFT JOIN episodes e ON p.target_kind = 'episode' AND e.id = p.target_id
  LEFT JOIN media s ON s.id = e.show_id`;

const SHOW_SELECT = `
  SELECT s.*,
    COUNT(e.id)::int AS episode_count,
    COALESCE(SUM(CASE WHEN EXISTS (SELECT 1 FROM plays p WHERE p.target_kind = 'episode' AND p.target_id = e.id) THEN 1 ELSE 0 END), 0)::int AS watched_count,
    (SELECT MAX(p.watched_at) FROM plays p JOIN episodes e2 ON e2.id = p.target_id
       WHERE p.target_kind = 'episode' AND e2.show_id = s.id) AS last_watched_at,
    (SELECT rating FROM ratings r WHERE r.target_kind = 'show' AND r.target_id = s.id) AS rating
  FROM media s
  LEFT JOIN episodes e ON e.show_id = s.id`;

const MOVIE_SELECT = `
  SELECT m.*, COUNT(p.id)::int AS play_count, MAX(p.watched_at) AS last_watched_at,
    (SELECT rating FROM ratings r WHERE r.target_kind = 'movie' AND r.target_id = m.id) AS rating
  FROM media m
  LEFT JOIN plays p ON p.target_kind = 'movie' AND p.target_id = m.id`;

const MOVIE_ORDER: Record<SortOrder, string> = {
  recent: "MAX(p.watched_at) DESC NULLS LAST, m.created_at DESC, lower(m.title), m.id",
  title: "lower(m.title), m.year NULLS LAST, m.id",
  year: "m.year DESC NULLS LAST, lower(m.title), m.id",
  added: "m.created_at DESC, m.id DESC",
};

const SHOW_ORDER: Record<SortOrder, string> = {
  recent: "last_watched_at DESC NULLS LAST, s.created_at DESC, lower(s.title), s.id",
  title: "lower(s.title), s.year NULLS LAST, s.id",
  year: "s.year DESC NULLS LAST, lower(s.title), s.id",
  added: "s.created_at DESC, s.id DESC",
};

function movieHaving(filter: WatchFilter): string {
  return filter === "watched" ? "HAVING COUNT(p.id) > 0" : filter === "unwatched" ? "HAVING COUNT(p.id) = 0" : "";
}

export class Queries {
  constructor(private readonly db: Queryable) {}

  async stats(): Promise<Stats> {
    const { rows } = await this.db.query<Stats>(
      `SELECT
        (SELECT COUNT(*) FROM media WHERE kind = 'movie')::int AS movies,
        (SELECT COUNT(DISTINCT target_id) FROM plays WHERE target_kind = 'movie')::int AS movies_watched,
        (SELECT COUNT(*) FROM media WHERE kind = 'show')::int AS shows,
        (SELECT COUNT(*) FROM episodes)::int AS episodes,
        (SELECT COUNT(DISTINCT target_id) FROM plays WHERE target_kind = 'episode')::int AS episodes_watched,
        (SELECT COUNT(*) FROM plays)::int AS plays`,
    );
    return rows[0]!;
  }

  /** A `limit` of null returns every row. */
  async movies(filter: WatchFilter = "all", search = "", sort: SortOrder = "recent", limit: number | null = null, offset = 0): Promise<MovieView[]> {
    const { rows } = await this.db.query<MovieView>(
      `${MOVIE_SELECT}
       WHERE m.kind = 'movie' AND ($1 = '' OR m.title ILIKE '%' || $1 || '%')
       GROUP BY m.id ${movieHaving(filter)}
       ORDER BY ${MOVIE_ORDER[sort]}
       LIMIT $2 OFFSET $3`,
      [search, limit, offset],
    );
    return rows;
  }

  async movieCount(filter: WatchFilter = "all", search = ""): Promise<number> {
    const { rows } = await this.db.query<{ count: number }>(
      `SELECT COUNT(*)::int AS count FROM (
         SELECT m.id FROM media m
         LEFT JOIN plays p ON p.target_kind = 'movie' AND p.target_id = m.id
         WHERE m.kind = 'movie' AND ($1 = '' OR m.title ILIKE '%' || $1 || '%')
         GROUP BY m.id ${movieHaving(filter)}
       ) matched`,
      [search],
    );
    return rows[0]?.count ?? 0;
  }

  async movie(id: number): Promise<MovieView | null> {
    const { rows } = await this.db.query<MovieView>(`${MOVIE_SELECT} WHERE m.kind = 'movie' AND m.id = $1 GROUP BY m.id`, [id]);
    return rows[0] ?? null;
  }

  async shows(search = "", sort: SortOrder = "recent"): Promise<ShowView[]> {
    const { rows } = await this.db.query<ShowView>(
      `${SHOW_SELECT}
       WHERE s.kind = 'show' AND ($1 = '' OR s.title ILIKE '%' || $1 || '%')
       GROUP BY s.id ORDER BY ${SHOW_ORDER[sort]}`,
      [search],
    );
    return rows;
  }

  async show(id: number): Promise<ShowView | null> {
    const { rows } = await this.db.query<ShowView>(`${SHOW_SELECT} WHERE s.kind = 'show' AND s.id = $1 GROUP BY s.id`, [id]);
    return rows[0] ?? null;
  }

  async episodes(showId: number): Promise<EpisodeView[]> {
    const { rows } = await this.db.query<EpisodeView>(
      `SELECT e.*, COUNT(p.id)::int AS play_count, MAX(p.watched_at) AS last_watched_at,
         pr.position_ms AS position_ms, pr.state AS progress_state
       FROM episodes e
       LEFT JOIN plays p ON p.target_kind = 'episode' AND p.target_id = e.id
       LEFT JOIN progress pr ON pr.target_kind = 'episode' AND pr.target_id = e.id
       WHERE e.show_id = $1
       GROUP BY e.id, pr.position_ms, pr.state ORDER BY e.season, e.number`,
      [showId],
    );
    return rows;
  }

  async history(limit = 50, offset = 0): Promise<HistoryEntry[]> {
    const { rows } = await this.db.query<HistoryEntry>(
      `${HISTORY_SELECT} ORDER BY p.watched_at DESC, p.id DESC LIMIT $1 OFFSET $2`,
      [limit, offset],
    );
    return rows;
  }

  async historyCount(): Promise<number> {
    const { rows } = await this.db.query<{ count: number }>("SELECT COUNT(*)::int AS count FROM plays");
    return rows[0]?.count ?? 0;
  }

  async inProgress(): Promise<ProgressView[]> {
    const { rows } = await this.db.query<ProgressView>(
      `SELECT pr.*,
         CASE pr.target_kind WHEN 'movie' THEN m.title ELSE e.title END AS title,
         CASE pr.target_kind WHEN 'movie' THEN NULL ELSE s.id END AS show_id,
         CASE pr.target_kind WHEN 'movie' THEN NULL ELSE s.title END AS show_title,
         e.season AS season, e.number AS number,
         CASE pr.target_kind WHEN 'movie' THEN m.poster_path ELSE s.poster_path END AS poster_path
       FROM progress pr
       LEFT JOIN media m ON pr.target_kind = 'movie' AND m.id = pr.target_id
       LEFT JOIN episodes e ON pr.target_kind = 'episode' AND e.id = pr.target_id
       LEFT JOIN media s ON s.id = e.show_id
       ORDER BY pr.updated_at DESC`,
    );
    return rows;
  }

  async watchlist(): Promise<WatchlistItem[]> {
    const { rows } = await this.db.query<WatchlistItem>(
      `SELECT w.target_kind AS kind, m.id, m.title, m.year, m.poster_path, w.listed_at, w.rank,
         CASE w.target_kind
           WHEN 'movie' THEN (SELECT COUNT(*) FROM plays p WHERE p.target_kind = 'movie' AND p.target_id = m.id)
           ELSE (SELECT COUNT(DISTINCT p.target_id) FROM plays p JOIN episodes e ON e.id = p.target_id
                 WHERE p.target_kind = 'episode' AND e.show_id = m.id)
         END::int AS watched_count
       FROM watchlist w JOIN media m ON m.id = w.target_id
       ORDER BY w.rank NULLS LAST, w.listed_at`,
    );
    return rows;
  }

  async isOnWatchlist(kind: "movie" | "show", id: number): Promise<boolean> {
    const { rows } = await this.db.query("SELECT 1 FROM watchlist WHERE target_kind = $1 AND target_id = $2", [kind, id]);
    return rows.length > 0;
  }

  /** Local ids of media rows that carry one of the given TMDB ids. */
  async localByTmdb(kind: "movie" | "show", tmdbIds: number[]): Promise<Map<number, number>> {
    const result = new Map<number, number>();
    if (tmdbIds.length === 0) return result;
    const { rows } = await this.db.query<{ id: number; tmdb_id: number }>(
      "SELECT id, tmdb_id FROM media WHERE kind = $1 AND tmdb_id = ANY($2::bigint[])",
      [kind, tmdbIds],
    );
    for (const row of rows) result.set(row.tmdb_id, row.id);
    return result;
  }

  async recentWebhooks(limit = 50): Promise<WebhookLogEntry[]> {
    const { rows } = await this.db.query<WebhookLogEntry>(
      `SELECT id, received_at, event, account, player, media_type, title, outcome
       FROM webhook_events ORDER BY id DESC LIMIT $1`,
      [limit],
    );
    return rows;
  }
}
