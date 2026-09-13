import { Hono } from "hono";
import type { AppContext } from "../app.ts";
import type { TargetKind } from "../db.ts";
import type { WatchFilter } from "../queries.ts";
import { renderDashboard, renderHistory, renderMovies, renderShow, renderShows, renderWatchlist, renderWebhooks } from "../ui/render.ts";

function filterOf(value: string | undefined): WatchFilter {
  return value === "watched" || value === "unwatched" ? value : "all";
}

export function uiRoutes(ctx: AppContext): Hono {
  const app = new Hono();
  const images = ctx.config.imageBaseUrl;

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
    return c.html(renderMovies({ images, movies: await ctx.queries.movies(filter, search), filter, search }));
  });

  app.get("/shows", async (c) => {
    const filter = filterOf(c.req.query("status"));
    const search = c.req.query("q") ?? "";
    return c.html(renderShows({ images, shows: await ctx.views.shows(filter, search), filter, search }));
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
