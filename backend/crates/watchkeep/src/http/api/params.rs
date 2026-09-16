//! Query strings and request bodies of the API, as typed structs.

use axum::body::Bytes;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use watchkeep_storage::lists::{SortOrder, WatchFilter};

/// The largest `limit` a list route accepts.
pub const MAX_LIST_LIMIT: i64 = 500;

/// The `limit` of the history route, and of a list route that asks for `limit=0`.
pub const DEFAULT_PAGE_LIMIT: i64 = 50;

/// How many webhook events the log returns.
pub const WEBHOOK_LOG_LIMIT: i64 = 100;

/// How many movies and how many shows a catalog search returns.
pub const SEARCH_RESULT_LIMIT: i64 = 20;

/// Shorter search terms return no results.
pub const MIN_SEARCH_LENGTH: usize = 2;

/// `limit` and `offset` of a list route. Without `limit`, the list is complete.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub limit: Option<i64>,
    pub offset: i64,
}

fn clamp_limit(limit: i64) -> i64 {
    if limit <= 0 {
        DEFAULT_PAGE_LIMIT
    } else {
        limit.min(MAX_LIST_LIMIT)
    }
}

fn default_filter() -> WatchFilter {
    WatchFilter::DEFAULT
}

fn default_sort() -> SortOrder {
    SortOrder::DEFAULT
}

/// `/api/movies` and `/api/shows`.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_filter")]
    pub status: WatchFilter,
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_sort")]
    pub sort: SortOrder,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

impl ListQuery {
    pub fn window(&self) -> Window {
        Window {
            limit: self.limit.map(clamp_limit),
            offset: self.offset.unwrap_or(0).max(0),
        }
    }
}

/// `/api/history`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct HistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl HistoryQuery {
    pub fn window(&self) -> Window {
        Window {
            limit: Some(clamp_limit(self.limit.unwrap_or(DEFAULT_PAGE_LIMIT))),
            offset: self.offset.unwrap_or(0).max(0),
        }
    }
}

/// `/api/statistics`. `tz` is an IANA time zone name, for example `Europe/Berlin`.
/// An empty or unknown name falls back to UTC.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StatisticsQuery {
    pub tz: String,
}

/// `/api/search`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    pub q: String,
}

/// `POST /api/movies/:id/watched` and `POST /api/episodes/:id/watched`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WatchedBody {
    pub watched_at: Option<String>,
}

/// `POST /api/movies` and `POST /api/shows`. Every field may be absent, so the
/// TypeScript fields are optional, not nullable.
#[derive(Debug, Default, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(default)]
pub struct AddMediaBody {
    #[ts(optional)]
    pub tmdb_id: Option<i64>,
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub year: Option<i32>,
    #[ts(optional)]
    pub watchlist: Option<bool>,
}

/// Bodies are optional: a missing or invalid JSON body reads as the default.
pub fn lenient_json<T: DeserializeOwned + Default>(body: &Bytes) -> T {
    serde_json::from_slice(body).unwrap_or_default()
}
