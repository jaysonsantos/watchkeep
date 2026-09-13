import { randomBytes } from "node:crypto";
import { createApp, createContext, type AppContext } from "../src/app.ts";
import { ensureCatalogSchema } from "../src/catalog/import-sqlite.ts";
import { loadConfig, type Config } from "../src/config.ts";
import { createPool, migrate, type Pool } from "../src/db.ts";
import type { Clock } from "../src/library.ts";

const ADMIN_URL = process.env.WATCHKEEP_TEST_DATABASE_URL;
if (!ADMIN_URL) {
  throw new Error("WATCHKEEP_TEST_DATABASE_URL is not set. Run the tests through `npm test` (starts Postgres in Docker).");
}

export class FakeClock implements Clock {
  constructor(private current = new Date("2026-01-01T12:00:00.000Z")) {}
  now(): Date {
    return new Date(this.current);
  }
  advanceMinutes(minutes: number): void {
    this.current = new Date(this.current.getTime() + minutes * 60_000);
  }
}

export function testConfig(overrides: Partial<Config> = {}): Config {
  return { ...loadConfig({}), webhookToken: "", ...overrides };
}

function urlFor(database: string): string {
  const url = new URL(ADMIN_URL!);
  url.pathname = `/${database}`;
  return url.toString();
}

async function admin(sql: string): Promise<void> {
  const pool = createPool(ADMIN_URL!, 1);
  try {
    await pool.query(sql);
  } finally {
    await pool.end();
  }
}

export interface TestContext extends AppContext {
  clock: FakeClock;
}

/**
 * Build an app context on two fresh databases: one for Watchkeep and, when
 * `withCatalog` is true, one seeded TMDB catalog. `close()` drops both.
 */
export async function testContext(
  overrides: Partial<Config> = {},
  options: { withCatalog?: boolean; fetchImpl?: typeof fetch } = {},
): Promise<TestContext> {
  const name = `wk_${randomBytes(6).toString("hex")}`;
  await admin(`CREATE DATABASE ${name}`);
  const pool = createPool(urlFor(name), 3);
  await migrate(pool);

  let catalogPool: Pool | null = null;
  if (options.withCatalog) {
    await admin(`CREATE DATABASE ${name}_cat`);
    catalogPool = createPool(urlFor(`${name}_cat`), 2);
    await ensureCatalogSchema(catalogPool);
    await seedCatalog(catalogPool);
  }

  const clock = new FakeClock();
  const ctx = createContext({
    pool,
    catalogPool,
    config: testConfig({ ...overrides, catalogDatabaseUrl: catalogPool ? urlFor(`${name}_cat`) : "" }),
    clock,
    fetchImpl: options.fetchImpl,
    log: () => {},
  }) as TestContext;
  const close = ctx.close;
  ctx.close = async () => {
    await close();
    await admin(`DROP DATABASE IF EXISTS ${name}`);
    if (catalogPool) await admin(`DROP DATABASE IF EXISTS ${name}_cat`);
  };
  return ctx;
}

export async function testApp(overrides: Partial<Config> = {}, options: { withCatalog?: boolean; fetchImpl?: typeof fetch } = {}) {
  const ctx = await testContext(overrides, options);
  return { ctx, app: createApp(ctx) };
}

/** A small sample catalog: Severance and two Michael Mann films. */
export async function seedCatalog(pool: Pool): Promise<void> {
  await pool.query(
    `INSERT INTO tmdb_show (id, imdb_id, tvdb_id, name_en, name_pt, first_air_date, number_of_seasons, number_of_episodes, poster_path_en, overview_en, fetched_at, raw_json)
     VALUES (95396, 'tt11280740', 371980, 'Severance', 'Ruptura', '2022-02-17', 2, 4, '/pPHpeI2X1qEd1CS1SeyrdhZ4qnT.jpg', 'Mark leads a team of office workers.', 0, '{}')`,
  );
  await pool.query(
    `INSERT INTO tmdb_season (id, show_id, season_number, name, fetched_at) VALUES
       (1000, 95396, 0, 'Specials', 0), (1001, 95396, 1, 'Season 1', 0), (1002, 95396, 2, 'Season 2', 0)`,
  );
  await pool.query(
    `INSERT INTO tmdb_episode (id, season_id, episode_number, name, air_date, runtime) VALUES
       (5995805, 1000, 1, 'Welcome to Lumon', '2021-12-15', 7),
       (1982925, 1001, 1, 'Good News About Hell', '2022-02-17', 57),
       (3396429, 1001, 2, 'Half Loop', '2022-02-17', 53),
       (3396430, 1001, 3, 'In Perpetuity', '2022-02-24', 56),
       (4000001, 1002, 1, 'Hello, Ms. Cobel', '2025-01-17', 60),
       (4000002, 1002, 2, 'Far From Home', '2099-01-01', 60)`,
  );
  await pool.query(
    `INSERT INTO tmdb_movie (id, imdb_id, title_en, title_pt, release_date, runtime, poster_path_en, fetched_at, raw_json) VALUES
       (949, 'tt0113277', 'Heat', 'Fogo Contra Fogo', '1995-12-15', 170, '/e09dLw1Ljtccd2P4NsuUvVtS5du.jpg', 0, '{}'),
       (4638, 'tt0369339', 'Collateral', 'Colateral', '2004-08-04', 120, '/collateral.jpg', 0, '{}')`,
  );
}

export function episodePayload(overrides: Record<string, unknown> = {}, metadata: Record<string, unknown> = {}) {
  return {
    event: "media.play",
    user: true,
    owner: true,
    Account: { id: 1, title: "jayson" },
    Server: { title: "home", uuid: "s1" },
    Player: { title: "Plex Web", uuid: "p1", local: true },
    Metadata: {
      librarySectionType: "show",
      type: "episode",
      title: "Pilot",
      grandparentTitle: "Severance",
      grandparentGuid: "plex://show/show1",
      parentGuid: "plex://season/season1",
      guid: "plex://episode/ep1",
      parentIndex: 1,
      index: 1,
      duration: 3_400_000,
      viewOffset: 120_000,
      originallyAvailableAt: "2022-02-18",
      Guid: [{ id: "imdb://tt11280740" }, { id: "tmdb://1982925" }, { id: "tvdb://371980" }],
      ...metadata,
    },
    ...overrides,
  };
}

export function moviePayload(overrides: Record<string, unknown> = {}, metadata: Record<string, unknown> = {}) {
  return {
    event: "media.play",
    Account: { id: 1, title: "jayson" },
    Player: { title: "Living room TV" },
    Metadata: {
      librarySectionType: "movie",
      type: "movie",
      title: "Heat",
      year: 1995,
      guid: "plex://movie/heat",
      duration: 10_000_000,
      viewOffset: 0,
      Guid: [{ id: "imdb://tt0113277" }, { id: "tmdb://949" }],
      ...metadata,
    },
    ...overrides,
  };
}

export function multipart(payload: unknown): Request {
  const form = new FormData();
  form.set("payload", JSON.stringify(payload));
  return new Request("http://localhost/webhook/plex", { method: "POST", body: form });
}
