#!/usr/bin/env bash
# Regenerate the offline query data in `.sqlx`, so that `cargo build`
# works without a database (for example in Docker with SQLX_OFFLINE=true).
# Run it after a change to a query or to a migration, and commit the result:
#
#   scripts/test-db.sh scripts/sqlx-prepare.sh
#
# Needs sqlx-cli 0.9: cargo install sqlx-cli --no-default-features --features postgres,rustls,sqlx-toml
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(dirname "$script_dir")"

: "${WATCHKEEP_DATABASE_URL:?set WATCHKEEP_DATABASE_URL to a database with the Watchkeep migrations applied}"
: "${WATCHKEEP_CATALOG_DATABASE_URL:?set WATCHKEEP_CATALOG_DATABASE_URL to a database with catalog/schema.sql applied}"

cd "$repo_dir"
cargo sqlx prepare --workspace -- --all-targets
echo "offline query data written to .sqlx"
