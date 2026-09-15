//! `/api/search`: catalog search by title.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use watchkeep_storage::model::MediaKind;

use super::{MIN_SEARCH_LENGTH, SEARCH_RESULT_LIMIT, SearchQuery, SearchResult, SearchResults};
use crate::app::SharedContext;
use crate::http::{ApiResult, bad_request};

pub fn routes() -> Router<SharedContext> {
    Router::new().route("/search", get(search))
}

/// Every result carries the local id of the same item when the library has it.
async fn search(State(ctx): State<SharedContext>, Query(query): Query<SearchQuery>) -> ApiResult {
    let query = query.q.trim();
    let Some(catalog) = &ctx.catalog else {
        return Ok(bad_request(
            "No catalog configured. Set WATCHKEEP_CATALOG_DATABASE_URL.",
        ));
    };
    if query.chars().count() < MIN_SEARCH_LENGTH {
        return Ok(Json(SearchResults {
            movies: Vec::new(),
            shows: Vec::new(),
        })
        .into_response());
    }
    let (movies, shows) = tokio::try_join!(
        catalog.search_movies(query, SEARCH_RESULT_LIMIT),
        catalog.search_shows(query, SEARCH_RESULT_LIMIT)
    )?;
    let movie_ids: Vec<i64> = movies.iter().map(|movie| movie.tmdb_id).collect();
    let show_ids: Vec<i64> = shows.iter().map(|show| show.tmdb_id).collect();
    let local_movies = ctx
        .queries
        .local_by_tmdb(MediaKind::Movie, &movie_ids)
        .await?;
    let local_shows = ctx
        .queries
        .local_by_tmdb(MediaKind::Show, &show_ids)
        .await?;
    Ok(Json(SearchResults {
        movies: movies
            .into_iter()
            .map(|movie| SearchResult {
                local_id: local_movies.get(&movie.tmdb_id).copied(),
                item: movie,
            })
            .collect(),
        shows: shows
            .into_iter()
            .map(|show| SearchResult {
                local_id: local_shows.get(&show.tmdb_id).copied(),
                item: show,
            })
            .collect(),
    })
    .into_response())
}
