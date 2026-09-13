# Watchkeep

Watchkeep is a self-hosted tracker for the movies and TV episodes that you
watched. Plex sends webhooks to Watchkeep. Watchkeep records plays, playback
positions, and ratings. A web UI and a JSON API show what you watched and what
you did not watch.

## Features

- Plex webhook scrobbling for movies and episodes: play, pause, resume, stop, scrobble, and rate.
- Playback positions per movie and episode, with a "watched" threshold for stop events.
- Full episode lists from a TMDB catalog database, so unwatched episodes are visible.
- Optional Plex library sync that imports items, watch counts, and resume positions.
- Manual actions: mark a movie, an episode, or a whole show watched or unwatched.
- Web UI with dashboard, movies, shows, history, and a webhook log.
- PostgreSQL storage. No native modules.

## Architecture

Watchkeep uses two PostgreSQL databases on the same instance:

| Database | Owner | Content |
|---|---|---|
| `watchkeep` | Watchkeep | Plays, progress, ratings, webhook log, and the local media index. |
| `catalog` | Shared, read-only for Watchkeep | TMDB movies, shows, seasons, and episodes. |

The catalog is optional. Without it, Watchkeep tracks only the items that Plex
reported. With it, Watchkeep resolves TMDB, IMDb, and TVDB ids, posters,
runtimes, and the complete episode list of each show.

The catalog tables are defined in `catalog/schema.sql`. Any tool that fills them works.

## Quick start with Docker Compose

1. Copy `.env.example` to `.env`.
2. Set `POSTGRES_PASSWORD` and `WATCHKEEP_WEBHOOK_TOKEN` in `.env`.
3. Run `docker compose up -d`.
4. Open `http://<host>:8484`.

The first start creates both databases and loads the catalog schema.

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

## Load the TMDB catalog

Watchkeep does not fetch TMDB data itself. Fill the `catalog` database in one of two ways:

- Import a SQLite file that has the same tables (see `catalog/schema.sql`):

```
docker compose run --rm -v /path/to/catalog.sqlite:/catalog.sqlite:ro watchkeep catalog:import /catalog.sqlite
```

- Insert rows into the tables in `catalog/schema.sql` with your own tool.

The import is idempotent. Run it again after the source changes.

## Sync the Plex library

Set `WATCHKEEP_PLEX_URL` (for example `http://plex:32400`) and `WATCHKEEP_PLEX_TOKEN`.
Then either:

- Press "Sync Plex library" on the dashboard, or
- Run `docker compose run --rm watchkeep sync`, or
- Set `WATCHKEEP_SYNC_INTERVAL_MINUTES` to sync on a timer.

The sync imports every movie and episode, marks items that Plex counts as
watched, and copies resume positions.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `WATCHKEEP_DATABASE_URL` | `postgres://watchkeep:watchkeep@localhost:5432/watchkeep` | Watchkeep database. |
| `WATCHKEEP_CATALOG_DATABASE_URL` | empty | TMDB catalog database. Empty disables the catalog. |
| `WATCHKEEP_CATALOG_LANGUAGE` | `en` | Title language: `en` or `pt`. |
| `WATCHKEEP_WEBHOOK_TOKEN` | empty | Required token on the webhook URL. Empty accepts any caller. |
| `WATCHKEEP_WATCHED_THRESHOLD_PERCENT` | `85` | A stop event at or above this percentage records a play. |
| `WATCHKEEP_REWATCH_WINDOW_MINUTES` | `360` | Two plays of one item inside this window count as one play. |
| `WATCHKEEP_PLEX_ACCOUNTS` | empty | Accepted Plex account titles or ids. Empty accepts all. |
| `WATCHKEEP_PLEX_URL` | empty | Plex server URL for library sync. |
| `WATCHKEEP_PLEX_TOKEN` | empty | Plex token for library sync. |
| `WATCHKEEP_SYNC_INTERVAL_MINUTES` | `0` | Timer for library sync. `0` disables. |
| `WATCHKEEP_WEBHOOK_RETENTION_DAYS` | `30` | Days to keep raw webhook events. `0` keeps them. |
| `WATCHKEEP_IMAGE_BASE_URL` | `https://image.tmdb.org/t/p/` | Base URL for posters. |
| `WATCHKEEP_HOST` / `WATCHKEEP_PORT` | `0.0.0.0` / `8484` | Listen address. |

## Security

Watchkeep has no login. Run it on a private network or behind a reverse proxy
with authentication. Only the webhook route checks a token.

## JSON API

| Method | Path | Meaning |
|---|---|---|
| `GET` | `/api/stats` | Counts of movies, shows, episodes, and plays. |
| `GET` | `/api/progress` | Items with a saved playback position. |
| `GET` | `/api/history?limit=50&offset=0` | Plays, newest first. |
| `DELETE` | `/api/history/:id` | Remove one play. |
| `GET` | `/api/movies?status=all\|watched\|unwatched&q=` | Movies. |
| `GET` | `/api/movies/:id` | One movie with its progress. |
| `POST` / `DELETE` | `/api/movies/:id/watched` | Mark a movie watched or unwatched. |
| `GET` | `/api/shows?status=&q=` | Shows with watched and total episode counts. |
| `GET` | `/api/shows/:id` | One show with the merged episode list. |
| `POST` / `DELETE` | `/api/shows/:id/watched` | Mark every aired episode watched or unwatched. |
| `POST` / `DELETE` | `/api/episodes/:id/watched` | Mark a known episode. |
| `POST` / `DELETE` | `/api/shows/:id/seasons/:s/episodes/:e/watched` | Mark an episode by number. |
| `POST` | `/api/sync` | Run a Plex library sync. |
| `GET` | `/api/webhooks` | The last 100 webhook events. |
| `POST` | `/webhook/plex?token=` | Plex webhook endpoint. Accepts multipart or JSON. |

## How scrobbling works

- `media.play`, `media.pause`, `media.resume`: save the position and state.
- `media.scrobble`: Plex sends it at about 90%. Watchkeep records a play and clears the position.
- `media.stop`: if the position is at or above the threshold, record a play. Otherwise save the position.
- `media.rate`: save the rating.
- A play inside the rewatch window of the last play of the same item is not recorded twice.

Item identity: Plex guid, then TMDB id, then TVDB id, then IMDb id, then title and year.
With a catalog, an episode TMDB id also resolves its show.

## Development

Requirements: Node 24, pnpm, and Docker (for the test database).

```
pnpm install
pnpm typecheck
pnpm test           # starts a throwaway Postgres container
pnpm build
pnpm dev            # needs WATCHKEEP_DATABASE_URL
```

For local development with direnv, copy `.envrc.example` to `.envrc`, set the
two database URLs, and run `direnv allow`. `.envrc` is ignored by git.

Set `WATCHKEEP_TEST_DATABASE_URL` to run the tests against an existing Postgres
server instead of Docker. The tests create and drop their own databases.

## License

MIT. See `LICENSE`.
