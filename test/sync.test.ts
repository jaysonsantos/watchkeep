import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { PlexClient } from "../src/lib/server/plex/sync.ts";
import { testContext, type TestContext } from "./helpers.ts";

let ctx: TestContext | undefined;
afterEach(async () => {
  const current = ctx;
  ctx = undefined;
  await current?.close();
});

/** A tiny fake Plex Media Server that answers the endpoints the sync uses. */
function fakePlex(): typeof fetch {
  const shows = [
    { ratingKey: "10", guid: "plex://show/show1", type: "show", title: "Severance", year: 2022, Guid: [{ id: "tmdb://95396" }] },
  ];
  const episodes = [
    { ratingKey: "11", guid: "plex://episode/ep1", grandparentRatingKey: "10", grandparentGuid: "plex://show/show1", grandparentTitle: "Severance", parentIndex: 1, index: 1, title: "Good News About Hell", duration: 3_400_000, viewCount: 2, lastViewedAt: 1_700_000_000, originallyAvailableAt: "2022-02-18" },
    { ratingKey: "12", guid: "plex://episode/ep2", grandparentRatingKey: "10", grandparentGuid: "plex://show/show1", grandparentTitle: "Severance", parentIndex: 1, index: 2, title: "Half Loop", duration: 3_000_000, viewOffset: 600_000 },
    { ratingKey: "13", guid: "plex://episode/ep3", grandparentRatingKey: "10", grandparentGuid: "plex://show/show1", grandparentTitle: "Severance", parentIndex: 1, index: 3, title: "In Perpetuity", duration: 3_000_000 },
  ];
  const movies = [
    { ratingKey: "20", guid: "plex://movie/heat", type: "movie", title: "Heat", year: 1995, duration: 10_000_000, viewCount: 1, lastViewedAt: 1_600_000_000, Guid: [{ id: "imdb://tt0113277" }] },
    { ratingKey: "21", guid: "plex://movie/collateral", type: "movie", title: "Collateral", year: 2004, duration: 7_000_000 },
  ];
  return async (input, init) => {
    const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
    const headers = new Headers(init?.headers);
    assert.equal(headers.get("X-Plex-Token"), "tok");
    const start = Number(headers.get("X-Plex-Container-Start") ?? 0);
    const size = Number(headers.get("X-Plex-Container-Size") ?? 500);
    const page = <T>(items: T[]) => ({
      MediaContainer: { size: Math.min(size, items.length - start), totalSize: items.length, Metadata: items.slice(start, start + size) },
    });
    if (url.pathname === "/library/sections") {
      return Response.json({ MediaContainer: { Directory: [{ key: "1", type: "movie", title: "Movies" }, { key: "2", type: "show", title: "TV" }, { key: "3", type: "artist", title: "Music" }] } });
    }
    const type = url.searchParams.get("type");
    if (url.pathname === "/library/sections/1/all" && type === "1") return Response.json(page(movies));
    if (url.pathname === "/library/sections/2/all" && type === "2") return Response.json(page(shows));
    if (url.pathname === "/library/sections/2/all" && type === "4") return Response.json(page(episodes));
    return new Response("not found", { status: 404 });
  };
}

describe("syncPlexLibrary", () => {
  it("imports the library with watch state", async () => {
    ctx = await testContext({ plexUrl: "http://plex.test:32400", plexToken: "tok" }, { fetchImpl: fakePlex() });
    const report = await ctx.sync();
    assert.deepEqual(report, { sections: 2, movies: 2, shows: 1, episodes: 3, playsImported: 2, progressImported: 1 });

    const stats = await ctx.queries.stats();
    assert.equal(stats.movies, 2);
    assert.equal(stats.movies_watched, 1);
    assert.equal(stats.episodes, 3);
    assert.equal(stats.episodes_watched, 1);
    const watched = (await ctx.queries.movies("watched"))[0]!;
    assert.equal((await ctx.library.lastPlay("movie", watched.id))?.watched_at, "2020-09-13T12:26:40.000Z");

    const show = (await ctx.queries.shows())[0]!;
    assert.equal(show.year, 2022);
    assert.equal(show.tmdb_id, 95396);
    const episodes = await ctx.queries.episodes(show.id);
    assert.equal(episodes[1]?.position_ms, 600_000);
    assert.equal(episodes[1]?.progress_state, "stopped");

    // A second sync changes nothing.
    const again = await ctx.sync();
    assert.equal(again.playsImported, 0);
    assert.equal((await ctx.queries.stats()).plays, 2);
  });

  it("enriches synced items from the catalog", async () => {
    ctx = await testContext({ plexUrl: "http://plex.test:32400", plexToken: "tok" }, { fetchImpl: fakePlex(), withCatalog: true });
    await ctx.sync();
    const movies = await ctx.queries.movies("all", "", "title");
    assert.deepEqual(movies.map((m) => [m.title, m.tmdb_id]), [["Collateral", 4638], ["Heat", 949]]);
    const show = (await ctx.queries.shows())[0]!;
    assert.equal(show.imdb_id, "tt11280740");
    assert.equal((await ctx.queries.episodes(show.id))[2]?.tmdb_id, 3396430);
  });

  it("paginates with the container headers", async () => {
    const calls: string[] = [];
    const base = fakePlex();
    const counting: typeof fetch = (input, init) => {
      calls.push(`${new Headers(init?.headers).get("X-Plex-Container-Start")}`);
      return base(input, init);
    };
    const client = new PlexClient({ plexUrl: "http://plex.test:32400", plexToken: "tok", fetchImpl: counting, pageSize: 2 });
    const episodes = await client.items("2", 4);
    assert.equal(episodes.length, 3);
    assert.deepEqual(calls, ["0", "2"]);
  });

  it("fails clearly when Plex is not configured", async () => {
    ctx = await testContext();
    await assert.rejects(ctx.sync(), /WATCHKEEP_PLEX_URL/);
  });
});
