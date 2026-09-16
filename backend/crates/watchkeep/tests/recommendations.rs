mod common;

use axum::http::{Method, StatusCode};
use common::{TestContext, call, event, get, id_of, json_request, movie_payload, test_context};
use eyre::Result;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;
use watchkeep_storage::model::RatingKind;

const PATH: &str = "/api/recommendations";

/// Genres, votes, a collection, and one more movie on top of the sample catalog.
/// The statements are test fixtures on a schema the storage crate does not own,
/// so they are plain SQL, not macros.
async fn seed_recommendations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(AssertSqlSafe(
        // TMDB gives id 18 to a movie genre and to a show genre. `genre.id` is
        // the only primary key, so the mirror keeps one row with one `kind`
        // word. A movie of genre 18 must still count as Drama.
        "INSERT INTO genre (id, kind, name_en, name_pt) VALUES
           (28, 'movie', 'Action', 'Ação'),
           (80, 'movie', 'Crime', 'Crime'),
           (35, 'movie', 'Comedy', 'Comédia'),
           (878, 'movie', 'Science Fiction', 'Ficção Científica'),
           (18, 'tv', 'Drama', 'Drama'),
           (9648, 'tv', 'Mystery', 'Mistério');
         INSERT INTO collection (id, name_en, name_pt) VALUES (1000, 'The Mann Collection', 'Coleção Mann');
         UPDATE tmdb_movie SET vote_count = 5000, vote_average = 8.0, original_language = 'en', collection_id = 1000;
         UPDATE tmdb_show SET vote_count = 3000, vote_average = 8.5, original_language = 'en', in_production = true;
         INSERT INTO tmdb_movie_genre (movie_id, genre_id) VALUES (949, 28), (949, 80), (949, 18), (4638, 28), (4638, 80);
         INSERT INTO tmdb_show_genre (show_id, genre_id) VALUES (95396, 18), (95396, 9648);
         INSERT INTO tmdb_movie (id, title_en, release_date, vote_count, vote_average, original_language, fetched_at, raw_json)
           VALUES (600, 'Thief', '1981-03-27', 1200, 7.4, 'en', 0, '{}'),
                  (601, 'The Big Lebowski', '1998-03-06', 1200, 7.4, 'en', 0, '{}'),
                  (602, 'Blade Runner', '1982-06-25', 1200, 7.4, 'en', 0, '{}');
         INSERT INTO tmdb_movie_genre (movie_id, genre_id) VALUES (600, 28), (600, 80), (601, 35), (602, 18), (602, 878);",
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
    assert_eq!(
        genres,
        vec!["Action", "Crime", "Drama"],
        "Drama counts for a movie, although the genre row says kind = tv"
    );

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
    assert_eq!(
        reasons(&body["movies"], 602),
        vec!["Drama"],
        "Science Fiction is no reason: the profile gives it no weight"
    );
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

/// The catalog total of a show counts the regular seasons only, so the counts
/// of the profile must do the same. A watched special would otherwise stand
/// for an episode that nobody watched.
#[tokio::test]
async fn leaves_the_specials_of_a_show_out_of_the_taste_counts() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    seed_recommendations(t.catalog_pool.as_ref().expect("a catalog pool")).await?;
    let (_, show) = call(
        &t.app(),
        json_request(Method::POST, "/api/shows", &json!({ "tmdb_id": 95396 })),
    )
    .await;
    let show_id: Uuid = id_of(&show);
    // Season 0 episode 1 is the only episode that this library holds.
    let (status, _) = call(
        &t.app(),
        json_request(
            Method::POST,
            &format!("/api/shows/{show_id}/seasons/0/episodes/1/watched"),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let item = t
        .queries
        .taste()
        .await?
        .into_iter()
        .find(|item| item.id == show_id)
        .expect("the show");
    assert_eq!(item.watched_count, 0, "a special is no watched episode");
    assert_eq!(item.play_count, 0, "a play of a special does not count");
    assert_eq!(item.episode_count, 0, "a special is no episode to watch");
    assert_eq!(item.last_watched_at, None);

    let (_, body) = get(&t.app(), PATH).await;
    assert_eq!(
        body["profile"]["items"], 0,
        "the show feeds no taste, so nothing is watched"
    );
    t.close().await
}

/// Plex and Trakt rate episodes far more often than shows, so a show without a
/// rating of its own takes the average of the ratings of its episodes.
#[tokio::test]
async fn folds_episode_ratings_into_the_rating_of_a_show() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    seed_recommendations(t.catalog_pool.as_ref().expect("a catalog pool")).await?;
    let (_, show) = call(
        &t.app(),
        json_request(Method::POST, "/api/shows", &json!({ "tmdb_id": 95396 })),
    )
    .await;
    let show_id: Uuid = id_of(&show);
    call(
        &t.app(),
        json_request(
            Method::POST,
            &format!("/api/shows/{show_id}/watched"),
            &json!({}),
        ),
    )
    .await;
    let episodes = t.episode_ids(show_id).await?;

    let show_rating = || async {
        let items = t.queries.taste().await.expect("the taste items");
        items
            .into_iter()
            .find(|item| item.id == show_id)
            .expect("the show")
            .rating
    };
    assert_eq!(show_rating().await, None, "no rating anywhere yet");

    // Each write takes its own connection and gives it back. `close()` waits
    // for every connection of the pool, so a held one would block the test.
    rate(&t, RatingKind::Episode, episodes[0], 9.0).await?;
    rate(&t, RatingKind::Episode, episodes[1], 7.0).await?;
    assert_eq!(
        show_rating().await,
        Some(8.0),
        "the average of the rated episodes"
    );

    rate(&t, RatingKind::Show, show_id, 5.0).await?;
    assert_eq!(
        show_rating().await,
        Some(5.0),
        "a rating of the show itself wins"
    );
    t.close().await
}

async fn rate(t: &TestContext, kind: RatingKind, id: Uuid, rating: f64) -> Result<()> {
    t.library()
        .await?
        .set_rating(kind, id, rating, None)
        .await?;
    Ok(())
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
