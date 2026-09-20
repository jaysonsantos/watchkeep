mod common;

use axum::http::{Method, StatusCode};
use common::{
    TestContext, call, episode_payload, get, json_request, missing_id, movie_payload, multipart,
    request, test_context,
};
use eyre::Result;
use serde_json::json;
use watchkeep::actions::AddMediaInput;
use watchkeep_storage::model::{
    MAX_USER_RATING, MIN_USER_RATING, MediaKind, RatingKind, valid_user_rating,
};

#[test]
fn accepts_the_scale_and_rejects_the_rest() {
    assert!(valid_user_rating(MIN_USER_RATING));
    assert!(valid_user_rating(MAX_USER_RATING));
    assert!(valid_user_rating(8.5));
    assert!(!valid_user_rating(MIN_USER_RATING - 1.0));
    assert!(!valid_user_rating(MAX_USER_RATING + 1.0));
    assert!(!valid_user_rating(f64::NAN));
    assert!(!valid_user_rating(f64::INFINITY));
}

async fn add_movie(t: &TestContext, title: &str) -> Result<uuid::Uuid> {
    let row = t
        .actions
        .add_media(AddMediaInput {
            kind: MediaKind::Movie,
            tmdb_id: None,
            title: Some(title.to_owned()),
            year: Some(1995),
            watchlist: false,
        })
        .await?
        .expect("movie");
    Ok(row.id)
}

async fn add_show(t: &TestContext, title: &str) -> Result<uuid::Uuid> {
    let row = t
        .actions
        .add_media(AddMediaInput {
            kind: MediaKind::Show,
            tmdb_id: None,
            title: Some(title.to_owned()),
            year: Some(2022),
            watchlist: false,
        })
        .await?
        .expect("show");
    Ok(row.id)
}

#[tokio::test]
async fn reads_sets_and_clears_a_movie_rating() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let movie = add_movie(&t, "Heat").await?;
    let path = format!("/api/ratings/movie/{movie}");

    let (_, body) = get(&app, &path).await;
    assert!(body["rating"].is_null(), "no rating yet");

    let (status, _) = call(
        &app,
        json_request(Method::POST, &path, &json!({ "rating": 8 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&app, &path).await;
    assert_eq!(body["rating"], 8.0);
    let (_, movie_body) = get(&app, &format!("/api/movies/{movie}")).await;
    assert_eq!(
        movie_body["rating"], 8.0,
        "the movie view reads the same store"
    );

    let (status, _) = call(&app, request(Method::DELETE, &path)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&app, &path).await;
    assert!(body["rating"].is_null());
    t.close().await
}

#[tokio::test]
async fn reads_sets_and_clears_a_show_rating() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let show = add_show(&t, "Severance").await?;
    let path = format!("/api/ratings/show/{show}");

    let (status, _) = call(
        &app,
        json_request(Method::POST, &path, &json!({ "rating": 10 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&app, &path).await;
    assert_eq!(body["rating"], 10.0);
    let (_, show_body) = get(&app, &format!("/api/shows/{show}")).await;
    assert_eq!(show_body["rating"], 10.0);

    let (status, _) = call(&app, request(Method::DELETE, &path)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&app, &path).await;
    assert!(body["rating"].is_null());
    t.close().await
}

#[tokio::test]
async fn reads_sets_and_clears_an_episode_rating() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (_, played) = call(&app, multipart(&episode_payload(json!({}), json!({})))).await;
    let episode = played["targetId"]
        .as_str()
        .expect("an episode id")
        .to_owned();
    let show = t.first_show().await?;
    let path = format!("/api/ratings/episode/{episode}");

    let (status, _) = call(
        &app,
        json_request(Method::POST, &path, &json!({ "rating": 7 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&app, &path).await;
    assert_eq!(body["rating"], 7.0);
    let (_, show_body) = get(&app, &format!("/api/shows/{show}")).await;
    assert_eq!(show_body["episodes"][0]["rating"], 7.0);
    assert!(
        show_body["rating"].is_null(),
        "an episode rating is not a show rating"
    );

    let (status, _) = call(&app, request(Method::DELETE, &path)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = get(&app, &path).await;
    assert!(body["rating"].is_null());
    t.close().await
}

#[tokio::test]
async fn plex_media_rate_writes_the_same_store() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        multipart(&movie_payload(
            json!({ "event": "media.rate", "rating": 9 }),
            json!({}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["action"], "rating");
    let movie = body["targetId"].as_str().expect("a movie id");
    let (_, rating) = get(&app, &format!("/api/ratings/movie/{movie}")).await;
    assert_eq!(rating["rating"], 9.0);

    let (status, body) = call(
        &app,
        multipart(&episode_payload(
            json!({ "event": "media.rate", "rating": 6 }),
            json!({}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let episode = body["targetId"].as_str().expect("an episode id");
    let (_, rating) = get(&app, &format!("/api/ratings/episode/{episode}")).await;
    assert_eq!(rating["rating"], 6.0);
    t.close().await
}

#[tokio::test]
async fn plex_media_rate_rejects_a_rating_outside_the_scale() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let (status, body) = call(
        &app,
        multipart(&movie_payload(
            json!({ "event": "media.rate", "rating": 8 }),
            json!({}),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let movie = body["targetId"].as_str().expect("a movie id");

    for rating in [MIN_USER_RATING - 1.0, MAX_USER_RATING + 1.0] {
        let (status, response) = call(
            &app,
            multipart(&movie_payload(
                json!({ "event": "media.rate", "rating": rating }),
                json!({}),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{rating}");
        assert!(
            response["error"]
                .as_str()
                .expect("error")
                .contains("rating")
        );
        let (_, stored) = get(&app, &format!("/api/ratings/movie/{movie}")).await;
        assert_eq!(
            stored["rating"], 8.0,
            "an out-of-range Plex rating must not overwrite a stored rating"
        );
    }
    t.close().await
}

#[tokio::test]
async fn rejects_a_rating_outside_the_scale() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let movie = add_movie(&t, "Heat").await?;
    let path = format!("/api/ratings/movie/{movie}");

    for body in [
        json!({}),
        json!({ "rating": MIN_USER_RATING - 1.0 }),
        json!({ "rating": MAX_USER_RATING + 1.0 }),
    ] {
        let (status, response) = call(&app, json_request(Method::POST, &path, &body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            response["error"]
                .as_str()
                .expect("error")
                .contains("rating")
        );
    }

    let (status, _) = call(
        &app,
        json_request(Method::POST, &path, &json!({ "rating": MAX_USER_RATING })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "10 is on the scale");
    let (status, _) = call(
        &app,
        json_request(Method::POST, &path, &json!({ "rating": MIN_USER_RATING })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "0 is on the scale");
    t.close().await
}

#[tokio::test]
async fn missing_items_and_wrong_kinds_are_not_found() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    let movie = add_movie(&t, "Heat").await?;
    let show = add_show(&t, "Severance").await?;
    let missing = missing_id();

    let (status, _) = get(&app, &format!("/api/ratings/movie/{missing}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        json_request(
            Method::POST,
            &format!("/api/ratings/movie/{missing}"),
            &json!({ "rating": 8 }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        request(Method::DELETE, &format!("/api/ratings/show/{missing}")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        json_request(
            Method::POST,
            &format!("/api/ratings/movie/{show}"),
            &json!({ "rating": 8 }),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a show id is not a movie rating"
    );
    let (status, _) = call(
        &app,
        json_request(
            Method::POST,
            &format!("/api/ratings/show/{movie}"),
            &json!({ "rating": 8 }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&app, &format!("/api/ratings/season/{movie}")).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "kind is movie, show, or episode"
    );
    t.close().await
}

#[tokio::test]
async fn library_get_rating_matches_the_api() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let movie = add_movie(&t, "Heat").await?;
    t.actions.set_rating(RatingKind::Movie, movie, 9.0).await?;
    assert_eq!(
        t.library()
            .await?
            .get_rating(RatingKind::Movie, movie)
            .await?,
        Some(9.0)
    );
    t.actions.clear_rating(RatingKind::Movie, movie).await?;
    assert_eq!(
        t.library()
            .await?
            .get_rating(RatingKind::Movie, movie)
            .await?,
        None
    );
    t.close().await
}
