/**
 * Data and form handling for the UI pages. The SvelteKit load functions and
 * form actions call these functions. They do not import SvelteKit, so the
 * `node:test` suites can call them directly.
 */
import { filterOf, LIST_PAGE_SIZE, pageOf, sortOf, type ListState } from "../lists.ts";
import type { AppContext } from "./app.ts";
import type { TargetKind } from "./db.ts";

const HISTORY_PAGE_SIZE = 50;

function listParams(params: URLSearchParams) {
  return { filter: filterOf(params.get("status")), search: params.get("q") ?? "", sort: sortOf(params.get("sort")) };
}

export async function dashboardPage(ctx: AppContext) {
  return {
    stats: await ctx.queries.stats(),
    inProgress: await ctx.queries.inProgress(),
    recent: await ctx.queries.history(15),
    syncConfigured: Boolean(ctx.config.plexUrl && ctx.config.plexToken),
    catalogConfigured: ctx.catalog !== null,
  };
}

export async function moviesPage(ctx: AppContext, params: URLSearchParams) {
  const { filter, search, sort } = listParams(params);
  const total = await ctx.queries.movieCount(filter, search);
  const page = pageOf(params.get("page"), total);
  const movies = await ctx.queries.movies(filter, search, sort, LIST_PAGE_SIZE, (page - 1) * LIST_PAGE_SIZE);
  const list: ListState = { filter, search, sort, page, pageSize: LIST_PAGE_SIZE };
  return { movies, total, list };
}

export async function showsPage(ctx: AppContext, params: URLSearchParams) {
  const { filter, search, sort } = listParams(params);
  // The watched filter needs the catalog episode counts, so the page is cut from the full list.
  const all = await ctx.views.shows(filter, search, sort);
  const page = pageOf(params.get("page"), all.length);
  const shows = all.slice((page - 1) * LIST_PAGE_SIZE, page * LIST_PAGE_SIZE);
  const list: ListState = { filter, search, sort, page, pageSize: LIST_PAGE_SIZE };
  return { shows, total: all.length, list };
}

/** Returns null when no show has this id. */
export async function showPage(ctx: AppContext, id: number) {
  const detail = Number.isInteger(id) && id > 0 ? await ctx.views.show(id) : null;
  if (!detail) return null;
  return { show: detail.show, episodes: detail.episodes, onWatchlist: await ctx.queries.isOnWatchlist("show", detail.show.id) };
}

export async function watchlistPage(ctx: AppContext) {
  return { items: await ctx.queries.watchlist() };
}

export async function historyPage(ctx: AppContext, params: URLSearchParams) {
  const total = await ctx.queries.historyCount();
  const page = pageOf(params.get("page"), total, HISTORY_PAGE_SIZE);
  const entries = await ctx.queries.history(HISTORY_PAGE_SIZE, (page - 1) * HISTORY_PAGE_SIZE);
  return { entries, page, pageSize: HISTORY_PAGE_SIZE, total };
}

export async function webhooksPage(ctx: AppContext) {
  return { events: await ctx.queries.recentWebhooks(100) };
}

export async function addPage(ctx: AppContext, params: URLSearchParams) {
  const query = (params.get("q") ?? "").trim();
  const catalog = ctx.catalog;
  const movies = catalog && query.length >= 2 ? await catalog.searchMovies(query) : [];
  const shows = catalog && query.length >= 2 ? await catalog.searchShows(query) : [];
  const localMovies = await ctx.queries.localByTmdb("movie", movies.map((movie) => movie.tmdbId));
  const localShows = await ctx.queries.localByTmdb("show", shows.map((show) => show.tmdbId));
  return {
    query,
    catalogConfigured: catalog !== null,
    movies: movies.map((movie) => ({ ...movie, localId: localMovies.get(movie.tmdbId) ?? null })),
    shows: shows.map((show) => ({ ...show, localId: localShows.get(show.tmdbId) ?? null })),
    error: params.get("error"),
  };
}

/** Add form: kind, tmdb_id or title, year, and an optional watchlist flag. Returns the path to redirect to. */
export async function addFromForm(ctx: AppContext, form: FormData): Promise<string> {
  const field = (name: string) => String(form.get(name) ?? "").trim();
  const kind = field("kind") === "show" ? "show" : "movie";
  const row = await ctx.actions.addMedia({
    kind,
    tmdbId: Number(field("tmdb_id")) || null,
    title: field("title") || null,
    year: Number(field("year")) || null,
    watchlist: field("watchlist") === "1",
  });
  if (!row) return `/add?q=${encodeURIComponent(field("q"))}&error=title`;
  if (field("watchlist") === "1") return "/watchlist";
  return kind === "show" ? `/shows/${row.id}` : "/movies?status=unwatched";
}

/** The buttons on the list and detail pages post `action`, `id`, and sometimes `kind`, `season`, and `number`. */
export async function applyAction(ctx: AppContext, form: FormData): Promise<void> {
  const field = (name: string) => String(form.get(name) ?? "");
  const action = field("action");
  const id = Number(field("id") || 0);
  if (!Number.isInteger(id) || id <= 0) {
    if (action === "sync" && ctx.config.plexUrl && ctx.config.plexToken) {
      ctx.sync().catch((error: Error) => ctx.log(`sync failed: ${error.message}`));
    }
    return;
  }
  switch (action) {
    case "watch-movie":
      await ctx.actions.markWatched("movie", id);
      break;
    case "unwatch-movie":
      await ctx.actions.markUnwatched("movie", id);
      break;
    case "watch-episode":
      await ctx.actions.markWatched("episode", id);
      break;
    case "unwatch-episode":
      await ctx.actions.markUnwatched("episode", id);
      break;
    case "watch-episode-number":
    case "unwatch-episode-number":
      await ctx.actions.markEpisodeByNumber(id, Number(field("season")), Number(field("number")), action === "watch-episode-number");
      break;
    case "watch-show":
      await ctx.actions.markShowWatched(id);
      break;
    case "unwatch-show":
      await ctx.actions.markShowUnwatched(id);
      break;
    case "remove-play":
      await ctx.actions.removePlay(id);
      break;
    case "watchlist-add-movie":
    case "watchlist-remove-movie":
      await ctx.actions.setWatchlist("movie", id, action === "watchlist-add-movie");
      break;
    case "watchlist-add-show":
    case "watchlist-remove-show":
      await ctx.actions.setWatchlist("show", id, action === "watchlist-add-show");
      break;
    case "hide-show":
    case "unhide-show":
      await ctx.actions.setHidden(id, action === "hide-show");
      break;
    case "clear-progress": {
      const kind = field("kind") as TargetKind;
      if (kind === "movie" || kind === "episode") await ctx.actions.clearProgress(kind, id);
      break;
    }
    default:
      break;
  }
}
