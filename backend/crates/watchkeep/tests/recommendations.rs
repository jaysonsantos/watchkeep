mod common;

use axum::http::{Method, StatusCode};
use common::{call, event, get, json_request, movie_payload, test_context};
use eyre::Result;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};

const PATH: &str = "/api/recommendations";

/// Genres, votes, a collection, and one more movie on top of the sample catalog.
/// The statements are test fixtures on a schema the storage crate does not own,
/// so they are plain SQL, not macros.
async fn seed_recommendations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(AssertSqlSafe(
        "INSERT INTO genre (id, kind, name_en, name_pt) VALUES
           (28, 'movie', 'Action', 'Ação'),
           (80, 'movie', 'Crime', 'Crime'),
           (35, 'movie', 'Comedy', 'Comédia'),
           (18, 'tv', 'Drama', 'Drama'),
           (9648, 'tv', 'Mystery', 'Mistério');
         INSERT INTO collection (id, name_en, name_pt) VALUES (1000, 'The Mann Collection', 'Coleção Mann');
         UPDATE tmdb_movie SET vote_count = 5000, vote_average = 8.0, original_language = 'en', collection_id = 1000;
         UPDATE tmdb_show SET vote_count = 3000, vote_average = 8.5, original_language = 'en', in_production = true;
         INSERT INTO tmdb_movie_genre (movie_id, genre_id) VALUES (949, 28), (949, 80), (4638, 28), (4638, 80);
         INSERT INTO tmdb_show_genre (show_id, genre_id) VALUES (95396, 18), (95396, 9648);
         INSERT INTO tmdb_movie (id, title_en, release_date, vote_count, vote_average, original_language, fetched_at, raw_json)
           VALUES (600, 'Thief', '1981-03-27', 1200, 7.4, 'en', 0, '{}'),
                  (601, 'The Big Lebowski', '1998-03-06', 1200, 7.4, 'en', 0, '{}');
         INSERT INTO tmdb_movie_genre (movie_id, genre_id) VALUES (600, 28), (600, 80), (601, 35);",
    ))
    .execute(pool)
    .await?;
    Ok(())
}

fn tmdb_ids(items: &Value) -> Vec<i64> {
    items
        .as_array()
        .expect("an array")
        .iter()
        .map(|item| item["tmdbId"].as_i64().expect("a TMDB id"))
        .collect()
}

fn reasons(items: &Value, tmdb_id: i64) -> Vec<String> {
    items
        .as_array()
        .expect("an array")
        .iter()
        .find(|item| item["tmdbId"].as_i64() == Some(tmdb_id))
        .unwrap_or_else(|| panic!("no item with TMDB id {tmdb_id}"))["reasons"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|reason| reason.as_str().unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test]
async fn recommends_by_genre_and_names_the_matching_genres() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    seed_recommendations(t.catalog_pool.as_ref().expect("a catalog pool")).await?;
    t.scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;

    let (status, body) = get(&t.app(), PATH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["profile"]["items"], 1,
        "one watched movie feeds the profile"
    );
    let genres: Vec<&str> = body["profile"]["genres"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|genre| genre["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(genres, vec!["Action", "Crime"]);

    let movies = tmdb_ids(&body["movies"]);
    assert!(
        movies.contains(&600),
        "Thief shares both genres: {movies:?}"
    );
    assert!(
        !movies.contains(&601),
        "The Big Lebowski shares no genre: {movies:?}"
    );
    assert!(
        !movies.contains(&949),
        "the watched movie is in the library"
    );
    assert_eq!(reasons(&body["movies"], 600), vec!["Action", "Crime"]);
    t.close().await
}

#[tokio::test]
async fn offers_the_rest_of_a_collection_and_lists_it_only_once() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    seed_recommendations(t.catalog_pool.as_ref().expect("a catalog pool")).await?;
    t.scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;

    let (_, body) = get(&t.app(), PATH).await;
    let collection = tmdb_ids(&body["nextInCollection"]);
    assert_eq!(
        collection,
        vec![4638],
        "Collateral shares the collection of Heat"
    );
    assert_eq!(
        reasons(&body["nextInCollection"], 4638),
        vec!["The Mann Collection"]
    );
    assert!(
        !tmdb_ids(&body["movies"]).contains(&4638),
        "a movie of a collection appears in one list only"
    );
    t.close().await
}

#[tokio::test]
async fn lists_a_finished_show_that_still_makes_episodes() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    seed_recommendations(t.catalog_pool.as_ref().expect("a catalog pool")).await?;
    let (status, show) = call(
        &t.app(),
        json_request(
            Method::POST,
            "/api/shows",
            &json!({ "tmdb_id": 95396, "watchlist": false }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let show_id = show["id"].as_str().expect("an id");
    let (status, _) = call(
        &t.app(),
        json_request(
            Method::POST,
            &format!("/api/shows/{show_id}/watched"),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&t.app(), PATH).await;
    let returning = &body["returning"];
    assert_eq!(tmdb_ids(returning), vec![95396]);
    assert_eq!(returning[0]["localId"], show_id);
    assert_eq!(
        returning[0]["reasons"][0], "New episodes in production",
        "the show is complete and in production"
    );
    t.close().await
}

#[tokio::test]
async fn returns_empty_lists_without_a_catalog() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let (status, body) = get(&t.app(), PATH).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["profile"]["items"], 0);
    for list in ["movies", "shows", "nextInCollection", "returning"] {
        assert_eq!(
            body[list].as_array().map(Vec::len),
            Some(0),
            "{list} is empty"
        );
    }
    t.close().await
}
