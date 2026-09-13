/**
 * Copy the TMDB tables from a SQLite file with the same table layout into the
 * Postgres catalog database. Column names are identical on both sides, so the
 * import copies every column that exists in both schemas.
 */
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Pool } from "../db.ts";

export const CATALOG_TABLES = [
  "genre",
  "collection",
  "tmdb_movie",
  "tmdb_movie_genre",
  "tmdb_show",
  "tmdb_show_genre",
  "tmdb_season",
  "tmdb_episode",
] as const;

const PRIMARY_KEYS: Record<(typeof CATALOG_TABLES)[number], string[]> = {
  genre: ["id"],
  collection: ["id"],
  tmdb_movie: ["id"],
  tmdb_movie_genre: ["movie_id", "genre_id"],
  tmdb_show: ["id"],
  tmdb_show_genre: ["show_id", "genre_id"],
  tmdb_season: ["id"],
  tmdb_episode: ["id"],
};

export interface ImportReport {
  tables: Record<string, number>;
}

export function catalogSchemaSql(): string {
  const here = dirname(fileURLToPath(import.meta.url));
  // Works from `src/catalog` (dev) and from `dist/catalog` (build): both sit two levels below the repo root.
  return readFileSync(join(here, "..", "..", "catalog", "schema.sql"), "utf8");
}

export async function ensureCatalogSchema(pool: Pool): Promise<void> {
  await pool.query(catalogSchemaSql());
}

function normalise(value: unknown, dataType: string): unknown {
  if (value === null || value === undefined) return null;
  if (dataType === "boolean") return value === 1 || value === true || value === "1";
  if (typeof value === "bigint") return Number(value);
  return value;
}

export async function importCatalogFromSqlite(
  sqlitePath: string,
  pool: Pool,
  options: { batchSize?: number; log?: (message: string) => void } = {},
): Promise<ImportReport> {
  const batchSize = options.batchSize ?? 500;
  const log = options.log ?? (() => {});
  await ensureCatalogSchema(pool);
  const sqlite = new DatabaseSync(sqlitePath, { readOnly: true });
  const report: ImportReport = { tables: {} };
  try {
    for (const table of CATALOG_TABLES) {
      const { rows: pgColumns } = await pool.query<{ column_name: string; data_type: string }>(
        `SELECT column_name, data_type FROM information_schema.columns
         WHERE table_schema = current_schema() AND table_name = $1 AND is_generated = 'NEVER' ORDER BY ordinal_position`,
        [table],
      );
      const sqliteColumns = new Set(
        (sqlite.prepare(`SELECT name FROM pragma_table_xinfo(?) WHERE hidden = 0`).all(table) as Array<{ name: string }>).map(
          (row) => row.name,
        ),
      );
      const columns = pgColumns.filter((column) => sqliteColumns.has(column.column_name));
      if (columns.length === 0) {
        log(`skip ${table}: no shared columns`);
        continue;
      }
      const names = columns.map((column) => column.column_name);
      const keys = PRIMARY_KEYS[table];
      const updates = names.filter((name) => !keys.includes(name)).map((name) => `${name} = EXCLUDED.${name}`);
      const conflict = updates.length > 0 ? `DO UPDATE SET ${updates.join(", ")}` : "DO NOTHING";
      const select = sqlite.prepare(`SELECT ${names.join(", ")} FROM ${table}`);
      let batch: unknown[][] = [];
      let count = 0;
      const flush = async () => {
        if (batch.length === 0) return;
        const values: unknown[] = [];
        const tuples = batch.map((row) => {
          const placeholders = row.map((value) => {
            values.push(value);
            return `$${values.length}`;
          });
          return `(${placeholders.join(", ")})`;
        });
        await pool.query(
          `INSERT INTO ${table} (${names.join(", ")}) VALUES ${tuples.join(", ")} ON CONFLICT (${keys.join(", ")}) ${conflict}`,
          values,
        );
        count += batch.length;
        batch = [];
      };
      for (const row of select.iterate() as Iterable<Record<string, unknown>>) {
        batch.push(columns.map((column) => normalise(row[column.column_name], column.data_type)));
        if (batch.length >= batchSize) await flush();
      }
      await flush();
      report.tables[table] = count;
      log(`${table}: ${count} rows`);
    }
  } finally {
    sqlite.close();
  }
  return report;
}
