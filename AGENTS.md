# Watchkeep — notes for agents

Watchkeep is a self-hosted watch tracker with Plex webhook scrobbling. Read
`README.md` first. This file lists what is not obvious from the code.

## Layout

| Path | Purpose |
|---|---|
| `src/routes/`, `src/lib/components/` | SvelteKit pages and components. |
| `src/hooks.server.ts` | Opens the databases at start-up, sends `/api`, `/webhook`, and `/healthz` to the Hono app, and checks the origin of UI form posts. |
| `src/lib/lists.ts`, `src/lib/format.ts` | Code for the server and the browser: list filter, sort, and page state, and display helpers. |
| `src/lib/server/pages.ts` | Page data and form handling. The SvelteKit load functions and form actions call it. |
| `src/lib/server/main.ts` | CLI entry point. Commands: `serve`, `sync`, `catalog:import <file>`, `trakt:import <zip>`. `serve` starts the SvelteKit build in `build/`. |
| `src/lib/server/app.ts` | Builds the app context (pools, catalog, services) and the Hono app. |
| `src/lib/server/http/` | Hono routes for the JSON API and the Plex webhook. |
| `src/lib/server/db.ts` | Postgres pool, migrations (`MIGRATIONS` array), `transaction()`. |
| `src/lib/server/library.ts` | Writes and single-row reads for media, episodes, plays, progress, ratings. |
| `src/lib/server/scrobble.ts` | Turns a Plex event into progress or plays. Catalog enrichment lives here. |
| `src/lib/server/queries.ts` | List and detail reads for the UI and API. |
| `src/lib/server/views.ts` | Merges local rows with catalog episode lists. |
| `src/lib/server/actions.ts` | Manual watched and unwatched changes. |
| `src/lib/server/catalog/` | Read-only TMDB catalog client and the SQLite importer. |
| `src/lib/server/plex/` | Webhook payload parser and library sync client. |
| `src/lib/server/trakt/` | Trakt export importer and a minimal ZIP reader. |
| `catalog/schema.sql` | TMDB tables for the second database. |
| `test/` | `node:test` suites. `helpers.ts` creates fresh databases per test. |

## Rules

- Add a migration as a new entry at the end of `MIGRATIONS` in `src/lib/server/db.ts`. Never edit an applied entry.
- Never join across the two databases. Use the `Catalog` class for catalog reads.
- Keep the app working when `catalogPool` is null.
- Timestamps are ISO-8601 text columns. Use the `Clock` interface, not `new Date()`, inside services.
- Put page data and form handling in `src/lib/server/pages.ts`, not in `+page.server.ts`. The tests call these functions directly, because `node:test` cannot load SvelteKit modules.
- Code in `src/lib/server/` must not import SvelteKit modules (`$app/*`, `$lib`), so that the CLI and the tests can load it. Use relative imports there.
- Svelte escapes text. Never use `{@html}` with data from a database or from Plex.
- Plex posts webhook forms without an Origin header. So the SvelteKit origin check is off, and `src/hooks.server.ts` checks form posts to the UI.
- Use pnpm, never npm. Run `pnpm typecheck` (tsc and svelte-check), `pnpm test`, and `pnpm build` before you commit. The tests need Docker or `WATCHKEEP_TEST_DATABASE_URL`.

## Plex facts

- Plex sends `multipart/form-data` with a `payload` field that holds JSON. The webhook also accepts plain JSON.
- Episode payloads carry ids of the episode, not of the show. The show has only `grandparentGuid` and `grandparentTitle`.
- `viewOffset` is present on some events only. Keep the last known position when it is absent.
- `media.scrobble` fires at about 90%. A `media.stop` can follow it. The rewatch window prevents a second play.
