mod common;

use std::time::Duration;

use common::{episode_payload, event, missing_id, movie_payload, test_context};
use eyre::Result;
use serde_json::json;
use watchkeep::scrobble::{ScrobbleAction, percent_of};
use watchkeep_storage::clock::Clock;
use watchkeep_storage::lists::{SortOrder, WatchFilter};
use watchkeep_storage::model::{PlayState, RatingKind, TargetKind};
use watchkeep_storage::queries::MovieView;

fn titles(movies: Vec<MovieView>) -> Vec<String> {
    movies.into_iter().map(|movie| movie.title).collect()
}

#[test]
fn computes_percentages_with_one_decimal() {
    assert_eq!(
        percent_of(Duration::from_secs(1), Some(Duration::from_secs(3))),
        Some(33.3)
    );
    assert_eq!(
        percent_of(Duration::from_secs(5), Some(Duration::from_secs(4))),
        Some(100.0),
        "capped at 100"
    );
    assert_eq!(percent_of(Duration::from_secs(5), None), None);
    assert_eq!(
        percent_of(Duration::from_secs(5), Some(Duration::ZERO)),
        None
    );
}

#[tokio::test]
async fn tracks_progress_for_play_pause_and_resume() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let result = t
        .scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.play" }),
            json!({}),
        )))
        .await?;
    let episode = result.target_id.expect("an episode id");
    let mut library = t.library().await?;
    let progress = library
        .get_progress(TargetKind::Episode, episode)
        .await?
        .expect("progress");
    assert_eq!(progress.state, PlayState::Playing);
    assert_eq!(progress.position(), Duration::from_secs(120));

    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.pause" }),
            json!({ "viewOffset": 900_000 }),
        )))
        .await?;
    let progress = library
        .get_progress(TargetKind::Episode, episode)
        .await?
        .expect("progress");
    assert_eq!(progress.state, PlayState::Paused);
    assert_eq!(progress.position(), Duration::from_secs(900));

    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.resume" }),
            json!({ "viewOffset": null }),
        )))
        .await?;
    let progress = library
        .get_progress(TargetKind::Episode, episode)
        .await?
        .expect("progress");
    assert_eq!(progress.state, PlayState::Playing);
    assert_eq!(
        progress.position(),
        Duration::from_secs(900),
        "keeps the last position when Plex sends none"
    );
    assert_eq!(library.play_count(TargetKind::Episode, episode).await?, 0);
    drop(library);
    t.close().await
}

#[tokio::test]
async fn records_a_play_on_media_scrobble_and_clears_progress() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.play" }),
            json!({}),
        )))
        .await?;
    let result = t
        .scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    assert_eq!(result.action, ScrobbleAction::Play);
    let episode = result.target_id.expect("an episode id");
    let mut library = t.library().await?;
    assert_eq!(library.play_count(TargetKind::Episode, episode).await?, 1);
    assert!(
        library
            .get_progress(TargetKind::Episode, episode)
            .await?
            .is_none()
    );
    let play = library
        .last_play(TargetKind::Episode, episode)
        .await?
        .expect("play");
    assert_eq!(play.source, "plex-scrobble");
    assert_eq!(play.watched_at, t.clock.now());
    assert_eq!(play.account.as_deref(), Some("jayson"));
    assert_eq!(play.player.as_deref(), Some("Plex Web"));
    drop(library);
    t.close().await
}

#[tokio::test]
async fn records_a_play_on_media_stop_past_the_threshold() -> Result<()> {
    let t = test_context(|config| config.watched_threshold_percent = 85.0, false).await?;
    let stopped = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.stop" }),
            json!({ "viewOffset": 4_000_000 }),
        )))
        .await?;
    assert_eq!(stopped.action, ScrobbleAction::Progress);
    assert_eq!(stopped.percent, Some(40.0));
    let movie = stopped.target_id.expect("a movie id");
    let mut library = t.library().await?;
    assert_eq!(
        library
            .get_progress(TargetKind::Movie, movie)
            .await?
            .expect("progress")
            .state,
        PlayState::Stopped
    );
    assert_eq!(library.play_count(TargetKind::Movie, movie).await?, 0);

    let finished = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.stop" }),
            json!({ "viewOffset": 9_000_000 }),
        )))
        .await?;
    assert_eq!(finished.action, ScrobbleAction::Play);
    assert_eq!(
        library
            .last_play(TargetKind::Movie, movie)
            .await?
            .expect("play")
            .source,
        "plex-stop"
    );
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
async fn does_not_count_scrobble_plus_stop_as_two_plays() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let first = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    let movie = first.target_id.expect("a movie id");
    let second = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.stop" }),
            json!({ "viewOffset": 9_900_000 }),
        )))
        .await?;
    assert_eq!(second.action, ScrobbleAction::DuplicatePlay);
    assert_eq!(
        t.library()
            .await?
            .play_count(TargetKind::Movie, movie)
            .await?,
        1
    );

    t.clock.advance_minutes(60 * 24);
    let rewatch = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    assert_eq!(rewatch.action, ScrobbleAction::Play);
    assert_eq!(
        t.library()
            .await?
            .play_count(TargetKind::Movie, movie)
            .await?,
        2
    );
    t.close().await
}

#[tokio::test]
async fn keeps_an_episode_watched_when_plex_stops_and_pauses_after_the_scrobble() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let played = t
        .scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    let episode = played.target_id.expect("an episode id");
    assert_eq!(played.action, ScrobbleAction::Play);

    for name in ["media.stop", "media.pause"] {
        let late = t
            .scrobbler
            .apply(&event(&episode_payload(
                json!({ "event": name }),
                json!({ "viewOffset": null }),
            )))
            .await?;
        assert_eq!(late.action, ScrobbleAction::AlreadyWatched, "{name}");
    }
    let mut library = t.library().await?;
    assert!(
        library
            .get_progress(TargetKind::Episode, episode)
            .await?
            .is_none(),
        "no progress row comes back after the play"
    );
    assert_eq!(library.play_count(TargetKind::Episode, episode).await?, 1);

    t.clock.advance_minutes(60 * 24);
    let later = t
        .scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.play" }),
            json!({}),
        )))
        .await?;
    assert_eq!(
        later.action,
        ScrobbleAction::Progress,
        "a rewatch after the window tracks progress again"
    );
    drop(library);
    t.close().await
}

#[tokio::test]
async fn matches_the_same_episode_across_guid_and_season_number() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.play" }),
            json!({ "guid": null, "Guid": null, "grandparentGuid": null }),
        )))
        .await?;
    let shows = t.queries.shows("", SortOrder::Recent).await?;
    assert_eq!(shows.len(), 1);
    assert_eq!(t.queries.episodes(shows[0].id).await?.len(), 1);
    t.close().await
}

#[tokio::test]
async fn stores_ratings() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    let result = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.rate", "rating": 9 }),
            json!({}),
        )))
        .await?;
    let movie = result.target_id.expect("a movie id");
    assert_eq!(
        t.library()
            .await?
            .get_rating(RatingKind::Movie, movie)
            .await?,
        Some(9.0)
    );
    assert_eq!(
        t.queries.movie(movie).await?.expect("movie").rating,
        Some(9.0)
    );
    t.close().await
}

#[tokio::test]
async fn ignores_accounts_outside_the_allow_list() -> Result<()> {
    let t = test_context(
        |config| config.plex_accounts = vec!["someone-else".to_owned()],
        false,
    )
    .await?;
    let result = t
        .scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    assert_eq!(result.action, ScrobbleAction::IgnoredAccount);
    assert_eq!(result.target_id, None);
    assert_eq!(t.queries.stats().await?.movies, 0);
    t.close().await
}

#[tokio::test]
async fn marks_episodes_and_whole_shows_watched_and_unwatched() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    t.scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.play" }),
            json!({}),
        )))
        .await?;
    let second = t
        .scrobbler
        .apply(&event(&episode_payload(
            json!({ "event": "media.play" }),
            json!({ "index": 2, "guid": "plex://episode/ep2", "title": "Half Loop", "Guid": [] }),
        )))
        .await?;
    let show = t.first_show().await?;
    let episode2 = second.target_id.expect("an episode id");
    assert_eq!(t.actions.mark_show_watched(show, None).await?, Some(2));
    assert_eq!(t.queries.show(show).await?.expect("show").watched_count, 2);
    assert_eq!(t.queries.in_progress().await?.len(), 0);
    assert!(
        t.actions
            .mark_unwatched(TargetKind::Episode, episode2)
            .await?
    );
    assert_eq!(t.queries.show(show).await?.expect("show").watched_count, 1);
    assert_eq!(t.actions.mark_show_unwatched(show).await?, Some(1));
    assert_eq!(t.queries.show(show).await?.expect("show").watched_count, 0);
    assert_eq!(t.actions.mark_show_watched(missing_id(), None).await?, None);
    assert!(
        !t.actions
            .mark_watched(TargetKind::Movie, missing_id(), None)
            .await?
    );
    t.close().await
}

#[tokio::test]
async fn filters_movies_by_watch_state() -> Result<()> {
    let t = test_context(|_| {}, false).await?;
    t.scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.scrobble" }),
            json!({}),
        )))
        .await?;
    t.scrobbler
        .apply(&event(&movie_payload(
            json!({ "event": "media.play" }),
            json!({ "title": "Collateral", "year": 2004, "guid": "plex://movie/collateral", "Guid": [] }),
        )))
        .await?;
    assert_eq!(
        titles(
            t.queries
                .movies(WatchFilter::Watched, "", SortOrder::Recent, None, 0)
                .await?
        ),
        ["Heat"]
    );
    assert_eq!(
        titles(
            t.queries
                .movies(WatchFilter::Unwatched, "", SortOrder::Recent, None, 0)
                .await?
        ),
        ["Collateral"]
    );
    assert_eq!(
        titles(
            t.queries
                .movies(WatchFilter::All, "coll", SortOrder::Recent, None, 0)
                .await?
        ),
        ["Collateral"]
    );
    t.close().await
}
