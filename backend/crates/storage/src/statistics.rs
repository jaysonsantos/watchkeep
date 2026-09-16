//! Aggregate reads over the play history for the statistics page.
//!
//! Every query reads the `play_details` view, which carries the runtime of each
//! play. The caller gives a time zone name and the current time: the buckets of
//! a calendar (month, weekday, hour, day) are local to the reader, not UTC, and
//! the current time comes from the `Clock`, never from the database.

use chrono::{DateTime, Utc};
use eyre::Result;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::model::TargetKind;
use crate::queries::{Queries, Stats};

/// The time zone that applies when the caller asks for one that Postgres does not know.
pub const DEFAULT_TIMEZONE: &str = "UTC";

/// How many months the activity chart covers, the current month included.
pub const MONTHS: i32 = 24;

/// How many rows a top list and the player list return.
pub const TOP_LIMIT: i64 = 10;

/// Days of the week, as `EXTRACT(dow)` numbers them: Sunday is zero.
const WEEKDAYS: i32 = 7;

/// Hours of the day, as `EXTRACT(hour)` numbers them.
const HOURS: i32 = 24;

/// A streak continues while no local day is missing, so a run that ended
/// yesterday is still the current one.
const STREAK_GRACE_DAYS: i32 = 1;

// region: shapes

/// Everything the statistics page shows.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Statistics {
    /// The time zone the buckets use. It differs from the request when the name is unknown.
    pub timezone: String,
    pub library: Stats,
    pub totals: PlayTotals,
    pub streak: Streak,
    /// One entry per month, oldest first, months without a play included.
    pub months: Vec<PeriodCount>,
    /// Seven entries, Sunday first.
    pub weekdays: Vec<BucketCount>,
    /// Twenty-four entries, midnight first.
    pub hours: Vec<BucketCount>,
    pub sources: Vec<LabelCount>,
    pub players: Vec<LabelCount>,
    /// Shows, by the time their episodes took.
    pub top_shows: Vec<TopItem>,
    /// Movies, by the number of plays.
    pub top_movies: Vec<TopItem>,
}

/// The counters over the whole history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PlayTotals {
    pub plays: i64,
    pub movie_plays: i64,
    pub episode_plays: i64,
    /// Movies with at least one play. A rewatch does not count twice.
    pub movies_watched: i64,
    pub episodes_watched: i64,
    pub shows_watched: i64,
    pub runtime_ms: i64,
    pub movie_runtime_ms: i64,
    pub episode_runtime_ms: i64,
    /// Local days with at least one play.
    pub days_watched: i64,
    pub first_play_at: Option<DateTime<Utc>>,
    pub last_play_at: Option<DateTime<Utc>>,
}

/// Days in a row with a play.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Streak {
    /// The run that reaches today or yesterday. Zero when the last play is older.
    pub current: i64,
    pub longest: i64,
}

/// One calendar period of the activity chart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PeriodCount {
    /// The month as `YYYY-MM`.
    pub period: String,
    pub plays: i64,
    pub runtime_ms: i64,
}

/// One weekday or one hour of the day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct BucketCount {
    pub bucket: i32,
    pub plays: i64,
    pub runtime_ms: i64,
}

/// One source or one player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LabelCount {
    pub label: String,
    pub plays: i64,
    pub runtime_ms: i64,
}

/// One row of a top list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct TopItem {
    pub id: Uuid,
    pub title: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub plays: i64,
    /// Episodes of the show with a play, or one for a movie.
    pub items: i64,
    pub runtime_ms: i64,
    pub last_watched_at: Option<DateTime<Utc>>,
}

// endregion: shapes

impl Queries {
    /// The statistics page, with the calendar buckets in `timezone`.
    pub async fn statistics(&self, timezone: &str, now: DateTime<Utc>) -> Result<Statistics> {
        let pool = self.pool();
        let timezone = known_timezone(pool, timezone).await?;
        Ok(Statistics {
            library: self.stats().await?,
            totals: play_totals(pool, &timezone).await?,
            streak: streak(pool, &timezone, now).await?,
            months: months(pool, &timezone, now).await?,
            weekdays: weekdays(pool, &timezone).await?,
            hours: hours(pool, &timezone).await?,
            sources: sources(pool).await?,
            players: players(pool).await?,
            top_shows: top_shows(pool).await?,
            top_movies: top_movies(pool).await?,
            timezone,
        })
    }
}

// region: queries

/// The name when Postgres knows it, else [`DEFAULT_TIMEZONE`]. An unchecked name
/// in `AT TIME ZONE` makes every later query fail.
async fn known_timezone(pool: &PgPool, requested: &str) -> Result<String> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COALESCE((SELECT name FROM pg_timezone_names WHERE name = $1), $2) AS "name!""#,
        requested,
        DEFAULT_TIMEZONE
    )
    .fetch_one(pool)
    .await?)
}

async fn play_totals(pool: &PgPool, timezone: &str) -> Result<PlayTotals> {
    Ok(sqlx::query_as!(
        PlayTotals,
        r#"SELECT
             COUNT(*) AS "plays!",
             COUNT(*) FILTER (WHERE target_kind = $1) AS "movie_plays!",
             COUNT(*) FILTER (WHERE target_kind = $2) AS "episode_plays!",
             COUNT(DISTINCT target_id) FILTER (WHERE target_kind = $1) AS "movies_watched!",
             COUNT(DISTINCT target_id) FILTER (WHERE target_kind = $2) AS "episodes_watched!",
             COUNT(DISTINCT media_id) FILTER (WHERE target_kind = $2) AS "shows_watched!",
             COALESCE(SUM(runtime_ms), 0)::bigint AS "runtime_ms!",
             COALESCE(SUM(runtime_ms) FILTER (WHERE target_kind = $1), 0)::bigint AS "movie_runtime_ms!",
             COALESCE(SUM(runtime_ms) FILTER (WHERE target_kind = $2), 0)::bigint AS "episode_runtime_ms!",
             COUNT(DISTINCT (watched_at AT TIME ZONE $3)::date) AS "days_watched!",
             MIN(watched_at) AS "first_play_at?",
             MAX(watched_at) AS "last_play_at?"
           FROM play_details"#,
        TargetKind::Movie.as_str(),
        TargetKind::Episode.as_str(),
        timezone
    )
    .fetch_one(pool)
    .await?)
}

/// Gaps and islands over the local days with a play: a day minus its row number
/// is constant inside a run of consecutive days.
async fn streak(pool: &PgPool, timezone: &str, now: DateTime<Utc>) -> Result<Streak> {
    Ok(sqlx::query_as!(
        Streak,
        r#"WITH days AS (
             SELECT DISTINCT (watched_at AT TIME ZONE $1)::date AS day FROM play_details
           ), islands AS (
             SELECT day, day - (ROW_NUMBER() OVER (ORDER BY day))::int AS island FROM days
           ), runs AS (
             SELECT COUNT(*) AS length, MAX(day) AS last_day FROM islands GROUP BY island
           )
           SELECT
             COALESCE(MAX(length), 0) AS "longest!",
             COALESCE(MAX(length) FILTER (WHERE last_day >= ($2 AT TIME ZONE $1)::date - $3::int), 0) AS "current!"
           FROM runs"#,
        timezone,
        now,
        STREAK_GRACE_DAYS
    )
    .fetch_one(pool)
    .await?)
}

/// The last [`MONTHS`] months, the current one last. A month without a play is a zero row.
async fn months(pool: &PgPool, timezone: &str, now: DateTime<Utc>) -> Result<Vec<PeriodCount>> {
    Ok(sqlx::query_as!(
        PeriodCount,
        r#"WITH bounds AS (
             SELECT date_trunc('month', $2 AT TIME ZONE $1) AS last_month
           ), series AS (
             SELECT generate_series(last_month - make_interval(months => $3), last_month, interval '1 month') AS start
             FROM bounds
           )
           SELECT to_char(s.start, 'YYYY-MM') AS "period!",
                  COUNT(d.id) AS "plays!",
                  COALESCE(SUM(d.runtime_ms), 0)::bigint AS "runtime_ms!"
           FROM series s
           LEFT JOIN play_details d ON date_trunc('month', d.watched_at AT TIME ZONE $1) = s.start
           GROUP BY s.start
           ORDER BY s.start"#,
        timezone,
        now,
        MONTHS - 1
    )
    .fetch_all(pool)
    .await?)
}

async fn weekdays(pool: &PgPool, timezone: &str) -> Result<Vec<BucketCount>> {
    bucketed(pool, timezone, true, WEEKDAYS - 1).await
}

async fn hours(pool: &PgPool, timezone: &str) -> Result<Vec<BucketCount>> {
    bucketed(pool, timezone, false, HOURS - 1).await
}

/// Plays per weekday or per hour, every bucket present. One static query covers
/// both, because `EXTRACT` takes the field as a keyword and not as a parameter.
async fn bucketed(
    pool: &PgPool,
    timezone: &str,
    weekday: bool,
    last: i32,
) -> Result<Vec<BucketCount>> {
    Ok(sqlx::query_as!(
        BucketCount,
        r#"SELECT b.bucket AS "bucket!",
                  COUNT(d.id) AS "plays!",
                  COALESCE(SUM(d.runtime_ms), 0)::bigint AS "runtime_ms!"
           FROM generate_series(0, $2) AS b(bucket)
           LEFT JOIN play_details d ON b.bucket = CASE
             WHEN $3::bool THEN EXTRACT(dow FROM d.watched_at AT TIME ZONE $1)
             ELSE EXTRACT(hour FROM d.watched_at AT TIME ZONE $1)
           END
           GROUP BY b.bucket
           ORDER BY b.bucket"#,
        timezone,
        last,
        weekday
    )
    .fetch_all(pool)
    .await?)
}

async fn sources(pool: &PgPool) -> Result<Vec<LabelCount>> {
    Ok(sqlx::query_as!(
        LabelCount,
        r#"SELECT source AS "label!",
                  COUNT(*) AS "plays!",
                  COALESCE(SUM(runtime_ms), 0)::bigint AS "runtime_ms!"
           FROM play_details
           GROUP BY source
           ORDER BY COUNT(*) DESC, source"#
    )
    .fetch_all(pool)
    .await?)
}

async fn players(pool: &PgPool) -> Result<Vec<LabelCount>> {
    Ok(sqlx::query_as!(
        LabelCount,
        r#"SELECT player AS "label!",
                  COUNT(*) AS "plays!",
                  COALESCE(SUM(runtime_ms), 0)::bigint AS "runtime_ms!"
           FROM play_details
           WHERE player IS NOT NULL AND player <> ''
           GROUP BY player
           ORDER BY COUNT(*) DESC, player
           LIMIT $1"#,
        TOP_LIMIT
    )
    .fetch_all(pool)
    .await?)
}

/// Shows by the time their episodes took, then by the number of plays.
async fn top_shows(pool: &PgPool) -> Result<Vec<TopItem>> {
    Ok(sqlx::query_as!(
        TopItem,
        r#"SELECT media_id AS "id!",
                  media_title AS "title!",
                  media_year AS "year?",
                  poster_path AS "poster_path?",
                  COUNT(*) AS "plays!",
                  COUNT(DISTINCT target_id) AS "items!",
                  COALESCE(SUM(runtime_ms), 0)::bigint AS "runtime_ms!",
                  MAX(watched_at) AS "last_watched_at?"
           FROM play_details
           WHERE target_kind = $1 AND media_id IS NOT NULL
           GROUP BY media_id, media_title, media_year, poster_path
           ORDER BY COALESCE(SUM(runtime_ms), 0)::bigint DESC, COUNT(*) DESC, media_title
           LIMIT $2"#,
        TargetKind::Episode.as_str(),
        TOP_LIMIT
    )
    .fetch_all(pool)
    .await?)
}

/// Movies by the number of plays, so that a rewatch lifts a film to the top.
async fn top_movies(pool: &PgPool) -> Result<Vec<TopItem>> {
    Ok(sqlx::query_as!(
        TopItem,
        r#"SELECT media_id AS "id!",
                  media_title AS "title!",
                  media_year AS "year?",
                  poster_path AS "poster_path?",
                  COUNT(*) AS "plays!",
                  COUNT(DISTINCT target_id) AS "items!",
                  COALESCE(SUM(runtime_ms), 0)::bigint AS "runtime_ms!",
                  MAX(watched_at) AS "last_watched_at?"
           FROM play_details
           WHERE target_kind = $1 AND media_id IS NOT NULL
           GROUP BY media_id, media_title, media_year, poster_path
           ORDER BY COUNT(*) DESC, COALESCE(SUM(runtime_ms), 0)::bigint DESC, media_title
           LIMIT $2"#,
        TargetKind::Movie.as_str(),
        TOP_LIMIT
    )
    .fetch_all(pool)
    .await?)
}

// endregion: queries
