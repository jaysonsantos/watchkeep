#!/usr/bin/env bash
# Runs once on the first start of the Postgres container.
# Creates the `catalog` database next to `watchkeep` and loads the TMDB schema.
set -euo pipefail
psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "CREATE DATABASE catalog"
psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d catalog -f /docker-entrypoint-initdb.d/catalog-schema.sql
