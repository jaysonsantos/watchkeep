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
}
