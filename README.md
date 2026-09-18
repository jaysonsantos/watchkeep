<p align="center"><img src="docs/logo.svg" width="96" height="96" alt="Watchkeep logo"></p>

# Watchkeep

Watchkeep is a self-hosted tracker for the movies and TV episodes that you
watched. Plex sends webhooks to Watchkeep. Other players send the same events
to a generic webhook. Watchkeep records plays, playback positions, and ratings.
A web UI and a JSON API show what you watched and what you did not watch.

The server is one Rust binary. It serves the JSON API, the webhooks, and the
web UI.

## Features

- Plex webhook scrobbling for movies and episodes: play, pause, resume, stop, scrobble, and rate.
- Generic scrobble webhook for every other player, with TMDB, IMDb, or TVDB ids and one id per event.
- Playback positions per movie and episode, with a "watched" threshold for stop events.
- Full episode lists from a TMDB catalog database, so unwatched episodes are visible.
- Optional Plex library sync that imports items, watch counts, and resume positions.
- Import of a Trakt data export: history, ratings, playback positions, watchlist, and hidden shows.
- Watchlist for movies and shows.
- Statistics page: watch time, plays per month, per weekday, and per hour, day streaks, top shows, and top movies.
- Recommendations from the watch history: movies, shows, the rest of a collection, and shows that return.
- Add page: search the catalog by title and add a movie or show to the library or the watchlist.
- Manual actions: mark a movie, an episode, or a whole show watched or unwatched.
- Web UI with dashboard, movies, shows, recommendations, watchlist, history, statistics, add page, and a webhook log. The UI needs JavaScript.
- PostgreSQL storage through `sqlx`. The compiler checks every query against the schema. One binary, no runtime dependencies.

## Architecture

Watchkeep uses two PostgreSQL databases on the same instance. The server
needs PostgreSQL 17 or newer with the `pg_uuidv7` extension. The
`ghcr.io/jaysonsantos/bunderwar:postgres-*` images have it.

| Database | Owner | Content |
|---|---|---|
| `watchkeep` | Watchkeep | Plays, progress, ratings, webhook log, and the local media index. |
| `catalog` | Shared, read-only for Watchkeep | TMDB movies, shows, seasons, and episodes. |

The catalog is optional. Without it, Watchkeep tracks only the items that Plex
reported. With it, Watchkeep resolves TMDB, IMDb, and TVDB ids, posters,
runtimes, and the complete episode list of each show.

The catalog tables are defined in `catalog/schema.sql`. That file is the
contract of a central TMDB database: one tool mirrors TMDB into it, and every
app that needs TMDB data reads it with a read-only role. Nobody copies the
data. Watchkeep is one of those readers.

The repository has two source trees. The manifests, the lockfiles, and the tool
configs live at the root, so `cargo` and `pnpm` run from there.

| Path | Content |
|---|---|
| `backend/crates/` | The Rust crates of the Cargo workspace. `storage` owns the Watchkeep database, `catalog` owns the catalog database, and `watchkeep` is the server and the CLI. |
| `frontend/` | The sources of the SvelteKit single-page app. The binary serves its build from `frontend/build`. |
| `frontend/src/lib/generated/` | The TypeScript types of the API. ts-rs writes them from the Rust structs, so the two parts cannot drift. |

Every path outside `/api`, `/webhook`, and `/healthz` returns `index.html`,
and the UI loads its data from the JSON API.

Row ids are UUID v7 values from `uuid_generate_v7()`, so they sort by creation time. Timestamps are
`timestamptz` columns and RFC 3339 text in the API, for example
`2026-01-01T12:00:00Z`. Air dates are `date` columns and `YYYY-MM-DD` text.
Durations and positions are `*_ms` fields in milliseconds.

## Quick start with Docker Compose

1. Copy `.env.example` to `.env`.
2. Set `POSTGRES_PASSWORD` and `WATCHKEEP_WEBHOOK_TOKEN` in `.env`.
3. Run `docker compose up -d`.
4. Open `http://<host>:8484`.

The first start creates both databases and loads the catalog schema.

The image is distroless: it has the binary, the web UI, glibc, and CA
certificates, and no shell. `watchkeep health` is the Docker health check.
A version tag (`v1.2.3`) publishes `ghcr.io/<owner>/watchkeep` for
`linux/amd64` and `linux/arm64`, tagged `1.2.3`, `1.2`, and `latest`.

## Connect Plex

Plex webhooks need a Plex Pass subscription.

1. Open Plex Web and go to Settings, then Webhooks.
2. Add this URL:

```
http://<watchkeep-host>:8484/webhook/plex?token=<WATCHKEEP_WEBHOOK_TOKEN>
```

3. Play a video. Open the Webhooks page in Watchkeep and check that the event arrived.

Plex sends events for every account on the server. Set `WATCHKEEP_PLEX_ACCOUNTS`
to a comma-separated list of account titles or ids to accept only some of them.

## Connect another player

A media server or a player that is not Plex sends its events to
`POST /webhook/scrobble`. The sender names the item with external ids, so it
needs no Watchkeep id.

```
http://<watchkeep-host>:8484/webhook/scrobble?token=<WATCHKEEP_WEBHOOK_TOKEN>
```

The token also goes into the `x-webhook-token` header. The body is
`application/json` and holds one event:

```json
{
  "event_id": "01926f3a-8b7c-7d41-9e2f-5a6b7c8d9e0f",
  "event": "progress",
  "occurred_at": "2026-09-15T20:41:07Z",
  "client": "my-media-server",
  "account": "living-room",
  "player": "Apple TV",
  "media": {
    "type": "episode",
    "title": "The Wolf and the Lion",
    "season": 1,
    "number": 5,
    "ids": { "tmdb": 63060 },
    "show": {
      "title": "Game of Thrones",
      "year": 2011,
      "ids": { "tmdb": 1399, "imdb": "tt0944947", "tvdb": "121361" }
    }
  },
  "position_ms": 1520000,
  "duration_ms": 3300000
}
```

| Field | Required | Meaning |
|---|---|---|
| `event_id` | yes | A UUID from the sender. The sender repeats it when it sends the event again. |
| `event` | yes | `start`, `progress`, `pause`, `stop`, `watched`, or `unwatched`. |
| `occurred_at` | yes | The time of the event on the sender, RFC 3339. |
| `client` | yes | The name of the sending application. |
| `account` | no | The viewer on the sender. |
| `player` | no | The device. The default is `client`. |
| `media.type` | yes | `movie` or `episode`. |
| `media.title`, `media.year` | no | The identity when no id matches. |
| `media.ids` | no | `tmdb`, `imdb`, and `tvdb` of the movie or of the episode. `tmdb` and `tvdb` must be positive numbers; Watchkeep drops any other value. |
| `media.show` | for episodes | `title`, `year`, and `ids` of the show. The show needs a title or an id of its own. |
| `media.season`, `media.number` | for episodes | Season and episode number in TMDB order. |
| `position_ms` | for `start`, `progress`, `pause`, `stop` | The playback position. |
| `duration_ms` | no | The runtime that the player measured. It wins over the catalog runtime. |
| `watched_at` | no | For `watched` only. The time of the play. The default is `occurred_at`. |

What each event does:

| Event | Result |
|---|---|
| `start`, `progress` | Save the position with state `playing`. |
| `pause` | Save the position with state `paused`. |
| `stop` | At or above `WATCHKEEP_WATCHED_THRESHOLD_PERCENT`, record a play. Below it, save the position with state `stopped`. |
| `watched` | Record a play at `watched_at` and clear the position. |
| `unwatched` | Remove every play of the item and clear the position. |

The status code tells the sender what to do next:

| Status | When | Sender action |
|---|---|---|
| `200` | The event was applied, was a duplicate, or was ignored for its account. | Remove the event. |
| `400` | The JSON is not valid, or a required field is missing. | Remove the event. Do not retry. |
| `401` | The token is not correct. | Stop the delivery. Keep the event. |
| `422` | The event has no ids and no title, so no item can match. | Remove the event. Do not retry. |
| `5xx` | A database or server error. | Retry later with the same `event_id`. |

A position event that arrives after a play of the same window answers
`already-watched` and writes nothing, so a watched item stays out of the
in-progress list.

Three rules make retries safe. A play carries its `event_id`, so a second
delivery of the same event adds no second play and answers with the action
`duplicate-event`. An event that is older than the stored position leaves the
position alone and answers with `stale-event`. A play inside
`WATCHKEEP_REWATCH_WINDOW_MINUTES` of a play that the item already has answers
with `duplicate-play`, whether it is older or newer than that play.

Events of one item apply one at a time, so two senders, or a sender and its
own retry, cannot both pass a check and then both write.

Set `WATCHKEEP_SCROBBLE_ACCOUNTS` to a comma-separated list of `account` values
to accept only some viewers. The Webhooks page shows every event under the name
`scrobble.<event>`.

## Load the TMDB catalog

Watchkeep does not fetch TMDB data itself. It reads a central TMDB database
that a mirror tool fills, so that several apps share one copy. Create the
tables from `catalog/schema.sql`, fill them with your tool, and give Watchkeep
a read-only role. Run the grants as the role that owns the tables:

```sql
CREATE ROLE watchkeep LOGIN PASSWORD '...';
GRANT CONNECT ON DATABASE tmdb TO watchkeep;
GRANT USAGE ON SCHEMA public TO watchkeep;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO watchkeep;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO watchkeep;
```

Watchkeep only reads `tmdb_movie`, `tmdb_show`, `tmdb_season`, and
`tmdb_episode`. The tool can run at any time, also while Watchkeep runs.

## Sync the Plex library

Set `WATCHKEEP_PLEX_URL` (for example `http://plex:32400`) and `WATCHKEEP_PLEX_TOKEN`.
Then do one of these:

- Press "Sync Plex library" on the dashboard.
- Run `docker compose run --rm watchkeep sync`.
- Set `WATCHKEEP_SYNC_INTERVAL_MINUTES` to sync on a timer.

The sync imports every movie and episode, marks items that Plex counts as
watched, and copies resume positions.

## Import your Trakt data

1. On trakt.tv, open Settings, then Data, and request an export. Trakt sends a ZIP file.
2. Run the import:

```
docker compose run --rm -v /path/to/trakt-export.zip:/export.zip:ro watchkeep trakt:import /export.zip
```

Add `--dry-run` to see the counts without a write.

The import reads these files from the ZIP:

| File | Result |
|---|---|
| `watched-history-*.json` | One play per record, with the Trakt history id. A second import adds no duplicate. |
| `ratings-movies.json`, `ratings-shows.json`, `ratings-episodes-*.json` | Ratings. |
| `watched-playback.json` | Resume positions. Needs a runtime from the catalog. |
| `lists-watchlist.json` | The watchlist. |
| `hidden-progress-watched.json` | Hidden shows. Hidden shows do not appear as unwatched. |
| `_errors.json` | Reported in the summary. |

Other files (collection, comments, likes, custom lists, notes, network, profile) are ignored.
Plex guids from the export are stored, so later Plex webhooks match the same items.

## Commands

The binary has four commands. `watchkeep --help` lists every flag. Flags go
before the command: `watchkeep --port 9000 health`.

| Command | Meaning |
|---|---|
| `watchkeep serve` | Serve the web UI, the JSON API, and the webhooks. This is the default. |
| `watchkeep sync` | Run a Plex library sync and print the report as JSON. |
| `watchkeep trakt:import <zip> [--dry-run]` | Import a Trakt export and print the report as JSON. |
| `watchkeep health` | Ask the server on this port for `/healthz`. Exit code 0 means healthy. |

## Configuration

Every setting is a flag and an environment variable. `--port 9000` and
`WATCHKEEP_PORT=9000` do the same.

| Variable | Default | Meaning |
|---|---|---|
| `WATCHKEEP_DATABASE_URL` | `postgres://watchkeep:watchkeep@localhost:5432/watchkeep` | Watchkeep database. |
| `WATCHKEEP_CATALOG_DATABASE_URL` | empty | TMDB catalog database. Empty disables the catalog. |
| `WATCHKEEP_CATALOG_LANGUAGE` | `en` | Title language: `en` or `pt`. |
| `WATCHKEEP_WEBHOOK_TOKEN` | empty | Required token on the webhook URL. Empty accepts any caller. |
| `WATCHKEEP_WATCHED_THRESHOLD_PERCENT` | `85` | A stop event at or above this percentage records a play. |
| `WATCHKEEP_REWATCH_WINDOW_MINUTES` | `360` | Two plays of one item inside this window count as one play. |
| `WATCHKEEP_PLEX_ACCOUNTS` | empty | Accepted Plex account titles or ids. Empty accepts all. |
| `WATCHKEEP_SCROBBLE_ACCOUNTS` | empty | Accepted accounts on `/webhook/scrobble`. Empty accepts all. |
| `WATCHKEEP_PLEX_URL` | empty | Plex server URL for library sync. |
| `WATCHKEEP_PLEX_TOKEN` | empty | Plex token for library sync. |
| `WATCHKEEP_SYNC_INTERVAL_MINUTES` | `0` | Timer for library sync. `0` disables. |
| `WATCHKEEP_WEBHOOK_RETENTION_DAYS` | `30` | Days to keep raw webhook events. `0` keeps them. |
| `WATCHKEEP_IMAGE_BASE_URL` | `https://image.tmdb.org/t/p/` | Base URL for posters. |
| `WATCHKEEP_HOST` / `WATCHKEEP_PORT` | `0.0.0.0` / `8484` | Listen address. |
| `WATCHKEEP_STATIC_DIR` | `frontend/build` | Directory with the built web UI. |
| `WATCHKEEP_HTTP_ORIGIN` | empty | Public URL, for example `https://watchkeep.example.com`. Set it behind a reverse proxy that changes the `Host` header. |
| `WATCHKEEP_HTTP_HOST_HEADER` | `host` | Header that carries the host of the request, for example `x-forwarded-host`. |
| `RUST_LOG` | `info` | Log filter of the server. See [Observability](#observability). |

The UI buttons call the JSON API with JSON bodies. Watchkeep rejects a form
post (`application/x-www-form-urlencoded`, `multipart/form-data`, or
`text/plain`) to the UI or the API when the host in the `Origin` header of
the browser is not the host of the request. A reverse proxy that changes the
`Host` header makes such posts return "403 Cross-site form submissions are
forbidden". In that case, set `WATCHKEEP_HTTP_ORIGIN`, or set
`WATCHKEEP_HTTP_HOST_HEADER=x-forwarded-host`. The webhook is exempt, because
Plex posts forms without an `Origin` header.

## Observability

Watchkeep exports traces and metrics with OpenTelemetry over OTLP/HTTP, and it
writes logs to stdout. The SDK sends to `http://127.0.0.1:4318` when no
variable points somewhere else. No collector address is in the code. Without a
collector the exporters give up after two seconds, and the server continues.

| Variable | Default | Meaning |
|---|---|---|
| `OTEL_SERVICE_NAME` | `watchkeep` | Name of this process in the collector. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://127.0.0.1:4318` | Address of the collector. |
| `OTEL_EXPORTER_OTLP_HEADERS` | empty | Extra headers, for example `Authorization=<token>`. Keep the token in a secret, not in a unit file or a manifest. |
| `RUST_LOG` | `info` | Filter of the log lines and of the exported spans. |
| `CONSOLE_SUBSCRIBER` | empty | A port. When it is set, tokio-console starts. |
| `CONSOLE_SUBSCRIBER_ADDRESS` | `127.0.0.1` | Bind address of tokio-console. |

Docker Compose passes `OTEL_EXPORTER_OTLP_ENDPOINT` and
`OTEL_EXPORTER_OTLP_HEADERS` to the container when `.env` sets them. The
address `127.0.0.1` is the container itself, so a collector on another host
needs the endpoint variable.

A span never carries a query string, because the webhook URL carries the token.
The span of a request holds the path only.

Without a collector, the SDK writes one error per failed export. To silence
it, set `RUST_LOG=info,opentelemetry_sdk=off,opentelemetry-otlp=off`.

Every span and every metric carries the resource of the process: the service
name and `service.version` from the manifest.

The server makes one `server` span for each HTTP request. The span takes its
parent from the `traceparent` header of the caller, and it carries the method,
the matched route, the URL, and the status code. Each service call below it is
one span.

The Plex client makes one `client` span per call and sends the trace context
with it, so a Plex side with tracing joins the same trace. The database driver
reports each statement as an event on the target `sqlx::query`: the summary,
the SQL, the row counts, and the time. The level is `debug`, and `warn` for a
statement that takes more than one second. Use `RUST_LOG=info,sqlx::query=debug`
to see every query of a request in its span.

| Instrument | Unit | Labels |
|---|---|---|
| `watchkeep_http_requests_total` | requests | `http.method`, `http.route`, `status`, `otel.kind` |
| `watchkeep_http_request_duration_ms` | ms | the same |
| `watchkeep_http_active_requests` | requests | `http.method`, `http.route`, `otel.kind` |
| `watchkeep_webhook_events_total` | events | `plex_event`, `webhook_outcome` |
| `watchkeep_scrobble_events_total` | events | `plex_event`, `scrobble_action` (`error` when the event failed) |
| `watchkeep_plex_sync_runs_total`, `watchkeep_plex_sync_duration_ms` | runs, ms | `sync_status` |
| `watchkeep_trakt_imports_total`, `watchkeep_trakt_import_duration_ms` | runs, ms | `import_status` |

## Security

Watchkeep has no login. Run it on a private network or behind a reverse proxy
with authentication. Only the webhook route checks a token.

## JSON API

Ids in paths and bodies are UUID strings. A path with a value that is not a
UUID returns `400 Bad Request`.

| Method | Path | Meaning |
|---|---|---|
| `GET` | `/api/config` | Image base URL, and whether the Plex sync and the catalog are configured. |
| `GET` | `/api/stats` | Counts of movies, shows, episodes, and plays. |
| `GET` | `/api/statistics?tz=Europe/Berlin` | Watch time, calendar buckets, streaks, and top lists. `tz` is an IANA name; an unknown name reads as UTC. |
| `GET` | `/api/progress` | Items with a saved playback position. |
| `DELETE` | `/api/progress/:kind/:id` | Remove the playback position of a `movie` or an `episode`. |
| `GET` | `/api/history?limit=50&offset=0` | Plays, newest first. |
| `DELETE` | `/api/history/:id` | Remove one play. |
| `GET` | `/api/movies?status=all\|watched\|unwatched&q=&sort=&limit=&offset=` | Movies. See [List order and pages](#list-order-and-pages). |
| `GET` | `/api/movies/:id` | One movie with its progress. |
| `POST` / `DELETE` | `/api/movies/:id/watched` | Mark a movie watched or unwatched. The body can carry `watched_at`, RFC 3339. |
| `GET` | `/api/shows?status=&q=&sort=&limit=&offset=` | Shows with watched and total episode counts. |
| `GET` | `/api/shows/:id` | One show with the merged episode list and `on_watchlist`. |
| `POST` / `DELETE` | `/api/shows/:id/watched` | Mark every aired episode watched or unwatched. |
| `POST` / `DELETE` | `/api/episodes/:id/watched` | Mark a known episode. The body can carry `watched_at`. |
| `POST` / `DELETE` | `/api/shows/:id/seasons/:s/episodes/:e/watched` | Mark an episode by number. |
| `POST` | `/api/sync` | Run a Plex library sync. |
| `GET` | `/api/search?q=` | Catalog search by title. Needs the catalog. Each result carries `localId` when the library has the item. |
| `POST` | `/api/movies`, `/api/shows` | Add an item. Body: `tmdb_id` or `title`, optional `year` and `watchlist`. |
| `GET` | `/api/recommendations` | What to watch next, with the taste profile. Needs the catalog. See [How recommendations work](#how-recommendations-work). |
| `GET` | `/api/watchlist` | Watchlist with watched counts. |
| `POST` / `DELETE` | `/api/movies/:id/watchlist`, `/api/shows/:id/watchlist` | Add to or remove from the watchlist. |
| `POST` / `DELETE` | `/api/shows/:id/hidden` | Hide a show from the unwatched list, or unhide it. |
| `GET` | `/api/webhooks` | The last 100 webhook events. |
| `POST` | `/webhook/plex?token=` | Plex webhook endpoint. Accepts multipart or JSON. |
| `POST` | `/webhook/scrobble?token=` | Generic scrobble endpoint. Accepts JSON. |
| `GET` | `/healthz` | Health check. Runs one query on the Watchkeep database. |

An unknown `status` or `sort` value returns `400 Bad Request`.

### List order and pages

The movie and show lists accept a `sort` value:

| Value | Order |
|---|---|
| `recent` | Last watched first. Items with no play come next, newest addition first. This is the default. |
| `title` | Title, A to Z. |
| `year` | Year, newest first. |
| `added` | Date added to Watchkeep, newest first. |

The UI pages show 60 items on each page. Use `page` to go to a different page.

The API returns the full list when `limit` is absent. `limit` accepts 1 to 500.
The `X-Total-Count` response header gives the number of matches before
`limit` and `offset`.

## How scrobbling works

- `media.play`, `media.pause`, `media.resume`: save the position and state.
- `media.scrobble`: Plex sends it at about 90%. Watchkeep records a play and clears the position.
- `media.stop`: if the position is at or above the threshold, record a play. Otherwise save the position.
- `media.rate`: save the rating.
- A play inside the rewatch window of the last play of the same item is not recorded twice.
- A position event inside that window is dropped too, so a late `media.stop` or `media.pause`
  does not send a watched item back to the in-progress list. The webhook log shows `already-watched`.

Item identity: Plex guid, then TMDB id, then TVDB id, then IMDb id, then title and year.
With a catalog, an episode TMDB id also resolves its show.

## How recommendations work

The recommendations need the catalog. Without it, every list is empty.

There is no rating page. The watch history is the input, and a rating only
changes a weight when there is one. Watchkeep gives each watched item a weight
from these signals:

| Signal | Effect on the weight |
|---|---|
| A play | The base weight. An item with no play weighs nothing. |
| A replay | Up to twice the weight. |
| The part of a show that is watched | A show that was dropped early keeps 20% of the weight. |
| The age of the last play | Half of the weight after two years, and never less than 30%. |
| A rating from Plex or from Trakt | A rating above 6.5 lifts the weight, a rating below it cuts the weight. |

The weights add up per genre and per original language. The genres travel by
name, because TMDB numbers the genres of a movie and of a show apart: your
taste in "Drama" therefore applies to both. The catalog then scores its own
items against that vector, times the Bayesian TMDB rating, plus a lift for a
language that you watch often. The page shows the best of four lists:

- **Movies for you** and **Shows for you**: the closest match to your genres.
- **Next in a collection**: a movie of a collection that your library holds a part of.
- **Returning shows**: a show of your library that is complete and still makes episodes.

Everything that the library already holds stays out of the lists, so a
recommendation is always new.

## Development

`flake.nix` gives a shell with Rust, sqlx-cli, Node 24, pnpm, psql, and cmake.
Enter it with `nix develop`, or with direnv:

```
cat > .envrc <<'EOF'
dotenv_if_exists
use flake
EOF
direnv allow
```

`.envrc` and `.env` stay local. `.env.example` lists the variables. The
development section at its end names the two databases for the query macros
and the server for the tests. Without them, `cargo build` uses the offline
query data in `.sqlx`, and the tests start a Postgres container.

Every SQL statement is a `sqlx` macro. The compiler checks it against the
database that `WATCHKEEP_DATABASE_URL` or `WATCHKEEP_CATALOG_DATABASE_URL`
names, or against `.sqlx` when the variable is absent. Migrations are the SQL
files in `backend/crates/storage/migrations/`; the server applies them at start.

```
scripts/test-db.sh cargo test --workspace     # starts Postgres in Docker, loads both schemas, runs the tests
scripts/test-db.sh scripts/sqlx-prepare.sh    # refreshes .sqlx after a change to a query or a migration
cargo test --workspace --lib export_bindings  # rewrites frontend/src/lib/generated from the Rust structs
cargo clippy --workspace --all-targets -- -D warnings
cargo run -- serve                            # needs WATCHKEEP_DATABASE_URL, serves frontend/build on port 8484
cargo run -- --help                           # every command, flag, and variable
```

```
pnpm install
pnpm lint                                     # Biome: lint, format check, import order
pnpm format                                   # Biome: write the fixes
pnpm typecheck                                # svelte-check
pnpm test                                     # unit tests of the list helpers
pnpm build                                    # static UI in frontend/build
pnpm dev                                      # Vite dev server on port 5173, proxies /api to port 8484
```

Every linter runs through one command from the repository root. `prek` reads
`.pre-commit-config.yaml`; the tools come from the flake.

```
prek run --all-files                          # cargo fmt, clippy, taplo, Biome, svelte-check, shellcheck, hadolint, nixfmt, typos
prek install                                  # run them on each commit
```

Set `WATCHKEEP_TEST_DATABASE_URL` to run the tests against an existing Postgres
server instead of Docker. The tests create and drop their own databases.

GitHub Actions run the same linters and tests on every push and pull request
(`.github/workflows/ci.yml`), and build and push the image on a `v*` tag
(`.github/workflows/release.yml`). The Dockerfile cross-compiles the arm64
binary, so one amd64 runner builds both platforms.

## Make a release

The commit messages follow [Conventional Commits](https://www.conventionalcommits.org).
git-cliff reads them and writes `CHANGELOG.md`.

The simple way is one button: start the `Release` workflow from the Actions
page, or from the command line.

```
gh workflow run Release                       # the version comes from the commits
gh workflow run Release -f version=0.3.0      # the version comes from the input
```

The workflow makes the release commit and the tag on the runner, pushes them,
builds the image, and publishes the GitHub release.

The same release also runs from a machine, which is the way when `main` is
protected:

```
scripts/release.sh                            # the version comes from the commits
scripts/release.sh 0.3.0                      # the version comes from the argument
git push --follow-tags                        # start the release workflow
```

Both ways run the same script. It computes the next version, writes it into
`Cargo.toml` and `package.json`, regenerates `CHANGELOG.md`, commits, and makes
the annotated tag. A `feat` commit bumps the minor number and a `fix` commit
bumps the patch number. The version is below 1.0.0, so a breaking change bumps
the minor number too. The workflow then pushes the image and publishes the
GitHub release with the notes from the tag.

Without the flake: Rust 1.94 or newer, Node 24, pnpm, Docker (the tests pull
the Postgres image), and sqlx-cli 0.9,
installed with
`cargo install sqlx-cli --no-default-features --features postgres,rustls,sqlx-toml`.

## License

MIT. See `LICENSE`.
