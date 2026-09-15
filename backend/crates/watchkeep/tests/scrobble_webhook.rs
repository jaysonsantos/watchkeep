//! `POST /webhook/scrobble`: the generic scrobble endpoint.

mod common;

use std::time::Duration;

use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use common::{
    at, call, json_request, scrobble_episode, scrobble_movie, scrobble_request, send, test_context,
};
use eyre::Result;
use serde_json::{Value, json};
use watchkeep::http::header;
use watchkeep_storage::model::{PlayState, TargetKind};

/// The runtime of Heat in the catalog is 170 minutes.
const HEAT_MS: i64 = 170 * 60 * 1_000;

/// A second event of the same play, so that the idempotency check does not fire.
fn other_id() -> Value {
    json!({ "event_id": "01926f3c-2d3e-7f40-9b5c-6d7e8f9a0b1c" })
}

#[tokio::test]
async fn saves_the_position_for_start_progress_and_pause() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "start",
            "position_ms": 0,
            "duration_ms": HEAT_MS,
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "progress");
    assert_eq!(body["title"], "Heat (1995)");
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "progress",
            "occurred_at": "2026-01-01T12:30:00Z",
            "position_ms": 5_100_000,
            "duration_ms": HEAT_MS,
        }))),
    )
    .await;
    assert_eq!(body["percent"], 50.0);
    assert_eq!(body["positionMs"], 5_100_000);

    call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "pause",
            "occurred_at": "2026-01-01T12:40:00Z",
            "position_ms": 6_000_000,
        }))),
    )
    .await;
    let mut library = t.library().await?;
    let progress = library
        .get_progress(TargetKind::Movie, movie)
        .await?
        .expect("progress");
    assert_eq!(progress.state, PlayState::Paused);
    assert_eq!(progress.position(), Duration::from_secs(6_000));
    assert_eq!(progress.updated_at, at("2026-01-01T12:40:00Z"));
    assert_eq!(progress.account.as_deref(), Some("jayson"));
    assert_eq!(progress.player.as_deref(), Some("Living room TV"));
    drop(library);
    t.close().await
}

#[tokio::test]
async fn a_stop_past_the_threshold_records_a_play() -> Result<()> {
    let t = test_context(|config| config.watched_threshold_percent = 85.0, false).await?;
    let app = t.app();
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "stop",
            "position_ms": 1_000_000,
            "duration_ms": HEAT_MS,
        }))),
    )
    .await;
    assert_eq!(body["action"], "progress", "below the threshold");
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "stop",
            "occurred_at": "2026-01-01T14:00:00Z",
            "position_ms": 10_000_000,
            "duration_ms": HEAT_MS,
        }
        ))),
    )
    .await;
    assert_eq!(body["action"], "play");
    let mut library = t.library().await?;
    assert_eq!(library.play_count(TargetKind::Movie, movie).await?, 1);
    let play = library
        .last_play(TargetKind::Movie, movie)
        .await?
        .expect("play");
    assert_eq!(play.source, "scrobble");
    assert_eq!(play.watched_at, at("2026-01-01T14:00:00Z"));
    assert_eq!(play.player.as_deref(), Some("Living room TV"));
    assert!(
        library
            .get_progress(TargetKind::Movie, movie)
            .await?
            .is_none()
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn a_watched_event_records_a_play_at_its_own_time() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "watched",
            "watched_at": "2025-12-30T21:30:00Z",
            "position_ms": null,
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "play");
    let movie = body["targetId"].as_str().expect("an id").parse()?;
    let mut library = t.library().await?;
    let play = library
        .last_play(TargetKind::Movie, movie)
        .await?
        .expect("play");
    assert_eq!(play.watched_at, at("2025-12-30T21:30:00Z"));
    assert_eq!(
        play.external_id.as_deref(),
        Some("01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b")
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn the_same_event_id_adds_one_play() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let watched = scrobble_movie(json!({ "event": "watched", "position_ms": null }));
    let (_, first) = call(&app, scrobble_request(&watched)).await;
    assert_eq!(first["action"], "play");
    let movie = first["targetId"].as_str().expect("an id").parse()?;

    // The sender saw no answer and delivers the same event again, outside the rewatch window.
    t.clock.advance_minutes(24 * 60);
    let (status, again) = call(&app, scrobble_request(&watched)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["action"], "duplicate-event");
    let mut library = t.library().await?;
    assert_eq!(library.play_count(TargetKind::Movie, movie).await?, 1);
    drop(library);
    t.close().await
}

#[tokio::test]
async fn an_older_event_does_not_replace_a_newer_position() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "occurred_at": "2026-01-01T13:00:00Z",
            "position_ms": 6_000_000,
        }))),
    )
    .await;
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    let mut late = scrobble_movie(json!({
        "occurred_at": "2026-01-01T12:10:00Z",
        "position_ms": 60_000,
    }));
    late["event_id"] = other_id()["event_id"].clone();
    let (status, body) = call(&app, scrobble_request(&late)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "stale-event");
    assert_eq!(body["positionMs"], 6_000_000);
    let mut library = t.library().await?;
    let progress = library
        .get_progress(TargetKind::Movie, movie)
        .await?
        .expect("progress");
    assert_eq!(progress.position(), Duration::from_secs(6_000));
    assert_eq!(progress.updated_at, at("2026-01-01T13:00:00Z"));
    drop(library);
    t.close().await
}

#[tokio::test]
async fn the_body_duration_wins_over_the_catalog_runtime() -> Result<()> {
    let t = test_context(|config| config.watched_threshold_percent = 85.0, true).await?;
    let app = t.app();
    // 3 600 000 ms of the 10 200 000 ms that the catalog reports is 35 %.
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "stop",
            "position_ms": 3_600_000,
        }))),
    )
    .await;
    assert_eq!(body["action"], "progress");
    assert_eq!(body["percent"], 35.3);

    // The same position is 90 % of the runtime that the player measured.
    let mut cut = scrobble_movie(json!({
        "event": "stop",
        "occurred_at": "2026-01-01T13:00:00Z",
        "position_ms": 3_600_000,
        "duration_ms": 4_000_000,
    }));
    cut["event_id"] = other_id()["event_id"].clone();
    let (_, body) = call(&app, scrobble_request(&cut)).await;
    assert_eq!(body["action"], "play");
    t.close().await
}

#[tokio::test]
async fn tracks_an_episode_by_its_show_ids() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        scrobble_request(&scrobble_episode(json!({
            "event": "watched",
            "position_ms": null,
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "play");
    assert_eq!(body["title"], "Severance S01E01 Good News About Hell");
    assert_eq!(t.queries.stats().await?.episodes_watched, 1);

    let unwatch = scrobble_episode(json!({
        "event": "unwatched",
        "event_id": other_id()["event_id"],
        "position_ms": null,
    }));
    let (status, body) = call(&app, scrobble_request(&unwatch)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "unwatched");
    assert_eq!(t.queries.stats().await?.episodes_watched, 0);
    t.close().await
}

#[tokio::test]
async fn writes_every_event_to_the_webhook_log() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    call(&app, scrobble_request(&scrobble_movie(json!({})))).await;
    let log = t.queries.recent_webhooks(50).await?;
    assert_eq!(log[0].event, "scrobble.progress");
    assert_eq!(log[0].outcome, "progress");
    assert_eq!(log[0].media_type.as_deref(), Some("movie"));
    assert_eq!(log[0].title.as_deref(), Some("Heat (1995)"));
    assert_eq!(log[0].player.as_deref(), Some("Living room TV"));
    t.close().await
}

#[tokio::test]
async fn skips_accounts_that_are_not_accepted() -> Result<()> {
    let t = test_context(
        |config| config.scrobble_accounts = vec!["someone-else".to_owned()],
        false,
    )
    .await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        scrobble_request(&scrobble_movie(
            json!({ "event": "watched", "position_ms": null }),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "ignored-account");
    assert_eq!(t.queries.stats().await?.movies, 0);
    t.close().await
}

#[tokio::test]
async fn rejects_a_wrong_token() -> Result<()> {
    let t = test_context(|config| config.webhook_token = "secret".to_owned(), false).await?;
    let app = t.app();
    let body = scrobble_movie(json!({}));
    assert_eq!(
        send(&app, scrobble_request(&body)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            json_request(
                Method::POST,
                &format!("{}?token=secret", common::SCROBBLE_PATH),
                &body
            )
        )
        .await
        .status(),
        StatusCode::OK
    );
    let mut with_header = scrobble_request(&body);
    with_header.headers_mut().insert(
        HeaderName::from_static(header::WEBHOOK_TOKEN),
        HeaderValue::from_static("secret"),
    );
    assert_eq!(send(&app, with_header).await.status(), StatusCode::OK);
    t.close().await
}

#[tokio::test]
async fn refuses_bodies_that_cannot_be_applied() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();

    let (status, _) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({ "position_ms": null }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "progress needs a position");

    let (status, _) = call(
        &app,
        scrobble_request(&json!({
            "event_id": "01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b",
            "event": "watched",
            "occurred_at": common::START,
            "client": "my-media-server",
            "media": { "type": "episode", "season": 1, "number": 1, "ids": { "tmdb": 1982925 } }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "an episode needs its show");

    let (status, _) = call(
        &app,
        scrobble_request(&json!({
            "event_id": "01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b",
            "event": "watched",
            "occurred_at": common::START,
            "client": "my-media-server",
            "media": { "type": "movie" }
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "no ids and no title"
    );
    assert!(
        t.queries.recent_webhooks(50).await?[0]
            .outcome
            .starts_with("invalid"),
        "the log keeps the refused events"
    );

    let (status, _) = call(&app, scrobble_request(&json!({ "event": "start" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "missing fields");

    let mut form = scrobble_request(&scrobble_movie(json!({})));
    form.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    assert_eq!(
        send(&app, form).await.status(),
        StatusCode::BAD_REQUEST,
        "JSON only"
    );
    t.close().await
}
