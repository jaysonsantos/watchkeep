import { serve } from "@hono/node-server";
import { createApp, createContext } from "./app.ts";
import { loadConfig } from "./config.ts";
import { createPool, openDatabase } from "./db.ts";

const config = loadConfig();
const command = process.argv[2] ?? "serve";

if (command === "catalog:import") {
  const { importCatalogFromSqlite } = await import("./catalog/import-sqlite.ts");
  const source = process.argv[3];
  if (!source) {
    console.error("usage: watchkeep catalog:import <path/to/catalog.sqlite>");
    process.exit(2);
  }
  if (!config.catalogDatabaseUrl) {
    console.error("WATCHKEEP_CATALOG_DATABASE_URL is empty");
    process.exit(2);
  }
  const pool = createPool(config.catalogDatabaseUrl, 2);
  const report = await importCatalogFromSqlite(source, pool, { log: (message) => console.log(`[catalog] ${message}`) });
  console.log(JSON.stringify(report));
  await pool.end();
} else {
  const pool = await openDatabase(config.databaseUrl);
  const catalogPool = config.catalogDatabaseUrl ? createPool(config.catalogDatabaseUrl, 3) : null;
  const ctx = createContext({ pool, catalogPool, config });

  if (command === "sync") {
    const report = await ctx.sync();
    console.log(JSON.stringify(report));
    await ctx.close();
  } else if (command === "serve") {
    const app = createApp(ctx);
    serve({ fetch: app.fetch, hostname: config.host, port: config.port }, (info) => {
      ctx.log(`listening on http://${info.address}:${info.port}`);
      ctx.log(`catalog ${catalogPool ? "enabled" : "disabled (set WATCHKEEP_CATALOG_DATABASE_URL to enable)"}`);
      if (!config.webhookToken) ctx.log("warning: WATCHKEEP_WEBHOOK_TOKEN is empty, the webhook accepts any caller");
    });
    if (config.syncIntervalMinutes > 0 && config.plexUrl && config.plexToken) {
      const run = () => ctx.sync().catch((error: Error) => ctx.log(`sync failed: ${error.message}`));
      setTimeout(run, 5_000);
      setInterval(run, config.syncIntervalMinutes * 60_000);
    }
    const shutdown = () => {
      ctx.log("shutting down");
      ctx.close().finally(() => process.exit(0));
    };
    process.on("SIGINT", shutdown);
    process.on("SIGTERM", shutdown);
  } else {
    console.error(`unknown command: ${command}. Use "serve", "sync", or "catalog:import <file>".`);
    process.exit(2);
  }
}
