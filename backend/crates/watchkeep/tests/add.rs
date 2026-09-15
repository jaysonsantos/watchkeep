mod common;

use axum::http::{Method, StatusCode};
use common::{call, get, id_of, json_request, test_context};
use eyre::Result;
use serde_json::json;
use watchkeep_storage::lists::{SortOrder, WatchFilter};

#[tokio::test]
async fn searches_the_catalog_and_adds_by_tmdb_id() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    let app = t.app();
    let (_, search) = get(&app, "/api/search?q=sever").await;
    assert_eq!(search["movies"].as_array().expect("array").len(), 0);
    assert_eq!(search["shows"][0]["tmdbId"], 95396);
    assert_eq!(search["shows"][0]["localId"], json!(null));

    let body = json!({ "tmdb_id": 95396, "watchlist": true });
    let (status, show) = call(&app, json_request(Method::POST, "/api/shows", &body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(show["title"], "Severance");
    assert_eq!(show["year"], 2022);
    assert_eq!(show["tvdb_id"], "371980");
    assert!(show["poster_path"].is_string());
    assert_eq!(show["created_at"], "2026-01-01T12:00:00Z");
    let show_id = id_of(&show);
    assert_eq!(t.queries.watchlist().await?[0].title, "Severance");
    assert_eq!(
        t.views.show(show_id).await?.expect("show").episodes.len(),
        6,
        "the catalog episode list is available at once"
    );

    let (_, again) = call(
        &app,
        json_request(Method::POST, "/api/shows", &json!({ "tmdb_id": 95396 })),
    )
    .await;
    assert_eq!(id_of(&again), show_id, "no duplicate");

    let (_, search) = get(&app, "/api/search?q=sever").await;
    assert_eq!(search["shows"][0]["title"], "Severance");
    assert_eq!(
        search["shows"][0]["localId"],
        show_id.to_string(),
        "the result shows that the library has it"
    );
    let (status, _) = call(
        &app,
        json_request(
            Method::POST,
            "/api/movies",
            &json!({ "tmdb_id": 123456789 }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    t.close().await
}

#[tokio::test]
async fn adds_by_title_without_a_catalog() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (status, _) = get(&app, "/api/search?q=heat").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body = json!({ "title": "Heat", "year": 1995 });
    let (status, _) = call(&app, json_request(Method::POST, "/api/movies", &body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        t.queries
            .movies(WatchFilter::Unwatched, "", SortOrder::Recent, None, 0)
            .await?
            .len(),
        1
    );

    let body = json!({ "title": "Pluribus", "year": 2025, "watchlist": true });
    let (status, _) = call(&app, json_request(Method::POST, "/api/shows", &body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let items = t.queries.watchlist().await?;
    assert_eq!(
        items
            .iter()
            .map(|item| (item.kind.as_str(), item.title.as_str()))
            .collect::<Vec<_>>(),
        [("show", "Pluribus")]
    );

    let (status, _) = call(
        &app,
        json_request(Method::POST, "/api/movies", &json!({ "title": "  " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, config) = get(&app, "/api/config").await;
    assert_eq!(config["catalog_configured"], false);
    t.close().await
}
