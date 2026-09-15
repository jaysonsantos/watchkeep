mod common;

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, ORIGIN};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use common::{
    call, episode_payload, get, id_of, json_request, missing_id, movie_payload, multipart,
    multipart_at, request, send, test_context,
};
use eyre::Result;
use serde_json::json;
use watchkeep::csrf::is_cross_site_form_post;
use watchkeep::http::header;
use watchkeep_storage::clock::Clock;
use watchkeep_storage::model::TargetKind;

#[tokio::test]
async fn rejects_a_wrong_token() -> Result<()> {
    let t = test_context(|config| config.webhook_token = "secret".to_owned(), false).await?;
    let app = t.app();
    let payload = episode_payload(json!({}), json!({}));
    let denied = send(
        &app,
        multipart_at(&format!("{}?token=wrong", common::WEBHOOK_PATH), &payload),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    let allowed = send(
        &app,
        multipart_at(&format!("{}?token=secret", common::WEBHOOK_PATH), &payload),
    )
    .await;
    assert_eq!(allowed.status(), StatusCode::OK);
    let missing = send(&app, multipart(&payload)).await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let mut with_header = multipart(&payload);
    with_header.headers_mut().insert(
        HeaderName::from_static(header::WEBHOOK_TOKEN),
        HeaderValue::from_static("secret"),
    );
    assert_eq!(
        send(&app, with_header).await.status(),
        StatusCode::OK,
        "the header works too"
    );
    t.close().await
}

#[tokio::test]
async fn accepts_the_multipart_form_plex_sends() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        multipart(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "play");
    assert_eq!(body["title"], "Severance S01E01 Pilot");
    assert_eq!(body["ok"], true);
    assert!(body["targetId"].is_string(), "the id is a UUID string");
    assert_eq!(t.queries.stats().await?.episodes_watched, 1);
    let log = t.queries.recent_webhooks(50).await?;
    assert_eq!(log[0].outcome, "play");
    assert_eq!(log[0].received_at, t.clock.now());
    t.close().await
}

#[tokio::test]
async fn accepts_plain_json() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let payload = movie_payload(
        json!({ "event": "media.pause" }),
        json!({ "viewOffset": 5_000_000 }),
    );
    let (status, body) = call(
        &app,
        json_request(Method::POST, common::WEBHOOK_PATH, &payload),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "progress");
    assert_eq!(body["percent"], 50.0);
    assert_eq!(body["positionMs"], 5_000_000);
    t.close().await
}

#[tokio::test]
async fn logs_and_ignores_untracked_events() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let payload =
        json!({ "event": "library.new", "Metadata": { "type": "movie", "title": "New" } });
    let (status, body) = call(&app, multipart(&payload)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert!(body["ignored"].is_string());
    assert!(
        t.queries.recent_webhooks(50).await?[0]
            .outcome
            .starts_with("ignored")
    );
    assert_eq!(t.queries.stats().await?.movies, 0);
    t.close().await
}

#[tokio::test]
async fn rejects_bodies_that_are_not_json() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let response = send(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri(common::WEBHOOK_PATH)
            .body(Body::from("not json"))
            .expect("request"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    t.close().await
}

#[tokio::test]
async fn serves_lists_details_and_manual_actions() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, played) = call(
        &app,
        multipart(&episode_payload(
            json!({ "event": "media.play" }),
            json!({}),
        )),
    )
    .await;
    let episode_id = played["targetId"]
        .as_str()
        .expect("an episode id")
        .to_owned();
    send(
        &app,
        multipart(&movie_payload(json!({ "event": "media.play" }), json!({}))),
    )
    .await;

    let (_, shows) = get(&app, "/api/shows").await;
    assert_eq!(shows.as_array().expect("array").len(), 1);
    assert_eq!(shows[0]["total_episodes"], 1);
    let show_id = id_of(&shows[0]);

    let (status, _) = call(
        &app,
        request(Method::POST, &format!("/api/episodes/{episode_id}/watched")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, show) = get(&app, &format!("/api/shows/{show_id}")).await;
    assert_eq!(show["watched_count"], 1);
    assert_eq!(show["on_watchlist"], false);

    let (_, movies) = get(&app, "/api/movies?status=unwatched").await;
    assert_eq!(movies.as_array().expect("array").len(), 1);
    let movie_id = id_of(&movies[0]);
    let (status, _) = call(
        &app,
        request(Method::POST, &format!("/api/movies/{movie_id}/watched")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &app,
        request(
            Method::POST,
            &format!("/api/movies/{}/watched", missing_id()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        request(Method::POST, &format!("/api/movies/{show_id}/watched")),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the id is a show, not a movie"
    );
    let (status, _) = call(&app, request(Method::POST, "/api/movies/1/watched")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "ids are UUIDs");
    let (_, history) = get(&app, "/api/history").await;
    assert_eq!(history["total"], 2);
    let (_, movie) = get(&app, &format!("/api/movies/{movie_id}")).await;
    assert_eq!(movie["play_count"], 1);
    assert!(movie["progress"].is_null(), "a play clears the position");

    let (status, _) = call(
        &app,
        request(Method::DELETE, &format!("/api/movies/{movie_id}/watched")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, stats) = get(&app, "/api/stats").await;
    assert_eq!(stats["movies_watched"], 0);

    let (status, _) = call(&app, request(Method::POST, "/api/sync")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, health) = get(&app, "/healthz").await;
    assert_eq!(health, json!({ "ok": true, "catalog": false }));
    let (status, _) = get(&app, "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    t.close().await
}

#[tokio::test]
async fn accepts_a_watched_at_time_for_manual_plays() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, played) = call(
        &app,
        multipart(&movie_payload(json!({ "event": "media.play" }), json!({}))),
    )
    .await;
    let movie_id = played["targetId"].as_str().expect("a movie id").to_owned();
    let (status, _) = call(
        &app,
        json_request(
            Method::POST,
            &format!("/api/movies/{movie_id}/watched"),
            &json!({ "watched_at": "2025-06-01T20:30:00Z" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let play = t
        .library()
        .await?
        .last_play(TargetKind::Movie, movie_id.parse()?)
        .await?
        .expect("play");
    assert_eq!(play.watched_at, common::at("2025-06-01T20:30:00Z"));
    assert_eq!(play.source, "manual");
    let (_, history) = get(&app, "/api/history").await;
    assert_eq!(history["items"][0]["watched_at"], "2025-06-01T20:30:00Z");
    t.close().await
}

#[tokio::test]
async fn serves_the_data_of_every_page() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, played) = call(
        &app,
        multipart(&episode_payload(
            json!({ "event": "media.play" }),
            json!({}),
        )),
    )
    .await;
    let episode_id = played["targetId"]
        .as_str()
        .expect("an episode id")
        .to_owned();

    let (_, config) = get(&app, "/api/config").await;
    assert_eq!(config["catalog_configured"], false);
    assert_eq!(config["sync_configured"], false);
    assert_eq!(config["image_base_url"], "https://image.tmdb.org/t/p/");
    let (_, history) = get(&app, "/api/history?limit=15").await;
    assert_eq!(history["items"].as_array().expect("array").len(), 0);
    let response = send(&app, request(Method::GET, "/api/movies?status=watched&q=x")).await;
    assert_eq!(response.headers()[header::TOTAL_COUNT], "0");
    let response = send(&app, request(Method::GET, "/api/shows?limit=60")).await;
    assert_eq!(response.headers()[header::TOTAL_COUNT], "1");
    let show_id = t.first_show().await?;
    let (_, show) = get(&app, &format!("/api/shows/{show_id}")).await;
    assert_eq!(show["title"], "Severance");
    let (status, _) = get(&app, &format!("/api/shows/{}", missing_id())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, events) = get(&app, "/api/webhooks").await;
    assert_eq!(events.as_array().expect("array").len(), 1);
    let (_, watchlist) = get(&app, "/api/watchlist").await;
    assert_eq!(watchlist, json!([]));
    let (_, progress) = get(&app, "/api/progress").await;
    assert_eq!(progress.as_array().expect("array").len(), 1);
    assert_eq!(progress[0]["target_id"], episode_id);
    let (status, _) = call(
        &app,
        request(
            Method::DELETE,
            &format!("/api/progress/episode/{episode_id}"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, progress) = get(&app, "/api/progress").await;
    assert_eq!(progress, json!([]));
    t.close().await
}

#[tokio::test]
async fn serves_the_web_ui_with_an_index_fallback() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("watchkeep-ui-{}", common::unique_name()));
    std::fs::create_dir_all(dir.join("_app"))?;
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>Watchkeep</title>",
    )?;
    std::fs::write(dir.join("_app").join("app.js"), "console.log(1)")?;
    let t = test_context(|config| config.static_dir = dir.clone(), false).await?;
    let app = t.app();
    let response = send(&app, request(Method::GET, "/_app/app.js")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = send(&app, request(Method::GET, "/shows/1")).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unknown paths get index.html for the SPA router"
    );
    assert!(
        response.headers()[CONTENT_TYPE]
            .to_str()?
            .starts_with("text/html")
    );
    std::fs::remove_dir_all(&dir)?;
    t.close().await
}

#[tokio::test]
async fn rejects_cross_site_form_posts_to_the_ui() -> Result<()> {
    let host = "watchkeep.test:8484";
    let form = "application/x-www-form-urlencoded";
    let headers = |pairs: &[(&str, &str)]| {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header"),
            );
        }
        headers
    };
    let post = Method::POST;
    assert!(
        is_cross_site_form_post(&post, &headers(&[("content-type", form)]), host),
        "no Origin header"
    );
    assert!(is_cross_site_form_post(
        &post,
        &headers(&[("content-type", form), ("origin", "null")]),
        host
    ));
    assert!(is_cross_site_form_post(
        &post,
        &headers(&[
            ("content-type", "multipart/form-data; boundary=x"),
            ("origin", "http://evil.example")
        ]),
        host
    ));
    assert!(
        is_cross_site_form_post(
            &post,
            &headers(&[("content-type", form), ("origin", "http://watchkeep.test")]),
            host
        ),
        "another port"
    );
    assert!(
        !is_cross_site_form_post(
            &post,
            &headers(&[
                ("content-type", form),
                ("origin", &format!("http://{host}"))
            ]),
            host
        ),
        "plain http behind the https guess"
    );
    assert!(!is_cross_site_form_post(
        &post,
        &headers(&[("content-type", "application/json")]),
        host
    ));
    assert!(!is_cross_site_form_post(&Method::GET, &headers(&[]), host));

    // Through the router: form posts to the API need a matching Origin, JSON posts do not.
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let path = format!("/api/movies/{}/watched", missing_id());
    let form_post = |origin: Option<&str>| {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(&path)
            .header("host", host)
            .header(CONTENT_TYPE, form);
        if let Some(origin) = origin {
            builder = builder.header(ORIGIN, origin);
        }
        builder.body(Body::from("a=1")).expect("request")
    };
    assert_eq!(
        send(&app, form_post(None)).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, form_post(Some("http://evil.example")))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, form_post(Some(&format!("http://{host}"))))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let json_post = json_request(Method::POST, &path, &json!({}));
    assert_eq!(send(&app, json_post).await.status(), StatusCode::NOT_FOUND);
    // Plex posts forms without an Origin header; the webhook is exempt.
    assert_eq!(
        send(&app, multipart(&movie_payload(json!({}), json!({}))))
            .await
            .status(),
        StatusCode::OK
    );
    t.close().await
}
