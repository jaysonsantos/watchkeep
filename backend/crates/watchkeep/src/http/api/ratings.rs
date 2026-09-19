//! `/api/ratings/:kind/:id`: the stored user rating of a movie, a show, or an episode.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;
use watchkeep_storage::model::{MAX_USER_RATING, MIN_USER_RATING, RatingKind, valid_user_rating};

use super::{RatingBody, RatingView, lenient_json};
use crate::app::SharedContext;
use crate::http::{ApiResult, bad_request, not_found, ok};

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/ratings/{kind}/{id}", get(read).post(set).delete(clear))
}

async fn read(
    State(ctx): State<SharedContext>,
    Path((kind, id)): Path<(RatingKind, Uuid)>,
) -> ApiResult {
    Ok(match ctx.actions.rating(kind, id).await? {
        Some(rating) => Json(RatingView { rating }).into_response(),
        None => not_found(),
    })
}

async fn set(
    State(ctx): State<SharedContext>,
    Path((kind, id)): Path<(RatingKind, Uuid)>,
    body: Bytes,
) -> ApiResult {
    let body: RatingBody = lenient_json(&body);
    let Some(rating) = body.rating else {
        return Ok(bad_request("rating is required"));
    };
    if !valid_user_rating(rating) {
        return Ok(bad_request(&format!(
            "rating must be between {MIN_USER_RATING} and {MAX_USER_RATING}"
        )));
    }
    Ok(if ctx.actions.set_rating(kind, id, rating).await? {
        ok()
    } else {
        not_found()
    })
}

async fn clear(
    State(ctx): State<SharedContext>,
    Path((kind, id)): Path<(RatingKind, Uuid)>,
) -> ApiResult {
    Ok(if ctx.actions.clear_rating(kind, id).await? {
        ok()
    } else {
        not_found()
    })
}
