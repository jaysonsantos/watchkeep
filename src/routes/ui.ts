import { Hono } from "hono";
import type { AppContext } from "../app.ts";
import type { TargetKind } from "../db.ts";
import { sortOf, type WatchFilter } from "../queries.ts";
import { LOGO_SVG, renderAdd, renderDashboard, renderHistory, renderMovies, renderShow, renderShows, renderWatchlist, renderWebhooks } from "../ui/render.ts";

const LIST_PAGE_SIZE = 60;

function filterOf(value: string | undefined): WatchFilter {
  return value === "watched" || value === "unwatched" ? value : "all";
}

/** The requested page, clamped to the pages that exist. */
function pageOf(value: string | undefined, total: number): number {
  const last = Math.max(1, Math.ceil(total / LIST_PAGE_SIZE));
  return Math.min(last, Math.max(1, Math.floor(Number(value ?? 1)) || 1));
}

export function uiRoutes(ctx: AppContext): Hono {
  const app = new Hono();
  const images = ctx.config.imageBaseUrl;

  app.get("/logo.svg", (c) => {
    c.header("Content-Type", "image/svg+xml");
    c.header("Cache-Control", "public, max-age=86400");
    return c.body(LOGO_SVG);
  });

  app.get("/", async (c) =>
    c.html(
      renderDashboard({
        images,
        stats: await ctx.queries.stats(),
        inProgress: await ctx.queries.inProgress(),
        recent: await ctx.queries.history(15),
        syncConfigured: Boolean(ctx.config.plexUrl && ctx.config.plexToken),
        catalogConfigured: ctx.catalog !== null,
      }),
    ),
  );

  app.get("/movies", async (c) => {
    const filter = filterOf(c.req.query("status"));
    const search = c.req.query("q") ?? "";
    const sort = sortOf(c.req.query("sort"));
    const total = await ctx.queries.movieCount(filter, search);
    const page = pageOf(c.req.query("page"), total);
    const movies = await ctx.queries.movies(filter, search, sort, LIST_PAGE_SIZE, (page - 1) * LIST_PAGE_SIZE);
    return c.html(renderMovies({ images, movies, total, list: { filter, search, sort, page, pageSize: LIST_PAGE_SIZE } }));
  });

  app.get("/shows", async (c) => {
    const filter = filterOf(c.req.query("status"));
    const search = c.req.query("q") ?? "";
    const sort = sortOf(c.req.query("sort"));
    const all = await ctx.views.shows(filter, search, sort);
    const page = pageOf(c.req.query("page"), all.length);
    const shows = all.slice((page - 1) * LIST_PAGE_SIZE, page * LIST_PAGE_SIZE);
    return c.html(renderShows({ images, shows, total: all.length, list: { filter, search, sort, page, pageSize: LIST_PAGE_SIZE } }));
  });

  app.get("/shows/:id", async (c) => {
    const id = Number(c.req.param("id"));
    const detail = Number.isInteger(id) ? await ctx.views.show(id) : null;
    if (!detail) return c.notFound();
    return c.html(
      renderShow({
        images,
        show: detail.show,
        episodes: detail.episodes,
        onWatchlist: await ctx.queries.isOnWatchlist("show", detail.show.id),
      }),
    );
  });

  app.get("/watchlist", async (c) => c.html(renderWatchlist({ images, items: await ctx.queries.watchlist() })));

  app.get("/add", async (c) => {
    const query = (c.req.query("q") ?? "").trim();
    const catalog = ctx.catalog;
    const movies = catalog && query.length >= 2 ? await catalog.searchMovies(query) : [];
    const shows = catalog && query.length >= 2 ? await catalog.searchShows(query) : [];
    const localMovies = await ctx.queries.localByTmdb("movie", movies.map((movie) => movie.tmdbId));
    const localShows = await ctx.queries.localByTmdb("show", shows.map((show) => show.tmdbId));
    return c.html(
      renderAdd({
        images,
        query,
        catalogConfigured: catalog !== null,
        movies: movies.map((movie) => ({ ...movie, localId: localMovies.get(movie.tmdbId) ?? null })),
        shows: shows.map((show) => ({ ...show, localId: localShows.get(show.tmdbId) ?? null })),
        error: c.req.query("error"),
      }),
    );
  });

  /** Add form: kind, tmdb_id or title, year, and an optional watchlist flag. */
  app.post("/add", async (c) => {
    const form = await c.req.parseBody();
    const field = (name: string) => String(form[name] ?? "").trim();
    const kind = field("kind") === "show" ? "show" : "movie";
    const row = await ctx.actions.addMedia({
      kind,
      tmdbId: Number(field("tmdb_id")) || null,
      title: field("title") || null,
      year: Number(field("year")) || null,
      watchlist: field("watchlist") === "1",
    });
    if (!row) return c.redirect(`/add?q=${encodeURIComponent(field("q"))}&error=title`, 303);
    if (field("watchlist") === "1") return c.redirect("/watchlist", 303);
    return c.redirect(kind === "show" ? `/shows/${row.id}` : "/movies?status=unwatched", 303);
  });

  app.get("/history", async (c) => {
    const pageSize = 50;
    const page = Math.max(1, Number(c.req.query("page") ?? 1) || 1);
    const total = await ctx.queries.historyCount();
    return c.html(
      renderHistory({ images, entries: await ctx.queries.history(pageSize, (page - 1) * pageSize), page, pageSize, total }),
    );
  });

  app.get("/webhooks", async (c) => c.html(renderWebhooks({ events: await ctx.queries.recentWebhooks(100) })));

  /** Form actions. Each one redirects back to the page named in `back`. */
  app.post("/actions", async (c) => {
    const form = await c.req.parseBody();
    const field = (name: string) => String(form[name] ?? "");
    const action = field("action");
    const id = Number(field("id") || 0);
    const back = field("back") || "/";
    const target = back.startsWith("/") && !back.startsWith("//") ? back : "/";
    if (Number.isInteger(id) && id > 0) {
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
    } else if (action === "sync") {
      if (ctx.config.plexUrl && ctx.config.plexToken) {
        ctx.sync().catch((error: Error) => ctx.log(`sync failed: ${error.message}`));
      }
    }
    return c.redirect(target, 303);
  });

  return app;
}
