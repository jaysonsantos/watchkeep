//! The aggregates of `/api/statistics`: the totals, the calendar buckets, the
//! streak, and the time zone that moves a play from one local day to the next.

mod common;

use axum::http::StatusCode;
use common::{TestContext, get, test_context};
use eyre::Result;
use serde_json::json;
use watchkeep::actions::AddMediaInput;
use watchkeep_storage::clock::Clock;
use watchkeep_storage::model::{MediaKind, TargetKind};
use watchkeep_storage::statistics::{DEFAULT_TIMEZONE, MONTHS};

/// TMDB ids of the sample catalog in `common`.
const SEVERANCE: i64 = 95396;
const HEAT: i64 = 949;

const MINUTES_PER_DAY: i64 = 24 * 60;
const MS_PER_MINUTE: i64 = 60_000;

/// Runtimes of the three watched episodes and of the movie, in minutes.
const EPISODE_MINUTES: i64 = 57 + 53 + 56;
const MOVIE_MINUTES: i64 = 170;

/// `EXTRACT(dow)` of 2026-01-01, a Thursday.
const THURSDAY: usize = 4;

/// Three episodes on three days, then a movie late on the third day. The fake
/// clock starts at 2026-01-01T12:00:00Z.
async fn watch_a_week(t: &TestContext) -> Result<()> {
    let show = t
        .actions
        .add_media(AddMediaInput {
            kind: MediaKind::Show,
            tmdb_id: Some(SEVERANCE),
            title: None,
            year: None,
            watchlist: false,
        })
        .await?
        .expect("the show");
    for number in 1..=3 {
        t.actions
            .mark_episode_by_number(show.id, 1, number, true)
            .await?;
        t.clock.advance_minutes(MINUTES_PER_DAY);
    }
    // Back to the third day, at 22:30 UTC.
    t.clock.advance_minutes(-MINUTES_PER_DAY + 10 * 60 + 30);
    let movie = t
        .actions
        .add_media(AddMediaInput {
            kind: MediaKind::Movie,
            tmdb_id: Some(HEAT),
            title: None,
            year: None,
            watchlist: false,
        })
        .await?
        .expect("the movie");
    t.actions
        .mark_watched(TargetKind::Movie, movie.id, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn counts_plays_runtime_and_the_calendar_buckets() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    watch_a_week(&t).await?;
    let stats = t
        .queries
        .statistics(DEFAULT_TIMEZONE, t.clock.now())
        .await?;

    assert_eq!(stats.timezone, DEFAULT_TIMEZONE);
    assert_eq!(stats.totals.plays, 4);
    assert_eq!(stats.totals.movie_plays, 1);
    assert_eq!(stats.totals.episode_plays, 3);
    assert_eq!(stats.totals.movies_watched, 1);
    assert_eq!(stats.totals.episodes_watched, 3);
    assert_eq!(stats.totals.shows_watched, 1);
    assert_eq!(
        stats.totals.episode_runtime_ms,
        EPISODE_MINUTES * MS_PER_MINUTE
    );
    assert_eq!(stats.totals.movie_runtime_ms, MOVIE_MINUTES * MS_PER_MINUTE);
    assert_eq!(
        stats.totals.runtime_ms,
        (EPISODE_MINUTES + MOVIE_MINUTES) * MS_PER_MINUTE
    );
    assert_eq!(stats.totals.days_watched, 3);
    assert_eq!(
        stats
            .totals
            .first_play_at
            .expect("a first play")
            .to_string(),
        "2026-01-01 12:00:00 UTC"
    );
    assert_eq!(
        stats.totals.last_play_at.expect("a last play").to_string(),
        "2026-01-03 22:30:00 UTC"
    );

    assert_eq!(stats.streak.current, 3, "three days in a row, ending today");
    assert_eq!(stats.streak.longest, 3);

    assert_eq!(stats.months.len() as i32, MONTHS);
    let current = stats.months.last().expect("the current month");
    assert_eq!(current.period, "2026-01");
    assert_eq!(current.plays, 4);
    assert_eq!(
        stats.months[..stats.months.len() - 1]
            .iter()
            .map(|month| month.plays)
            .sum::<i64>(),
        0,
        "the months before hold no play"
    );

    let weekday_plays: Vec<i64> = stats.weekdays.iter().map(|day| day.plays).collect();
    assert_eq!(weekday_plays.len(), 7);
    assert_eq!(weekday_plays[THURSDAY], 1);
    assert_eq!(weekday_plays[THURSDAY + 1], 1);
    assert_eq!(
        weekday_plays[THURSDAY + 2],
        2,
        "the movie joins the third episode"
    );

    let hour_plays: Vec<i64> = stats.hours.iter().map(|hour| hour.plays).collect();
    assert_eq!(hour_plays.len(), 24);
    assert_eq!(hour_plays[12], 3);
    assert_eq!(hour_plays[22], 1);

    assert_eq!(stats.sources.len(), 1, "every play is manual");
    assert_eq!(stats.sources[0].label, "manual");
    assert_eq!(stats.sources[0].plays, 4);
    assert!(stats.players.is_empty(), "a manual play has no player");

    let show = &stats.top_shows[0];
    assert_eq!(show.title, "Severance");
    assert_eq!(show.plays, 3);
    assert_eq!(show.items, 3);
    assert_eq!(show.runtime_ms, EPISODE_MINUTES * MS_PER_MINUTE);
    let movie = &stats.top_movies[0];
    assert_eq!(movie.title, "Heat");
    assert_eq!(movie.year, Some(1995));
    assert_eq!(movie.plays, 1);
    assert_eq!(movie.runtime_ms, MOVIE_MINUTES * MS_PER_MINUTE);

    t.close().await
}

#[tokio::test]
async fn the_time_zone_moves_a_play_to_another_local_day() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    watch_a_week(&t).await?;
    // 2026-01-03T22:30Z is 2026-01-04T07:30 in Tokyo, so the movie makes a fourth day.
    let stats = t.queries.statistics("Asia/Tokyo", t.clock.now()).await?;

    assert_eq!(stats.timezone, "Asia/Tokyo");
    assert_eq!(stats.totals.days_watched, 4);
    assert_eq!(stats.streak.current, 4);
    assert_eq!(stats.totals.plays, 4, "the plays themselves do not move");
    let hour_plays: Vec<i64> = stats.hours.iter().map(|hour| hour.plays).collect();
    assert_eq!(hour_plays[21], 3, "12:00 UTC is 21:00 in Tokyo");
    assert_eq!(hour_plays[7], 1);

    t.close().await
}

#[tokio::test]
async fn an_unknown_time_zone_falls_back_to_utc() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    let stats = t
        .queries
        .statistics("Nowhere/Fictional", t.clock.now())
        .await?;

    assert_eq!(stats.timezone, DEFAULT_TIMEZONE);
    assert_eq!(stats.totals.plays, 0);
    assert_eq!(stats.streak.current, 0);
    assert_eq!(stats.streak.longest, 0);
    assert_eq!(
        stats.months.len() as i32,
        MONTHS,
        "an empty history still fills the chart"
    );
    assert_eq!(stats.weekdays.len(), 7);
    assert_eq!(stats.hours.len(), 24);
    assert!(stats.top_shows.is_empty());

    t.close().await
}

#[tokio::test]
async fn the_route_answers_with_the_library_counts() -> Result<()> {
    let t = test_context(|_| {}, true).await?;
    watch_a_week(&t).await?;
    let app = t.app();

    let (status, body) = get(&app, "/api/statistics?tz=Asia/Tokyo").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["timezone"], "Asia/Tokyo");
    assert_eq!(body["library"]["movies"], 1);
    assert_eq!(body["library"]["shows"], 1);
    assert_eq!(body["library"]["episodes"], 3);
    assert_eq!(body["totals"]["plays"], 4);
    assert_eq!(body["top_shows"][0]["title"], "Severance");
    assert_eq!(body["players"], json!([]));

    let (status, body) = get(&app, "/api/statistics").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["timezone"], DEFAULT_TIMEZONE, "no tz means UTC");

    t.close().await
}
