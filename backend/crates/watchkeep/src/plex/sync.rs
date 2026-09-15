//! Import the Plex library so that Watchkeep knows which movies and episodes
//! exist and which of them Plex already marks as watched.
//! Uses the Plex Media Server HTTP API with `X-Plex-Token`.

use std::collections::HashMap;
use std::ops::DerefMut;
use std::time::Duration;

use chrono::{DateTime, Utc};
use eyre::{Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use watchkeep_catalog::Catalog;
use watchkeep_storage::clock::SharedClock;
use watchkeep_storage::library::{Library, PlayInput, ProgressInput};
use watchkeep_storage::model::{
    EpisodeInput, EpisodeRef, ExternalIds, MediaKind, MovieRef, PlaySource, PlayState, ShowRef,
    TargetKind, from_millis, non_empty, parse_date,
};

use crate::plex::payload::{PlexMetadata, parse_ids};
use crate::scrobble::{enrich_episode, enrich_movie};

/// Headers of the Plex Media Server API.
mod header {
    pub const TOKEN: &str = "X-Plex-Token";
    pub const CONTAINER_START: &str = "X-Plex-Container-Start";
    pub const CONTAINER_SIZE: &str = "X-Plex-Container-Size";
    pub const ACCEPT: &str = "Accept";
    pub const JSON: &str = "application/json";
}

const SECTIONS_PATH: &str = "/library/sections";

/// Items per page of the library API.
pub const DEFAULT_PAGE_SIZE: usize = 500;

/// The `type` values of the library API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlexItemType {
    Movie = 1,
    Show = 2,
    Episode = 4,
}

#[derive(Clone, Debug)]
pub struct SyncOptions {
    pub plex_url: String,
    pub plex_token: String,
    pub page_size: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub sections: u64,
    pub movies: u64,
    pub shows: u64,
    pub episodes: u64,
    pub plays_imported: u64,
    pub progress_imported: u64,
}

#[derive(serde::Deserialize)]
struct PlexContainer<T> {
    #[serde(rename = "MediaContainer")]
    media_container: Option<MediaContainer<T>>,
}

#[derive(serde::Deserialize)]
struct MediaContainer<T> {
    #[serde(rename = "totalSize")]
    total_size: Option<u64>,
    #[serde(rename = "Directory")]
    directory: Option<Vec<T>>,
    #[serde(rename = "Metadata")]
    metadata: Option<Vec<T>>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct PlexSection {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
}

/// A library item: the webhook metadata fields plus the fields of the library API.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct PlexItem {
    #[serde(flatten)]
    pub metadata: PlexMetadata,
    #[serde(rename = "ratingKey")]
    pub rating_key: Option<String>,
    #[serde(rename = "viewCount")]
    pub view_count: Option<i64>,
    /// Unix seconds.
    #[serde(rename = "lastViewedAt")]
    pub last_viewed_at: Option<i64>,
    #[serde(rename = "grandparentRatingKey")]
    pub grandparent_rating_key: Option<String>,
}

pub struct PlexClient {
    client: reqwest::Client,
    options: SyncOptions,
}

impl PlexClient {
    pub fn new(options: SyncOptions) -> Self {
        Self {
            client: reqwest::Client::new(),
            options,
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, start: usize) -> Result<PlexContainer<T>> {
        let response = self
            .client
            .get(format!("{}{path}", self.options.plex_url))
            .header(header::ACCEPT, header::JSON)
            .header(header::TOKEN, &self.options.plex_token)
            .header(header::CONTAINER_START, start.to_string())
            .header(header::CONTAINER_SIZE, self.options.page_size.to_string())
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Plex request failed: {} {path}", response.status().as_u16());
        }
        Ok(response.json().await?)
    }

    pub async fn sections(&self) -> Result<Vec<PlexSection>> {
        let body = self.get::<PlexSection>(SECTIONS_PATH, 0).await?;
        Ok(body
            .media_container
            .and_then(|container| container.directory)
            .unwrap_or_default())
    }

    /// Fetch every item of one type inside a section, page by page.
    pub async fn items(&self, section_key: &str, kind: PlexItemType) -> Result<Vec<PlexItem>> {
        let path = format!(
            "{SECTIONS_PATH}/{section_key}/all?type={}&includeGuids=1",
            kind as u8
        );
        let mut items = Vec::new();
        let mut start = 0;
        loop {
            let body = self.get::<PlexItem>(&path, start).await?;
            let (page, total_size) = match body.media_container {
                Some(container) => (container.metadata.unwrap_or_default(), container.total_size),
                None => (Vec::new(), None),
            };
            let count = page.len();
            let total = total_size.unwrap_or(count as u64);
            items.extend(page);
            start += count;
            if count == 0 || start as u64 >= total {
                break;
            }
        }
        Ok(items)
    }
}

/// The time Plex reports as unix seconds, or `fallback` when it reports none.
fn time_of(seconds: Option<i64>, fallback: DateTime<Utc>) -> DateTime<Utc> {
    seconds
        .filter(|seconds| *seconds != 0)
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        .unwrap_or(fallback)
}

async fn import_watch_state<C: DerefMut<Target = PgConnection>>(
    library: &mut Library<C>,
    report: &mut SyncReport,
    kind: TargetKind,
    id: Uuid,
    item: &PlexItem,
    duration: Option<Duration>,
) -> Result<()> {
    if item.view_count.unwrap_or(0) > 0 && library.play_count(kind, id).await? == 0 {
        let now = library.now();
        library
            .record_play(PlayInput {
                kind,
                id,
                watched_at: Some(time_of(item.last_viewed_at, now)),
                source: PlaySource::PlexSync,
                account: None,
                player: None,
                external_id: None,
            })
            .await?;
        report.plays_imported += 1;
    }
    if let Some(position) =
        from_millis(item.metadata.view_offset_ms).filter(|offset| !offset.is_zero())
        && library.get_progress(kind, id).await?.is_none()
    {
        library
            .set_progress(ProgressInput {
                kind,
                id,
                position,
                duration,
                state: PlayState::Stopped,
                account: None,
                player: None,
                updated_at: None,
            })
            .await?;
        report.progress_imported += 1;
    }
    Ok(())
}

struct ShowEntry {
    title: String,
    year: Option<i32>,
    ids: ExternalIds,
}

pub async fn sync_plex_library(
    pool: &PgPool,
    catalog: Option<&Catalog>,
    options: &SyncOptions,
    clock: SharedClock,
) -> Result<SyncReport> {
    if options.plex_url.is_empty() || options.plex_token.is_empty() {
        bail!("Plex sync needs WATCHKEEP_PLEX_URL and WATCHKEEP_PLEX_TOKEN");
    }
    let client = PlexClient::new(options.clone());
    let mut report = SyncReport::default();

    for section in client.sections().await? {
        let Some(kind) = MediaKind::parse(&section.kind) else {
            continue;
        };
        report.sections += 1;
        tracing::info!("sync: section \"{}\" ({})", section.title, section.kind);

        if kind == MediaKind::Movie {
            let movies = client.items(&section.key, PlexItemType::Movie).await?;
            let mut tx = pool.begin().await?;
            {
                let mut library = Library::new(&mut *tx, clock.clone());
                for item in &movies {
                    let Some(title) = non_empty(item.metadata.title.as_deref()) else {
                        continue;
                    };
                    let movie = enrich_movie(
                        catalog,
                        MovieRef {
                            title: title.to_owned(),
                            year: item.metadata.year,
                            ids: parse_ids(
                                item.metadata.guid.as_deref(),
                                item.metadata.guids.as_deref(),
                            ),
                            duration: from_millis(item.metadata.duration_ms),
                            summary: item.metadata.summary.clone(),
                            poster_path: None,
                        },
                    )
                    .await?;
                    let movie = library.upsert_movie(&movie).await?;
                    report.movies += 1;
                    import_watch_state(
                        &mut library,
                        &mut report,
                        TargetKind::Movie,
                        movie.id,
                        item,
                        movie.duration(),
                    )
                    .await?;
                }
            }
            tx.commit().await?;
            continue;
        }

        let shows = client.items(&section.key, PlexItemType::Show).await?;
        let episodes = client.items(&section.key, PlexItemType::Episode).await?;
        let mut tx = pool.begin().await?;
        {
            let mut library = Library::new(&mut *tx, clock.clone());
            let mut entries: Vec<ShowEntry> = Vec::new();
            let mut show_by_key: HashMap<String, usize> = HashMap::new();
            for item in &shows {
                let Some(title) = non_empty(item.metadata.title.as_deref()) else {
                    continue;
                };
                let ids = parse_ids(
                    item.metadata.guid.as_deref(),
                    item.metadata.guids.as_deref(),
                );
                library
                    .upsert_show(&ShowRef {
                        title: title.to_owned(),
                        year: item.metadata.year,
                        ids: ids.clone(),
                        summary: item.metadata.summary.clone(),
                        poster_path: None,
                    })
                    .await?;
                report.shows += 1;
                let index = entries.len();
                entries.push(ShowEntry {
                    title: title.to_owned(),
                    year: item.metadata.year,
                    ids,
                });
                if let Some(key) = non_empty(item.rating_key.as_deref()) {
                    show_by_key.insert(key.to_owned(), index);
                }
                if let Some(guid) = non_empty(item.metadata.guid.as_deref()) {
                    show_by_key.insert(guid.to_owned(), index);
                }
            }
            for item in &episodes {
                let (Some(season), Some(number)) =
                    (item.metadata.parent_index, item.metadata.index)
                else {
                    continue;
                };
                let parent = non_empty(item.grandparent_rating_key.as_deref())
                    .and_then(|key| show_by_key.get(key))
                    .or_else(|| {
                        non_empty(item.metadata.grandparent_guid.as_deref())
                            .and_then(|guid| show_by_key.get(guid))
                    })
                    .map(|index| &entries[*index]);
                let show_title = parent
                    .map(|parent| parent.title.clone())
                    .or_else(|| item.metadata.grandparent_title.clone())
                    .filter(|title| !title.is_empty());
                let Some(show_title) = show_title else {
                    continue;
                };
                let enriched = enrich_episode(
                    catalog,
                    EpisodeRef {
                        episode: EpisodeInput {
                            title: item.metadata.title.clone(),
                            season,
                            number,
                            ids: parse_ids(
                                item.metadata.guid.as_deref(),
                                item.metadata.guids.as_deref(),
                            ),
                            duration: from_millis(item.metadata.duration_ms),
                            aired_at: parse_date(item.metadata.originally_available_at.as_deref()),
                        },
                        show: ShowRef {
                            title: show_title,
                            year: parent.and_then(|parent| parent.year),
                            ids: parent.map(|parent| parent.ids.clone()).unwrap_or_else(|| {
                                parse_ids(item.metadata.grandparent_guid.as_deref(), None)
                            }),
                            summary: None,
                            poster_path: None,
                        },
                    },
                )
                .await?;
                let show = library.upsert_show(&enriched.show).await?;
                let episode = library.upsert_episode(show.id, &enriched).await?;
                report.episodes += 1;
                import_watch_state(
                    &mut library,
                    &mut report,
                    TargetKind::Episode,
                    episode.id,
                    item,
                    episode.duration(),
                )
                .await?;
            }
        }
        tx.commit().await?;
    }
    tracing::info!(
        "sync: {} movies, {} shows, {} episodes, {} plays and {} positions imported",
        report.movies,
        report.shows,
        report.episodes,
        report.plays_imported,
        report.progress_imported
    );
    Ok(report)
}
