//! `/api/movies`: the list, one movie, adding, and the watched and watchlist toggles.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;
use watchkeep_storage::model::{MediaKind, TargetKind};

use super::{
    AddMediaBody, ListQuery, MovieDetail, lenient_json, set_watched, set_watchlist, with_total,
};
use crate::actions::AddMediaInput;
use crate::app::SharedContext;
use crate::http::{ApiResult, bad_request, not_found};

pub fn routes() -> Router<SharedContext> {
    Router::new()
        .route("/movies", get(list).post(add))
        .route("/movies/{id}", get(detail))
        .route("/movies/{id}/watched", post(watch).delete(unwatch))
        .route("/movies/{id}/watchlist", post(list_add).delete(list_remove))
}

/// The number of matches before `limit` and `offset` travels in `X-Total-Count`.
async fn list(State(ctx): State<SharedContext>, Query(query): Query<ListQuery>) -> ApiResult {
    let window = query.window();
    let total = ctx.queries.movie_count(query.status, &query.q).await?;
    let items = ctx
        .queries
        .movies(
            query.status,
            &query.q,
            query.sort,
            window.limit,
            window.offset,
        )
        .await?;
    Ok(with_total(total, &items))
}

async fn detail(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    let Some(movie) = ctx.queries.movie(id).await? else {
        return Ok(not_found());
    };
    let progress = ctx
        .library()
        .await?
        .get_progress(TargetKind::Movie, movie.id)
        .await?;
    Ok(Json(MovieDetail { movie, progress }).into_response())
}

/// Body: `{ tmdb_id?, title?, year?, watchlist? }`. Returns the movie or show, created or existing.
pub async fn add_media(ctx: &SharedContext, kind: MediaKind, body: &Bytes) -> ApiResult {
    let body: AddMediaBody = lenient_json(body);
    let row = ctx
        .actions
        .add_media(AddMediaInput {
            kind,
            tmdb_id: body.tmdb_id,
            title: body.title,
            year: body.year,
            watchlist: body.watchlist == Some(true),
        })
        .await?;
    Ok(match row {
        Some(row) => (StatusCode::CREATED, Json(row)).into_response(),
        None => bad_request("title is required, or a tmdb_id that the catalog knows"),
    })
}

async fn add(State(ctx): State<SharedContext>, body: Bytes) -> ApiResult {
    add_media(&ctx, MediaKind::Movie, &body).await
}

async fn watch(State(ctx): State<SharedContext>, Path(id): Path<Uuid>, body: Bytes) -> ApiResult {
    set_watched(&ctx, TargetKind::Movie, id, Some(&body), true).await
}

async fn unwatch(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_watched(&ctx, TargetKind::Movie, id, None, false).await
}

async fn list_add(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_watchlist(&ctx, MediaKind::Movie, id, true).await
}

async fn list_remove(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_watchlist(&ctx, MediaKind::Movie, id, false).await
}
