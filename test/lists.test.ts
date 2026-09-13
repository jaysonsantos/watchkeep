import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { testApp, type TestContext } from "./helpers.ts";

let ctx: TestContext | undefined;
afterEach(async () => {
  const current = ctx;
  ctx = undefined;
  await current?.close();
});

async function add(context: TestContext, kind: "movie" | "show", title: string, year: number): Promise<number> {
  const row = await context.actions.addMedia({ kind, title, year });
  context.clock.advanceMinutes(1);
  return row!.id;
}

describe("movie and show lists", () => {
  it("sorts movies and returns one page at a time", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const heat = await add(ctx, "movie", "Heat", 1995);
    await add(ctx, "movie", "Collateral", 2004);
    const alien = await add(ctx, "movie", "Alien", 1979);
    await ctx.actions.markWatched("movie", heat, "2026-01-01T13:00:00.000Z");
    await ctx.actions.markWatched("movie", alien, "2025-06-01T00:00:00.000Z");

    const titles = async (sort: "recent" | "title" | "year" | "added", limit: number | null = null, offset = 0) =>
      (await ctx!.queries.movies("all", "", sort, limit, offset)).map((movie) => movie.title);
    assert.deepEqual(await titles("recent"), ["Heat", "Alien", "Collateral"]);
    assert.deepEqual(await titles("title"), ["Alien", "Collateral", "Heat"]);
    assert.deepEqual(await titles("year"), ["Collateral", "Heat", "Alien"]);
    assert.deepEqual(await titles("added"), ["Alien", "Collateral", "Heat"]);
    assert.deepEqual(await titles("recent", 2, 1), ["Alien", "Collateral"]);
    assert.equal(await ctx.queries.movieCount("watched"), 2);
    assert.equal(await ctx.queries.movieCount("all", "coll"), 1);

    const response = await built.app.request("/api/movies?sort=title&limit=1&offset=1");
    assert.equal(response.headers.get("x-total-count"), "3");
    assert.deepEqual(((await response.json()) as Array<{ title: string }>).map((movie) => movie.title), ["Collateral"]);
  });

  it("sorts shows and falls back to the newest addition", async () => {
    const built = await testApp();
    ctx = built.ctx;
    await add(ctx, "show", "Severance", 2022);
    await add(ctx, "show", "Andor", 2022);

    assert.deepEqual((await ctx.views.shows()).map((show) => show.title), ["Andor", "Severance"]);
    assert.deepEqual((await ctx.views.shows("all", "", "title")).map((show) => show.title), ["Andor", "Severance"]);
    assert.deepEqual((await ctx.views.shows("all", "", "added")).map((show) => show.title), ["Andor", "Severance"]);

    const response = await built.app.request("/api/shows?sort=title&limit=1&offset=1");
    assert.equal(response.headers.get("x-total-count"), "2");
    assert.deepEqual(((await response.json()) as Array<{ title: string }>).map((show) => show.title), ["Severance"]);
  });

  it("renders the sort select and keeps the state in page links", async () => {
    const built = await testApp();
    ctx = built.ctx;
    for (let index = 0; index < 61; index += 1) await ctx.actions.addMedia({ kind: "movie", title: `Movie ${index}` });

    const first = await (await built.app.request("/movies?sort=title&q=Movie")).text();
    assert.match(first, /<option value="title" selected>Title<\/option>/);
    assert.match(first, /Page 1 of 2/);
    assert.ok(first.includes('href="/movies?q=Movie&amp;sort=title&amp;page=2"'));

    const last = await (await built.app.request("/movies?sort=title&q=Movie&page=99")).text();
    assert.match(last, /Page 2 of 2/);
    assert.equal((last.match(/class="card"/g) ?? []).length, 1);

    assert.equal((await built.app.request("/shows?sort=year&page=2")).status, 200);
  });
});
