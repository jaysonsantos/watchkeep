//! The app context (pools, catalog, services) and the HTTP router.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use eyre::{Result, eyre};
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use serde::Serialize;
use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};
use tower_http::services::{ServeDir, ServeFile};
use tracing::Instrument;
use watchkeep_catalog::Catalog;
use watchkeep_storage::clock::{SharedClock, SystemClock};
use watchkeep_storage::db;
use watchkeep_storage::library::Library;
use watchkeep_storage::queries::Queries;
use watchkeep_telemetry::spawn;

use crate::actions::Actions;
use crate::config::Config;
use crate::csrf::{expected_host, is_cross_site_form_post};
use crate::http::{self, ApiResult};
use crate::plex::sync::{DEFAULT_PAGE_SIZE, SyncOptions, SyncReport, sync_plex_library};
use crate::scrobble::Scrobbler;
use crate::telemetry::{HttpMetrics, trace_request};
use crate::views::Views;

/// The path prefix of the Plex webhook. The origin check skips it.
const WEBHOOK_PREFIX: &str = "/webhook";
const API_PREFIX: &str = "/api";
pub const HEALTH_PATH: &str = "/healthz";

/// The file that the single-page app boots from.
const INDEX_FILE: &str = "index.html";

/// The name of the task that runs one Plex library sync. tokio-console shows it.
const SYNC_TASK: &str = "plex_sync";

/// The outcome of a library sync. The error is shared, because concurrent callers get the same result.
pub type SyncOutcome = Result<SyncReport, Arc<eyre::Report>>;
pub type SyncFuture = Shared<BoxFuture<'static, SyncOutcome>>;

pub struct AppContext {
    pub pool: PgPool,
    pub catalog_pool: Option<PgPool>,
    pub config: Arc<Config>,
    pub clock: SharedClock,
    pub catalog: Option<Catalog>,
    pub scrobbler: Scrobbler,
    pub queries: Queries,
    pub views: Views,
    pub actions: Actions,
    running_sync: Arc<Mutex<Option<SyncFuture>>>,
}

pub type SharedContext = Arc<AppContext>;

/// `GET /healthz`.
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct HealthResponse {
    pub ok: bool,
    pub catalog: bool,
}

impl AppContext {
    pub fn new(
        pool: PgPool,
        catalog_pool: Option<PgPool>,
        config: Config,
        clock: Option<SharedClock>,
    ) -> Self {
        let clock: SharedClock = clock.unwrap_or_else(|| Arc::new(SystemClock));
        let config = Arc::new(config);
        let catalog = catalog_pool
            .clone()
            .map(|pool| Catalog::new(pool, config.catalog_language));
        let queries = Queries::new(pool.clone());
        Self {
            scrobbler: Scrobbler::new(pool.clone(), config.clone(), catalog.clone(), clock.clone()),
            views: Views::new(queries.clone(), catalog.clone(), clock.clone()),
            actions: Actions::new(pool.clone(), catalog.clone(), clock.clone()),
            queries,
            pool,
            catalog_pool,
            config,
            clock,
            catalog,
            running_sync: Arc::new(Mutex::new(None)),
        }
    }

    /// A library on one pool connection, for reads and single writes outside a transaction.
    pub async fn library(&self) -> Result<Library<PoolConnection<Postgres>>> {
        Ok(Library::new(self.pool.acquire().await?, self.clock.clone()))
    }

    /// Run a Plex library sync. Callers that arrive while a sync runs share its result.
    /// The future fails when Plex is not configured.
    pub fn sync(&self) -> SyncFuture {
        let mut slot = self
            .running_sync
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(running) = slot.as_ref() {
            return running.clone();
        }
        let pool = self.pool.clone();
        let catalog = self.catalog.clone();
        let clock = self.clock.clone();
        let options = SyncOptions {
            plex_url: self.config.plex_url.clone(),
            plex_token: self.config.plex_token.clone(),
            page_size: DEFAULT_PAGE_SIZE,
        };
        let running_sync = self.running_sync.clone();
        // The task keeps the span of the caller, so that the sync stays in its trace.
        let task = async move {
            let result = sync_plex_library(&pool, catalog.as_ref(), &options, clock)
                .await
                .map_err(Arc::new);
            *running_sync
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            result
        }
        .instrument(tracing::Span::current());
        let handle = match spawn!(SYNC_TASK, task) {
            Ok(handle) => handle,
            Err(error) => {
                let report = Arc::new(eyre!(error));
                return async move { Err(report) }.boxed().shared();
            }
        };
        let future: SyncFuture = async move {
            handle
                .await
                .unwrap_or_else(|error| Err(Arc::new(eyre!(error))))
        }
        .boxed()
        .shared();
        *slot = Some(future.clone());
        future
    }

    pub async fn close(&self) {
        self.pool.close().await;
        if let Some(catalog_pool) = &self.catalog_pool {
            catalog_pool.close().await;
        }
    }
}

async fn healthz(State(ctx): State<SharedContext>) -> ApiResult {
    db::ping(&ctx.pool).await?;
    Ok(Json(HealthResponse {
        ok: true,
        catalog: ctx.catalog.is_some(),
    })
    .into_response())
}

/// Rejects cross-site form posts to the UI and the API. The webhook is exempt, because
/// Plex posts forms without an Origin header.
async fn origin_guard(State(ctx): State<SharedContext>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let is_webhook = path == WEBHOOK_PREFIX || path.starts_with(&format!("{WEBHOOK_PREFIX}/"));
    if !is_webhook {
        let host = expected_host(
            &ctx.config.http_origin,
            &ctx.config.host_header,
            request.headers(),
        );
        if is_cross_site_form_post(request.method(), request.headers(), &host) {
            return (
                StatusCode::FORBIDDEN,
                "Cross-site form submissions are forbidden",
            )
                .into_response();
        }
    }
    next.run(request).await
}

/// The built web UI: files from `static_dir`, and `index.html` for every other path.
fn static_service(dir: &Path) -> ServeDir<ServeFile> {
    ServeDir::new(dir).fallback(ServeFile::new(dir.join(INDEX_FILE)))
}

pub fn build_router(ctx: SharedContext) -> Router {
    let static_dir = ctx.config.static_dir.clone();
    Router::new()
        .route(HEALTH_PATH, get(healthz))
        .nest(WEBHOOK_PREFIX, http::webhook::routes())
        .nest(API_PREFIX, http::api::routes())
        .fallback_service(static_service(&static_dir))
        .layer(middleware::from_fn_with_state(ctx.clone(), origin_guard))
        // The span of the request wraps every other layer, and the matched path
        // of the router is the route label of the metrics.
        .layer(middleware::from_fn_with_state(
            HttpMetrics::new(),
            trace_request,
        ))
        .with_state(ctx)
}

/// Whether the web UI is present at the configured directory.
pub fn has_web_ui(dir: &Path) -> bool {
    dir.join(INDEX_FILE).is_file()
}
