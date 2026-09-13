# Watchkeep — notes for agents

Watchkeep is a self-hosted watch tracker with Plex webhook scrobbling. Read
`README.md` first. This file lists what is not obvious from the code.

## Layout

| Path | Purpose |
|---|---|
| `src/main.ts` | Entry point. Commands: `serve`, `sync`, `catalog:import <file>`, `trakt:import <zip>`. |
| `src/app.ts` | Builds the app context (pools, catalog, services) and the Hono app. |
| `src/db.ts` | Postgres pool, migrations (`MIGRATIONS` array), `transaction()`. |
| `src/library.ts` | Writes and single-row reads for media, episodes, plays, progress, ratings. |
| `src/scrobble.ts` | Turns a Plex event into progress or plays. Catalog enrichment lives here. |
| `src/queries.ts` | List and detail reads for the UI and API. |
| `src/views.ts` | Merges local rows with catalog episode lists. |
| `src/actions.ts` | Manual watched and unwatched changes. |
| `src/catalog/` | Read-only TMDB catalog client and the SQLite importer. |
| `src/plex/` | Webhook payload parser and library sync client. |
| `src/trakt/` | Trakt export importer and a minimal ZIP reader. |
| `src/routes/`, `src/ui/` | HTTP routes and server-rendered HTML. |
| `catalog/schema.sql` | TMDB tables for the second database. |
| `test/` | `node:test` suites. `helpers.ts` creates fresh databases per test. |

## Rules

- Add a migration as a new entry at the end of `MIGRATIONS` in `src/db.ts`. Never edit an applied entry.
- Never join across the two databases. Use the `Catalog` class for catalog reads.
- Keep the app working when `catalogPool` is null.
- Timestamps are ISO-8601 text columns. Use the `Clock` interface, not `new Date()`, inside services.
- Use the `html` tagged template in `src/ui/render.ts`. It escapes values. Wrap trusted markup with `raw()`.
- Use pnpm, never npm. Run `pnpm typecheck` and `pnpm test` before you commit. The tests need Docker or `WATCHKEEP_TEST_DATABASE_URL`.

## Plex facts

- Plex sends `multipart/form-data` with a `payload` field that holds JSON. The webhook also accepts plain JSON.
- Episode payloads carry ids of the episode, not of the show. The show has only `grandparentGuid` and `grandparentTitle`.
- `viewOffset` is present on some events only. Keep the last known position when it is absent.
- `media.scrobble` fires at about 90%. A `media.stop` can follow it. The rewatch window prevents a second play.
