mod common;

use axum::http::Method;
use common::{TestContext, at, body_json, request, send, test_context, titles};
use eyre::Result;
use uuid::Uuid;
use watchkeep::actions::AddMediaInput;
use watchkeep::http::header;
use watchkeep_storage::lists::{SortOrder, WatchFilter};
use watchkeep_storage::model::{MediaKind, TargetKind};

async fn add(t: &TestContext, kind: MediaKind, title: &str, year: Option<i32>) -> Result<Uuid> {
    let row = t
        .actions
        .add_media(AddMediaInput {
            kind,
            tmdb_id: None,
            title: Some(title.to_owned()),
            year,
            watchlist: false,
        })
        .await?
        .expect("row");
    t.clock.advance_minutes(1);
    Ok(row.id)
}

async fn list(
    t: &TestContext,
    sort: SortOrder,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<String>> {
    let movies = t
        .queries
        .movies(WatchFilter::All, "", sort, limit, offset)
        .await?;
    Ok(movies.into_iter().map(|movie| movie.title).collect())
}

#[tokio::test]
async fn sorts_movies_and_returns_one_page_at_a_time() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let heat = add(&t, MediaKind::Movie, "Heat", Some(1995)).await?;
    let collateral = add(&t, MediaKind::Movie, "Collateral", Some(2004)).await?;
    let alien = add(&t, MediaKind::Movie, "Alien", Some(1979)).await?;
    assert!(
        heat < collateral && collateral < alien,
        "UUID v7 ids sort by creation time"
    );
    t.actions
        .mark_watched(TargetKind::Movie, heat, Some(at("2026-01-01T13:00:00Z")))
        .await?;
    t.actions
        .mark_watched(TargetKind::Movie, alien, Some(at("2025-06-01T00:00:00Z")))
        .await?;

    assert_eq!(
        list(&t, SortOrder::Recent, None, 0).await?,
        ["Heat", "Alien", "Collateral"]
    );
    assert_eq!(
        list(&t, SortOrder::Title, None, 0).await?,
        ["Alien", "Collateral", "Heat"]
    );
    assert_eq!(
        list(&t, SortOrder::Year, None, 0).await?,
        ["Collateral", "Heat", "Alien"]
    );
    assert_eq!(
        list(&t, SortOrder::Added, None, 0).await?,
        ["Alien", "Collateral", "Heat"]
    );
    assert_eq!(
        list(&t, SortOrder::Recent, Some(2), 1).await?,
        ["Alien", "Collateral"]
    );
    assert_eq!(t.queries.movie_count(WatchFilter::Watched, "").await?, 2);
    assert_eq!(t.queries.movie_count(WatchFilter::All, "coll").await?, 1);

    let app = t.app();
    let response = send(
        &app,
        request(Method::GET, "/api/movies?sort=title&limit=1&offset=1"),
    )
    .await;
    assert_eq!(response.headers()[header::TOTAL_COUNT], "3");
    assert_eq!(titles(&body_json(response).await), ["Collateral"]);
    t.close().await
}

#[tokio::test]
async fn sorts_shows_and_falls_back_to_the_newest_addition() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    add(&t, MediaKind::Show, "Severance", Some(2022)).await?;
    add(&t, MediaKind::Show, "Andor", Some(2022)).await?;

    let names = |shows: Vec<watchkeep::views::ShowListItem>| {
        shows
            .into_iter()
            .map(|show| show.show.title)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(
            t.views
                .shows(WatchFilter::All, "", SortOrder::Recent)
                .await?
        ),
        ["Andor", "Severance"]
    );
    assert_eq!(
        names(
            t.views
                .shows(WatchFilter::All, "", SortOrder::Title)
                .await?
        ),
        ["Andor", "Severance"]
    );
    assert_eq!(
        names(
            t.views
                .shows(WatchFilter::All, "", SortOrder::Added)
                .await?
        ),
        ["Andor", "Severance"]
    );

    let app = t.app();
    let response = send(
        &app,
        request(Method::GET, "/api/shows?sort=title&limit=1&offset=1"),
    )
    .await;
    assert_eq!(response.headers()[header::TOTAL_COUNT], "2");
    assert_eq!(titles(&body_json(response).await), ["Severance"]);
    t.close().await
}

#[tokio::test]
async fn returns_one_page_of_the_list() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    for index in 0..61 {
        add(&t, MediaKind::Movie, &format!("Movie {index}"), None).await?;
    }
    let app = t.app();
    let response = send(
        &app,
        request(Method::GET, "/api/movies?sort=title&q=Movie&limit=60"),
    )
    .await;
    assert_eq!(response.headers()[header::TOTAL_COUNT], "61");
    assert_eq!(
        body_json(response).await.as_array().expect("array").len(),
        60
    );
    let response = send(
        &app,
        request(
            Method::GET,
            "/api/movies?sort=title&q=Movie&limit=60&offset=60",
        ),
    )
    .await;
    assert_eq!(
        body_json(response).await.as_array().expect("array").len(),
        1
    );
    let response = send(&app, request(Method::GET, "/api/movies?limit=1000")).await;
    assert_eq!(
        body_json(response).await.as_array().expect("array").len(),
        61,
        "limit is capped at 500"
    );
    let response = send(&app, request(Method::GET, "/api/movies?status=nope")).await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "an unknown filter is an error"
    );
    t.close().await
}
