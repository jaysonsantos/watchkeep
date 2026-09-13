#!/usr/bin/env bash
# Start a throwaway Postgres container, run the command given as arguments
# against it, then remove the container. Set WATCHKEEP_TEST_DATABASE_URL to
# reuse an existing server and skip the container.
set -euo pipefail

if [[ -n "${WATCHKEEP_TEST_DATABASE_URL:-}" ]]; then
  exec "$@"
fi

IMAGE="${WATCHKEEP_TEST_POSTGRES_IMAGE:-postgres:17-alpine}"
NAME="watchkeep-test-$$-$RANDOM"
CONTAINER=$(docker run -d --rm --name "$NAME" -e POSTGRES_PASSWORD=postgres -p 127.0.0.1::5432 "$IMAGE")
cleanup() { docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; }
trap cleanup EXIT

PORT=$(docker port "$CONTAINER" 5432/tcp | head -n1 | sed 's/.*://')
export WATCHKEEP_TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${PORT}/postgres"

for _ in $(seq 1 60); do
  if docker exec "$CONTAINER" pg_isready -U postgres -q 2>/dev/null; then
    break
  fi
  sleep 0.5
done
docker exec "$CONTAINER" pg_isready -U postgres -q

"$@"
