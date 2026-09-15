//! `/api/shows`: the list, one show with its episodes, adding, and the watched,
//! watchlist, and hidden toggles.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;
use watchkeep_storage::model::MediaKind;

use super::movies::add_media;
use super::{EpisodesChanged, ListQuery, PlaysRemoved, ShowPage, set_watchlist, with_total};
use crate::app::SharedContext;
use crate::http::{ApiResult, not_found, ok};

pub fn routes() -> Router<SharedContext> {
    Router::new()
        .route("/shows", get(list).post(add))
        .route("/shows/{id}", get(detail))
        .route("/shows/{id}/watched", post(watch_all).delete(unwatch_all))
        .route(
            "/shows/{id}/seasons/{season}/episodes/{number}/watched",
            post(watch_episode).delete(unwatch_episode),
        )
        .route("/shows/{id}/watchlist", post(list_add).delete(list_remove))
        .route("/shows/{id}/hidden", post(hide).delete(unhide))
}

/// The watched filter needs the catalog episode counts, so the page is cut from the full list.
async fn list(State(ctx): State<SharedContext>, Query(query): Query<ListQuery>) -> ApiResult {
    let window = query.window();
    let shows = ctx.views.shows(query.status, &query.q, query.sort).await?;
    let total = shows.len() as i64;
    let start = (window.offset as usize).min(shows.len());
    let end = window.limit.map_or(shows.len(), |limit| {
        (start + limit as usize).min(shows.len())
    });
    Ok(with_total(total, &shows[start..end]))
}

async fn detail(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    let Some(detail) = ctx.views.show(id).await? else {
        return Ok(not_found());
    };
    let on_watchlist = ctx
        .queries
        .is_on_watchlist(MediaKind::Show, detail.show.show.id)
        .await?;
    Ok(Json(ShowPage {
        detail,
        on_watchlist,
    })
    .into_response())
}

async fn add(State(ctx): State<SharedContext>, body: Bytes) -> ApiResult {
    add_media(&ctx, MediaKind::Show, &body).await
}

async fn watch_all(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    Ok(match ctx.actions.mark_show_watched(id, None).await? {
        Some(episodes_changed) => Json(EpisodesChanged {
            ok: true,
            episodes_changed,
        })
        .into_response(),
        None => not_found(),
    })
}

async fn unwatch_all(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    Ok(match ctx.actions.mark_show_unwatched(id).await? {
        Some(plays_removed) => Json(PlaysRemoved {
            ok: true,
            plays_removed,
        })
        .into_response(),
        None => not_found(),
    })
}

/// Mark an episode by number, for episodes that only the catalog knows.
async fn set_episode(
    ctx: &SharedContext,
    (id, season, number): (Uuid, i32, i32),
    watched: bool,
) -> ApiResult {
    Ok(
        if ctx
            .actions
            .mark_episode_by_number(id, season, number, watched)
            .await?
        {
            ok()
        } else {
            not_found()
        },
    )
}

async fn watch_episode(
    State(ctx): State<SharedContext>,
    Path(path): Path<(Uuid, i32, i32)>,
) -> ApiResult {
    set_episode(&ctx, path, true).await
}

async fn unwatch_episode(
    State(ctx): State<SharedContext>,
    Path(path): Path<(Uuid, i32, i32)>,
) -> ApiResult {
    set_episode(&ctx, path, false).await
}

async fn list_add(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_watchlist(&ctx, MediaKind::Show, id, true).await
}

async fn list_remove(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_watchlist(&ctx, MediaKind::Show, id, false).await
}

async fn set_hidden(ctx: &SharedContext, id: Uuid, hidden: bool) -> ApiResult {
    Ok(if ctx.actions.set_hidden(id, hidden).await? {
        ok()
    } else {
        not_found()
    })
}

async fn hide(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_hidden(&ctx, id, true).await
}

async fn unhide(State(ctx): State<SharedContext>, Path(id): Path<Uuid>) -> ApiResult {
    set_hidden(&ctx, id, false).await
}
