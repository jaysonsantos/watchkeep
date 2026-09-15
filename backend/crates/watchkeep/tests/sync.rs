mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use common::{at, test_context};
use eyre::{Result, eyre};
use serde_json::{Value, json};
use watchkeep::plex::sync::{PlexClient, PlexItemType, SyncOptions, SyncReport};
use watchkeep_storage::lists::{SortOrder, WatchFilter};
use watchkeep_storage::model::{PlayState, TargetKind};

const TOKEN: &str = "tok";

/// A tiny fake Plex Media Server that answers the endpoints the sync uses.
/// Records the `X-Plex-Container-Start` header of every item request.
#[derive(Clone)]
struct FakePlex {
    calls: Arc<Mutex<Vec<String>>>,
}

fn shows() -> Vec<Value> {
    vec![
        json!({ "ratingKey": "10", "guid": "plex://show/show1", "type": "show", "title": "Severance", "year": 2022, "Guid": [{ "id": "tmdb://95396" }] }),
    ]
}

fn episodes() -> Vec<Value> {
    vec![
        json!({ "ratingKey": "11", "guid": "plex://episode/ep1", "grandparentRatingKey": "10", "grandparentGuid": "plex://show/show1", "grandparentTitle": "Severance", "parentIndex": 1, "index": 1, "title": "Good News About Hell", "duration": 3_400_000, "viewCount": 2, "lastViewedAt": 1_700_000_000, "originallyAvailableAt": "2022-02-18" }),
        json!({ "ratingKey": "12", "guid": "plex://episode/ep2", "grandparentRatingKey": "10", "grandparentGuid": "plex://show/show1", "grandparentTitle": "Severance", "parentIndex": 1, "index": 2, "title": "Half Loop", "duration": 3_000_000, "viewOffset": 600_000 }),
        json!({ "ratingKey": "13", "guid": "plex://episode/ep3", "grandparentRatingKey": "10", "grandparentGuid": "plex://show/show1", "grandparentTitle": "Severance", "parentIndex": 1, "index": 3, "title": "In Perpetuity", "duration": 3_000_000 }),
    ]
}

fn movies() -> Vec<Value> {
    vec![
        json!({ "ratingKey": "20", "guid": "plex://movie/heat", "type": "movie", "title": "Heat", "year": 1995, "duration": 10_000_000, "viewCount": 1, "lastViewedAt": 1_600_000_000, "Guid": [{ "id": "imdb://tt0113277" }] }),
        json!({ "ratingKey": "21", "guid": "plex://movie/collateral", "type": "movie", "title": "Collateral", "year": 2004, "duration": 7_000_000 }),
    ]
}

fn header_number(headers: &HeaderMap, name: &str, fallback: usize) -> usize {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("x-plex-token")
        .and_then(|value| value.to_str().ok())
        == Some(TOKEN)
}

async fn sections(headers: HeaderMap) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "MediaContainer": { "Directory": [
        { "key": "1", "type": "movie", "title": "Movies" },
        { "key": "2", "type": "show", "title": "TV" },
        { "key": "3", "type": "artist", "title": "Music" },
    ] } }))
    .into_response()
}

async fn items(
    State(fake): State<FakePlex>,
    Path(key): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let start = header_number(&headers, "x-plex-container-start", 0);
    let size = header_number(&headers, "x-plex-container-size", 500);
    fake.calls.lock().expect("calls").push(start.to_string());
    let items = match (key.as_str(), params.get("type").map(String::as_str)) {
        ("1", Some("1")) => movies(),
        ("2", Some("2")) => shows(),
        ("2", Some("4")) => episodes(),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let end = (start + size).min(items.len());
    let page: Vec<Value> = items[start.min(items.len())..end].to_vec();
    Json(json!({ "MediaContainer": { "size": page.len(), "totalSize": items.len(), "Metadata": page } })).into_response()
}

/// Start the fake server on a random port and return its base URL.
async fn fake_plex() -> Result<(String, FakePlex)> {
    let fake = FakePlex {
        calls: Arc::new(Mutex::new(Vec::new())),
    };
    let app = Router::new()
        .route("/library/sections", get(sections))
        .route("/library/sections/{key}/all", get(items))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("fake plex");
    });
    Ok((format!("http://{address}"), fake))
}

#[tokio::test]
async fn imports_the_library_with_watch_state() -> Result<()> {
    let (url, _) = fake_plex().await?;
    let t = test_context(
        |config| {
            config.plex_url = url.clone();
            config.plex_token = TOKEN.to_owned();
        },
        false,
    )
    .await?;
    let report = t.sync().await.map_err(|error| eyre!("{error}"))?;
    assert_eq!(
        report,
        SyncReport {
            sections: 2,
            movies: 2,
            shows: 1,
            episodes: 3,
            plays_imported: 2,
            progress_imported: 1
        }
    );

    let stats = t.queries.stats().await?;
    assert_eq!(stats.movies, 2);
    assert_eq!(stats.movies_watched, 1);
    assert_eq!(stats.episodes, 3);
    assert_eq!(stats.episodes_watched, 1);
    let watched = t
        .queries
        .movies(WatchFilter::Watched, "", SortOrder::Recent, None, 0)
        .await?;
    let play = t
        .library()
        .await?
        .last_play(TargetKind::Movie, watched[0].id)
        .await?
        .expect("play");
    assert_eq!(play.watched_at, at("2020-09-13T12:26:40Z"));

    let show = &t.queries.shows("", SortOrder::Recent).await?[0];
    assert_eq!(show.year, Some(2022));
    assert_eq!(show.tmdb_id, Some(95396));
    let episodes = t.queries.episodes(show.id).await?;
    assert_eq!(episodes[1].position_ms, Some(600_000));
    assert_eq!(episodes[1].progress_state, Some(PlayState::Stopped));
    assert_eq!(
        episodes[0].aired_at.map(|date| date.to_string()).as_deref(),
        Some("2022-02-18")
    );

    // A second sync changes nothing.
    let again = t.sync().await.map_err(|error| eyre!("{error}"))?;
    assert_eq!(again.plays_imported, 0);
    assert_eq!(t.queries.stats().await?.plays, 2);
    t.close().await
}

#[tokio::test]
async fn enriches_synced_items_from_the_catalog() -> Result<()> {
    let (url, _) = fake_plex().await?;
    let t = test_context(
        |config| {
            config.plex_url = url.clone();
            config.plex_token = TOKEN.to_owned();
        },
        true,
    )
    .await?;
    t.sync().await.map_err(|error| eyre!("{error}"))?;
    let movies = t
        .queries
        .movies(WatchFilter::All, "", SortOrder::Title, None, 0)
        .await?;
    assert_eq!(
        movies
            .iter()
            .map(|movie| (movie.title.as_str(), movie.tmdb_id))
            .collect::<Vec<_>>(),
        [("Collateral", Some(4638)), ("Heat", Some(949))]
    );
    let show = &t.queries.shows("", SortOrder::Recent).await?[0];
    assert_eq!(show.imdb_id.as_deref(), Some("tt11280740"));
    assert_eq!(t.queries.episodes(show.id).await?[2].tmdb_id, Some(3396430));
    t.close().await
}

#[tokio::test]
async fn paginates_with_the_container_headers() -> Result<()> {
    let (url, fake) = fake_plex().await?;
    let client = PlexClient::new(SyncOptions {
        plex_url: url,
        plex_token: TOKEN.to_owned(),
        page_size: 2,
    });
    let episodes = client.items("2", PlexItemType::Episode).await?;
    assert_eq!(episodes.len(), 3);
    assert_eq!(*fake.calls.lock().expect("calls"), ["0", "2"]);
    Ok(())
}

#[tokio::test]
async fn fails_clearly_when_plex_is_not_configured() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let error = t.sync().await.expect_err("sync must fail");
    assert!(error.to_string().contains("WATCHKEEP_PLEX_URL"), "{error}");
    assert!(t.config.sync_interval.is_zero());
    assert_eq!(t.config.rewatch_window, Duration::from_secs(360 * 60));
    t.close().await
}
