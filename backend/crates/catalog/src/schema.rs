//! The catalog schema, for tools that create the catalog database, and for the tests.

use eyre::Result;
use sqlx::PgPool;

/// `catalog/schema.sql` at the repository root, embedded at build time.
pub const CATALOG_SCHEMA_SQL: &str = include_str!("../../../../catalog/schema.sql");

/// Create the catalog tables. Every statement is `IF NOT EXISTS`, so a second call changes nothing.
/// The schema is a file, so the query macros cannot check it.
pub async fn apply_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(CATALOG_SCHEMA_SQL).execute(pool).await?;
    Ok(())
}
