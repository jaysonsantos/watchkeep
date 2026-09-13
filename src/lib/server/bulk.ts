/**
 * Set-based writes for imports. Each function runs a handful of queries for
 * any number of rows, so imports stay fast over a slow network link.
 * The matching rules mirror `Library.findMedia` and `Library.upsertEpisode`.
 */
import type { EpisodeRow, MediaKind, MediaRow, Queryable, TargetKind } from "./db.ts";
import type { EpisodeInput, MovieInput, ShowInput } from "./library.ts";

const CHUNK = 500;

function tmdbNumber(value: string | null | undefined): number | null {
  if (!value) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

function chunks<T>(items: T[]): T[][] {
  const out: T[][] = [];
  for (let index = 0; index < items.length; index += CHUNK) out.push(items.slice(index, index + CHUNK));
  return out;
}

/** Build `($1, $2), ($3, $4)` placeholders for `rows.length` rows of `width` columns. */
function tuples(rows: unknown[][], values: unknown[]): string {
  return rows
    .map((row) => `(${row.map((value) => `$${values.push(value)}`).join(", ")})`)
    .join(", ");
}

type MediaInput = MovieInput | ShowInput;

/**
 * Tracks which row owns each unique id inside one batch, so that two inputs
 * that carry the same id never produce two rows with it.
 */
class Claims {
  private readonly owners = new Map<string, string>();

  /** Returns the value when `owner` may use it, or null when another row already has it. */
  claim<T extends string | number>(scope: string, value: T | null, owner: string): T | null {
    if (value === null) return null;
    const key = `${scope}:${value}`;
    const current = this.owners.get(key);
    if (current !== undefined && current !== owner) return null;
    this.owners.set(key, owner);
    return value;
  }
}

function durationOf(input: MediaInput): number | null {
  return "durationMs" in input ? input.durationMs : null;
}

interface MediaKeys {
  plex_guid: string | null;
  tmdb_id: number | null;
  tvdb_id: string | null;
  imdb_id: string | null;
  title: string;
  year: number | null;
}

function keysOf(input: MediaInput): MediaKeys {
  return {
    plex_guid: input.ids.plexGuid,
    tmdb_id: tmdbNumber(input.ids.tmdb),
    tvdb_id: input.ids.tvdb,
    imdb_id: input.ids.imdb,
    title: input.title,
    year: input.year,
  };
}

class MediaIndex<T extends MediaKeys> {
  private readonly byGuid = new Map<string, T>();
  private readonly byTmdb = new Map<number, T>();
  private readonly byTvdb = new Map<string, T>();
  private readonly byImdb = new Map<string, T>();
  private readonly byTitle = new Map<string, T[]>();

  add(row: T): void {
    if (row.plex_guid) this.byGuid.set(row.plex_guid, row);
    if (row.tmdb_id !== null) this.byTmdb.set(row.tmdb_id, row);
    if (row.tvdb_id) this.byTvdb.set(row.tvdb_id, row);
    if (row.imdb_id) this.byImdb.set(row.imdb_id, row);
    const list = this.byTitle.get(row.title.toLowerCase()) ?? [];
    list.push(row);
    this.byTitle.set(row.title.toLowerCase(), list);
  }

  find(input: MediaInput): T | null {
    const tmdb = tmdbNumber(input.ids.tmdb);
    const found =
      (input.ids.plexGuid ? this.byGuid.get(input.ids.plexGuid) : undefined) ??
      (tmdb !== null ? this.byTmdb.get(tmdb) : undefined) ??
      (input.ids.tvdb ? this.byTvdb.get(input.ids.tvdb) : undefined) ??
      (input.ids.imdb ? this.byImdb.get(input.ids.imdb) : undefined);
    if (found) return found;
    const candidates = this.byTitle.get(input.title.toLowerCase()) ?? [];
    return (
      candidates.find((row) => row.year === input.year) ??
      candidates.find((row) => row.year === null || input.year === null) ??
      null
    );
  }
}

/**
 * Insert or update many movies or shows. Returns one row per input, in input
 * order. Inputs that resolve to the same row share it.
 */
export async function bulkUpsertMedia(db: Queryable, kind: MediaKind, inputs: MediaInput[], now: string): Promise<MediaRow[]> {
  if (inputs.length === 0) return [];
  const index = new MediaIndex<MediaRow>();
  const guids = [...new Set(inputs.map((input) => input.ids.plexGuid).filter((v): v is string => v !== null))];
  const tmdbs = [...new Set(inputs.map((input) => tmdbNumber(input.ids.tmdb)).filter((v): v is number => v !== null))];
  const tvdbs = [...new Set(inputs.map((input) => input.ids.tvdb).filter((v): v is string => v !== null))];
  const imdbs = [...new Set(inputs.map((input) => input.ids.imdb).filter((v): v is string => v !== null))];
  const titles = [...new Set(inputs.map((input) => input.title.toLowerCase()))];
  const { rows: existing } = await db.query<MediaRow>(
    `SELECT * FROM media WHERE kind = $1 AND (plex_guid = ANY($2::text[]) OR tmdb_id = ANY($3::bigint[])
       OR tvdb_id = ANY($4::text[]) OR imdb_id = ANY($5::text[]) OR lower(title) = ANY($6::text[]))`,
    [kind, guids, tmdbs, tvdbs, imdbs, titles],
  );
  for (const row of existing) index.add(row);

  const claims = new Claims();
  for (const row of existing) {
    claims.claim("guid", row.plex_guid, `row:${row.id}`);
    claims.claim("tmdb", row.tmdb_id, `row:${row.id}`);
    claims.claim("tvdb", row.tvdb_id, `row:${row.id}`);
    claims.claim("imdb", row.imdb_id, `row:${row.id}`);
  }

  // Pass 1: match inputs to existing rows and collect merged updates.
  const result: Array<MediaRow | null> = inputs.map(() => null);
  const updates = new Map<number, MediaRow>();
  const pendingInsert: Array<{ position: number; input: MediaInput }> = [];
  inputs.forEach((input, position) => {
    const current = index.find(input);
    if (!current) {
      pendingInsert.push({ position, input });
      return;
    }
    const owner = `row:${current.id}`;
    const base = updates.get(current.id) ?? current;
    const merged: MediaRow = {
      ...base,
      title: input.title,
      year: input.year ?? base.year,
      plex_guid: claims.claim("guid", input.ids.plexGuid, owner) ?? base.plex_guid,
      imdb_id: claims.claim("imdb", input.ids.imdb, owner) ?? base.imdb_id,
      tmdb_id: claims.claim("tmdb", tmdbNumber(input.ids.tmdb), owner) ?? base.tmdb_id,
      tvdb_id: claims.claim("tvdb", input.ids.tvdb, owner) ?? base.tvdb_id,
      duration_ms: durationOf(input) ?? base.duration_ms,
      summary: input.summary ?? base.summary,
      poster_path: input.posterPath ?? base.poster_path,
      updated_at: now,
    };
    updates.set(current.id, merged);
    result[position] = merged;
  });

  // Pass 2: insert new rows. Inputs that match a queued row by any id share it.
  interface Pending extends MediaKeys {
    slot: number;
    input: MediaInput;
    positions: number[];
  }
  const pendingIndex = new MediaIndex<Pending>();
  const toInsert: Pending[] = [];
  for (const { position, input } of pendingInsert) {
    const queued = pendingIndex.find(input);
    if (queued) {
      queued.positions.push(position);
      continue;
    }
    const owner = `new:${toInsert.length}`;
    const keys = keysOf(input);
    const pending: Pending = {
      slot: toInsert.length,
      input,
      positions: [position],
      title: keys.title,
      year: keys.year,
      plex_guid: claims.claim("guid", keys.plex_guid, owner),
      tmdb_id: claims.claim("tmdb", keys.tmdb_id, owner),
      tvdb_id: claims.claim("tvdb", keys.tvdb_id, owner),
      imdb_id: claims.claim("imdb", keys.imdb_id, owner),
    };
    toInsert.push(pending);
    pendingIndex.add(pending);
  }
  for (const batch of chunks(toInsert)) {
    const values: unknown[] = [];
    const rows = batch.map((pending) => [
      kind,
      pending.title,
      pending.year,
      pending.plex_guid,
      pending.imdb_id,
      pending.tmdb_id,
      pending.tvdb_id,
      durationOf(pending.input),
      pending.input.summary ?? null,
      pending.input.posterPath ?? null,
      now,
      now,
    ]);
    const { rows: created } = await db.query<MediaRow>(
      `INSERT INTO media (kind, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, created_at, updated_at)
       VALUES ${tuples(rows, values)} RETURNING *`,
      values,
    );
    created.forEach((row, offset) => {
      for (const position of batch[offset]!.positions) result[position] = row;
    });
  }

  // Pass 3: apply merged updates in one statement per chunk.
  for (const batch of chunks([...updates.values()])) {
    await db.query(
      `UPDATE media AS m SET title = u.title, year = u.year, plex_guid = u.plex_guid, imdb_id = u.imdb_id, tmdb_id = u.tmdb_id,
         tvdb_id = u.tvdb_id, duration_ms = u.duration_ms, summary = u.summary, poster_path = u.poster_path, updated_at = u.updated_at
       FROM unnest($1::bigint[], $2::text[], $3::int[], $4::text[], $5::text[], $6::bigint[], $7::text[], $8::bigint[], $9::text[], $10::text[], $11::text[])
         AS u(id, title, year, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, summary, poster_path, updated_at)
       WHERE m.id = u.id`,
      [
        batch.map((row) => row.id),
        batch.map((row) => row.title),
        batch.map((row) => row.year),
        batch.map((row) => row.plex_guid),
        batch.map((row) => row.imdb_id),
        batch.map((row) => row.tmdb_id),
        batch.map((row) => row.tvdb_id),
        batch.map((row) => row.duration_ms),
        batch.map((row) => row.summary),
        batch.map((row) => row.poster_path),
        batch.map((row) => row.updated_at),
      ],
    );
  }
  return result as MediaRow[];
}

export interface EpisodeUpsert extends EpisodeInput {
  showId: number;
}

/** Insert or update many episodes. Returns one row per input, in input order. */
export async function bulkUpsertEpisodes(db: Queryable, inputs: EpisodeUpsert[], now: string): Promise<EpisodeRow[]> {
  if (inputs.length === 0) return [];
  const showIds = [...new Set(inputs.map((input) => input.showId))];
  const { rows: existing } = await db.query<EpisodeRow>("SELECT * FROM episodes WHERE show_id = ANY($1::bigint[])", [showIds]);
  const guids = inputs.map((input) => input.ids.plexGuid).filter((v): v is string => v !== null);
  if (guids.length > 0) {
    const { rows } = await db.query<EpisodeRow>("SELECT * FROM episodes WHERE plex_guid = ANY($1::text[])", [guids]);
    for (const row of rows) if (!existing.some((known) => known.id === row.id)) existing.push(row);
  }
  const byGuid = new Map<string, EpisodeRow>();
  const byNumber = new Map<string, EpisodeRow>();
  const numberKey = (showId: number, season: number, number: number) => `${showId}:${season}:${number}`;
  for (const row of existing) {
    if (row.plex_guid) byGuid.set(row.plex_guid, row);
    byNumber.set(numberKey(row.show_id, row.season, row.number), row);
  }

  const claims = new Claims();
  for (const row of existing) {
    claims.claim("guid", row.plex_guid, `row:${row.id}`);
    claims.claim("tmdb", row.tmdb_id, `row:${row.id}`);
  }

  const result: Array<EpisodeRow | null> = inputs.map(() => null);
  const updates = new Map<number, EpisodeRow>();
  const toInsert: Array<{ input: EpisodeUpsert; plexGuid: string | null; tmdbId: number | null; positions: number[] }> = [];
  const insertByKey = new Map<string, number>();
  inputs.forEach((input, position) => {
    const current =
      (input.ids.plexGuid ? byGuid.get(input.ids.plexGuid) : undefined) ?? byNumber.get(numberKey(input.showId, input.season, input.number));
    if (current) {
      const owner = `row:${current.id}`;
      const base = updates.get(current.id) ?? current;
      const merged: EpisodeRow = {
        ...base,
        show_id: input.showId,
        season: input.season,
        number: input.number,
        title: input.title ?? base.title,
        plex_guid: claims.claim("guid", input.ids.plexGuid, owner) ?? base.plex_guid,
        imdb_id: input.ids.imdb ?? base.imdb_id,
        tmdb_id: claims.claim("tmdb", tmdbNumber(input.ids.tmdb), owner) ?? base.tmdb_id,
        tvdb_id: input.ids.tvdb ?? base.tvdb_id,
        duration_ms: input.durationMs ?? base.duration_ms,
        aired_at: input.airedAt ?? base.aired_at,
        updated_at: now,
      };
      updates.set(current.id, merged);
      result[position] = merged;
      return;
    }
    const keys = [numberKey(input.showId, input.season, input.number), input.ids.plexGuid ? `guid:${input.ids.plexGuid}` : null];
    const slot = keys.map((key) => (key ? insertByKey.get(key) : undefined)).find((value) => value !== undefined);
    if (slot !== undefined) {
      toInsert[slot]!.positions.push(position);
      return;
    }
    const owner = `new:${toInsert.length}`;
    for (const key of keys) if (key) insertByKey.set(key, toInsert.length);
    toInsert.push({
      input,
      plexGuid: claims.claim("guid", input.ids.plexGuid, owner),
      tmdbId: claims.claim("tmdb", tmdbNumber(input.ids.tmdb), owner),
      positions: [position],
    });
  });

  for (const batch of chunks(toInsert)) {
    const values: unknown[] = [];
    const rows = batch.map(({ input, plexGuid, tmdbId }) => [
      input.showId,
      input.season,
      input.number,
      input.title,
      plexGuid,
      input.ids.imdb,
      tmdbId,
      input.ids.tvdb,
      input.durationMs,
      input.airedAt,
      now,
      now,
    ]);
    const { rows: created } = await db.query<EpisodeRow>(
      `INSERT INTO episodes (show_id, season, number, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, created_at, updated_at)
       VALUES ${tuples(rows, values)} RETURNING *`,
      values,
    );
    created.forEach((row, offset) => {
      for (const position of batch[offset]!.positions) result[position] = row;
    });
  }

  for (const batch of chunks([...updates.values()])) {
    await db.query(
      `UPDATE episodes AS e SET title = u.title, plex_guid = u.plex_guid, imdb_id = u.imdb_id, tmdb_id = u.tmdb_id, tvdb_id = u.tvdb_id,
         duration_ms = u.duration_ms, aired_at = u.aired_at, updated_at = u.updated_at
       FROM unnest($1::bigint[], $2::text[], $3::text[], $4::text[], $5::bigint[], $6::text[], $7::bigint[], $8::text[], $9::text[])
         AS u(id, title, plex_guid, imdb_id, tmdb_id, tvdb_id, duration_ms, aired_at, updated_at)
       WHERE e.id = u.id`,
      [
        batch.map((row) => row.id),
        batch.map((row) => row.title),
        batch.map((row) => row.plex_guid),
        batch.map((row) => row.imdb_id),
        batch.map((row) => row.tmdb_id),
        batch.map((row) => row.tvdb_id),
        batch.map((row) => row.duration_ms),
        batch.map((row) => row.aired_at),
        batch.map((row) => row.updated_at),
      ],
    );
  }
  return result as EpisodeRow[];
}

export interface PlayInsert {
  kind: TargetKind;
  id: number;
  watchedAt: string;
  source: string;
  externalId: string | null;
  account?: string | null;
  player?: string | null;
}

/** Insert plays; rows whose source and external id already exist are skipped. Returns the inserted count. */
export async function bulkRecordPlays(db: Queryable, plays: PlayInsert[]): Promise<number> {
  let inserted = 0;
  for (const batch of chunks(plays)) {
    const values: unknown[] = [];
    const rows = batch.map((play) => [play.kind, play.id, play.watchedAt, play.source, play.account ?? null, play.player ?? null, play.externalId]);
    const { rowCount } = await db.query(
      `INSERT INTO plays (target_kind, target_id, watched_at, source, account, player, external_id)
       VALUES ${tuples(rows, values)}
       ON CONFLICT (source, external_id) WHERE external_id IS NOT NULL DO NOTHING`,
      values,
    );
    inserted += rowCount ?? 0;
  }
  return inserted;
}

export interface RatingInsert {
  kind: "movie" | "show" | "episode";
  id: number;
  rating: number;
  ratedAt: string;
}

export async function bulkSetRatings(db: Queryable, ratings: RatingInsert[]): Promise<void> {
  for (const batch of chunks(ratings)) {
    const values: unknown[] = [];
    const rows = batch.map((rating) => [rating.kind, rating.id, rating.rating, rating.ratedAt]);
    await db.query(
      `INSERT INTO ratings (target_kind, target_id, rating, rated_at) VALUES ${tuples(rows, values)}
       ON CONFLICT (target_kind, target_id) DO UPDATE SET rating = EXCLUDED.rating, rated_at = EXCLUDED.rated_at`,
      values,
    );
  }
}
