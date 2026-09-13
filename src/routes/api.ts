import { Hono } from "hono";
import type { AppContext } from "../app.ts";
import type { TargetKind } from "../db.ts";
import type { WatchFilter } from "../queries.ts";

function filterOf(value: string | undefined): WatchFilter {
  return value === "watched" || value === "unwatched" ? value : "all";
}

function idOf(value: string | undefined): number | null {
  const id = Number(value);
  return Number.isInteger(id) && id > 0 ? id : null;
}

export function apiRoutes(ctx: AppContext): Hono {
  const app = new Hono();

  app.get("/stats", async (c) => c.json(await ctx.queries.stats()));
  app.get("/progress", async (c) => c.json(await ctx.queries.inProgress()));
  app.get("/history", async (c) => {
    const limit = Math.min(500, Math.max(1, Number(c.req.query("limit") ?? 50) || 50));
    const offset = Math.max(0, Number(c.req.query("offset") ?? 0) || 0);
    return c.json({ total: await ctx.queries.historyCount(), items: await ctx.queries.history(limit, offset) });
  });
  app.get("/webhooks", async (c) => c.json(await ctx.queries.recentWebhooks(100)));

  app.get("/movies", async (c) => c.json(await ctx.queries.movies(filterOf(c.req.query("status")), c.req.query("q") ?? "")));
  app.get("/movies/:id", async (c) => {
    const id = idOf(c.req.param("id"));
    const movie = id ? await ctx.queries.movie(id) : null;
    if (!movie) return c.json({ error: "not found" }, 404);
    return c.json({ ...movie, progress: await ctx.library.getProgress("movie", movie.id) });
  });

  app.get("/shows", async (c) => c.json(await ctx.views.shows(filterOf(c.req.query("status")), c.req.query("q") ?? "")));
  app.get("/shows/:id", async (c) => {
    const id = idOf(c.req.param("id"));
    const detail = id ? await ctx.views.show(id) : null;
    if (!detail) return c.json({ error: "not found" }, 404);
    return c.json({ ...detail.show, episodes: detail.episodes });
  });

  const watchedRoute = (kind: TargetKind, prefix: string) => {
    app.post(`${prefix}/:id/watched`, async (c) => {
      const id = idOf(c.req.param("id"));
      const body = (await c.req.json().catch(() => ({}))) as { watched_at?: string };
      if (!id || !(await ctx.actions.markWatched(kind, id, body.watched_at))) return c.json({ error: "not found" }, 404);
      return c.json({ ok: true });
    });
    app.delete(`${prefix}/:id/watched`, async (c) => {
      const id = idOf(c.req.param("id"));
      if (!id || !(await ctx.actions.markUnwatched(kind, id))) return c.json({ error: "not found" }, 404);
      return c.json({ ok: true });
    });
  };
  watchedRoute("movie", "/movies");
  watchedRoute("episode", "/episodes");

  /** Mark an episode by number, for episodes that only the catalog knows. */
  app.post("/shows/:id/seasons/:season/episodes/:number/watched", async (c) => {
    const id = idOf(c.req.param("id"));
    const season = Number(c.req.param("season"));
    const number = Number(c.req.param("number"));
    if (!id || !Number.isInteger(season) || !Number.isInteger(number)) return c.json({ error: "not found" }, 404);
    if (!(await ctx.actions.markEpisodeByNumber(id, season, number, true))) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true });
  });
  app.delete("/shows/:id/seasons/:season/episodes/:number/watched", async (c) => {
    const id = idOf(c.req.param("id"));
    const season = Number(c.req.param("season"));
    const number = Number(c.req.param("number"));
    if (!id || !Number.isInteger(season) || !Number.isInteger(number)) return c.json({ error: "not found" }, 404);
    if (!(await ctx.actions.markEpisodeByNumber(id, season, number, false))) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true });
  });

  app.post("/shows/:id/watched", async (c) => {
    const id = idOf(c.req.param("id"));
    const changed = id ? await ctx.actions.markShowWatched(id) : -1;
    if (changed < 0) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true, episodes_changed: changed });
  });
  app.delete("/shows/:id/watched", async (c) => {
    const id = idOf(c.req.param("id"));
    const changed = id ? await ctx.actions.markShowUnwatched(id) : -1;
    if (changed < 0) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true, plays_removed: changed });
  });

  app.get("/search", async (c) => {
    const query = (c.req.query("q") ?? "").trim();
    if (!ctx.catalog) return c.json({ error: "No catalog configured. Set WATCHKEEP_CATALOG_DATABASE_URL." }, 400);
    if (query.length < 2) return c.json({ movies: [], shows: [] });
    const [movies, shows] = await Promise.all([ctx.catalog.searchMovies(query), ctx.catalog.searchShows(query)]);
    return c.json({ movies, shows });
  });

  /** Body: { tmdb_id?, title?, year?, watchlist? }. Returns the movie or show, created or existing. */
  for (const [kind, prefix] of [["movie", "/movies"], ["show", "/shows"]] as const) {
    app.post(prefix, async (c) => {
      const body = (await c.req.json().catch(() => ({}))) as { tmdb_id?: number; title?: string; year?: number; watchlist?: boolean };
      const row = await ctx.actions.addMedia({
        kind,
        tmdbId: Number.isInteger(body.tmdb_id) ? Number(body.tmdb_id) : null,
        title: typeof body.title === "string" ? body.title : null,
        year: Number.isInteger(body.year) ? Number(body.year) : null,
        watchlist: body.watchlist === true,
      });
      if (!row) return c.json({ error: "title is required, or a tmdb_id that the catalog knows" }, 400);
      return c.json(row, 201);
    });
  }

  app.get("/watchlist", async (c) => c.json(await ctx.queries.watchlist()));
  for (const [kind, prefix] of [["movie", "/movies"], ["show", "/shows"]] as const) {
    app.post(`${prefix}/:id/watchlist`, async (c) => {
      const id = idOf(c.req.param("id"));
      if (!id || !(await ctx.actions.setWatchlist(kind, id, true))) return c.json({ error: "not found" }, 404);
      return c.json({ ok: true });
    });
    app.delete(`${prefix}/:id/watchlist`, async (c) => {
      const id = idOf(c.req.param("id"));
      if (!id || !(await ctx.actions.setWatchlist(kind, id, false))) return c.json({ error: "not found" }, 404);
      return c.json({ ok: true });
    });
  }
  app.post("/shows/:id/hidden", async (c) => {
    const id = idOf(c.req.param("id"));
    if (!id || !(await ctx.actions.setHidden(id, true))) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true });
  });
  app.delete("/shows/:id/hidden", async (c) => {
    const id = idOf(c.req.param("id"));
    if (!id || !(await ctx.actions.setHidden(id, false))) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true });
  });

  app.delete("/history/:id", async (c) => {
    const id = idOf(c.req.param("id"));
    if (!id || !(await ctx.actions.removePlay(id))) return c.json({ error: "not found" }, 404);
    return c.json({ ok: true });
  });

  app.post("/sync", async (c) => {
    if (!ctx.config.plexUrl || !ctx.config.plexToken) {
      return c.json({ error: "Plex sync is not configured. Set WATCHKEEP_PLEX_URL and WATCHKEEP_PLEX_TOKEN." }, 400);
    }
    return c.json({ ok: true, report: await ctx.sync() });
  });

  return app;
}
