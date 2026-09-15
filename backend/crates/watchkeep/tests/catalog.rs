mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{
    call, episode_payload, event, get, json_request, movie_payload, request, test_context,
};
use eyre::Result;
use serde_json::json;
use watchkeep_storage::lists::{SortOrder, WatchFilter};

#[tokio::test]
async fn fills_tmdb_ids_posters_and_runtimes_for_movies() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    t.scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.play" }),
            json!({ "Guid": [{ "id": "imdb://tt0369339" }], "title": "Collateral", "year": 2004, "duration": null }),
        )))
        .await?;
    let movies = t
        .queries
        .movies(WatchFilter::All, "", SortOrder::Recent, None, 0)
        .await?;
    let movie = &movies[0];
    assert_eq!(movie.tmdb_id, Some(4638));
    assert_eq!(movie.poster_path.as_deref(), Some("/collateral.jpg"));
    assert_eq!(
        movie.duration_ms,
        Some(Duration::from_secs(120 * 60).as_millis() as i64),
        "runtime comes from the catalog when Plex sends none"
    );
    t.close().await
}

#[tokio::test]
async fn matches_movies_by_title_and_year_when_plex_sends_no_ids() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    t.scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.play" }),
            json!({ "Guid": [], "guid": "local://1" }),
        )))
        .await?;
    let movies = t
        .queries
        .movies(WatchFilter::All, "", SortOrder::Recent, None, 0)
        .await?;
    assert_eq!(movies[0].tmdb_id, Some(949));
    t.close().await
}

#[tokio::test]
async fn resolves_the_show_from_an_episode_tmdb_id_and_keeps_ids_consistent() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    let shows = t.queries.shows("", SortOrder::Recent).await?;
    let show = &shows[0];
    assert_eq!(show.tmdb_id, Some(95396));
    assert_eq!(show.imdb_id.as_deref(), Some("tt11280740"));
    assert_eq!(show.tvdb_id.as_deref(), Some("371980"));
    assert_eq!(show.year, Some(2022));
    assert_eq!(
        show.poster_path.as_deref(),
        Some("/pPHpeI2X1qEd1CS1SeyrdhZ4qnT.jpg")
    );
    let episodes = t.queries.episodes(show.id).await?;
    assert_eq!(episodes[0].tmdb_id, Some(1982925));
    assert_eq!(
        episodes[0].aired_at.map(|date| date.to_string()).as_deref(),
        Some("2022-02-18"),
        "the Plex air date wins over the catalog"
    );

    // A second Plex server with different guids reports the same show by name only.
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({ "guid": "plex://episode/other", "grandparentGuid": "plex://show/other", "Guid": [], "index": 2, "title": null, "originallyAvailableAt": null }),
        )))
        .await?;
    assert_eq!(t.queries.shows("", SortOrder::Recent).await?.len(), 1);
    let episodes = t.queries.episodes(show.id).await?;
    assert_eq!(episodes.len(), 2);
    assert_eq!(
        episodes[1].title.as_deref(),
        Some("Half Loop"),
        "episode title comes from the catalog"
    );
    assert_eq!(
        episodes[1].aired_at.map(|date| date.to_string()).as_deref(),
        Some("2022-02-17"),
        "the air date comes from the catalog"
    );
    t.close().await
}

#[tokio::test]
async fn lists_unwatched_episodes_from_the_catalog_and_counts_only_aired_ones() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    let shows = t
        .views
        .shows(WatchFilter::All, "", SortOrder::Recent)
        .await?;
    let show = &shows[0];
    assert_eq!(show.show.watched_count, 1);
    assert_eq!(
        show.total_episodes, 4,
        "three season-1 episodes plus one aired season-2 episode"
    );
    assert_eq!(
        t.views
            .shows(WatchFilter::Unwatched, "", SortOrder::Recent)
            .await?
            .len(),
        1
    );
    assert_eq!(
        t.views
            .shows(WatchFilter::Watched, "", SortOrder::Recent)
            .await?
            .len(),
        0
    );
    let show_id = show.show.id;

    let detail = t.views.show(show_id).await?.expect("show");
    assert_eq!(detail.episodes.len(), 6);
    let half = detail
        .episodes
        .iter()
        .find(|e| e.season == 1 && e.number == 2)
        .expect("S01E02");
    assert_eq!(half.id, None);
    assert_eq!(half.title.as_deref(), Some("Half Loop"));

    assert!(
        t.actions
            .mark_episode_by_number(show_id, 1, 2, true)
            .await?
    );
    let after = t.views.show(show_id).await?.expect("show");
    let marked = after
        .episodes
        .iter()
        .find(|e| e.season == 1 && e.number == 2)
        .expect("S01E02");
    assert!(marked.id.is_some());
    assert_eq!(marked.play_count, 1);
    assert_eq!(marked.tmdb_id, Some(3396429));

    assert_eq!(
        t.actions.mark_show_watched(show_id, None).await?,
        Some(2),
        "the two remaining aired episodes"
    );
    assert_eq!(
        t.views
            .shows(WatchFilter::Watched, "", SortOrder::Recent)
            .await?
            .len(),
        1
    );
    let detail = t.views.show(show_id).await?.expect("show");
    assert_eq!(
        detail.episodes.iter().filter(|e| e.play_count > 0).count(),
        4
    );
    t.close().await
}

#[tokio::test]
async fn exposes_catalog_episodes_through_the_api() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    let app = t.app();
    let payload = episode_payload(json!({ "event": "media.play" }), json!({}));
    call(
        &app,
        json_request(Method::POST, common::WEBHOOK_PATH, &payload),
    )
    .await;
    let show_id = t.first_show().await?;
    let (_, show) = get(&app, &format!("/api/shows/{show_id}")).await;
    assert_eq!(show["total_episodes"], 4);
    assert_eq!(show["episodes"].as_array().expect("array").len(), 6);
    assert_eq!(
        show["episodes"][2]["aired_at"], "2022-02-17",
        "S01E02 comes from the catalog with its air date"
    );

    let (status, _) = call(
        &app,
        request(
            Method::POST,
            &format!("/api/shows/{show_id}/seasons/2/episodes/1/watched"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, show) = get(&app, &format!("/api/shows/{show_id}")).await;
    let episodes = show["episodes"].as_array().expect("array");
    assert!(
        episodes
            .iter()
            .any(|episode| episode["title"] == "Hello, Ms. Cobel")
    );
    assert_eq!(show["poster_path"], "/pPHpeI2X1qEd1CS1SeyrdhZ4qnT.jpg");
    let (status, _) = call(
        &app,
        request(
            Method::POST,
            &format!("/api/shows/{show_id}/seasons/1/episodes/3/watched"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        t.queries.show(show_id).await?.expect("show").watched_count,
        2
    );
    let (status, _) = call(
        &app,
        request(
            Method::DELETE,
            &format!("/api/shows/{show_id}/seasons/1/episodes/3/watched"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        t.queries.show(show_id).await?.expect("show").watched_count,
        1
    );
    t.close().await
}
