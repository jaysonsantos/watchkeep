//! Read-only access to the TMDB catalog database. The catalog lives in a
//! separate Postgres database on the same instance, so Watchkeep talks to it
//! through a second pool and never joins across databases.
//!
//! Every query is a `sqlx` macro, checked against the catalog schema at compile
//! time. The macros read `WATCHKEEP_CATALOG_DATABASE_URL` (see `sqlx.toml`) or
//! the offline data in `.sqlx/`.

pub mod schema;

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use eyre::Result;
use serde::Serialize;
use sqlx::PgPool;
use thiserror::Error;

/// The text format of the dates in the catalog, for example `2022-02-17`.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// The title language to prefer. The catalog stores English and Portuguese text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogLanguage {
    En,
    Pt,
}

impl CatalogLanguage {
    pub const ALL: &'static [CatalogLanguage] = &[Self::En, Self::Pt];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Pt => "pt",
        }
    }
}

impl fmt::Display for CatalogLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A language word that the catalog does not have.
#[derive(Debug, Error)]
#[error("invalid catalog language: {0} (use en or pt)")]
pub struct InvalidLanguage(pub String);

impl FromStr for CatalogLanguage {
    type Err = InvalidLanguage;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|language| language.as_str() == value)
            .ok_or_else(|| InvalidLanguage(value.to_owned()))
    }
}

/// How many votes a candidate needs, so that recommendations stay off the long tail.
pub const MIN_CANDIDATE_VOTES: i64 = 100;

/// How many of the best rated catalog items a candidate query scores. The
/// `weighted_rating` index orders the pool, so the query reads no more rows.
pub const CANDIDATE_POOL: i64 = 5_000;

/// The title of an item that has no name in any language.
const UNTITLED_PREFIX: &str = "TMDB";

/// How many years a movie release date can differ from the year that Plex reports.
const RELEASE_YEAR_TOLERANCE: i32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CatalogMovie {
    pub tmdb_id: i64,
    pub imdb_id: Option<String>,
    pub title: String,
    pub year: Option<i32>,
    pub runtime_min: Option<i32>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CatalogShow {
    pub tmdb_id: i64,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<String>,
    pub title: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
    pub number_of_seasons: Option<i32>,
    pub number_of_episodes: Option<i32>,
}

/// The `kind` words of the `genre` table. TMDB calls the genres of a show `tv`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenreKind {
    Movie,
    Tv,
}

impl GenreKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Tv => "tv",
        }
    }
}

/// What one watched item says about taste: its genres, its language, and the
/// collection or the production state that makes a follow-up possible.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TasteFacts {
    pub genres: Vec<i64>,
    pub language: Option<String>,
    pub collection_id: Option<i64>,
    pub in_production: bool,
}

/// A genre affinity vector, as the two arrays that a candidate query takes.
/// The weights are a unit vector, so that a dot product is a cosine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenreWeights {
    pub ids: Vec<i64>,
    pub weights: Vec<f64>,
}

impl GenreWeights {
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// A catalog item that matches the genre affinity, with the parts of the score
/// that only the catalog knows.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate<T> {
    pub item: T,
    pub genres: Vec<i64>,
    /// The cosine of the genre vector of the item and of the profile, from 0.0 to 1.0.
    pub affinity: f64,
    /// The Bayesian TMDB rating, on the scale of `vote_average`.
    pub quality: f64,
    pub language: Option<String>,
}

/// A movie of a collection that the library already holds a part of.
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionMovie {
    pub movie: CatalogMovie,
    pub collection_id: i64,
    pub collection_name: Option<String>,
    pub quality: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEpisode {
    pub tmdb_id: i64,
    pub season: i32,
    pub number: i32,
    pub title: Option<String>,
    pub air_date: Option<NaiveDate>,
    pub runtime_min: Option<i32>,
    pub still_path: Option<String>,
}

/// The columns of `tmdb_movie` that Watchkeep reads.
struct MovieRow {
    id: i64,
    imdb_id: Option<String>,
    title_en: Option<String>,
    title_pt: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    runtime: Option<i32>,
    poster_path_en: Option<String>,
    poster_path_pt: Option<String>,
    overview_en: Option<String>,
    overview_pt: Option<String>,
}

/// The columns of `tmdb_show` that Watchkeep reads.
struct ShowRow {
    id: i64,
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
    name_en: Option<String>,
    name_pt: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
    poster_path_en: Option<String>,
    poster_path_pt: Option<String>,
    overview_en: Option<String>,
    overview_pt: Option<String>,
    number_of_seasons: Option<i32>,
    number_of_episodes: Option<i32>,
}

/// An episode row joined with its season number.
struct EpisodeRow {
    id: i64,
    season_number: i32,
    episode_number: i32,
    name: Option<String>,
    air_date: Option<String>,
    runtime: Option<i32>,
    still_path: Option<String>,
}

/// The catalog stores dates as text. Read the year, or `None` for text that is not a date.
fn year_of(date: Option<&str>) -> Option<i32> {
    date?
        .get(0..4)?
        .parse::<i32>()
        .ok()
        .filter(|year| *year > 0)
}

/// The catalog stores dates as text. Read the date, or `None` for text that is not a date.
pub fn parse_date(date: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date?.get(0..10)?, DATE_FORMAT).ok()
}

fn text_list<S: AsRef<str>>(ids: &[S]) -> Vec<&str> {
    ids.iter().map(AsRef::as_ref).collect()
}

/// A year as the text that `substr(release_date, 1, 4)` produces, offset by `delta` years.
fn year_text(year: Option<i32>, delta: i32) -> String {
    year.map(|year| (year + delta).to_string())
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct Catalog {
    pool: PgPool,
    language: CatalogLanguage,
}

impl Catalog {
    pub fn new(pool: PgPool, language: CatalogLanguage) -> Self {
        Self { pool, language }
    }

    fn pick<T>(&self, en: Option<T>, pt: Option<T>) -> Option<T> {
        match self.language {
            CatalogLanguage::Pt => pt.or(en),
            CatalogLanguage::En => en.or(pt),
        }
    }

    fn movie(&self, row: MovieRow) -> CatalogMovie {
        CatalogMovie {
            tmdb_id: row.id,
            imdb_id: row.imdb_id,
            title: self
                .pick(row.title_en, row.title_pt)
                .or(row.original_title)
                .unwrap_or_else(|| format!("{UNTITLED_PREFIX} {}", row.id)),
            year: year_of(row.release_date.as_deref()),
            runtime_min: row.runtime,
            poster_path: self.pick(row.poster_path_en, row.poster_path_pt),
            overview: self.pick(row.overview_en, row.overview_pt),
        }
    }

    fn show(&self, row: ShowRow) -> CatalogShow {
        CatalogShow {
            tmdb_id: row.id,
            imdb_id: row.imdb_id,
            tvdb_id: row.tvdb_id.map(|id| id.to_string()),
            title: self
                .pick(row.name_en, row.name_pt)
                .or(row.original_name)
                .unwrap_or_else(|| format!("{UNTITLED_PREFIX} {}", row.id)),
            year: year_of(row.first_air_date.as_deref()),
            poster_path: self.pick(row.poster_path_en, row.poster_path_pt),
            overview: self.pick(row.overview_en, row.overview_pt),
            number_of_seasons: row.number_of_seasons,
            number_of_episodes: row.number_of_episodes,
        }
    }

    fn episode_of(row: EpisodeRow) -> CatalogEpisode {
        CatalogEpisode {
            tmdb_id: row.id,
            season: row.season_number,
            number: row.episode_number,
            title: row.name,
            air_date: parse_date(row.air_date.as_deref()),
            runtime_min: row.runtime,
            still_path: row.still_path,
        }
    }

    // region: movies

    pub async fn movie_by_tmdb_id(&self, id: i64) -> Result<Option<CatalogMovie>> {
        let row = sqlx::query_as!(
            MovieRow,
            "SELECT id, imdb_id, title_en, title_pt, original_title, release_date, runtime::int AS runtime, poster_path_en, poster_path_pt, overview_en, overview_pt
             FROM tmdb_movie WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.movie(row)))
    }

    pub async fn movie_by_imdb_id(&self, imdb_id: &str) -> Result<Option<CatalogMovie>> {
        let row = sqlx::query_as!(
            MovieRow,
            "SELECT id, imdb_id, title_en, title_pt, original_title, release_date, runtime::int AS runtime, poster_path_en, poster_path_pt, overview_en, overview_pt
             FROM tmdb_movie WHERE imdb_id = $1 LIMIT 1",
            imdb_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.movie(row)))
    }

    /// Exact title match in either language. The year narrows the match when given.
    pub async fn movie_by_title(
        &self,
        title: &str,
        year: Option<i32>,
    ) -> Result<Option<CatalogMovie>> {
        let row = sqlx::query_as!(
            MovieRow,
            "SELECT id, imdb_id, title_en, title_pt, original_title, release_date, runtime::int AS runtime, poster_path_en, poster_path_pt, overview_en, overview_pt
             FROM tmdb_movie
             WHERE (lower(title_en) = lower($1) OR lower(title_pt) = lower($1) OR lower(original_title) = lower($1))
               AND ($2::int IS NULL OR substr(release_date, 1, 4) IN ($3, $4, $5))
             ORDER BY (substr(release_date, 1, 4) = $3) DESC, vote_count DESC NULLS LAST LIMIT 1",
            title,
            year,
            year_text(year, 0),
            year_text(year, -RELEASE_YEAR_TOLERANCE),
            year_text(year, RELEASE_YEAR_TOLERANCE)
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.movie(row)))
    }

    pub async fn movies_by_tmdb_ids(&self, ids: &[i64]) -> Result<HashMap<i64, CatalogMovie>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as!(
            MovieRow,
            "SELECT id, imdb_id, title_en, title_pt, original_title, release_date, runtime::int AS runtime, poster_path_en, poster_path_pt, overview_en, overview_pt
             FROM tmdb_movie WHERE id = ANY($1)",
            ids
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.id, self.movie(row)))
            .collect())
    }

    /// Movies by IMDb id. The first row per id wins.
    pub async fn movies_by_imdb_ids<S: AsRef<str>>(
        &self,
        ids: &[S],
    ) -> Result<HashMap<String, CatalogMovie>> {
        let mut result = HashMap::new();
        if ids.is_empty() {
            return Ok(result);
        }
        let ids = text_list(ids);
        let rows = sqlx::query_as!(
            MovieRow,
            "SELECT id, imdb_id, title_en, title_pt, original_title, release_date, runtime::int AS runtime, poster_path_en, poster_path_pt, overview_en, overview_pt
             FROM tmdb_movie WHERE imdb_id = ANY($1)",
            &ids as &[&str]
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            if let Some(imdb_id) = row.imdb_id.clone().filter(|id| !id.is_empty()) {
                result.entry(imdb_id).or_insert_with(|| self.movie(row));
            }
        }
        Ok(result)
    }

    /// Title search for the add page. Matches any part of a title in either language.
    pub async fn search_movies(&self, query: &str, limit: i64) -> Result<Vec<CatalogMovie>> {
        let rows = sqlx::query_as!(
            MovieRow,
            "SELECT id, imdb_id, title_en, title_pt, original_title, release_date, runtime::int AS runtime, poster_path_en, poster_path_pt, overview_en, overview_pt
             FROM tmdb_movie
             WHERE title_en ILIKE '%' || $1 || '%' OR title_pt ILIKE '%' || $1 || '%' OR original_title ILIKE '%' || $1 || '%'
             ORDER BY (lower(title_en) = lower($1) OR lower(title_pt) = lower($1)) DESC, vote_count DESC NULLS LAST, release_date DESC NULLS LAST
             LIMIT $2",
            query,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| self.movie(row)).collect())
    }

    // endregion: movies

    // region: shows

    pub async fn show_by_tmdb_id(&self, id: i64) -> Result<Option<CatalogShow>> {
        let row = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.show(row)))
    }

    pub async fn shows_by_tmdb_ids(&self, ids: &[i64]) -> Result<HashMap<i64, CatalogShow>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show WHERE id = ANY($1)",
            ids
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.id, self.show(row)))
            .collect())
    }

    /// Shows by TVDB id. Ids that are not numbers match nothing. The first row per id wins.
    pub async fn shows_by_tvdb_ids<S: AsRef<str>>(
        &self,
        ids: &[S],
    ) -> Result<HashMap<String, CatalogShow>> {
        let mut result = HashMap::new();
        let numeric: Vec<i64> = ids
            .iter()
            .filter_map(|id| id.as_ref().trim().parse::<i64>().ok())
            .collect();
        if numeric.is_empty() {
            return Ok(result);
        }
        let rows = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show WHERE tvdb_id = ANY($1)",
            &numeric
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            if let Some(tvdb_id) = row.tvdb_id {
                result
                    .entry(tvdb_id.to_string())
                    .or_insert_with(|| self.show(row));
            }
        }
        Ok(result)
    }

    /// Shows by IMDb id. The first row per id wins.
    pub async fn shows_by_imdb_ids<S: AsRef<str>>(
        &self,
        ids: &[S],
    ) -> Result<HashMap<String, CatalogShow>> {
        let mut result = HashMap::new();
        if ids.is_empty() {
            return Ok(result);
        }
        let ids = text_list(ids);
        let rows = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show WHERE imdb_id = ANY($1)",
            &ids as &[&str]
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            if let Some(imdb_id) = row.imdb_id.clone().filter(|id| !id.is_empty()) {
                result.entry(imdb_id).or_insert_with(|| self.show(row));
            }
        }
        Ok(result)
    }

    pub async fn show_by_imdb_id(&self, imdb_id: &str) -> Result<Option<CatalogShow>> {
        let row = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show WHERE imdb_id = $1 LIMIT 1",
            imdb_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.show(row)))
    }

    pub async fn show_by_tvdb_id(&self, tvdb_id: &str) -> Result<Option<CatalogShow>> {
        let Ok(numeric) = tvdb_id.trim().parse::<i64>() else {
            return Ok(None);
        };
        let row = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show WHERE tvdb_id = $1 LIMIT 1",
            numeric
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.show(row)))
    }

    pub async fn show_by_name(&self, name: &str, year: Option<i32>) -> Result<Option<CatalogShow>> {
        let row = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show
             WHERE (lower(name_en) = lower($1) OR lower(name_pt) = lower($1) OR lower(original_name) = lower($1))
               AND ($2::int IS NULL OR substr(first_air_date, 1, 4) = $3)
             ORDER BY vote_count DESC NULLS LAST LIMIT 1",
            name,
            year,
            year_text(year, 0)
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| self.show(row)))
    }

    pub async fn search_shows(&self, query: &str, limit: i64) -> Result<Vec<CatalogShow>> {
        let rows = sqlx::query_as!(
            ShowRow,
            "SELECT id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons::int AS number_of_seasons, number_of_episodes::int AS number_of_episodes
             FROM tmdb_show
             WHERE name_en ILIKE '%' || $1 || '%' OR name_pt ILIKE '%' || $1 || '%' OR original_name ILIKE '%' || $1 || '%'
             ORDER BY (lower(name_en) = lower($1) OR lower(name_pt) = lower($1)) DESC, vote_count DESC NULLS LAST, first_air_date DESC NULLS LAST
             LIMIT $2",
            query,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| self.show(row)).collect())
    }

    // endregion: shows

    // region: episodes

    /// Find the show that an episode TMDB id belongs to, with the season and episode numbers.
    pub async fn show_for_episode(
        &self,
        episode_tmdb_id: i64,
    ) -> Result<Option<(CatalogShow, i32, i32)>> {
        let row = sqlx::query!(
            r#"SELECT s.id, s.imdb_id, s.tvdb_id, s.name_en, s.name_pt, s.original_name, s.first_air_date, s.poster_path_en,
                    s.poster_path_pt, s.overview_en, s.overview_pt,
                    s.number_of_seasons::int AS number_of_seasons, s.number_of_episodes::int AS number_of_episodes,
                    se.season_number::int AS "season_number!", e.episode_number::int AS "episode_number!"
             FROM tmdb_episode e
             JOIN tmdb_season se ON se.id = e.season_id
             JOIN tmdb_show s ON s.id = se.show_id
             WHERE e.id = $1"#,
            episode_tmdb_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| {
            let show = self.show(ShowRow {
                id: row.id,
                imdb_id: row.imdb_id,
                tvdb_id: row.tvdb_id,
                name_en: row.name_en,
                name_pt: row.name_pt,
                original_name: row.original_name,
                first_air_date: row.first_air_date,
                poster_path_en: row.poster_path_en,
                poster_path_pt: row.poster_path_pt,
                overview_en: row.overview_en,
                overview_pt: row.overview_pt,
                number_of_seasons: row.number_of_seasons,
                number_of_episodes: row.number_of_episodes,
            });
            (show, row.season_number, row.episode_number)
        }))
    }

    pub async fn episode(
        &self,
        show_tmdb_id: i64,
        season: i32,
        number: i32,
    ) -> Result<Option<CatalogEpisode>> {
        let row = sqlx::query_as!(
            EpisodeRow,
            r#"SELECT e.id, se.season_number::int AS "season_number!", e.episode_number::int AS "episode_number!",
                      e.name, e.air_date, e.runtime::int AS runtime, e.still_path
               FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
               WHERE se.show_id = $1 AND se.season_number = $2 AND e.episode_number = $3"#,
            show_tmdb_id,
            i64::from(season),
            i64::from(number)
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Self::episode_of))
    }

    pub async fn episodes(&self, show_tmdb_id: i64) -> Result<Vec<CatalogEpisode>> {
        let rows = sqlx::query_as!(
            EpisodeRow,
            r#"SELECT e.id, se.season_number::int AS "season_number!", e.episode_number::int AS "episode_number!",
                      e.name, e.air_date, e.runtime::int AS runtime, e.still_path
               FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
               WHERE se.show_id = $1 ORDER BY se.season_number, e.episode_number"#,
            show_tmdb_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Self::episode_of).collect())
    }

    /// Every episode of the given shows, keyed by `(show TMDB id, season, number)`.
    pub async fn episodes_for_shows(
        &self,
        show_tmdb_ids: &[i64],
    ) -> Result<HashMap<(i64, i32, i32), CatalogEpisode>> {
        if show_tmdb_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query!(
            r#"SELECT se.show_id, e.id, se.season_number::int AS "season_number!", e.episode_number::int AS "episode_number!",
                      e.name, e.air_date, e.runtime::int AS runtime, e.still_path
               FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
               WHERE se.show_id = ANY($1)"#,
            show_tmdb_ids
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let episode = Self::episode_of(EpisodeRow {
                    id: row.id,
                    season_number: row.season_number,
                    episode_number: row.episode_number,
                    name: row.name,
                    air_date: row.air_date,
                    runtime: row.runtime,
                    still_path: row.still_path,
                });
                ((row.show_id, episode.season, episode.number), episode)
            })
            .collect())
    }

    /// Count of aired regular-season episodes per show, for progress totals.
    pub async fn aired_episode_counts(
        &self,
        show_tmdb_ids: &[i64],
        today: NaiveDate,
    ) -> Result<HashMap<i64, i64>> {
        if show_tmdb_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let today = today.format(DATE_FORMAT).to_string();
        let rows = sqlx::query!(
            r#"SELECT se.show_id, COUNT(*) AS "count!"
               FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
               WHERE se.show_id = ANY($1) AND se.season_number > 0 AND (e.air_date IS NULL OR e.air_date <= $2)
               GROUP BY se.show_id"#,
            show_tmdb_ids,
            today
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.show_id, row.count))
            .collect())
    }

    // endregion: episodes

    // region: recommendations

    /// The name of every genre of one kind, for the reason line of a recommendation.
    pub async fn genre_names(&self, kind: GenreKind) -> Result<HashMap<i64, String>> {
        let rows = sqlx::query!(
            "SELECT id, name_en, name_pt FROM genre WHERE kind = $1",
            kind.as_str()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| Some((row.id, self.pick(row.name_en, row.name_pt)?)))
            .collect())
    }

    /// The taste facts of watched movies. One row per genre, grouped here.
    pub async fn movie_taste_facts(&self, ids: &[i64]) -> Result<HashMap<i64, TasteFacts>> {
        let mut facts: HashMap<i64, TasteFacts> = HashMap::new();
        if ids.is_empty() {
            return Ok(facts);
        }
        let rows = sqlx::query!(
            r#"SELECT m.id AS "id!", m.original_language, m.collection_id, g.genre_id AS "genre_id?"
               FROM tmdb_movie m LEFT JOIN tmdb_movie_genre g ON g.movie_id = m.id
               WHERE m.id = ANY($1)"#,
            ids
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let entry = facts.entry(row.id).or_default();
            entry.language = entry.language.take().or(row.original_language);
            entry.collection_id = entry.collection_id.or(row.collection_id);
            if let Some(genre_id) = row.genre_id {
                entry.genres.push(genre_id);
            }
        }
        Ok(facts)
    }

    /// The taste facts of watched shows. `in_production` marks a show that can return.
    pub async fn show_taste_facts(&self, ids: &[i64]) -> Result<HashMap<i64, TasteFacts>> {
        let mut facts: HashMap<i64, TasteFacts> = HashMap::new();
        if ids.is_empty() {
            return Ok(facts);
        }
        let rows = sqlx::query!(
            r#"SELECT s.id AS "id!", s.original_language, s.in_production, g.genre_id AS "genre_id?"
               FROM tmdb_show s LEFT JOIN tmdb_show_genre g ON g.show_id = s.id
               WHERE s.id = ANY($1)"#,
            ids
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let entry = facts.entry(row.id).or_default();
            entry.language = entry.language.take().or(row.original_language);
            entry.in_production |= row.in_production.unwrap_or(false);
            if let Some(genre_id) = row.genre_id {
                entry.genres.push(genre_id);
            }
        }
        Ok(facts)
    }

    /// Released movies that match the genre affinity, best first. `exclude` holds
    /// the TMDB ids that the library already has.
    pub async fn movie_candidates(
        &self,
        genres: &GenreWeights,
        exclude: &[i64],
        today: NaiveDate,
        limit: i64,
    ) -> Result<Vec<Candidate<CatalogMovie>>> {
        if genres.is_empty() {
            return Ok(Vec::new());
        }
        let today = today.format(DATE_FORMAT).to_string();
        let rows = sqlx::query!(
            r#"WITH affinity AS (
                 SELECT * FROM unnest($1::bigint[], $2::float8[]) AS t (genre_id, weight)
               ), pool AS (
                 SELECT m.id FROM tmdb_movie m
                 WHERE m.vote_count >= $3 AND NOT (m.id = ANY($4))
                   AND m.release_date IS NOT NULL AND m.release_date <= $5
                   AND m.adult IS NOT TRUE
                 ORDER BY m.weighted_rating DESC NULLS LAST, m.id DESC
                 LIMIT $6
               ), scored AS (
                 SELECT p.id,
                        SUM(COALESCE(a.weight, 0.0)) / sqrt(COUNT(*)::float8) AS affinity,
                        array_agg(g.genre_id) AS genres
                 FROM pool p
                 JOIN tmdb_movie_genre g ON g.movie_id = p.id
                 LEFT JOIN affinity a ON a.genre_id = g.genre_id
                 GROUP BY p.id
               )
               SELECT m.id, m.imdb_id, m.title_en, m.title_pt, m.original_title, m.release_date,
                      m.runtime::int AS runtime, m.poster_path_en, m.poster_path_pt,
                      m.overview_en, m.overview_pt, m.original_language,
                      s.affinity AS "affinity!", s.genres AS "genres!: Vec<i64>",
                      COALESCE(m.weighted_rating, 0.0) AS "quality!"
               FROM scored s JOIN tmdb_movie m ON m.id = s.id
               WHERE s.affinity > 0
               ORDER BY s.affinity * COALESCE(m.weighted_rating, 0.0) DESC, m.id DESC
               LIMIT $7"#,
            &genres.ids,
            &genres.weights,
            MIN_CANDIDATE_VOTES,
            exclude,
            today,
            CANDIDATE_POOL,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Candidate {
                genres: row.genres,
                affinity: row.affinity,
                quality: row.quality,
                language: row.original_language,
                item: self.movie(MovieRow {
                    id: row.id,
                    imdb_id: row.imdb_id,
                    title_en: row.title_en,
                    title_pt: row.title_pt,
                    original_title: row.original_title,
                    release_date: row.release_date,
                    runtime: row.runtime,
                    poster_path_en: row.poster_path_en,
                    poster_path_pt: row.poster_path_pt,
                    overview_en: row.overview_en,
                    overview_pt: row.overview_pt,
                }),
            })
            .collect())
    }

    /// Shows that match the genre affinity, best first.
    pub async fn show_candidates(
        &self,
        genres: &GenreWeights,
        exclude: &[i64],
        today: NaiveDate,
        limit: i64,
    ) -> Result<Vec<Candidate<CatalogShow>>> {
        if genres.is_empty() {
            return Ok(Vec::new());
        }
        let today = today.format(DATE_FORMAT).to_string();
        let rows = sqlx::query!(
            r#"WITH affinity AS (
                 SELECT * FROM unnest($1::bigint[], $2::float8[]) AS t (genre_id, weight)
               ), pool AS (
                 SELECT s.id FROM tmdb_show s
                 WHERE s.vote_count >= $3 AND NOT (s.id = ANY($4))
                   AND s.first_air_date IS NOT NULL AND s.first_air_date <= $5
                 ORDER BY s.weighted_rating DESC NULLS LAST, s.id DESC
                 LIMIT $6
               ), scored AS (
                 SELECT p.id,
                        SUM(COALESCE(a.weight, 0.0)) / sqrt(COUNT(*)::float8) AS affinity,
                        array_agg(g.genre_id) AS genres
                 FROM pool p
                 JOIN tmdb_show_genre g ON g.show_id = p.id
                 LEFT JOIN affinity a ON a.genre_id = g.genre_id
                 GROUP BY p.id
               )
               SELECT s2.id, s2.imdb_id, s2.tvdb_id, s2.name_en, s2.name_pt, s2.original_name,
                      s2.first_air_date, s2.poster_path_en, s2.poster_path_pt,
                      s2.overview_en, s2.overview_pt, s2.original_language,
                      s2.number_of_seasons::int AS number_of_seasons,
                      s2.number_of_episodes::int AS number_of_episodes,
                      sc.affinity AS "affinity!", sc.genres AS "genres!: Vec<i64>",
                      COALESCE(s2.weighted_rating, 0.0) AS "quality!"
               FROM scored sc JOIN tmdb_show s2 ON s2.id = sc.id
               WHERE sc.affinity > 0
               ORDER BY sc.affinity * COALESCE(s2.weighted_rating, 0.0) DESC, s2.id DESC
               LIMIT $7"#,
            &genres.ids,
            &genres.weights,
            MIN_CANDIDATE_VOTES,
            exclude,
            today,
            CANDIDATE_POOL,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Candidate {
                genres: row.genres,
                affinity: row.affinity,
                quality: row.quality,
                language: row.original_language,
                item: self.show(ShowRow {
                    id: row.id,
                    imdb_id: row.imdb_id,
                    tvdb_id: row.tvdb_id,
                    name_en: row.name_en,
                    name_pt: row.name_pt,
                    original_name: row.original_name,
                    first_air_date: row.first_air_date,
                    poster_path_en: row.poster_path_en,
                    poster_path_pt: row.poster_path_pt,
                    overview_en: row.overview_en,
                    overview_pt: row.overview_pt,
                    number_of_seasons: row.number_of_seasons,
                    number_of_episodes: row.number_of_episodes,
                }),
            })
            .collect())
    }

    /// Released movies of the given collections that the library does not have.
    pub async fn collection_movies(
        &self,
        collection_ids: &[i64],
        exclude: &[i64],
        today: NaiveDate,
        limit: i64,
    ) -> Result<Vec<CollectionMovie>> {
        if collection_ids.is_empty() {
            return Ok(Vec::new());
        }
        let today = today.format(DATE_FORMAT).to_string();
        let rows = sqlx::query!(
            r#"SELECT m.id, m.imdb_id, m.title_en, m.title_pt, m.original_title, m.release_date,
                      m.runtime::int AS runtime, m.poster_path_en, m.poster_path_pt,
                      m.overview_en, m.overview_pt,
                      m.collection_id AS "collection_id!", c.name_en, c.name_pt,
                      COALESCE(m.weighted_rating, 0.0) AS "quality!"
               FROM tmdb_movie m JOIN collection c ON c.id = m.collection_id
               WHERE m.collection_id = ANY($1) AND NOT (m.id = ANY($2))
                 AND m.release_date IS NOT NULL AND m.release_date <= $3
                 AND m.adult IS NOT TRUE
               ORDER BY m.release_date, m.id
               LIMIT $4"#,
            collection_ids,
            exclude,
            today,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| CollectionMovie {
                collection_id: row.collection_id,
                collection_name: self.pick(row.name_en, row.name_pt),
                quality: row.quality,
                movie: self.movie(MovieRow {
                    id: row.id,
                    imdb_id: row.imdb_id,
                    title_en: row.title_en,
                    title_pt: row.title_pt,
                    original_title: row.original_title,
                    release_date: row.release_date,
                    runtime: row.runtime,
                    poster_path_en: row.poster_path_en,
                    poster_path_pt: row.poster_path_pt,
                    overview_en: row.overview_en,
                    overview_pt: row.overview_pt,
                }),
            })
            .collect())
    }

    // endregion: recommendations
}
