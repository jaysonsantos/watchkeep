//! Read models that combine Watchkeep data with the TMDB catalog.
//! The catalog supplies the full episode list, so unwatched episodes show up
//! even when Plex never reported them.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use eyre::Result;
use serde::Serialize;
use tracing::instrument;
use uuid::Uuid;
use watchkeep_catalog::{Catalog, CatalogEpisode};
use watchkeep_storage::clock::{SharedClock, date_of};
use watchkeep_storage::lists::{SortOrder, WatchFilter};
use watchkeep_storage::model::{PlayState, millis};
use watchkeep_storage::queries::{EpisodeView, Queries, ShowView};

use crate::scrobble::runtime_to_duration;

/// Specials live in season 0. They do not count as episodes to watch.
pub const SPECIALS_SEASON: i32 = 0;

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShowListItem {
    #[serde(flatten)]
    pub show: ShowView,
    /// Episodes known to the catalog (aired, regular seasons) or, without a catalog, episodes seen locally.
    pub total_episodes: i64,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct MergedEpisode {
    /// Local episode id, or `None` when the episode exists only in the catalog.
    pub id: Option<Uuid>,
    pub season: i32,
    pub number: i32,
    pub title: Option<String>,
    pub aired_at: Option<NaiveDate>,
    pub duration_ms: Option<i64>,
    pub play_count: i64,
    pub last_watched_at: Option<DateTime<Utc>>,
    pub position_ms: Option<i64>,
    pub progress_state: Option<PlayState>,
    pub tmdb_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShowDetail {
    #[serde(flatten)]
    pub show: ShowListItem,
    pub episodes: Vec<MergedEpisode>,
}

/// An episode counts as aired when it has no air date or an air date up to today.
pub fn has_aired(air_date: Option<NaiveDate>, today: NaiveDate) -> bool {
    air_date.is_none_or(|date| date <= today)
}

#[derive(Clone)]
pub struct Views {
    queries: Queries,
    catalog: Option<Catalog>,
    clock: SharedClock,
}

impl Views {
    pub fn new(queries: Queries, catalog: Option<Catalog>, clock: SharedClock) -> Self {
        Self {
            queries,
            catalog,
            clock,
        }
    }

    fn today(&self) -> NaiveDate {
        date_of(self.clock.now())
    }

    /// Filters in memory, because the watched state needs the catalog episode counts.
    #[instrument(skip(self), err)]
    pub async fn shows(
        &self,
        filter: WatchFilter,
        search: &str,
        sort: SortOrder,
    ) -> Result<Vec<ShowListItem>> {
        let rows = self.queries.shows(search, sort).await?;
        let tmdb_ids: Vec<i64> = rows.iter().filter_map(|row| row.tmdb_id).collect();
        let totals = match &self.catalog {
            Some(catalog) => {
                catalog
                    .aired_episode_counts(&tmdb_ids, self.today())
                    .await?
            }
            None => Default::default(),
        };
        let items = rows.into_iter().map(|row| {
            let from_catalog = row
                .tmdb_id
                .and_then(|id| totals.get(&id).copied())
                .unwrap_or(0);
            ShowListItem {
                total_episodes: row.episode_count.max(from_catalog),
                show: row,
            }
        });
        Ok(match filter {
            WatchFilter::Watched => items
                .filter(|item| {
                    item.total_episodes > 0 && item.show.watched_count >= item.total_episodes
                })
                .collect(),
            WatchFilter::Unwatched => items
                .filter(|item| {
                    item.show.hidden_at.is_none()
                        && (item.show.watched_count < item.total_episodes
                            || item.total_episodes == 0)
                })
                .collect(),
            WatchFilter::All => items.collect(),
        })
    }

    #[instrument(skip(self), err)]
    pub async fn show(&self, id: Uuid) -> Result<Option<ShowDetail>> {
        let Some(show) = self.queries.show(id).await? else {
            return Ok(None);
        };
        let local = self.queries.episodes(id).await?;
        let from_catalog = match (show.tmdb_id.filter(|id| *id != 0), &self.catalog) {
            (Some(tmdb_id), Some(catalog)) => catalog.episodes(tmdb_id).await?,
            _ => Vec::new(),
        };
        let episodes = merge_episodes(&local, &from_catalog);
        let today = self.today();
        let total = if from_catalog.is_empty() {
            local.len()
        } else {
            from_catalog
                .iter()
                .filter(|episode| {
                    episode.season != SPECIALS_SEASON && has_aired(episode.air_date, today)
                })
                .count()
        };
        Ok(Some(ShowDetail {
            show: ShowListItem {
                show,
                total_episodes: total.max(local.len()) as i64,
            },
            episodes,
        }))
    }
}

pub fn merge_episodes(
    local: &[EpisodeView],
    from_catalog: &[CatalogEpisode],
) -> Vec<MergedEpisode> {
    let mut by_number: BTreeMap<(i32, i32), MergedEpisode> = BTreeMap::new();
    for episode in from_catalog {
        by_number.insert(
            (episode.season, episode.number),
            MergedEpisode {
                id: None,
                season: episode.season,
                number: episode.number,
                title: episode.title.clone(),
                aired_at: episode.air_date,
                duration_ms: millis(runtime_to_duration(episode.runtime_min)),
                play_count: 0,
                last_watched_at: None,
                position_ms: None,
                progress_state: None,
                tmdb_id: Some(episode.tmdb_id),
            },
        );
    }
    for view in local {
        let key = (view.season, view.number);
        let existing = by_number.get(&key);
        let merged = MergedEpisode {
            id: Some(view.id),
            season: view.season,
            number: view.number,
            title: view
                .title
                .clone()
                .or_else(|| existing.and_then(|e| e.title.clone())),
            aired_at: view.aired_at.or_else(|| existing.and_then(|e| e.aired_at)),
            duration_ms: view
                .duration_ms
                .or_else(|| existing.and_then(|e| e.duration_ms)),
            play_count: view.play_count,
            last_watched_at: view.last_watched_at,
            position_ms: view.position_ms,
            progress_state: view.progress_state,
            tmdb_id: view.tmdb_id.or_else(|| existing.and_then(|e| e.tmdb_id)),
        };
        by_number.insert(key, merged);
    }
    by_number.into_values().collect()
}
