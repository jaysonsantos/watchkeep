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
use uuid::Uuid;
use watchkeep::http::header;
use watchkeep_storage::library::ProgressInput;
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
async fn a_late_play_keeps_a_newer_position() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "occurred_at": "2026-01-01T13:00:00Z",
            "position_ms": 600_000,
        }))),
    )
    .await;
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    // The sender delivers a watched event of the earlier play after the retry.
    let mut late = scrobble_movie(json!({
        "event": "watched",
        "occurred_at": "2026-01-01T12:10:00Z",
        "position_ms": null,
    }));
    late["event_id"] = other_id()["event_id"].clone();
    let (status, body) = call(&app, scrobble_request(&late)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "stale-event");
    let mut library = t.library().await?;
    assert_eq!(library.play_count(TargetKind::Movie, movie).await?, 0);
    let progress = library
        .get_progress(TargetKind::Movie, movie)
        .await?
        .expect("the newer position stays");
    assert_eq!(progress.position(), Duration::from_secs(600));
    assert_eq!(progress.updated_at, at("2026-01-01T13:00:00Z"));
    drop(library);
    t.close().await
}

#[tokio::test]
async fn a_retry_of_a_suppressed_play_stays_suppressed() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let watched = |id: &str, occurred_at: &str| {
        scrobble_request(&scrobble_movie(json!({
            "event_id": id,
            "event": "watched",
            "occurred_at": occurred_at,
            "position_ms": null,
        })))
    };
    let first = "01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b";
    let inside = "01926f3c-2d3e-7f40-9b5c-6d7e8f9a0b1c";
    let later = "01926f3d-3e4f-7051-8c6d-7e8f9a0b1c2d";

    let (_, body) = call(&app, watched(first, "2026-01-01T12:00:00Z")).await;
    assert_eq!(body["action"], "play");
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    // The rewatch window is 360 minutes, so this event adds no play.
    let (_, body) = call(&app, watched(inside, "2026-01-01T13:00:00Z")).await;
    assert_eq!(body["action"], "duplicate-play");

    // A real second play, far outside the window.
    let (_, body) = call(&app, watched(later, "2026-01-03T12:00:00Z")).await;
    assert_eq!(body["action"], "play");

    // The sender retries the suppressed event. It must stay suppressed, even
    // though the newest play is now younger than the event.
    let (status, body) = call(&app, watched(inside, "2026-01-01T13:00:00Z")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "duplicate-play");
    let mut library = t.library().await?;
    assert_eq!(library.play_count(TargetKind::Movie, movie).await?, 2);
    drop(library);
    t.close().await
}

#[tokio::test]
async fn a_late_unwatched_keeps_a_newer_position() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "watched",
            "occurred_at": "2026-01-01T12:00:00Z",
            "position_ms": null,
        }))),
    )
    .await;
    let movie = body["targetId"].as_str().expect("an id").parse()?;
    // A rewatch on the next day, far outside the rewatch window of the play.
    let mut newer = scrobble_movie(json!({
        "occurred_at": "2026-01-02T12:00:00Z",
        "position_ms": 600_000,
    }));
    newer["event_id"] = other_id()["event_id"].clone();
    let (_, newer_body) = call(&app, scrobble_request(&newer)).await;
    assert_eq!(newer_body["action"], "progress");

    // The unwatch is older than the position that the item has now.
    let (status, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event_id": "01926f3d-3e4f-7051-8c6d-7e8f9a0b1c2d",
            "event": "unwatched",
            "occurred_at": "2026-01-01T18:00:00Z",
            "position_ms": null,
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "stale-event");
    let mut library = t.library().await?;
    assert_eq!(library.play_count(TargetKind::Movie, movie).await?, 1);
    assert_eq!(
        library
            .get_progress(TargetKind::Movie, movie)
            .await?
            .expect("the newer position stays")
            .position(),
        Duration::from_secs(600)
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn movies_without_a_title_keep_their_own_row() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let by_id = |event_id: &str, tmdb: i64| {
        scrobble_request(&json!({
            "event_id": event_id,
            "event": "watched",
            "occurred_at": common::START,
            "client": "my-media-server",
            "media": { "type": "movie", "ids": { "tmdb": tmdb } }
        }))
    };
    let (_, first) = call(&app, by_id("01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b", 949)).await;
    let (_, second) = call(&app, by_id("01926f3c-2d3e-7f40-9b5c-6d7e8f9a0b1c", 4638)).await;
    assert_eq!(first["action"], "play");
    assert_eq!(second["action"], "play");
    assert_ne!(
        first["targetId"], second["targetId"],
        "two TMDB ids are two movies, even without a title"
    );
    assert_eq!(t.queries.stats().await?.movies, 2);
    t.close().await
}

/// Two requests for one item can overlap, so the order rule lives in the
/// statement. These are the two writes that the handler uses.
#[tokio::test]
async fn the_statements_refuse_to_overwrite_a_newer_position() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "occurred_at": "2026-01-01T13:00:00Z",
            "position_ms": 600_000,
        }))),
    )
    .await;
    let movie: Uuid = body["targetId"].as_str().expect("an id").parse()?;
    let mut library = t.library().await?;
    let write_at = |time: &str| ProgressInput {
        kind: TargetKind::Movie,
        id: movie,
        position: Duration::from_secs(1),
        duration: None,
        state: PlayState::Playing,
        account: None,
        player: None,
        updated_at: Some(at(time)),
    };

    assert!(
        library
            .set_progress_if_newer(write_at("2026-01-01T12:00:00Z"))
            .await?
            .is_none(),
        "an older write changes nothing"
    );
    assert_eq!(
        library
            .get_progress(TargetKind::Movie, movie)
            .await?
            .expect("progress")
            .position(),
        Duration::from_secs(600)
    );
    assert!(
        library
            .set_progress_if_newer(write_at("2026-01-01T14:00:00Z"))
            .await?
            .is_some(),
        "a newer write wins"
    );

    library
        .clear_progress_if_older(TargetKind::Movie, movie, at("2026-01-01T13:00:00Z"))
        .await?;
    assert!(
        library
            .get_progress(TargetKind::Movie, movie)
            .await?
            .is_some(),
        "an older clear keeps the newer position"
    );
    library
        .clear_progress_if_older(TargetKind::Movie, movie, at("2026-01-01T14:00:00Z"))
        .await?;
    assert!(
        library
            .get_progress(TargetKind::Movie, movie)
            .await?
            .is_none(),
        "a clear at the time of the position removes it"
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn a_position_does_not_re_open_a_watched_item() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "event": "watched",
            "occurred_at": "2026-01-01T12:00:00Z",
            "position_ms": null,
        }))),
    )
    .await;
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    // A late progress of the same play must not put the movie back in progress.
    let mut late = scrobble_movie(json!({
        "occurred_at": "2026-01-01T12:05:00Z",
        "position_ms": 600_000,
    }));
    late["event_id"] = other_id()["event_id"].clone();
    let (status, body) = call(&app, scrobble_request(&late)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "already-watched");
    let mut library = t.library().await?;
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
async fn a_retried_unwatched_keeps_a_newer_play() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let unwatch = scrobble_movie(json!({
        "event": "unwatched",
        "occurred_at": "2026-01-01T12:00:00Z",
        "position_ms": null,
    }));
    let (_, body) = call(&app, scrobble_request(&unwatch)).await;
    assert_eq!(body["action"], "unwatched");
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    // The viewer watches it again, and only then does the sender retry the unwatch.
    let mut watched = scrobble_movie(json!({
        "event": "watched",
        "occurred_at": "2026-01-02T20:00:00Z",
        "position_ms": null,
    }));
    watched["event_id"] = other_id()["event_id"].clone();
    let (_, body) = call(&app, scrobble_request(&watched)).await;
    assert_eq!(body["action"], "play");

    let (status, body) = call(&app, scrobble_request(&unwatch)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "stale-event");
    let mut library = t.library().await?;
    assert_eq!(
        library.play_count(TargetKind::Movie, movie).await?,
        1,
        "the retry must not delete the newer play"
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn refuses_a_position_that_is_not_a_position() -> Result<()> {
    let t = test_context(|config| config.watched_threshold_percent = 85.0, false).await?;
    let app = t.app();
    // Seed an above-threshold position, so a fallback would record a play.
    let (_, body) = call(
        &app,
        scrobble_request(&scrobble_movie(json!({
            "position_ms": 9_500_000,
            "duration_ms": HEAT_MS,
        }))),
    )
    .await;
    let movie = body["targetId"].as_str().expect("an id").parse()?;

    let mut unknown = scrobble_movie(json!({
        "event": "stop",
        "occurred_at": "2026-01-01T14:00:00Z",
        "position_ms": -1,
    }));
    unknown["event_id"] = other_id()["event_id"].clone();
    let (status, _) = call(&app, scrobble_request(&unknown)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut library = t.library().await?;
    assert_eq!(
        library.play_count(TargetKind::Movie, movie).await?,
        0,
        "a sentinel position must not reuse the stored one"
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn refuses_a_tmdb_id_that_names_nothing() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    for id in [json!(0), json!(-5), json!("not-a-number")] {
        let (status, _) = call(
            &app,
            scrobble_request(&json!({
                "event_id": "01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b",
                "event": "watched",
                "occurred_at": common::START,
                "client": "my-media-server",
                "media": { "type": "movie", "ids": { "tmdb": id } }
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{id} is not an identity"
        );
    }
    assert_eq!(t.queries.stats().await?.movies, 0);
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

    let (status, _) = call(
        &app,
        scrobble_request(&json!({
            "event_id": "01926f3b-1c2d-7e3f-8a4b-5c6d7e8f9a0b",
            "event": "watched",
            "occurred_at": common::START,
            "client": "my-media-server",
            "media": {
                "type": "episode",
                "season": 1,
                "number": 1,
                "ids": { "tmdb": 1982925 },
                "show": {}
            }
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "the show has no ids and no title"
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
