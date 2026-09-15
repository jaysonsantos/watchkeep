//! A database from the Node version keeps working: its `schema_migrations` table
//! is adopted, sqlx applies only the migrations that came later, and the
//! conversion to UUID ids and timestamp columns keeps every row and relation.

mod common;

use chrono::{DateTime, Utc};
use common::{admin, at, unique_name, url_for};
use eyre::Result;
use sqlx::AssertSqlSafe;
use uuid::Uuid;
use watchkeep_storage::db::{MIGRATOR, create_pool, migrate};

/// The SQL of the first two migrations, as the Node version applied them.
const LEGACY_SCHEMA: [&str; 2] = [
    include_str!("../../storage/migrations/0001_initial.sql"),
    include_str!("../../storage/migrations/0002_external_ids_and_watchlist.sql"),
];

/// Rows the way the Node version wrote them: serial ids and ISO-8601 text.
/// Plain SQL, because the macros check against the current schema, not the legacy one.
const LEGACY_DATA: &str = "
CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
INSERT INTO schema_migrations (version, applied_at) VALUES (1, '2026-01-01T00:00:00.000Z'), (2, '2026-01-02T00:00:00.000Z');
INSERT INTO media (id, kind, title, created_at, updated_at) VALUES
  (1, 'movie', 'Heat', '2026-01-03T00:00:00.000Z', '2026-01-03T00:00:00.000Z'),
  (2, 'show', 'Severance', '2026-01-04T00:00:00.000Z', '2026-01-04T00:00:00.000Z');
INSERT INTO episodes (id, show_id, season, number, title, aired_at, created_at, updated_at) VALUES
  (1, 2, 1, 1, 'Good News About Hell', '2022-02-17', '2026-01-04T00:00:00.000Z', '2026-01-04T00:00:00.000Z');
INSERT INTO plays (id, target_kind, target_id, watched_at, source) VALUES
  (1, 'movie', 1, '2026-01-05T20:00:00.000Z', 'manual'),
  (2, 'episode', 1, '2026-01-06T21:00:00.000Z', 'plex-scrobble'),
  (3, 'movie', 999, '2026-01-07T21:00:00.000Z', 'manual');
INSERT INTO progress (target_kind, target_id, position_ms, duration_ms, state, updated_at) VALUES
  ('episode', 1, 1000, 2000, 'paused', '2026-01-08T00:00:00.000Z');
INSERT INTO ratings (target_kind, target_id, rating, rated_at) VALUES ('show', 2, 9, '2026-01-09T00:00:00.000Z');
INSERT INTO watchlist (target_kind, target_id, listed_at) VALUES ('movie', 1, '2026-01-10T00:00:00.000Z');
";

#[tokio::test]
async fn adopts_a_database_from_the_node_version() -> Result<()> {
    let name = unique_name();
    admin(format!("CREATE DATABASE {name}")).await?;
    let pool = create_pool(&url_for(&name), 2).await?;
    for sql in LEGACY_SCHEMA {
        sqlx::raw_sql(AssertSqlSafe(sql.to_owned()))
            .execute(&pool)
            .await?;
    }
    sqlx::raw_sql(LEGACY_DATA).execute(&pool).await?;

    migrate(&pool).await?;
    migrate(&pool).await?;

    let adopted: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {} WHERE success",
        MIGRATOR.table_name
    )))
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        adopted,
        MIGRATOR.iter().count() as i64,
        "every migration counts as applied"
    );
    let legacy_exists =
        sqlx::query_scalar!(r#"SELECT to_regclass('schema_migrations') IS NOT NULL AS "exists!""#)
            .fetch_one(&pool)
            .await?;
    assert!(!legacy_exists, "the old table is gone");

    let movie = sqlx::query!(r#"SELECT id, created_at FROM media WHERE kind = 'movie'"#)
        .fetch_one(&pool)
        .await?;
    let show = sqlx::query!(r#"SELECT id, created_at FROM media WHERE kind = 'show'"#)
        .fetch_one(&pool)
        .await?;
    assert_eq!(movie.created_at, at("2026-01-03T00:00:00Z"));
    assert!(movie.id < show.id, "ids keep the creation order");
    let id_time = DateTime::<Utc>::from_timestamp_millis(uuid_millis(movie.id)).expect("a time");
    assert!(
        (id_time - movie.created_at).abs() < chrono::Duration::seconds(1),
        "the id carries the creation time: {id_time} is not {}",
        movie.created_at
    );

    let episode = sqlx::query!("SELECT id, show_id, aired_at FROM episodes")
        .fetch_one(&pool)
        .await?;
    assert_eq!(episode.show_id, show.id);
    assert_eq!(
        episode.aired_at.map(|date| date.to_string()).as_deref(),
        Some("2022-02-17")
    );

    let plays =
        sqlx::query!("SELECT target_kind, target_id, watched_at FROM plays ORDER BY watched_at")
            .fetch_all(&pool)
            .await?;
    assert_eq!(plays.len(), 2, "the play of a missing movie is dropped");
    assert_eq!(plays[0].target_id, movie.id);
    assert_eq!(plays[0].watched_at, at("2026-01-05T20:00:00Z"));
    assert_eq!(plays[1].target_id, episode.id);

    let progress = sqlx::query!("SELECT target_id, updated_at FROM progress")
        .fetch_one(&pool)
        .await?;
    assert_eq!(progress.target_id, episode.id);
    assert_eq!(progress.updated_at, at("2026-01-08T00:00:00Z"));
    let rating = sqlx::query!("SELECT target_id, rated_at FROM ratings")
        .fetch_one(&pool)
        .await?;
    assert_eq!(rating.target_id, show.id);
    assert_eq!(rating.rated_at, at("2026-01-09T00:00:00Z"));
    let listed = sqlx::query!("SELECT target_id, listed_at FROM watchlist")
        .fetch_one(&pool)
        .await?;
    assert_eq!(listed.target_id, movie.id);
    assert_eq!(listed.listed_at, at("2026-01-10T00:00:00Z"));

    pool.close().await;
    admin(format!("DROP DATABASE {name}")).await
}

/// The milliseconds since the Unix epoch in the first 48 bits of a UUID v7.
fn uuid_millis(id: Uuid) -> i64 {
    let bytes = id.as_bytes();
    let mut millis: i64 = 0;
    for byte in &bytes[..6] {
        millis = (millis << 8) | i64::from(*byte);
    }
    millis
}
