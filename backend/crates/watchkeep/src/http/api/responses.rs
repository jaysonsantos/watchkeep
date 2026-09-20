//! Response bodies of the API that are not plain rows.

use serde::Serialize;
use uuid::Uuid;
use watchkeep_storage::model::ProgressRow;
use watchkeep_storage::queries::{HistoryEntry, MovieView};

use crate::plex::sync::SyncReport;
use crate::views::ShowDetail;

/// `GET /api/config`: what the web UI needs to know about the server.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ServerConfig {
    pub image_base_url: String,
    pub sync_configured: bool,
    pub catalog_configured: bool,
}

/// `GET /api/history`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct HistoryPage {
    pub total: i64,
    pub items: Vec<HistoryEntry>,
}

/// `GET /api/movies/:id`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct MovieDetail {
    #[serde(flatten)]
    pub movie: MovieView,
    pub progress: Option<ProgressRow>,
}

/// `GET /api/shows/:id`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ShowPage {
    #[serde(flatten)]
    pub detail: ShowDetail,
    pub on_watchlist: bool,
}

/// A catalog item with the library id of the same item, when the library has it.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult<T> {
    #[serde(flatten)]
    pub item: T,
    pub local_id: Option<Uuid>,
}

/// `GET /api/search`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SearchResults {
    pub movies: Vec<SearchResult<watchkeep_catalog::CatalogMovie>>,
    pub shows: Vec<SearchResult<watchkeep_catalog::CatalogShow>>,
}

/// `POST /api/shows/:id/watched`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct EpisodesChanged {
    pub ok: bool,
    pub episodes_changed: i64,
}

/// `DELETE /api/shows/:id/watched`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PlaysRemoved {
    pub ok: bool,
    pub plays_removed: i64,
}

/// `POST /api/sync`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SyncResponse {
    pub ok: bool,
    pub report: SyncReport,
}

/// `GET /api/ratings/:kind/:id`. `null` means the item has no rating.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct RatingView {
    pub rating: Option<f64>,
}
