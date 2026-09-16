//! Row types of the Watchkeep tables, the text enums of their `kind` columns,
//! and the references that describe an item before it has a row.
//!
//! Rows mirror the columns: durations and positions are `*_ms` integers. The
//! references that services build use `std::time::Duration`; `millis` and
//! `from_millis` convert at the database boundary.

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use thiserror::Error;
use uuid::{NoContext, Timestamp, Uuid};

/// The text format of a calendar date, for example `2022-02-17`.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// Specials live in season 0. They do not count as episodes to watch, so a
/// query and a view that measure the progress of a show both leave them out.
pub const SPECIALS_SEASON: i32 = 0;

/// A text column holds a word that is not one of the allowed values.
#[derive(Debug, Error)]
#[error("invalid kind: {0}")]
pub struct InvalidKind(pub String);

/// The calendar date at the start of an ISO-8601 text, or `None` for text that is not a date.
pub fn parse_date(text: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(non_empty(text)?.get(0..10)?, DATE_FORMAT).ok()
}

crate::text_enum! {
    /// A row of the `media` table is a movie or a show.
    MediaKind { Movie => "movie", Show => "show" }
}

crate::text_enum! {
    /// Plays and progress point at a movie or an episode.
    TargetKind { Movie => "movie", Episode => "episode" }
}

crate::text_enum! {
    /// Ratings point at a movie, a show, or an episode.
    RatingKind { Movie => "movie", Show => "show", Episode => "episode" }
}

crate::text_enum! {
    PlayState { Playing => "playing", Paused => "paused", Stopped => "stopped" }
}

crate::text_enum! {
    /// Where a play came from.
    PlaySource {
        Manual => "manual",
        PlexScrobble => "plex-scrobble",
        PlexStop => "plex-stop",
        PlexSync => "plex-sync",
        Trakt => "trakt",
    }
}

impl From<MediaKind> for RatingKind {
    fn from(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Movie => RatingKind::Movie,
            MediaKind::Show => RatingKind::Show,
        }
    }
}

impl From<TargetKind> for RatingKind {
    fn from(kind: TargetKind) -> Self {
        match kind {
            TargetKind::Movie => RatingKind::Movie,
            TargetKind::Episode => RatingKind::Episode,
        }
    }
}

/// A new row id: UUID v7, ordered by the time of creation.
pub fn new_id(at: DateTime<Utc>) -> Uuid {
    let seconds = u64::try_from(at.timestamp()).unwrap_or_default();
    Uuid::new_v7(Timestamp::from_unix(
        NoContext,
        seconds,
        at.timestamp_subsec_nanos(),
    ))
}

/// A `*_ms` column as a duration. Negative values give `None`.
pub fn from_millis(millis: Option<i64>) -> Option<Duration> {
    millis
        .and_then(|value| u64::try_from(value).ok())
        .map(Duration::from_millis)
}

/// A duration as a `*_ms` column value.
pub fn to_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub fn millis(duration: Option<Duration>) -> Option<i64> {
    duration.map(to_millis)
}

pub fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.is_empty())
}

/// A TMDB id string as a positive integer, or `None`.
pub fn tmdb_number(value: Option<&str>) -> Option<i64> {
    let text = value?.trim();
    let parsed = text.parse::<i64>().ok().or_else(|| {
        text.parse::<f64>()
            .ok()
            .filter(|number| number.is_finite() && number.fract() == 0.0)
            .map(|number| number as i64)
    })?;
    (parsed > 0).then_some(parsed)
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct MediaRow {
    pub id: Uuid,
    pub kind: MediaKind,
    pub title: String,
    pub year: Option<i32>,
    pub plex_guid: Option<String>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub summary: Option<String>,
    pub poster_path: Option<String>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MediaRow {
    pub fn duration(&self) -> Option<Duration> {
        from_millis(self.duration_ms)
    }
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct EpisodeRow {
    pub id: Uuid,
    pub show_id: Uuid,
    pub season: i32,
    pub number: i32,
    pub title: Option<String>,
    pub plex_guid: Option<String>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub aired_at: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EpisodeRow {
    pub fn duration(&self) -> Option<Duration> {
        from_millis(self.duration_ms)
    }
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PlayRow {
    pub id: Uuid,
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    pub watched_at: DateTime<Utc>,
    pub source: String,
    pub account: Option<String>,
    pub player: Option<String>,
    pub external_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct WatchlistRow {
    pub target_kind: MediaKind,
    pub target_id: Uuid,
    pub listed_at: DateTime<Utc>,
    pub rank: Option<i32>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ProgressRow {
    pub target_kind: TargetKind,
    pub target_id: Uuid,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    pub state: PlayState,
    pub account: Option<String>,
    pub player: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl ProgressRow {
    pub fn position(&self) -> Duration {
        from_millis(Some(self.position_ms)).unwrap_or_default()
    }

    pub fn duration(&self) -> Option<Duration> {
        from_millis(self.duration_ms)
    }
}

/// External ids of a movie, show, or episode as Plex or Trakt report them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalIds {
    pub plex_guid: Option<String>,
    pub imdb: Option<String>,
    pub tmdb: Option<String>,
    pub tvdb: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovieRef {
    pub title: String,
    pub year: Option<i32>,
    pub ids: ExternalIds,
    pub duration: Option<Duration>,
    pub summary: Option<String>,
    pub poster_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShowRef {
    pub title: String,
    pub year: Option<i32>,
    pub ids: ExternalIds,
    pub summary: Option<String>,
    pub poster_path: Option<String>,
}

/// The columns of an episode row that a source can supply.
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeInput {
    pub title: Option<String>,
    pub season: i32,
    pub number: i32,
    pub ids: ExternalIds,
    pub duration: Option<Duration>,
    pub aired_at: Option<NaiveDate>,
}

/// An episode together with its show. Derefs to the episode columns.
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeRef {
    pub episode: EpisodeInput,
    pub show: ShowRef,
}

impl std::ops::Deref for EpisodeRef {
    type Target = EpisodeInput;

    fn deref(&self) -> &EpisodeInput {
        &self.episode
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaRef {
    Movie(MovieRef),
    Episode(EpisodeRef),
}

impl MediaRef {
    pub fn target_kind(&self) -> TargetKind {
        match self {
            MediaRef::Movie(_) => TargetKind::Movie,
            MediaRef::Episode(_) => TargetKind::Episode,
        }
    }
}
