-- The Watchkeep schema. Ids are UUID v7 from the server: the pg_uuidv7
-- extension (PostgreSQL 17 or newer), so they sort by creation time. Timestamps are timestamptz and air dates are
-- date. Durations and positions are milliseconds. The words of the text columns
-- are the values of the text enums in model.rs.

CREATE EXTENSION IF NOT EXISTS pg_uuidv7;

CREATE TABLE media (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v7(),
  kind text NOT NULL CHECK (kind IN ('movie', 'show')),
  title text NOT NULL,
  year integer,
  plex_guid text,
  imdb_id text,
  tmdb_id bigint,
  tvdb_id text,
  duration_ms bigint,
  summary text,
  poster_path text,
  -- Hidden shows do not appear as unwatched.
  hidden_at timestamptz,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL
);
CREATE UNIQUE INDEX media_plex_guid ON media (plex_guid) WHERE plex_guid IS NOT NULL;
CREATE UNIQUE INDEX media_kind_imdb ON media (kind, imdb_id) WHERE imdb_id IS NOT NULL;
CREATE UNIQUE INDEX media_kind_tmdb ON media (kind, tmdb_id) WHERE tmdb_id IS NOT NULL;
CREATE UNIQUE INDEX media_kind_tvdb ON media (kind, tvdb_id) WHERE tvdb_id IS NOT NULL;
CREATE INDEX media_kind_title ON media (kind, lower(title));

CREATE TABLE episodes (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v7(),
  show_id uuid NOT NULL REFERENCES media (id) ON DELETE CASCADE,
  season integer NOT NULL,
  number integer NOT NULL,
  title text,
  plex_guid text,
  imdb_id text,
  tmdb_id bigint,
  tvdb_id text,
  duration_ms bigint,
  aired_at date,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  UNIQUE (show_id, season, number)
);
CREATE UNIQUE INDEX episodes_plex_guid ON episodes (plex_guid) WHERE plex_guid IS NOT NULL;
CREATE UNIQUE INDEX episodes_tmdb ON episodes (tmdb_id) WHERE tmdb_id IS NOT NULL;

CREATE TABLE plays (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v7(),
  target_kind text NOT NULL CHECK (target_kind IN ('movie', 'episode')),
  target_id uuid NOT NULL,
  watched_at timestamptz NOT NULL,
  source text NOT NULL,
  account text,
  player text,
  -- The id of the source record of an import, so that a second import adds no duplicate.
  external_id text
);
CREATE INDEX plays_target ON plays (target_kind, target_id, watched_at);
CREATE INDEX plays_watched_at ON plays (watched_at);
CREATE UNIQUE INDEX plays_source_external ON plays (source, external_id) WHERE external_id IS NOT NULL;

CREATE TABLE progress (
  target_kind text NOT NULL CHECK (target_kind IN ('movie', 'episode')),
  target_id uuid NOT NULL,
  position_ms bigint NOT NULL,
  duration_ms bigint,
  state text NOT NULL CHECK (state IN ('playing', 'paused', 'stopped')),
  account text,
  player text,
  updated_at timestamptz NOT NULL,
  PRIMARY KEY (target_kind, target_id)
);

CREATE TABLE ratings (
  target_kind text NOT NULL CHECK (target_kind IN ('movie', 'show', 'episode')),
  target_id uuid NOT NULL,
  rating double precision NOT NULL,
  rated_at timestamptz NOT NULL,
  PRIMARY KEY (target_kind, target_id)
);

CREATE TABLE watchlist (
  target_kind text NOT NULL CHECK (target_kind IN ('movie', 'show')),
  target_id uuid NOT NULL,
  listed_at timestamptz NOT NULL,
  rank integer,
  PRIMARY KEY (target_kind, target_id)
);

CREATE TABLE webhook_events (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v7(),
  received_at timestamptz NOT NULL,
  event text NOT NULL,
  account text,
  player text,
  media_type text,
  title text,
  outcome text NOT NULL,
  payload text NOT NULL
);
CREATE INDEX webhook_events_received_at ON webhook_events (received_at);
