import pg from "pg";

const { Pool } = pg;
export type Pool = pg.Pool;
export type PoolClient = pg.PoolClient;

/** Anything that can run a parameterised query: a pool or a checked-out client. */
export interface Queryable {
  query<R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, values?: unknown[]): Promise<pg.QueryResult<R>>;
}

export type MediaKind = "movie" | "show";
export type TargetKind = "movie" | "episode";
export type PlayState = "playing" | "paused" | "stopped";

export interface MediaRow {
  id: number;
  kind: MediaKind;
  title: string;
  year: number | null;
  plex_guid: string | null;
  imdb_id: string | null;
  tmdb_id: number | null;
  tvdb_id: string | null;
  duration_ms: number | null;
  summary: string | null;
  poster_path: string | null;
  created_at: string;
  updated_at: string;
}

export interface EpisodeRow {
  id: number;
  show_id: number;
  season: number;
  number: number;
  title: string | null;
  plex_guid: string | null;
  imdb_id: string | null;
  tmdb_id: number | null;
  tvdb_id: string | null;
  duration_ms: number | null;
  aired_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface PlayRow {
  id: number;
  target_kind: TargetKind;
  target_id: number;
  watched_at: string;
  source: string;
  account: string | null;
  player: string | null;
}

export interface ProgressRow {
  target_kind: TargetKind;
  target_id: number;
  position_ms: number;
  duration_ms: number | null;
  state: PlayState;
  account: string | null;
  player: string | null;
  updated_at: string;
}

/**
 * Schema migrations. Add a new entry at the end. Never edit an applied entry.
 * Timestamps are stored as ISO-8601 text so that the API returns them unchanged.
 */
export const MIGRATIONS: string[] = [
  `
  CREATE TABLE media (
    id BIGSERIAL PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('movie', 'show')),
    title TEXT NOT NULL,
    year INTEGER,
    plex_guid TEXT,
    imdb_id TEXT,
    tmdb_id BIGINT,
    tvdb_id TEXT,
    duration_ms BIGINT,
    summary TEXT,
    poster_path TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
  );
  CREATE UNIQUE INDEX media_plex_guid ON media (plex_guid) WHERE plex_guid IS NOT NULL;
  CREATE UNIQUE INDEX media_kind_imdb ON media (kind, imdb_id) WHERE imdb_id IS NOT NULL;
  CREATE UNIQUE INDEX media_kind_tmdb ON media (kind, tmdb_id) WHERE tmdb_id IS NOT NULL;
  CREATE UNIQUE INDEX media_kind_tvdb ON media (kind, tvdb_id) WHERE tvdb_id IS NOT NULL;
  CREATE INDEX media_kind_title ON media (kind, lower(title));

  CREATE TABLE episodes (
    id BIGSERIAL PRIMARY KEY,
    show_id BIGINT NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    season INTEGER NOT NULL,
    number INTEGER NOT NULL,
    title TEXT,
    plex_guid TEXT,
    imdb_id TEXT,
    tmdb_id BIGINT,
    tvdb_id TEXT,
    duration_ms BIGINT,
    aired_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (show_id, season, number)
  );
  CREATE UNIQUE INDEX episodes_plex_guid ON episodes (plex_guid) WHERE plex_guid IS NOT NULL;
  CREATE UNIQUE INDEX episodes_tmdb ON episodes (tmdb_id) WHERE tmdb_id IS NOT NULL;

  CREATE TABLE plays (
    id BIGSERIAL PRIMARY KEY,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('movie', 'episode')),
    target_id BIGINT NOT NULL,
    watched_at TEXT NOT NULL,
    source TEXT NOT NULL,
    account TEXT,
    player TEXT
  );
  CREATE INDEX plays_target ON plays (target_kind, target_id, watched_at);
  CREATE INDEX plays_watched_at ON plays (watched_at);

  CREATE TABLE progress (
    target_kind TEXT NOT NULL CHECK (target_kind IN ('movie', 'episode')),
    target_id BIGINT NOT NULL,
    position_ms BIGINT NOT NULL,
    duration_ms BIGINT,
    state TEXT NOT NULL CHECK (state IN ('playing', 'paused', 'stopped')),
    account TEXT,
    player TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (target_kind, target_id)
  );

  CREATE TABLE ratings (
    target_kind TEXT NOT NULL CHECK (target_kind IN ('movie', 'show', 'episode')),
    target_id BIGINT NOT NULL,
    rating DOUBLE PRECISION NOT NULL,
    rated_at TEXT NOT NULL,
    PRIMARY KEY (target_kind, target_id)
  );

  CREATE TABLE webhook_events (
    id BIGSERIAL PRIMARY KEY,
    received_at TEXT NOT NULL,
    event TEXT NOT NULL,
    account TEXT,
    player TEXT,
    media_type TEXT,
    title TEXT,
    outcome TEXT NOT NULL,
    payload TEXT NOT NULL
  );
  CREATE INDEX webhook_events_received_at ON webhook_events (received_at);
  `,
];

export function createPool(connectionString: string, max = 5): Pool {
  const pool = new Pool({ connectionString, max });
  // BIGINT and BIGSERIAL come back as strings by default. Every id fits in a JS number.
  pg.types.setTypeParser(20, (value: string) => Number(value));
  // COUNT(*) and SUM() return NUMERIC as text.
  pg.types.setTypeParser(1700, (value: string) => Number(value));
  return pool;
}

export async function migrate(db: Queryable): Promise<void> {
  await db.query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)");
  await db.query("SELECT pg_advisory_lock(727001)").catch(() => undefined);
  try {
    const { rows } = await db.query<{ version: number | null }>("SELECT MAX(version) AS version FROM schema_migrations");
    const current = Number(rows[0]?.version ?? 0);
    for (let index = current; index < MIGRATIONS.length; index += 1) {
      await db.query("BEGIN");
      try {
        await db.query(MIGRATIONS[index]!);
        await db.query("INSERT INTO schema_migrations (version, applied_at) VALUES ($1, $2)", [
          index + 1,
          new Date().toISOString(),
        ]);
        await db.query("COMMIT");
      } catch (error) {
        await db.query("ROLLBACK");
        throw error;
      }
    }
  } finally {
    await db.query("SELECT pg_advisory_unlock(727001)").catch(() => undefined);
  }
}

/** Open a pool and apply pending migrations on one dedicated connection. */
export async function openDatabase(connectionString: string): Promise<Pool> {
  const pool = createPool(connectionString);
  const client = await pool.connect();
  try {
    await migrate(client);
  } finally {
    client.release();
  }
  return pool;
}

export async function transaction<T>(pool: Pool, work: (client: PoolClient) => Promise<T>): Promise<T> {
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const result = await work(client);
    await client.query("COMMIT");
    return result;
  } catch (error) {
    await client.query("ROLLBACK").catch(() => undefined);
    throw error;
  } finally {
    client.release();
  }
}
