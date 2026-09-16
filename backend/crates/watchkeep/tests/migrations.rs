//! The schema applies to an empty database, a second run changes nothing, and
//! the server gives a new row a UUID v7 id.

mod common;

use chrono::{DateTime, Utc};
use common::{admin, at, unique_name, url_for};
use eyre::Result;
use sqlx::AssertSqlSafe;
use uuid::Uuid;
use watchkeep_storage::db::{MIGRATOR, create_pool, migrate};
use watchkeep_telemetry::testing::init_goodies;

#[tokio::test]
async fn applies_the_schema_and_gives_new_rows_a_uuid_v7_from_the_server() -> Result<()> {
    let _guard = init_goodies();
    let name = unique_name();
    admin(format!("CREATE DATABASE {name}")).await?;
    let pool = create_pool(&url_for(&name), 2).await?;
    migrate(&pool).await?;
    migrate(&pool).await?;

    // The migrations table is not part of the schema, so the macros cannot check this statement.
    let applied: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {} WHERE success",
        MIGRATOR.table_name
    )))
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        applied,
        MIGRATOR.iter().count() as i64,
        "every migration is applied once"
    );

    let created_at = at("2026-01-03T00:00:00Z");
    let id = sqlx::query_scalar!(
        "INSERT INTO media (kind, title, created_at, updated_at) VALUES ('movie', 'Heat', $1, $1) RETURNING id",
        created_at
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(id.get_version_num(), 7, "the server makes UUID v7 ids");
    let id_time = DateTime::<Utc>::from_timestamp_millis(uuid_millis(id)).expect("a time");
    assert!(
        (id_time - Utc::now()).abs() < chrono::Duration::seconds(60),
        "the id carries the time of the insert: {id_time}"
    );

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
