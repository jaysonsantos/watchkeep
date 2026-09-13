import type { Handle, ServerInit } from "@sveltejs/kit";
import type { Hono } from "hono";
import { createApp, createContext, type AppContext } from "$lib/server/app.ts";
import { loadConfig } from "$lib/server/config.ts";
import { isCrossSiteFormPost } from "$lib/server/csrf.ts";
import { createPool, openDatabase } from "$lib/server/db.ts";

/** Paths that the Hono app serves: the JSON API, the Plex webhook, and the health check. */
const HONO_PATHS = /^\/(api|webhook)(\/|$)|^\/healthz$/;

let ctx: AppContext;
let hono: Hono;

export const init: ServerInit = async () => {
  const config = loadConfig();
  const pool = await openDatabase(config.databaseUrl);
  const catalogPool = config.catalogDatabaseUrl ? createPool(config.catalogDatabaseUrl, 3) : null;
  ctx = createContext({ pool, catalogPool, config });
  hono = createApp(ctx);
  ctx.log(`catalog ${catalogPool ? "enabled" : "disabled (set WATCHKEEP_CATALOG_DATABASE_URL to enable)"}`);
  if (!config.webhookToken) ctx.log("warning: WATCHKEEP_WEBHOOK_TOKEN is empty, the webhook accepts any caller");

  const timers: NodeJS.Timeout[] = [];
  if (config.syncIntervalMinutes > 0 && config.plexUrl && config.plexToken) {
    const run = () => ctx.sync().catch((error: Error) => ctx.log(`sync failed: ${error.message}`));
    timers.push(setTimeout(run, 5_000), setInterval(run, config.syncIntervalMinutes * 60_000));
  }
  // adapter-node emits this event after the HTTP server closes.
  process.on("sveltekit:shutdown", () => {
    ctx.log("shutting down");
    for (const timer of timers) clearTimeout(timer);
    void ctx.close();
  });
};

export const handle: Handle = async ({ event, resolve }) => {
  if (HONO_PATHS.test(event.url.pathname)) return hono.fetch(event.request);
  if (isCrossSiteFormPost(event.request, event.url.host)) {
    return new Response("Cross-site form submissions are forbidden", { status: 403 });
  }
  event.locals.ctx = ctx;
  return resolve(event);
};
