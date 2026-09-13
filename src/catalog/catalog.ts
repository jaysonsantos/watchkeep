/**
 * Read-only access to the TMDB catalog database. The catalog lives in a
 * separate Postgres database on the same instance, so Watchkeep talks to it
 * through a second pool and never joins across databases.
 */
import type { Pool, Queryable } from "../db.ts";

export type CatalogLanguage = "en" | "pt";

export interface CatalogMovie {
  tmdbId: number;
  imdbId: string | null;
  title: string;
  year: number | null;
  runtimeMin: number | null;
  posterPath: string | null;
  overview: string | null;
}

export interface CatalogShow {
  tmdbId: number;
  imdbId: string | null;
  tvdbId: string | null;
  title: string;
  year: number | null;
  posterPath: string | null;
  overview: string | null;
  numberOfSeasons: number | null;
  numberOfEpisodes: number | null;
}

export interface CatalogEpisode {
  tmdbId: number;
  season: number;
  number: number;
  title: string | null;
  airDate: string | null;
  runtimeMin: number | null;
  stillPath: string | null;
}

interface MovieRow {
  id: number;
  imdb_id: string | null;
  title_en: string | null;
  title_pt: string | null;
  original_title: string | null;
  release_date: string | null;
  runtime: number | null;
  poster_path_en: string | null;
  poster_path_pt: string | null;
  overview_en: string | null;
  overview_pt: string | null;
}

interface ShowRow {
  id: number;
  imdb_id: string | null;
  tvdb_id: number | null;
  name_en: string | null;
  name_pt: string | null;
  original_name: string | null;
  first_air_date: string | null;
  poster_path_en: string | null;
  poster_path_pt: string | null;
  overview_en: string | null;
  overview_pt: string | null;
  number_of_seasons: number | null;
  number_of_episodes: number | null;
}

interface EpisodeRow {
  id: number;
  season_number: number;
  episode_number: number;
  name: string | null;
  air_date: string | null;
  runtime: number | null;
  still_path: string | null;
}

function yearOf(date: string | null): number | null {
  const year = Number(date?.slice(0, 4));
  return Number.isInteger(year) && year > 0 ? year : null;
}

const MOVIE_COLUMNS =
  "id, imdb_id, title_en, title_pt, original_title, release_date, runtime, poster_path_en, poster_path_pt, overview_en, overview_pt";
const SHOW_COLUMNS =
  "id, imdb_id, tvdb_id, name_en, name_pt, original_name, first_air_date, poster_path_en, poster_path_pt, overview_en, overview_pt, number_of_seasons, number_of_episodes";

export class Catalog {
  constructor(
    private readonly db: Queryable,
    private readonly language: CatalogLanguage = "en",
  ) {}

  private pick<T>(en: T | null, pt: T | null): T | null {
    return this.language === "pt" ? (pt ?? en) : (en ?? pt);
  }

  private movie(row: MovieRow): CatalogMovie {
    return {
      tmdbId: row.id,
      imdbId: row.imdb_id,
      title: this.pick(row.title_en, row.title_pt) ?? row.original_title ?? `TMDB ${row.id}`,
      year: yearOf(row.release_date),
      runtimeMin: row.runtime,
      posterPath: this.pick(row.poster_path_en, row.poster_path_pt),
      overview: this.pick(row.overview_en, row.overview_pt),
    };
  }

  private show(row: ShowRow): CatalogShow {
    return {
      tmdbId: row.id,
      imdbId: row.imdb_id,
      tvdbId: row.tvdb_id === null ? null : String(row.tvdb_id),
      title: this.pick(row.name_en, row.name_pt) ?? row.original_name ?? `TMDB ${row.id}`,
      year: yearOf(row.first_air_date),
      posterPath: this.pick(row.poster_path_en, row.poster_path_pt),
      overview: this.pick(row.overview_en, row.overview_pt),
      numberOfSeasons: row.number_of_seasons,
      numberOfEpisodes: row.number_of_episodes,
    };
  }

  // --- movies --------------------------------------------------------------

  async movieByTmdbId(id: number): Promise<CatalogMovie | null> {
    const { rows } = await this.db.query<MovieRow>(`SELECT ${MOVIE_COLUMNS} FROM tmdb_movie WHERE id = $1`, [id]);
    return rows[0] ? this.movie(rows[0]) : null;
  }

  async movieByImdbId(imdbId: string): Promise<CatalogMovie | null> {
    const { rows } = await this.db.query<MovieRow>(`SELECT ${MOVIE_COLUMNS} FROM tmdb_movie WHERE imdb_id = $1 LIMIT 1`, [imdbId]);
    return rows[0] ? this.movie(rows[0]) : null;
  }

  /** Exact title match in either language. The year narrows the match when given. */
  async movieByTitle(title: string, year: number | null): Promise<CatalogMovie | null> {
    const { rows } = await this.db.query<MovieRow>(
      `SELECT ${MOVIE_COLUMNS} FROM tmdb_movie
       WHERE (lower(title_en) = lower($1) OR lower(title_pt) = lower($1) OR lower(original_title) = lower($1))
         AND ($2::int IS NULL OR substr(release_date, 1, 4) IN ($3, $4, $5))
       ORDER BY (substr(release_date, 1, 4) = $3) DESC, vote_count DESC NULLS LAST LIMIT 1`,
      [title, year, String(year), String((year ?? 0) - 1), String((year ?? 0) + 1)],
    );
    return rows[0] ? this.movie(rows[0]) : null;
  }

  // --- shows ---------------------------------------------------------------

  async showByTmdbId(id: number): Promise<CatalogShow | null> {
    const { rows } = await this.db.query<ShowRow>(`SELECT ${SHOW_COLUMNS} FROM tmdb_show WHERE id = $1`, [id]);
    return rows[0] ? this.show(rows[0]) : null;
  }

  async showsByTmdbIds(ids: number[]): Promise<Map<number, CatalogShow>> {
    const result = new Map<number, CatalogShow>();
    if (ids.length === 0) return result;
    const { rows } = await this.db.query<ShowRow>(`SELECT ${SHOW_COLUMNS} FROM tmdb_show WHERE id = ANY($1::int[])`, [ids]);
    for (const row of rows) result.set(row.id, this.show(row));
    return result;
  }

  async showByImdbId(imdbId: string): Promise<CatalogShow | null> {
    const { rows } = await this.db.query<ShowRow>(`SELECT ${SHOW_COLUMNS} FROM tmdb_show WHERE imdb_id = $1 LIMIT 1`, [imdbId]);
    return rows[0] ? this.show(rows[0]) : null;
  }

  async showByTvdbId(tvdbId: string): Promise<CatalogShow | null> {
    const numeric = Number(tvdbId);
    if (!Number.isInteger(numeric)) return null;
    const { rows } = await this.db.query<ShowRow>(`SELECT ${SHOW_COLUMNS} FROM tmdb_show WHERE tvdb_id = $1 LIMIT 1`, [numeric]);
    return rows[0] ? this.show(rows[0]) : null;
  }

  async showByName(name: string, year: number | null): Promise<CatalogShow | null> {
    const { rows } = await this.db.query<ShowRow>(
      `SELECT ${SHOW_COLUMNS} FROM tmdb_show
       WHERE (lower(name_en) = lower($1) OR lower(name_pt) = lower($1) OR lower(original_name) = lower($1))
         AND ($2::int IS NULL OR substr(first_air_date, 1, 4) = $3)
       ORDER BY vote_count DESC NULLS LAST LIMIT 1`,
      [name, year, String(year)],
    );
    return rows[0] ? this.show(rows[0]) : null;
  }

  // --- episodes ------------------------------------------------------------

  /** Find the show that an episode TMDB id belongs to. */
  async showForEpisode(episodeTmdbId: number): Promise<{ show: CatalogShow; season: number; number: number } | null> {
    const { rows } = await this.db.query<ShowRow & { season_number: number; episode_number: number }>(
      `SELECT ${SHOW_COLUMNS.replaceAll(/(^|, )/g, "$1s.")}, se.season_number, e.episode_number
       FROM tmdb_episode e
       JOIN tmdb_season se ON se.id = e.season_id
       JOIN tmdb_show s ON s.id = se.show_id
       WHERE e.id = $1`,
      [episodeTmdbId],
    );
    const row = rows[0];
    if (!row) return null;
    return { show: this.show(row), season: row.season_number, number: row.episode_number };
  }

  async episode(showTmdbId: number, season: number, number: number): Promise<CatalogEpisode | null> {
    const { rows } = await this.db.query<EpisodeRow>(
      `SELECT e.id, se.season_number, e.episode_number, e.name, e.air_date, e.runtime, e.still_path
       FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
       WHERE se.show_id = $1 AND se.season_number = $2 AND e.episode_number = $3`,
      [showTmdbId, season, number],
    );
    const row = rows[0];
    return row ? this.episodeOf(row) : null;
  }

  async episodes(showTmdbId: number): Promise<CatalogEpisode[]> {
    const { rows } = await this.db.query<EpisodeRow>(
      `SELECT e.id, se.season_number, e.episode_number, e.name, e.air_date, e.runtime, e.still_path
       FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
       WHERE se.show_id = $1 ORDER BY se.season_number, e.episode_number`,
      [showTmdbId],
    );
    return rows.map((row) => this.episodeOf(row));
  }

  /** Count of aired regular-season episodes per show, for progress totals. */
  async airedEpisodeCounts(showTmdbIds: number[], today: string): Promise<Map<number, number>> {
    const result = new Map<number, number>();
    if (showTmdbIds.length === 0) return result;
    const { rows } = await this.db.query<{ show_id: number; count: number }>(
      `SELECT se.show_id, COUNT(*)::int AS count
       FROM tmdb_episode e JOIN tmdb_season se ON se.id = e.season_id
       WHERE se.show_id = ANY($1::int[]) AND se.season_number > 0 AND (e.air_date IS NULL OR e.air_date <= $2)
       GROUP BY se.show_id`,
      [showTmdbIds, today],
    );
    for (const row of rows) result.set(row.show_id, row.count);
    return result;
  }

  private episodeOf(row: EpisodeRow): CatalogEpisode {
    return {
      tmdbId: row.id,
      season: row.season_number,
      number: row.episode_number,
      title: row.name,
      airDate: row.air_date,
      runtimeMin: row.runtime,
      stillPath: row.still_path,
    };
  }
}

export function createCatalog(pool: Pool | null, language: CatalogLanguage): Catalog | null {
  return pool ? new Catalog(pool, language) : null;
}
