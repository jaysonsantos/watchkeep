//! Pool creation and schema migrations. sqlx applies the SQL files in
//! `migrations/` and records them in `_sqlx_migrations`.

use eyre::{Result, WrapErr};
use sqlx::migrate::{Migrate, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Connection, PgPool};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub const DEFAULT_POOL_SIZE: u32 = 5;

/// The table where the Node version of Watchkeep recorded its schema version.
const LEGACY_MIGRATIONS_TABLE: &str = "schema_migrations";

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
    adopt_legacy_migrations(pool).await?;
    MIGRATOR
        .run(pool)
        .await
        .wrap_err("database migration failed")?;
    Ok(())
}

/// Databases from the Node version record their schema version in `schema_migrations`.
/// Copy those versions into the sqlx table once, so that sqlx does not apply them again,
/// then drop the old table.
async fn adopt_legacy_migrations(pool: &PgPool) -> Result<()> {
    let legacy_exists = sqlx::query_scalar!(
        r#"SELECT to_regclass($1) IS NOT NULL AS "exists!""#,
        LEGACY_MIGRATIONS_TABLE
    )
    .fetch_one(pool)
    .await?;
    if !legacy_exists {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    // The legacy table is not part of the schema, so the macros cannot check these statements.
    let applied: Option<i64> = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT MAX(version)::bigint FROM {LEGACY_MIGRATIONS_TABLE}"
    )))
    .fetch_one(&mut *conn)
    .await?;
    let applied = applied.unwrap_or(0);
    conn.ensure_migrations_table(&MIGRATOR.table_name).await?;
    let mut tx = conn.begin().await?;
    for migration in MIGRATOR
        .iter()
        .filter(|migration| migration.version <= applied)
    {
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO {} (version, description, success, checksum, execution_time)
             VALUES ($1, $2, TRUE, $3, 0) ON CONFLICT (version) DO NOTHING",
            MIGRATOR.table_name
        )))
        .bind(migration.version)
        .bind(&*migration.description)
        .bind(&*migration.checksum)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(AssertSqlSafe(format!(
        "DROP TABLE {LEGACY_MIGRATIONS_TABLE}"
    )))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!("adopted {applied} migrations from the legacy {LEGACY_MIGRATIONS_TABLE} table");
    Ok(())
}

/// One round trip on the pool, for the health check.
pub async fn ping(pool: &PgPool) -> Result<()> {
    sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
        .fetch_one(pool)
        .await?;
    Ok(())
}
