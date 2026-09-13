export interface Config {
  host: string;
  port: number;
  /** Postgres connection string of the Watchkeep database. */
  databaseUrl: string;
  /** Postgres connection string of the TMDB catalog database. Empty disables catalog lookups. */
  catalogDatabaseUrl: string;
  /** Which catalog title column to prefer: `en` or `pt`. */
  catalogLanguage: "en" | "pt";
  /** Base URL for TMDB poster paths. */
  imageBaseUrl: string;
  /** Shared secret that Plex must send as `?token=` on the webhook URL. Empty disables the check. */
  webhookToken: string;
  /** Percentage of a movie or episode that counts as watched when playback stops. */
  watchedThresholdPercent: number;
  /** Two plays of the same item inside this window count as one play. */
  rewatchWindowMinutes: number;
  /** Plex account titles or ids to accept. Empty accepts every account. */
  plexAccounts: string[];
  /** Base URL of the Plex server for library sync, for example http://plex:32400. */
  plexUrl: string;
  plexToken: string;
  /** Run a library sync on this interval. 0 disables the timer. */
  syncIntervalMinutes: number;
  /** Keep raw webhook events for this many days. 0 keeps them forever. */
  webhookRetentionDays: number;
}

function num(value: string | undefined, fallback: number): number {
  if (value === undefined || value.trim() === "") return fallback;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) throw new Error(`Invalid number: ${value}`);
  return parsed;
}

function list(value: string | undefined): string[] {
  if (!value) return [];
  return value
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const language = env.WATCHKEEP_CATALOG_LANGUAGE ?? "en";
  if (language !== "en" && language !== "pt") throw new Error("WATCHKEEP_CATALOG_LANGUAGE must be en or pt");
  return {
    host: env.WATCHKEEP_HOST ?? "0.0.0.0",
    port: num(env.WATCHKEEP_PORT ?? env.PORT, 8484),
    databaseUrl: env.WATCHKEEP_DATABASE_URL ?? "postgres://watchkeep:watchkeep@localhost:5432/watchkeep",
    catalogDatabaseUrl: env.WATCHKEEP_CATALOG_DATABASE_URL ?? "",
    catalogLanguage: language,
    imageBaseUrl: (env.WATCHKEEP_IMAGE_BASE_URL ?? "https://image.tmdb.org/t/p/").replace(/\/+$/, "") + "/",
    webhookToken: env.WATCHKEEP_WEBHOOK_TOKEN ?? "",
    watchedThresholdPercent: num(env.WATCHKEEP_WATCHED_THRESHOLD_PERCENT, 85),
    rewatchWindowMinutes: num(env.WATCHKEEP_REWATCH_WINDOW_MINUTES, 360),
    plexAccounts: list(env.WATCHKEEP_PLEX_ACCOUNTS),
    plexUrl: (env.WATCHKEEP_PLEX_URL ?? "").replace(/\/+$/, ""),
    plexToken: env.WATCHKEEP_PLEX_TOKEN ?? "",
    syncIntervalMinutes: num(env.WATCHKEEP_SYNC_INTERVAL_MINUTES, 0),
    webhookRetentionDays: num(env.WATCHKEEP_WEBHOOK_RETENTION_DAYS, 30),
  };
}
