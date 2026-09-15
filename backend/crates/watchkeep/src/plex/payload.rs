//! Parse a Plex webhook payload into a normalized event.
//! Reference: https://support.plex.tv/articles/115002267687-webhooks/

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use watchkeep_storage::model::{
    EpisodeInput, EpisodeRef, ExternalIds, MediaRef, MovieRef, ShowRef, TargetKind, from_millis,
    non_empty, parse_date,
};

/// Modern Plex agents identify items with `plex://<kind>/<id>` guids.
pub const PLEX_GUID_SCHEME: &str = "plex://";

/// The provider prefixes inside the `Guid` list, for example `imdb://tt0113277`.
mod provider_scheme {
    pub const IMDB: &str = "imdb";
    pub const TMDB: &str = "tmdb";
    pub const TVDB: &str = "tvdb";
}

/// Legacy agents encode the provider id in the guid, for example
/// `com.plexapp.agents.thetvdb://12345/1/5?lang=en`.
const LEGACY_AGENTS: &[(&str, Provider)] = &[
    ("com.plexapp.agents.imdb", Provider::Imdb),
    ("com.plexapp.agents.themoviedb", Provider::Tmdb),
    ("com.plexapp.agents.thetvdb", Provider::Tvdb),
    ("tv.plex.agents.movie", Provider::Tmdb),
    ("tv.plex.agents.series", Provider::Tmdb),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlexEventName {
    #[serde(rename = "media.play")]
    Play,
    #[serde(rename = "media.pause")]
    Pause,
    #[serde(rename = "media.resume")]
    Resume,
    #[serde(rename = "media.stop")]
    Stop,
    #[serde(rename = "media.scrobble")]
    Scrobble,
    #[serde(rename = "media.rate")]
    Rate,
}

impl PlexEventName {
    pub const ALL: &'static [PlexEventName] = &[
        Self::Play,
        Self::Pause,
        Self::Resume,
        Self::Stop,
        Self::Scrobble,
        Self::Rate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Play => "media.play",
            Self::Pause => "media.pause",
            Self::Resume => "media.resume",
            Self::Stop => "media.stop",
            Self::Scrobble => "media.scrobble",
            Self::Rate => "media.rate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|event| event.as_str() == value)
    }
}

impl fmt::Display for PlexEventName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An id that Plex sends as a number or as text.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum PlexId {
    Number(i64),
    Text(String),
}

impl fmt::Display for PlexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlexId::Number(number) => write!(f, "{number}"),
            PlexId::Text(text) => f.write_str(text),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlexAccount {
    pub id: Option<PlexId>,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlexPlayer {
    pub title: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlexGuid {
    pub id: Option<String>,
}

/// The `Metadata` object of a webhook, and the item fields of the library API.
/// Plex reports durations and positions in milliseconds.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlexMetadata {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub guid: Option<String>,
    #[serde(rename = "Guid")]
    pub guids: Option<Vec<PlexGuid>>,
    #[serde(rename = "duration")]
    pub duration_ms: Option<i64>,
    #[serde(rename = "viewOffset")]
    pub view_offset_ms: Option<i64>,
    pub summary: Option<String>,
    pub index: Option<i32>,
    #[serde(rename = "parentIndex")]
    pub parent_index: Option<i32>,
    #[serde(rename = "grandparentGuid")]
    pub grandparent_guid: Option<String>,
    #[serde(rename = "grandparentTitle")]
    pub grandparent_title: Option<String>,
    #[serde(rename = "grandparentYear")]
    pub grandparent_year: Option<i32>,
    #[serde(rename = "originallyAvailableAt")]
    pub originally_available_at: Option<String>,
    #[serde(rename = "userRating")]
    pub user_rating: Option<f64>,
}

/// The webhook body as Plex sends it. Unknown fields are ignored.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlexPayload {
    pub event: Option<String>,
    pub rating: Option<f64>,
    #[serde(rename = "Account")]
    pub account: Option<PlexAccount>,
    #[serde(rename = "Player")]
    pub player: Option<PlexPlayer>,
    #[serde(rename = "Metadata")]
    pub metadata: Option<PlexMetadata>,
}

#[derive(Clone, Debug)]
pub struct PlexEvent {
    pub event: PlexEventName,
    pub account: Option<String>,
    pub account_id: Option<String>,
    pub player: Option<String>,
    pub media: MediaRef,
    /// Playback position when Plex reports one.
    pub view_offset: Option<std::time::Duration>,
    /// Rating between 0 and 10 for media.rate events.
    pub rating: Option<f64>,
}

#[derive(Clone, Debug)]
pub enum ParseResult {
    Event(Box<PlexEvent>),
    Ignored {
        reason: String,
        event: Option<String>,
        media_type: Option<String>,
        title: Option<String>,
    },
}

impl ParseResult {
    pub fn event(self) -> Option<PlexEvent> {
        match self {
            ParseResult::Event(event) => Some(*event),
            ParseResult::Ignored { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Provider {
    Imdb,
    Tmdb,
    Tvdb,
}

impl Provider {
    fn from_scheme(scheme: &str) -> Option<Self> {
        match scheme {
            provider_scheme::IMDB => Some(Self::Imdb),
            provider_scheme::TMDB => Some(Self::Tmdb),
            provider_scheme::TVDB => Some(Self::Tvdb),
            _ => None,
        }
    }
}

fn get_id(ids: &ExternalIds, provider: Provider) -> Option<&str> {
    match provider {
        Provider::Imdb => ids.imdb.as_deref(),
        Provider::Tmdb => ids.tmdb.as_deref(),
        Provider::Tvdb => ids.tvdb.as_deref(),
    }
}

fn set_id(ids: &mut ExternalIds, provider: Provider, value: String) {
    match provider {
        Provider::Imdb => ids.imdb = Some(value),
        Provider::Tmdb => ids.tmdb = Some(value),
        Provider::Tvdb => ids.tvdb = Some(value),
    }
}

fn legacy_agent(name: &str) -> Option<Provider> {
    LEGACY_AGENTS
        .iter()
        .find(|(agent, _)| *agent == name)
        .map(|(_, provider)| *provider)
}

/// `<agent>://<id>` at the start of a legacy guid.
static AGENT_GUID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^([a-z.]+)://([^/?]+)").expect("valid regex"));

/// `<provider>://<id>` entries of the `Guid` list.
static PROVIDER_GUID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(imdb|tmdb|tvdb)://(.+)$").expect("valid regex"));

/// Extract external ids from a Plex `guid` string and `Guid` list.
/// Modern agents use `plex://movie/<id>` plus `Guid: [{id: "imdb://tt..."}]`.
/// Legacy agents encode the id in the guid.
pub fn parse_ids(guid: Option<&str>, guids: Option<&[PlexGuid]>) -> ExternalIds {
    let mut ids = ExternalIds::default();
    if let Some(guid) = non_empty(guid) {
        if guid.starts_with(PLEX_GUID_SCHEME) {
            ids.plex_guid = Some(guid.to_owned());
        } else {
            let captures = AGENT_GUID.captures(guid);
            let provider = captures
                .as_ref()
                .and_then(|captures| legacy_agent(&captures[1]));
            match (
                provider,
                captures.as_ref().map(|captures| captures[2].to_owned()),
            ) {
                (Some(provider), Some(id)) => {
                    ids.plex_guid = Some(guid.split('?').next().unwrap_or(guid).to_owned());
                    set_id(&mut ids, provider, id);
                }
                _ => ids.plex_guid = Some(guid.to_owned()),
            }
        }
    }
    for entry in guids.unwrap_or_default() {
        let id = entry.id.as_deref().unwrap_or_default();
        let Some(captures) = PROVIDER_GUID.captures(id) else {
            continue;
        };
        let Some(provider) = Provider::from_scheme(&captures[1]) else {
            continue;
        };
        if non_empty(get_id(&ids, provider)).is_none() {
            set_id(&mut ids, provider, captures[2].to_owned());
        }
    }
    ids
}

/// Ids of the show that owns a legacy episode guid, for example `...thetvdb://12345/1/5`.
fn legacy_show_ids(episode_guid: Option<&str>) -> ExternalIds {
    let mut ids = ExternalIds::default();
    let Some(guid) = non_empty(episode_guid).filter(|guid| !guid.starts_with(PLEX_GUID_SCHEME))
    else {
        return ids;
    };
    if let Some(captures) = AGENT_GUID.captures(guid)
        && let Some(provider) = legacy_agent(&captures[1])
    {
        set_id(&mut ids, provider, captures[2].to_owned());
        ids.plex_guid = Some(format!("{}://{}", &captures[1], &captures[2]));
    }
    ids
}

/// Text with at least one character that is not white space.
fn optional_string(value: Option<&str>) -> Option<String> {
    value
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

pub fn parse_media(metadata: &PlexMetadata) -> Option<MediaRef> {
    let guid = metadata.guid.as_deref();
    let guids = metadata.guids.as_deref();
    match metadata.kind.as_deref().and_then(TargetKind::parse) {
        Some(TargetKind::Movie) => {
            let title = optional_string(metadata.title.as_deref())?;
            Some(MediaRef::Movie(MovieRef {
                title,
                year: metadata.year,
                ids: parse_ids(guid, guids),
                duration: from_millis(metadata.duration_ms),
                summary: optional_string(metadata.summary.as_deref()),
                poster_path: None,
            }))
        }
        Some(TargetKind::Episode) => {
            let show_title = optional_string(metadata.grandparent_title.as_deref())?;
            let season = metadata.parent_index?;
            let number = metadata.index?;
            let show_ids = match non_empty(metadata.grandparent_guid.as_deref()) {
                Some(show_guid) => parse_ids(Some(show_guid), None),
                None => legacy_show_ids(guid),
            };
            Some(MediaRef::Episode(EpisodeRef {
                episode: EpisodeInput {
                    title: optional_string(metadata.title.as_deref()),
                    season,
                    number,
                    ids: parse_ids(guid, guids),
                    duration: from_millis(metadata.duration_ms),
                    aired_at: parse_date(metadata.originally_available_at.as_deref()),
                },
                show: ShowRef {
                    title: show_title,
                    year: metadata.grandparent_year,
                    ids: show_ids,
                    summary: None,
                    poster_path: None,
                },
            }))
        }
        None => None,
    }
}

pub fn parse_plex_payload(raw: &Value) -> ParseResult {
    if !raw.is_object() {
        return ParseResult::Ignored {
            reason: "payload is not an object".to_owned(),
            event: None,
            media_type: None,
            title: None,
        };
    }
    let payload: PlexPayload = match serde_json::from_value(raw.clone()) {
        Ok(payload) => payload,
        Err(error) => {
            return ParseResult::Ignored {
                reason: format!("payload has an unexpected shape: {error}"),
                event: None,
                media_type: None,
                title: None,
            };
        }
    };
    let event = optional_string(payload.event.as_deref());
    let media_type = optional_string(
        payload
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.kind.as_deref()),
    );
    let title = optional_string(
        payload
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.title.as_deref()),
    );
    let ignored = |reason: String| ParseResult::Ignored {
        reason,
        event: event.clone(),
        media_type: media_type.clone(),
        title: title.clone(),
    };
    let Some(event_name) = event.as_deref() else {
        return ignored("missing event".to_owned());
    };
    let Some(event_kind) = PlexEventName::parse(event_name) else {
        return ignored(format!("event {event_name} is not tracked"));
    };
    let Some(metadata) = payload.metadata.as_ref() else {
        return ignored("missing Metadata".to_owned());
    };
    let Some(media) = parse_media(metadata) else {
        return ignored(format!(
            "media type {} is not tracked",
            media_type.as_deref().unwrap_or("unknown")
        ));
    };
    let rating = payload.rating.or(metadata.user_rating);
    let account = payload.account.as_ref();
    ParseResult::Event(Box::new(PlexEvent {
        event: event_kind,
        account: optional_string(account.and_then(|account| account.title.as_deref())),
        account_id: account
            .and_then(|account| account.id.as_ref())
            .map(PlexId::to_string),
        player: optional_string(
            payload
                .player
                .as_ref()
                .and_then(|player| player.title.as_deref()),
        ),
        media,
        view_offset: from_millis(metadata.view_offset_ms),
        rating,
    }))
}
