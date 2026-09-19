//! The JSON API under `/api`. Each submodule owns one resource and its routes.

mod dashboard;
mod episodes;
mod history;
mod movies;
mod params;
mod ratings;
mod recommendations;
mod responses;
mod search;
mod shows;
mod statistics;
mod sync;
mod watchlist;

use axum::Router;
use axum::body::Bytes;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use uuid::Uuid;
use watchkeep_storage::clock::parse_iso;
use watchkeep_storage::model::{MediaKind, TargetKind};

use super::{ApiResult, header, not_found, ok};
use crate::app::SharedContext;

pub use params::*;
pub use responses::*;

pub fn routes() -> Router<SharedContext> {
    Router::new()
        .merge(dashboard::routes())
        .merge(episodes::routes())
        .merge(history::routes())
        .merge(movies::routes())
        .merge(ratings::routes())
        .merge(recommendations::routes())
        .merge(search::routes())
        .merge(shows::routes())
        .merge(statistics::routes())
        .merge(sync::routes())
        .merge(watchlist::routes())
        .fallback(|| async { not_found() })
}

/// A list body with the number of matches before `limit` and `offset` in a header.
fn with_total<T: Serialize + ?Sized>(total: i64, items: &T) -> Response {
    (
        [(header::TOTAL_COUNT, total.to_string())],
        axum::Json(items),
    )
        .into_response()
}

/// Mark a movie or an episode watched (with an optional `watched_at`) or unwatched.
async fn set_watched(
    ctx: &SharedContext,
    kind: TargetKind,
    id: Uuid,
    body: Option<&Bytes>,
    watched: bool,
) -> ApiResult {
    let changed = if watched {
        let watched_at = body
            .map(lenient_json::<WatchedBody>)
            .and_then(|body| body.watched_at)
            .and_then(|text| parse_iso(&text));
        ctx.actions.mark_watched(kind, id, watched_at).await?
    } else {
        ctx.actions.mark_unwatched(kind, id).await?
    };
    Ok(if changed { ok() } else { not_found() })
}

async fn set_watchlist(ctx: &SharedContext, kind: MediaKind, id: Uuid, listed: bool) -> ApiResult {
    Ok(if ctx.actions.set_watchlist(kind, id, listed).await? {
        ok()
    } else {
        not_found()
    })
}
