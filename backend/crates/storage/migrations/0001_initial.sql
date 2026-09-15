-- Timestamps are ISO-8601 text, so that the API returns them unchanged.
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
