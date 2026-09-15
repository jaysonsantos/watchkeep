mod common;

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{TestContext, at, call, get, json_request, request, test_context};
use eyre::Result;
use serde_json::{Value, json};
use watchkeep::trakt::import::{
    ImportOptions, Section, TraktImportReport, import_trakt_export, read_export, section_of,
};
use watchkeep_storage::lists::{SortOrder, WatchFilter};
use watchkeep_storage::model::{PlayState, RatingKind, TargetKind};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

/// Build a small ZIP in memory. Even entries are stored, odd entries deflated.
fn build_zip(files: &[(&str, String)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (index, (name, content)) in files.iter().enumerate() {
        let method = if index % 2 == 0 {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(method),
            )
            .expect("start file");
        writer.write_all(content.as_bytes()).expect("write file");
    }
    writer.finish().expect("finish zip").into_inner()
}

fn severance() -> Value {
    json!({ "ids": { "imdb": "tt11280740", "plex": { "guid": "5d9c0f1a" }, "slug": "severance", "tmdb": 95396, "tvdb": 371980, "trakt": 153017 }, "year": 2022, "title": "Severance" })
}

fn heat() -> Value {
    json!({ "ids": { "imdb": "tt0113277", "plex": { "guid": "5d776825" }, "slug": "heat-1995", "tmdb": 949, "trakt": 1 }, "year": 1995, "title": "Heat" })
}

fn episode1() -> Value {
    json!({ "ids": { "imdb": "tt1", "plex": { "guid": "aa1" }, "tmdb": 1982925, "tvdb": 1, "trakt": 501 }, "title": "Good News About Hell", "number": 1, "season": 1 })
}

fn episode2() -> Value {
    json!({ "ids": { "imdb": "tt2", "plex": { "guid": "aa2" }, "tmdb": 3396429, "tvdb": 2, "trakt": 502 }, "title": "Half Loop", "number": 2, "season": 1 })
}

fn export_files() -> Vec<(&'static str, String)> {
    vec![
        ("_errors.json", json!([{ "endpoint": "users/me/stats", "error": "Unexpected end of JSON input" }]).to_string()),
        (
            "watched-history-1.json",
            json!([
                { "id": 9001, "watched_at": "2026-08-21T06:41:00.000Z", "action": "watch", "type": "movie", "movie": heat() },
                { "id": 9002, "watched_at": "2026-07-21T19:19:00.000Z", "action": "scrobble", "type": "episode", "episode": episode1(), "show": severance() },
            ])
            .to_string(),
        ),
        (
            "watched-history-2.json",
            json!([
                { "id": 9003, "watched_at": "2026-07-22T19:19:00.000Z", "action": "checkin", "type": "episode", "episode": episode2(), "show": severance() },
                { "id": 9004, "watched_at": "2020-01-01T00:00:00.000Z", "action": "watch", "type": "movie", "movie": heat() },
            ])
            .to_string(),
        ),
        ("ratings-movies.json", json!([{ "rated_at": "2026-06-30T19:55:52.000Z", "rating": 9, "type": "movie", "movie": heat() }]).to_string()),
        ("ratings-shows.json", json!([{ "rated_at": "2020-06-18T19:30:29.000Z", "rating": 10, "type": "show", "show": severance() }]).to_string()),
        (
            "ratings-episodes-1.json",
            json!([{ "rated_at": "2021-05-24T21:00:40.000Z", "rating": 8, "type": "episode", "episode": episode1(), "show": severance() }]).to_string(),
        ),
        (
            "watched-playback.json",
            json!([
                { "progress": 25, "paused_at": "2026-08-19T21:09:06.000Z", "id": 1, "type": "episode", "episode": { "ids": { "tmdb": 3396430, "trakt": 503 }, "title": "In Perpetuity", "number": 3, "season": 1 }, "show": severance() },
                { "progress": 50, "paused_at": "2026-08-19T21:09:06.000Z", "id": 2, "type": "movie", "movie": { "ids": { "trakt": 77 }, "year": 1999, "title": "Unknown Film" } },
            ])
            .to_string(),
        ),
        (
            "lists-watchlist.json",
            json!([
                { "type": "movie", "movie": { "ids": { "imdb": "tt0369339", "tmdb": 4638, "trakt": 2 }, "year": 2004, "title": "Collateral" }, "rank": 1, "id": 1, "listed_at": "2026-01-04T14:02:03.000Z" },
                { "type": "show", "show": severance(), "rank": 2, "id": 2, "listed_at": "2026-01-04T14:02:03.000Z" },
            ])
            .to_string(),
        ),
        (
            "hidden-progress-watched.json",
            json!([{ "hidden_at": "2024-05-09T14:53:35.000Z", "type": "show", "show": severance() }]).to_string(),
        ),
        ("comments-movies.json", "[]".to_owned()),
        ("user-profile.json", json!({ "username": "someone" }).to_string()),
    ]
}

fn write_export(files: &[(&str, String)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "trakt-{}-{}",
        std::process::id(),
        common::unique_name()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("export.zip");
    std::fs::write(&path, build_zip(files)).expect("write zip");
    path
}

async fn import(t: &TestContext, path: &Path, dry_run: bool) -> Result<TraktImportReport> {
    let options = ImportOptions {
        clock: t.clock.clone(),
        dry_run,
    };
    import_trakt_export(&t.pool, t.catalog.as_ref(), path, options).await
}

#[test]
fn reads_stored_and_deflated_entries() {
    let long = json!({ "long": "x".repeat(5000) }).to_string();
    let entries = read_export(&build_zip(&[
        ("a.json", "[1]".to_owned()),
        ("dir/b.json", long),
    ]))
    .expect("zip");
    assert_eq!(
        entries
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["a.json", "dir/b.json"]
    );
    assert_eq!(entries[0].1, "[1]");
    let parsed: Value = serde_json::from_str(&entries[1].1).expect("json");
    assert_eq!(parsed["long"].as_str().expect("long").len(), 5000);
    assert!(read_export(b"not a zip").is_err());
}

#[test]
fn maps_file_names_to_sections() {
    assert_eq!(
        section_of("watched-history-12.json"),
        Some(Section::History)
    );
    assert_eq!(
        section_of("ratings-episodes-2.json"),
        Some(Section::Ratings)
    );
    assert_eq!(section_of("watched-playback.json"), Some(Section::Playback));
    assert_eq!(section_of("lists-watchlist.json"), Some(Section::Watchlist));
    assert_eq!(
        section_of("hidden-progress-watched.json"),
        Some(Section::Hidden)
    );
    assert_eq!(section_of("watched-movies-1.json"), None);
    assert_eq!(section_of("ratings-seasons.json"), None);
}

#[tokio::test]
async fn imports_plays_ratings_playback_watchlist_and_hidden_shows_with_the_catalog() -> Result<()>
{
    let t = test_context(|_| {}, true).await?;
    let report = import(&t, &write_export(&export_files()), false).await?;
    assert_eq!(report.plays, 4);
    assert_eq!(report.plays_skipped, 0);
    assert_eq!(report.movies, 3);
    assert_eq!(report.shows, 1);
    assert_eq!(report.episodes, 3);
    assert_eq!(report.ratings, 3);
    assert_eq!(report.playback, 1);
    assert_eq!(
        report.playback_skipped, 1,
        "the unknown film has no runtime"
    );
    assert_eq!(report.watchlist, 2);
    assert_eq!(report.hidden, 1);
    assert_eq!(
        report.ignored_files,
        ["comments-movies.json", "user-profile.json"]
    );
    assert_eq!(report.export_errors[0]["endpoint"], "users/me/stats");

    let movies = t
        .queries
        .movies(WatchFilter::Watched, "", SortOrder::Recent, None, 0)
        .await?;
    let movie = &movies[0];
    assert_eq!(movie.title, "Heat");
    assert_eq!(movie.play_count, 2);
    assert_eq!(movie.plex_guid.as_deref(), Some("plex://movie/5d776825"));
    assert_eq!(movie.tmdb_id, Some(949));
    assert_eq!(movie.rating, Some(9.0));
    assert_eq!(movie.last_watched_at, Some(at("2026-08-21T06:41:00Z")));
    let mut library = t.library().await?;
    let play = library
        .last_play(TargetKind::Movie, movie.id)
        .await?
        .expect("play");
    assert_eq!(play.source, "trakt");
    assert_eq!(play.external_id.as_deref(), Some("9001"));

    let shows = t
        .views
        .shows(WatchFilter::All, "", SortOrder::Recent)
        .await?;
    let show = &shows[0].show;
    assert_eq!(show.tmdb_id, Some(95396));
    assert_eq!(show.plex_guid.as_deref(), Some("plex://show/5d9c0f1a"));
    assert_eq!(show.watched_count, 2);
    assert_eq!(show.hidden_at, Some(at("2024-05-09T14:53:35Z")));
    assert_eq!(show.rating, Some(10.0));
    assert_eq!(
        t.views
            .shows(WatchFilter::Unwatched, "", SortOrder::Recent)
            .await?
            .len(),
        0,
        "hidden shows are not unwatched"
    );
    let episodes = t.queries.episodes(show.id).await?;
    assert_eq!(episodes[0].plex_guid.as_deref(), Some("plex://episode/aa1"));
    assert_eq!(episodes[2].progress_state, Some(PlayState::Stopped));
    assert_eq!(
        episodes[2].position_ms,
        Some(Duration::from_secs(56 * 60 / 4).as_millis() as i64)
    );
    assert_eq!(
        library
            .get_rating(RatingKind::Episode, episodes[0].id)
            .await?,
        Some(8.0)
    );
    let position = library
        .get_progress(TargetKind::Episode, episodes[2].id)
        .await?
        .expect("position");
    assert_eq!(position.updated_at, at("2026-08-19T21:09:06Z"));
    drop(library);

    let watchlist = t.queries.watchlist().await?;
    assert_eq!(
        watchlist
            .iter()
            .map(|item| (item.kind.as_str(), item.title.as_str(), item.watched_count))
            .collect::<Vec<_>>(),
        [("movie", "Collateral", 0), ("show", "Severance", 2)]
    );
    assert_eq!(watchlist[0].listed_at, at("2026-01-04T14:02:03Z"));
    let unwatched = t
        .queries
        .movies(WatchFilter::Unwatched, "", SortOrder::Recent, None, 0)
        .await?;
    let collateral = unwatched
        .iter()
        .find(|movie| movie.title == "Collateral")
        .expect("Collateral");
    assert_eq!(collateral.tmdb_id, Some(4638));
    t.close().await
}

#[tokio::test]
async fn works_without_a_catalog_and_is_idempotent() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let path = write_export(&export_files());
    let first = import(&t, &path, false).await?;
    assert_eq!(first.plays, 4);
    assert_eq!(first.playback, 0, "no runtime without a catalog");
    let second = import(&t, &path, false).await?;
    assert_eq!(second.plays, 0);
    assert_eq!(second.plays_skipped, 4);
    assert_eq!(t.queries.stats().await?.plays, 4);
    assert_eq!(t.queries.shows("", SortOrder::Recent).await?.len(), 1);
    assert_eq!(t.queries.watchlist().await?.len(), 2);
    t.close().await
}

#[tokio::test]
async fn writes_nothing_on_a_dry_run() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let report = import(&t, &write_export(&export_files()), true).await?;
    assert_eq!(report.plays, 4);
    assert!(report.dry_run);
    assert_eq!(t.queries.stats().await?.plays, 0);
    assert_eq!(t.queries.stats().await?.movies, 0);
    t.close().await
}

#[tokio::test]
async fn matches_a_plex_webhook_to_an_imported_item_by_plex_guid() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    import(&t, &write_export(&export_files()), false).await?;
    t.clock.advance_minutes(60 * 24 * 365);
    let payload = json!({
        "event": "media.scrobble",
        "Metadata": { "type": "movie", "title": "Heat (Director's Cut)", "guid": "plex://movie/5d776825", "duration": 1 },
    });
    call(
        &app,
        json_request(Method::POST, common::WEBHOOK_PATH, &payload),
    )
    .await;
    let movies = t
        .queries
        .movies(WatchFilter::All, "", SortOrder::Recent, None, 0)
        .await?;
    assert_eq!(movies.len(), 3);
    assert_eq!(
        movies
            .iter()
            .find(|movie| movie.tmdb_id == Some(949))
            .expect("Heat")
            .play_count,
        3
    );
    t.close().await
}

#[tokio::test]
async fn merges_trakt_items_that_share_an_external_id() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let twin = json!({ "ids": { "imdb": "tt99", "tmdb": 777, "tvdb": 371980, "trakt": 999 }, "year": 2023, "title": "Severance (duplicate)" });
    let mut files = export_files();
    files.push((
        "watched-history-3.json",
        json!([
            { "id": 9101, "watched_at": "2026-01-02T00:00:00.000Z", "action": "watch", "type": "episode", "episode": { "ids": { "tmdb": 1982925, "trakt": 601 }, "title": "Same TMDB episode", "number": 7, "season": 3 }, "show": twin },
            { "id": 9102, "watched_at": "2026-01-03T00:00:00.000Z", "action": "watch", "type": "movie", "movie": { "ids": { "imdb": "tt0113277", "tmdb": 424242, "trakt": 55 }, "year": 1995, "title": "Heat (other entry)" } },
        ])
        .to_string(),
    ));
    let report = import(&t, &write_export(&files), false).await?;
    assert_eq!(report.plays, 6);
    let shows = t.queries.shows("", SortOrder::Recent).await?;
    assert_eq!(shows.len(), 1, "the same tvdb id merges into one show");
    assert_eq!(shows[0].tmdb_id, Some(95396), "the first tmdb id is kept");
    assert_eq!(t.queries.episodes(shows[0].id).await?.len(), 4);
    let movies = t
        .queries
        .movies(WatchFilter::All, "", SortOrder::Recent, None, 0)
        .await?;
    assert_eq!(movies.len(), 3, "the same imdb id merges into one movie");
    assert_eq!(
        movies
            .iter()
            .find(|movie| movie.imdb_id.as_deref() == Some("tt0113277"))
            .expect("Heat")
            .play_count,
        3
    );
    let again = import(&t, &write_export(&files), false).await?;
    assert_eq!(again.plays, 0);
    t.close().await
}

#[tokio::test]
async fn adds_and_removes_watchlist_items_and_hides_shows() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let app = t.app();
    import(&t, &write_export(&export_files()), false).await?;
    let show = t.queries.shows("", SortOrder::Recent).await?[0].clone();
    let (status, _) = call(
        &app,
        request(Method::DELETE, &format!("/api/shows/{}/hidden", show.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        t.queries.show(show.id).await?.expect("show").hidden_at,
        None
    );
    let (status, _) = call(
        &app,
        request(Method::DELETE, &format!("/api/shows/{}/watchlist", show.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t.queries.watchlist().await?.len(), 1);
    let (status, _) = call(
        &app,
        request(Method::POST, &format!("/api/movies/{}/watchlist", show.id)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a show is not a movie");
    let (_, watchlist) = get(&app, "/api/watchlist").await;
    assert!(
        watchlist
            .as_array()
            .expect("array")
            .iter()
            .any(|item| item["title"] == "Collateral")
    );
    let (_, detail) = get(&app, &format!("/api/shows/{}", show.id)).await;
    assert_eq!(detail["on_watchlist"], false);
    assert_eq!(detail["hidden_at"], Value::Null);
    t.close().await
}
