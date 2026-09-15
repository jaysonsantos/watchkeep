#!/usr/bin/env bash
# Start a throwaway Postgres container, run the command given as arguments
# against it, then remove the container. Set WATCHKEEP_TEST_DATABASE_URL to
# reuse an existing server and skip the container.
#
# The container also serves the query macros at compile time: the script sets
# WATCHKEEP_DATABASE_URL and WATCHKEEP_CATALOG_DATABASE_URL to two databases
# with the current schemas, unless they are already set.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(dirname "$script_dir")"

if [[ -z "${WATCHKEEP_TEST_DATABASE_URL:-}" ]]; then
  IMAGE="${WATCHKEEP_TEST_POSTGRES_IMAGE:-ghcr.io/jaysonsantos/bunderwar:postgres-17.10}"
  NAME="watchkeep-test-$$-$RANDOM"
  CONTAINER=$(docker run -d --rm --name "$NAME" -e POSTGRES_PASSWORD=postgres -p 127.0.0.1::5432 "$IMAGE")
  cleanup() { docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; }
  trap cleanup EXIT

  PORT=$(docker port "$CONTAINER" 5432/tcp | head -n1 | sed 's/.*://')
  export WATCHKEEP_TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${PORT}/postgres"
  export WATCHKEEP_TEST_CONTAINER="$CONTAINER"

  # The image creates its extensions on the first start, which takes a while. The check goes over
  # TCP: during that phase a temporary server answers on the socket only.
  for _ in $(seq 1 240); do
    if docker exec "$CONTAINER" pg_isready -U postgres -h 127.0.0.1 -q 2>/dev/null; then
      break
    fi
    sleep 0.5
  done
  docker exec "$CONTAINER" pg_isready -U postgres -h 127.0.0.1 -q
fi

# Databases for the query macros, so that `cargo build` checks the SQL against the current schemas.
if [[ -z "${WATCHKEEP_DATABASE_URL:-}" && -n "${WATCHKEEP_TEST_CONTAINER:-}" ]]; then
  docker exec "$WATCHKEEP_TEST_CONTAINER" psql -U postgres -q -c "CREATE DATABASE watchkeep_check" -c "CREATE DATABASE catalog_check"
  docker exec -i "$WATCHKEEP_TEST_CONTAINER" psql -U postgres -q -d catalog_check -v ON_ERROR_STOP=1 < "$repo_dir/catalog/schema.sql"
  export WATCHKEEP_DATABASE_URL="${WATCHKEEP_TEST_DATABASE_URL%/postgres}/watchkeep_check"
  export WATCHKEEP_CATALOG_DATABASE_URL="${WATCHKEEP_TEST_DATABASE_URL%/postgres}/catalog_check"
  (cd "$repo_dir/backend/crates/storage" && sqlx migrate run --database-url "$WATCHKEEP_DATABASE_URL" >/dev/null)
fi

"$@"
