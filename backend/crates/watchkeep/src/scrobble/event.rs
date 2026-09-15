//! The body of `POST /webhook/scrobble`: one playback event from a player that
//! is not Plex. The sender names the item with external ids, so it needs no
//! Watchkeep id. Times are RFC 3339 and positions are milliseconds, like every
//! other request body.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;
use watchkeep_storage::model::{
    EpisodeInput, EpisodeRef, ExternalIds, MediaRef, MovieRef, ShowRef, TargetKind, from_millis,
    non_empty,
};
use watchkeep_storage::text_enum;

/// The `event` column of the webhook log starts with this word.
pub const LOG_PREFIX: &str = "scrobble";

/// The field names that the error messages spell.
mod field {
    pub const SEASON: &str = "media.season";
    pub const NUMBER: &str = "media.number";
    pub const SHOW: &str = "media.show";
    pub const POSITION_MS: &str = "position_ms";
}

text_enum! {
    /// What the sender reports about the playback.
    ScrobbleEventName {
        Start => "start",
        Progress => "progress",
        Pause => "pause",
        Stop => "stop",
        Watched => "watched",
        Unwatched => "unwatched",
    }
}

impl ScrobbleEventName {
    /// The `event` value of the webhook log row, for example `scrobble.progress`.
    pub fn log_name(self) -> String {
        format!("{LOG_PREFIX}.{}", self.as_str())
    }

    /// True for the events that carry a playback position.
    pub fn needs_position(self) -> bool {
        matches!(
            self,
            Self::Start | Self::Progress | Self::Pause | Self::Stop
        )
    }
}

/// A body that the endpoint cannot apply. The handler turns each case into a status.
#[derive(Debug, Error)]
pub enum InvalidEvent {
    /// The event needs a field that the body does not have.
    #[error("{0} is required")]
    Missing(&'static str),
    /// The body has no ids and no title, so no item can match.
    #[error("the event has no ids and no title")]
    Unidentified,
}

/// An id that a sender writes as a number or as text.
#[derive(Clone, Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(untagged)]
pub enum ScrobbleId {
    Number(i64),
    Text(String),
}

impl fmt::Display for ScrobbleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScrobbleId::Number(number) => write!(f, "{number}"),
            ScrobbleId::Text(text) => f.write_str(text),
        }
    }
}

/// The external ids of one movie, show, or episode.
#[derive(Clone, Debug, Default, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(default)]
pub struct ScrobbleIds {
    #[ts(optional)]
    pub tmdb: Option<ScrobbleId>,
    #[ts(optional)]
    pub imdb: Option<String>,
    #[ts(optional)]
    pub tvdb: Option<ScrobbleId>,
}

/// An id as the storage layer holds it: trimmed text, and `None` when blank.
fn id_text(id: Option<&ScrobbleId>) -> Option<String> {
    let text = id?.to_string();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

impl ScrobbleIds {
    fn external(&self) -> ExternalIds {
        ExternalIds {
            plex_guid: None,
            imdb: non_empty(self.imdb.as_deref().map(str::trim)).map(str::to_owned),
            tmdb: id_text(self.tmdb.as_ref()),
            tvdb: id_text(self.tvdb.as_ref()),
        }
    }
}

/// The show that owns an episode.
#[derive(Clone, Debug, Default, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(default)]
pub struct ScrobbleShow {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub year: Option<i32>,
    pub ids: ScrobbleIds,
}

/// The item that the sender played.
#[derive(Clone, Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ScrobbleMedia {
    #[serde(rename = "type")]
    pub kind: TargetKind,
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub year: Option<i32>,
    #[serde(default)]
    pub ids: ScrobbleIds,
    /// The season in TMDB order. Episodes only.
    #[serde(default)]
    #[ts(optional)]
    pub season: Option<i32>,
    /// The episode number in TMDB order. Episodes only.
    #[serde(default)]
    #[ts(optional)]
    pub number: Option<i32>,
    #[serde(default)]
    #[ts(optional)]
    pub show: Option<ScrobbleShow>,
}

/// Text without leading or trailing blanks. Blank text becomes empty.
fn trimmed(text: Option<&str>) -> String {
    text.unwrap_or_default().trim().to_owned()
}

impl ScrobbleMedia {
    /// The media reference that `resolve_target` takes, with the runtime the player measured.
    fn media_ref(&self, duration: Option<Duration>) -> Result<MediaRef, InvalidEvent> {
        let title = trimmed(self.title.as_deref());
        let ids = self.ids.external();
        match self.kind {
            TargetKind::Movie => {
                if ids.is_empty() && title.is_empty() {
                    return Err(InvalidEvent::Unidentified);
                }
                Ok(MediaRef::Movie(MovieRef {
                    title,
                    year: self.year,
                    ids,
                    duration,
                    summary: None,
                    poster_path: None,
                }))
            }
            TargetKind::Episode => {
                let show = self
                    .show
                    .as_ref()
                    .ok_or(InvalidEvent::Missing(field::SHOW))?;
                let season = self.season.ok_or(InvalidEvent::Missing(field::SEASON))?;
                let number = self.number.ok_or(InvalidEvent::Missing(field::NUMBER))?;
                let show_ids = show.ids.external();
                let show_title = trimmed(show.title.as_deref());
                if show_ids.is_empty() && show_title.is_empty() && ids.is_empty() {
                    return Err(InvalidEvent::Unidentified);
                }
                Ok(MediaRef::Episode(EpisodeRef {
                    episode: EpisodeInput {
                        title: (!title.is_empty()).then_some(title),
                        season,
                        number,
                        ids,
                        duration,
                        aired_at: None,
                    },
                    show: ShowRef {
                        title: show_title,
                        year: show.year,
                        ids: show_ids,
                        summary: None,
                        poster_path: None,
                    },
                }))
            }
        }
    }
}

/// One event of one player. The sender repeats `event_id` when it sends the event again.
#[derive(Clone, Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ScrobbleEvent {
    pub event_id: Uuid,
    pub event: ScrobbleEventName,
    /// The time of the event on the sender.
    pub occurred_at: DateTime<Utc>,
    /// The name of the sending application.
    pub client: String,
    /// The viewer on the sender.
    #[serde(default)]
    #[ts(optional)]
    pub account: Option<String>,
    /// The device that played the item.
    #[serde(default)]
    #[ts(optional)]
    pub player: Option<String>,
    pub media: ScrobbleMedia,
    #[serde(default)]
    #[ts(optional)]
    pub position_ms: Option<i64>,
    /// The runtime that the player measured. It wins over the catalog runtime.
    #[serde(default)]
    #[ts(optional)]
    pub duration_ms: Option<i64>,
    /// The time of the play, for `watched` events. The default is `occurred_at`.
    #[serde(default)]
    #[ts(optional)]
    pub watched_at: Option<DateTime<Utc>>,
}

impl ScrobbleEvent {
    pub fn position(&self) -> Option<Duration> {
        from_millis(self.position_ms)
    }

    pub fn duration(&self) -> Option<Duration> {
        from_millis(self.duration_ms)
    }

    /// The time of the play. A backfill sets `watched_at`, every other event does not.
    pub fn played_at(&self) -> DateTime<Utc> {
        self.watched_at.unwrap_or(self.occurred_at)
    }

    /// The device of the play, or the sending application when the sender names no device.
    pub fn device(&self) -> Option<String> {
        non_empty(self.player.as_deref().map(str::trim))
            .or_else(|| non_empty(Some(self.client.trim())))
            .map(str::to_owned)
    }

    pub fn viewer(&self) -> Option<String> {
        non_empty(self.account.as_deref().map(str::trim)).map(str::to_owned)
    }

    /// The item of the event, once the body holds everything the event needs.
    pub fn media_ref(&self) -> Result<MediaRef, InvalidEvent> {
        if self.event.needs_position() && self.position_ms.is_none() {
            return Err(InvalidEvent::Missing(field::POSITION_MS));
        }
        self.media.media_ref(self.duration())
    }
}
