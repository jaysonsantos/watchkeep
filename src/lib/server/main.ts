import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { createContext } from "./app.ts";
import { loadConfig } from "./config.ts";
import { createPool, openDatabase } from "./db.ts";

const config = loadConfig();
const command = process.argv[2] ?? "serve";

if (command === "serve") {
  // Start the SvelteKit server from `pnpm build`. `src/hooks.server.ts` opens the databases,
  // mounts the API and the webhook, and starts the sync timer.
  process.env.WATCHKEEP_HTTP_HOST ??= config.host;
  process.env.WATCHKEEP_HTTP_PORT ??= String(config.port);
  await import(pathToFileURL(resolve("build/index.js")).href);
} else if (command === "catalog:import") {
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

  if (command === "trakt:import") {
    const { importTraktExport } = await import("./trakt/import.ts");
    const args = process.argv.slice(3);
    const dryRun = args.includes("--dry-run");
    const source = args.find((arg) => !arg.startsWith("--"));
    if (!source) {
      console.error("usage: watchkeep trakt:import <trakt-export.zip> [--dry-run]");
      process.exit(2);
    }
    const report = await importTraktExport(pool, ctx.catalog, source, {
      dryRun,
      log: (message) => console.log(`[trakt] ${message}`),
    });
    console.log(JSON.stringify(report));
    await ctx.close();
  } else if (command === "sync") {
    const report = await ctx.sync();
    console.log(JSON.stringify(report));
    await ctx.close();
  } else {
    console.error(`unknown command: ${command}. Use "serve", "sync", "catalog:import <file>", or "trakt:import <zip>".`);
    process.exit(2);
  }
}
