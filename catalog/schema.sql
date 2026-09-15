-- The contract of the central TMDB database. One tool mirrors TMDB into these
-- tables, and every app that needs TMDB data reads them with a read-only role,
-- so nobody copies the data. Watchkeep only reads tmdb_movie, tmdb_show,
-- tmdb_season, and tmdb_episode. This file creates the same tables for the
-- tests and for a setup without a mirror tool.

CREATE TABLE collection (
    id bigint NOT NULL,
    name_en text COLLATE pg_catalog."C",
    name_pt text COLLATE pg_catalog."C",
    poster_path text COLLATE pg_catalog."C",
    backdrop_path text COLLATE pg_catalog."C",
    raw_json text COLLATE pg_catalog."C"
);
CREATE TABLE genre (
    id bigint NOT NULL,
    kind text NOT NULL COLLATE pg_catalog."C",
    name_en text COLLATE pg_catalog."C",
    name_pt text COLLATE pg_catalog."C",
    CONSTRAINT chk_genre_kind CHECK ((kind = ANY (ARRAY['movie'::text, 'tv'::text])))
);
CREATE TABLE tmdb_episode (
    id bigint NOT NULL,
    season_id bigint NOT NULL,
    episode_number bigint NOT NULL,
    name text COLLATE pg_catalog."C",
    overview text COLLATE pg_catalog."C",
    air_date text COLLATE pg_catalog."C",
    runtime bigint,
    still_path text COLLATE pg_catalog."C",
    vote_average double precision,
    vote_count bigint,
    guest_stars_json text COLLATE pg_catalog."C",
    crew_json text COLLATE pg_catalog."C"
);
CREATE TABLE tmdb_movie (
    id bigint NOT NULL,
    imdb_id text COLLATE pg_catalog."C",
    title_en text COLLATE pg_catalog."C",
    title_pt text COLLATE pg_catalog."C",
    original_title text COLLATE pg_catalog."C",
    original_language text COLLATE pg_catalog."C",
    overview_en text COLLATE pg_catalog."C",
    overview_pt text COLLATE pg_catalog."C",
    tagline_en text COLLATE pg_catalog."C",
    tagline_pt text COLLATE pg_catalog."C",
    release_date text COLLATE pg_catalog."C",
    runtime bigint,
    status text COLLATE pg_catalog."C",
    adult boolean,
    budget bigint,
    revenue bigint,
    popularity double precision,
    vote_average double precision,
    vote_count bigint,
    homepage text COLLATE pg_catalog."C",
    poster_path_en text COLLATE pg_catalog."C",
    poster_path_pt text COLLATE pg_catalog."C",
    backdrop_path text COLLATE pg_catalog."C",
    certification_br text COLLATE pg_catalog."C",
    certification_us text COLLATE pg_catalog."C",
    collection_id bigint,
    origin_countries_json text COLLATE pg_catalog."C",
    spoken_languages_json text COLLATE pg_catalog."C",
    production_companies_json text COLLATE pg_catalog."C",
    production_countries_json text COLLATE pg_catalog."C",
    keywords_json text COLLATE pg_catalog."C",
    videos_json text COLLATE pg_catalog."C",
    alternative_titles_json text COLLATE pg_catalog."C",
    images_json text COLLATE pg_catalog."C",
    external_ids_json text COLLATE pg_catalog."C",
    fetched_at bigint NOT NULL,
    raw_json text NOT NULL COLLATE pg_catalog."C",
    weighted_rating double precision GENERATED ALWAYS AS (((((((COALESCE(vote_count, (0)::bigint))::numeric * 1.0) / ((COALESCE(vote_count, (0)::bigint))::numeric + 2000.0)))::double precision * COALESCE(vote_average, (0)::double precision)) + (((2000.0 / ((COALESCE(vote_count, (0)::bigint))::numeric + 2000.0)) * 6.5))::double precision)) STORED
);
CREATE TABLE tmdb_movie_genre (
    movie_id bigint NOT NULL,
    genre_id bigint NOT NULL
);
CREATE TABLE tmdb_season (
    id bigint NOT NULL,
    show_id bigint NOT NULL,
    season_number bigint NOT NULL,
    name text COLLATE pg_catalog."C",
    overview text COLLATE pg_catalog."C",
    air_date text COLLATE pg_catalog."C",
    poster_path text COLLATE pg_catalog."C",
    vote_average double precision,
    fetched_at bigint NOT NULL,
    raw_json text COLLATE pg_catalog."C"
);
CREATE TABLE tmdb_show (
    id bigint NOT NULL,
    imdb_id text COLLATE pg_catalog."C",
    tvdb_id bigint,
    name_en text COLLATE pg_catalog."C",
    name_pt text COLLATE pg_catalog."C",
    original_name text COLLATE pg_catalog."C",
    original_language text COLLATE pg_catalog."C",
    overview_en text COLLATE pg_catalog."C",
    overview_pt text COLLATE pg_catalog."C",
    tagline_en text COLLATE pg_catalog."C",
    tagline_pt text COLLATE pg_catalog."C",
    first_air_date text COLLATE pg_catalog."C",
    last_air_date text COLLATE pg_catalog."C",
    in_production boolean,
    status text COLLATE pg_catalog."C",
    type text COLLATE pg_catalog."C",
    number_of_seasons bigint,
    number_of_episodes bigint,
    episode_run_time_json text COLLATE pg_catalog."C",
    popularity double precision,
    vote_average double precision,
    vote_count bigint,
    poster_path_en text COLLATE pg_catalog."C",
    poster_path_pt text COLLATE pg_catalog."C",
    backdrop_path text COLLATE pg_catalog."C",
    content_rating_br text COLLATE pg_catalog."C",
    content_rating_us text COLLATE pg_catalog."C",
    created_by_json text COLLATE pg_catalog."C",
    networks_json text COLLATE pg_catalog."C",
    keywords_json text COLLATE pg_catalog."C",
    videos_json text COLLATE pg_catalog."C",
    alternative_titles_json text COLLATE pg_catalog."C",
    images_json text COLLATE pg_catalog."C",
    external_ids_json text COLLATE pg_catalog."C",
    fetched_at bigint NOT NULL,
    raw_json text NOT NULL COLLATE pg_catalog."C",
    weighted_rating double precision GENERATED ALWAYS AS (((((((COALESCE(vote_count, (0)::bigint))::numeric * 1.0) / ((COALESCE(vote_count, (0)::bigint))::numeric + 600.0)))::double precision * COALESCE(vote_average, (0)::double precision)) + (((600.0 / ((COALESCE(vote_count, (0)::bigint))::numeric + 600.0)) * 7.4))::double precision)) STORED
);
CREATE TABLE tmdb_show_genre (
    show_id bigint NOT NULL,
    genre_id bigint NOT NULL
);
ALTER TABLE ONLY collection
    ADD CONSTRAINT collection_pkey PRIMARY KEY (id);
ALTER TABLE ONLY genre
    ADD CONSTRAINT genre_pkey PRIMARY KEY (id);
ALTER TABLE ONLY tmdb_episode
    ADD CONSTRAINT tmdb_episode_pkey PRIMARY KEY (id);
ALTER TABLE ONLY tmdb_episode
    ADD CONSTRAINT tmdb_episode_season_id_episode_number_key UNIQUE (season_id, episode_number);
ALTER TABLE ONLY tmdb_movie_genre
    ADD CONSTRAINT tmdb_movie_genre_pkey PRIMARY KEY (movie_id, genre_id);
ALTER TABLE ONLY tmdb_movie
    ADD CONSTRAINT tmdb_movie_pkey PRIMARY KEY (id);
ALTER TABLE ONLY tmdb_season
    ADD CONSTRAINT tmdb_season_pkey PRIMARY KEY (id);
ALTER TABLE ONLY tmdb_season
    ADD CONSTRAINT tmdb_season_show_id_season_number_key UNIQUE (show_id, season_number);
ALTER TABLE ONLY tmdb_show_genre
    ADD CONSTRAINT tmdb_show_genre_pkey PRIMARY KEY (show_id, genre_id);
ALTER TABLE ONLY tmdb_show
    ADD CONSTRAINT tmdb_show_pkey PRIMARY KEY (id);
CREATE INDEX idx_tmdb_movie_collection ON tmdb_movie USING btree (collection_id);
CREATE INDEX idx_tmdb_movie_genre_genre ON tmdb_movie_genre USING btree (genre_id);
CREATE INDEX idx_tmdb_movie_popularity ON tmdb_movie USING btree (popularity DESC NULLS LAST, id DESC);
CREATE INDEX idx_tmdb_movie_weighted ON tmdb_movie USING btree (weighted_rating DESC NULLS LAST, id DESC);
CREATE INDEX idx_tmdb_show_genre_genre ON tmdb_show_genre USING btree (genre_id);
CREATE INDEX idx_tmdb_show_popularity ON tmdb_show USING btree (popularity DESC NULLS LAST, id DESC);
CREATE INDEX idx_tmdb_show_weighted ON tmdb_show USING btree (weighted_rating DESC NULLS LAST, id DESC);
ALTER TABLE ONLY tmdb_episode
    ADD CONSTRAINT tmdb_episode_season_id_fkey FOREIGN KEY (season_id) REFERENCES tmdb_season(id) ON DELETE CASCADE;
ALTER TABLE ONLY tmdb_movie
    ADD CONSTRAINT tmdb_movie_collection_id_fkey FOREIGN KEY (collection_id) REFERENCES collection(id);
ALTER TABLE ONLY tmdb_movie_genre
    ADD CONSTRAINT tmdb_movie_genre_genre_id_fkey FOREIGN KEY (genre_id) REFERENCES genre(id) ON DELETE CASCADE;
ALTER TABLE ONLY tmdb_movie_genre
    ADD CONSTRAINT tmdb_movie_genre_movie_id_fkey FOREIGN KEY (movie_id) REFERENCES tmdb_movie(id) ON DELETE CASCADE;
ALTER TABLE ONLY tmdb_season
    ADD CONSTRAINT tmdb_season_show_id_fkey FOREIGN KEY (show_id) REFERENCES tmdb_show(id) ON DELETE CASCADE;
ALTER TABLE ONLY tmdb_show_genre
    ADD CONSTRAINT tmdb_show_genre_genre_id_fkey FOREIGN KEY (genre_id) REFERENCES genre(id) ON DELETE CASCADE;
ALTER TABLE ONLY tmdb_show_genre
    ADD CONSTRAINT tmdb_show_genre_show_id_fkey FOREIGN KEY (show_id) REFERENCES tmdb_show(id) ON DELETE CASCADE;
