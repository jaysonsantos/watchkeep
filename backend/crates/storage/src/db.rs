//! Pool creation and schema migrations. sqlx applies the SQL files in
//! `migrations/` and records them in `_sqlx_migrations`.

use eyre::{Result, WrapErr};
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub const DEFAULT_POOL_SIZE: u32 = 5;

pub async fn create_pool(url: &str, max_connections: u32) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await?)
}

/// Open a pool and apply pending migrations.
pub async fn open_database(url: &str) -> Result<PgPool> {
    let pool = create_pool(url, DEFAULT_POOL_SIZE).await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// Apply pending migrations. sqlx keeps the state in `_sqlx_migrations` and takes an advisory lock.
pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .wrap_err("database migration failed")?;
    Ok(())
}

/// One round trip on the pool, for the health check.
pub async fn ping(pool: &PgPool) -> Result<()> {
    sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
        .fetch_one(pool)
        .await?;
    Ok(())
}
