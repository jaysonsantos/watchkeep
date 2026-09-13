import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { addFromForm, addPage } from "../src/lib/server/pages.ts";
import { formData, testApp, type TestContext } from "./helpers.ts";

let ctx: TestContext | undefined;
afterEach(async () => {
  const current = ctx;
  ctx = undefined;
  await current?.close();
});

describe("adding movies and shows", () => {
  it("searches the catalog and adds by TMDB id", async () => {
    const built = await testApp({}, { withCatalog: true });
    ctx = built.ctx;
    const search = (await (await built.app.request("/api/search?q=sever")).json()) as { movies: unknown[]; shows: Array<{ tmdbId: number }> };
    assert.equal(search.movies.length, 0);
    assert.equal(search.shows[0]?.tmdbId, 95396);

    const created = await built.app.request("/api/shows", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tmdb_id: 95396, watchlist: true }),
    });
    assert.equal(created.status, 201);
    const show = (await created.json()) as { id: number; title: string; year: number; poster_path: string | null; tvdb_id: string | null };
    assert.equal(show.title, "Severance");
    assert.equal(show.year, 2022);
    assert.equal(show.tvdb_id, "371980");
    assert.ok(show.poster_path);
    assert.equal((await ctx.queries.watchlist())[0]?.title, "Severance");
    assert.equal((await ctx.views.show(show.id))?.episodes.length, 6, "the catalog episode list is available at once");

    const again = await built.app.request("/api/shows", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tmdb_id: 95396 }),
    });
    assert.equal(((await again.json()) as { id: number }).id, show.id, "no duplicate");

    const page = await addPage(ctx, new URLSearchParams("q=sever"));
    assert.equal(page.shows[0]?.title, "Severance");
    assert.equal(page.shows[0]?.localId, show.id, "the result shows that the library has it");
    const unknown = await built.app.request("/api/movies", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tmdb_id: 123456789 }),
    });
    assert.equal(unknown.status, 400);
  });

  it("adds by title without a catalog and through the form", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const search = await built.app.request("/api/search?q=heat");
    assert.equal(search.status, 400);

    const created = await built.app.request("/api/movies", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ title: "Heat", year: 1995 }),
    });
    assert.equal(created.status, 201);
    assert.equal((await ctx.queries.movies("unwatched")).length, 1);

    const location = await addFromForm(ctx, formData({ kind: "show", title: "Pluribus", year: "2025", watchlist: "1" }));
    assert.equal(location, "/watchlist");
    const items = await ctx.queries.watchlist();
    assert.deepEqual(items.map((item) => [item.kind, item.title]), [["show", "Pluribus"]]);

    assert.equal(await addFromForm(ctx, formData({ kind: "movie", title: "  " })), "/add?q=&error=title");
    assert.equal((await addPage(ctx, new URLSearchParams())).catalogConfigured, false);
  });
});
