import { Hono } from "hono";
import { Actions } from "./actions.ts";
import { Catalog } from "./catalog/catalog.ts";
import type { Config } from "./config.ts";
import type { Pool } from "./db.ts";
import { Library, type Clock, systemClock } from "./library.ts";
import { syncPlexLibrary, type SyncReport } from "./plex/sync.ts";
import { Queries } from "./queries.ts";
import { Scrobbler } from "./scrobble.ts";
import { apiRoutes } from "./http/api.ts";
import { webhookRoutes } from "./http/webhook.ts";
import { Views } from "./views.ts";

export interface AppContext {
  pool: Pool;
  catalogPool: Pool | null;
  config: Config;
  clock: Clock;
  catalog: Catalog | null;
  library: Library;
  scrobbler: Scrobbler;
  queries: Queries;
  views: Views;
  actions: Actions;
  log: (message: string) => void;
  /** Run a Plex library sync. Rejects when Plex is not configured. */
  sync: () => Promise<SyncReport>;
  close: () => Promise<void>;
}

export interface AppOptions {
  pool: Pool;
  catalogPool?: Pool | null;
  config: Config;
  clock?: Clock;
  fetchImpl?: typeof fetch;
  log?: (message: string) => void;
}

export function createContext(options: AppOptions): AppContext {
  const clock = options.clock ?? systemClock;
  const log = options.log ?? ((message: string) => console.log(`[watchkeep] ${message}`));
  const catalogPool = options.catalogPool ?? null;
  const catalog = catalogPool ? new Catalog(catalogPool, options.config.catalogLanguage) : null;
  const queries = new Queries(options.pool);
  let running: Promise<SyncReport> | null = null;
  return {
    pool: options.pool,
    catalogPool,
    config: options.config,
    clock,
    catalog,
    library: new Library(options.pool, clock),
    scrobbler: new Scrobbler(options.pool, options.config, catalog, clock),
    queries,
    views: new Views(queries, catalog, clock),
    actions: new Actions(options.pool, catalog, clock),
    log,
    sync: () => {
      if (!running) {
        running = syncPlexLibrary(
          options.pool,
          catalog,
          {
            plexUrl: options.config.plexUrl,
            plexToken: options.config.plexToken,
            fetchImpl: options.fetchImpl,
            log,
          },
          clock,
        ).finally(() => {
          running = null;
        });
      }
      return running;
    },
    close: async () => {
      await options.pool.end();
      await catalogPool?.end();
    },
  };
}

export function createApp(ctx: AppContext): Hono {
  const app = new Hono();
  app.get("/healthz", async (c) => {
    await ctx.pool.query("SELECT 1");
    return c.json({ ok: true, catalog: ctx.catalog !== null });
  });
  app.route("/webhook", webhookRoutes(ctx));
  app.route("/api", apiRoutes(ctx));
  app.onError((error, c) => {
    ctx.log(`error: ${error.stack ?? error.message}`);
    return c.json({ error: error.message }, 500);
  });
  return app;
}
