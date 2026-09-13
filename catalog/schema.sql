-- TMDB catalog schema for PostgreSQL.
-- Column names follow the benito-tv SQLite catalog so that `npm run catalog:import`
-- can copy rows without a mapping step. Watchkeep only reads these tables.

CREATE TABLE IF NOT EXISTS genre (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  name_en TEXT,
  name_pt TEXT
);

CREATE TABLE IF NOT EXISTS collection (
  id INTEGER PRIMARY KEY,
  name_en TEXT,
  name_pt TEXT,
  poster_path TEXT,
  backdrop_path TEXT,
  raw_json TEXT
);

CREATE TABLE IF NOT EXISTS tmdb_movie (
  id INTEGER PRIMARY KEY,
  imdb_id TEXT,
  title_en TEXT,
  title_pt TEXT,
  original_title TEXT,
  original_language TEXT,
  overview_en TEXT,
  overview_pt TEXT,
  tagline_en TEXT,
  tagline_pt TEXT,
  release_date TEXT,
  runtime INTEGER,
  status TEXT,
  adult BOOLEAN,
  budget BIGINT,
  revenue BIGINT,
  popularity DOUBLE PRECISION,
  vote_average DOUBLE PRECISION,
  vote_count INTEGER,
  homepage TEXT,
  poster_path_en TEXT,
  poster_path_pt TEXT,
  backdrop_path TEXT,
  certification_br TEXT,
  certification_us TEXT,
  collection_id INTEGER, -- no foreign key: the source catalog can reference collections it never fetched
  origin_countries_json TEXT,
  spoken_languages_json TEXT,
  production_companies_json TEXT,
  production_countries_json TEXT,
  keywords_json TEXT,
  videos_json TEXT,
  alternative_titles_json TEXT,
  images_json TEXT,
  external_ids_json TEXT,
  fetched_at BIGINT NOT NULL,
  raw_json TEXT NOT NULL,
  weighted_rating DOUBLE PRECISION GENERATED ALWAYS AS (
    (COALESCE(vote_count, 0) * 1.0 / (COALESCE(vote_count, 0) + 2000.0)) * COALESCE(vote_average, 0)
    + (2000.0 / (COALESCE(vote_count, 0) + 2000.0)) * 6.5
  ) STORED
);
CREATE INDEX IF NOT EXISTS tmdb_movie_imdb ON tmdb_movie (imdb_id);
CREATE INDEX IF NOT EXISTS tmdb_movie_title_en ON tmdb_movie (lower(title_en));
CREATE INDEX IF NOT EXISTS tmdb_movie_title_pt ON tmdb_movie (lower(title_pt));

CREATE TABLE IF NOT EXISTS tmdb_movie_genre (
  movie_id INTEGER NOT NULL REFERENCES tmdb_movie (id) ON DELETE CASCADE,
  genre_id INTEGER NOT NULL REFERENCES genre (id) ON DELETE CASCADE,
  PRIMARY KEY (movie_id, genre_id)
);

CREATE TABLE IF NOT EXISTS tmdb_show (
  id INTEGER PRIMARY KEY,
  imdb_id TEXT,
  tvdb_id INTEGER,
  name_en TEXT,
  name_pt TEXT,
  original_name TEXT,
  original_language TEXT,
  overview_en TEXT,
  overview_pt TEXT,
  tagline_en TEXT,
  tagline_pt TEXT,
  first_air_date TEXT,
  last_air_date TEXT,
  in_production BOOLEAN,
  status TEXT,
  type TEXT,
  number_of_seasons INTEGER,
  number_of_episodes INTEGER,
  episode_run_time_json TEXT,
  popularity DOUBLE PRECISION,
  vote_average DOUBLE PRECISION,
  vote_count INTEGER,
  poster_path_en TEXT,
  poster_path_pt TEXT,
  backdrop_path TEXT,
  content_rating_br TEXT,
  content_rating_us TEXT,
  created_by_json TEXT,
  networks_json TEXT,
  keywords_json TEXT,
  videos_json TEXT,
  alternative_titles_json TEXT,
  images_json TEXT,
  external_ids_json TEXT,
  fetched_at BIGINT NOT NULL,
  raw_json TEXT NOT NULL,
  weighted_rating DOUBLE PRECISION GENERATED ALWAYS AS (
    (COALESCE(vote_count, 0) * 1.0 / (COALESCE(vote_count, 0) + 600.0)) * COALESCE(vote_average, 0)
    + (600.0 / (COALESCE(vote_count, 0) + 600.0)) * 7.4
  ) STORED
);
CREATE INDEX IF NOT EXISTS tmdb_show_imdb ON tmdb_show (imdb_id);
CREATE INDEX IF NOT EXISTS tmdb_show_tvdb ON tmdb_show (tvdb_id);
CREATE INDEX IF NOT EXISTS tmdb_show_name_en ON tmdb_show (lower(name_en));
CREATE INDEX IF NOT EXISTS tmdb_show_name_pt ON tmdb_show (lower(name_pt));

CREATE TABLE IF NOT EXISTS tmdb_show_genre (
  show_id INTEGER NOT NULL REFERENCES tmdb_show (id) ON DELETE CASCADE,
  genre_id INTEGER NOT NULL REFERENCES genre (id) ON DELETE CASCADE,
  PRIMARY KEY (show_id, genre_id)
);

CREATE TABLE IF NOT EXISTS tmdb_season (
  id INTEGER PRIMARY KEY,
  show_id INTEGER NOT NULL REFERENCES tmdb_show (id) ON DELETE CASCADE,
  season_number INTEGER NOT NULL,
  name TEXT,
  overview TEXT,
  air_date TEXT,
  poster_path TEXT,
  vote_average DOUBLE PRECISION,
  fetched_at BIGINT NOT NULL,
  raw_json TEXT,
  UNIQUE (show_id, season_number)
);

CREATE TABLE IF NOT EXISTS tmdb_episode (
  id INTEGER PRIMARY KEY,
  season_id INTEGER NOT NULL REFERENCES tmdb_season (id) ON DELETE CASCADE,
  episode_number INTEGER NOT NULL,
  name TEXT,
  overview TEXT,
  air_date TEXT,
  runtime INTEGER,
  still_path TEXT,
  vote_average DOUBLE PRECISION,
  vote_count INTEGER,
  guest_stars_json TEXT,
  crew_json TEXT,
  UNIQUE (season_id, episode_number)
);
