import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { parsePlexPayload, type PlexEvent } from "../src/plex/payload.ts";
import { episodePayload, moviePayload, testApp, testContext, type TestContext } from "./helpers.ts";

function event(payload: unknown): PlexEvent {
  const parsed = parsePlexPayload(payload);
  assert.ok(parsed.ok);
  return parsed.event;
}

let ctx: TestContext | undefined;
afterEach(async () => {
  const current = ctx;
  ctx = undefined;
  await current?.close();
});

describe("catalog enrichment", () => {
  it("fills TMDB ids, posters and runtimes for movies", async () => {
    ctx = await testContext({}, { withCatalog: true });
    await ctx.scrobbler.apply(event(moviePayload({ event: "media.play" }, { Guid: [{ id: "imdb://tt0369339" }], title: "Collateral", year: 2004, duration: undefined })));
    const movie = (await ctx.queries.movies())[0]!;
    assert.equal(movie.tmdb_id, 4638);
    assert.equal(movie.poster_path, "/collateral.jpg");
    assert.equal(movie.duration_ms, 120 * 60_000, "runtime comes from the catalog when Plex sends none");
  });

  it("matches movies by title and year when Plex sends no ids", async () => {
    ctx = await testContext({}, { withCatalog: true });
    await ctx.scrobbler.apply(event(moviePayload({ event: "media.play" }, { Guid: [], guid: "local://1" })));
    assert.equal((await ctx.queries.movies())[0]?.tmdb_id, 949);
  });

  it("resolves the show from an episode TMDB id and keeps ids consistent", async () => {
    ctx = await testContext({}, { withCatalog: true });
    await ctx.scrobbler.apply(event(episodePayload({ event: "media.scrobble" })));
    const show = (await ctx.queries.shows())[0]!;
    assert.equal(show.tmdb_id, 95396);
    assert.equal(show.imdb_id, "tt11280740");
    assert.equal(show.tvdb_id, "371980");
    assert.equal(show.year, 2022);
    assert.equal(show.poster_path, "/pPHpeI2X1qEd1CS1SeyrdhZ4qnT.jpg");
    const episode = (await ctx.queries.episodes(show.id))[0]!;
    assert.equal(episode.tmdb_id, 1982925);

    // A second Plex server with different guids reports the same show by name only.
    await ctx.scrobbler.apply(
      event(episodePayload({ event: "media.scrobble" }, { guid: "plex://episode/other", grandparentGuid: "plex://show/other", Guid: [], index: 2, title: undefined })),
    );
    assert.equal((await ctx.queries.shows()).length, 1);
    assert.equal((await ctx.queries.episodes(show.id)).length, 2);
    assert.equal((await ctx.queries.episodes(show.id))[1]?.title, "Half Loop", "episode title comes from the catalog");
  });

  it("lists unwatched episodes from the catalog and counts only aired ones", async () => {
    ctx = await testContext({}, { withCatalog: true });
    await ctx.scrobbler.apply(event(episodePayload({ event: "media.scrobble" })));
    const [show] = await ctx.views.shows();
    assert.equal(show?.watched_count, 1);
    assert.equal(show?.total_episodes, 4, "three season-1 episodes plus one aired season-2 episode");
    assert.equal((await ctx.views.shows("unwatched")).length, 1);
    assert.equal((await ctx.views.shows("watched")).length, 0);

    const detail = await ctx.views.show(show!.id);
    assert.equal(detail?.episodes.length, 6);
    const half = detail!.episodes.find((e) => e.season === 1 && e.number === 2)!;
    assert.equal(half.id, null);
    assert.equal(half.title, "Half Loop");

    assert.ok(await ctx.actions.markEpisodeByNumber(show!.id, 1, 2, true));
    const after = await ctx.views.show(show!.id);
    const marked = after!.episodes.find((e) => e.season === 1 && e.number === 2)!;
    assert.ok(marked.id !== null);
    assert.equal(marked.play_count, 1);
    assert.equal(marked.tmdb_id, 3396429);

    assert.equal(await ctx.actions.markShowWatched(show!.id), 2, "the two remaining aired episodes");
    assert.equal((await ctx.views.shows("watched")).length, 1);
    assert.equal((await ctx.views.show(show!.id))?.episodes.filter((e) => e.play_count > 0).length, 4);
  });

  it("exposes catalog episodes through the API and UI", async () => {
    const built = await testApp({}, { withCatalog: true });
    ctx = built.ctx;
    await built.app.request("/webhook/plex", { method: "POST", body: JSON.stringify(episodePayload({ event: "media.play" })), headers: { "content-type": "application/json" } });
    const show = (await (await built.app.request("/api/shows/1")).json()) as { total_episodes: number; episodes: Array<{ id: number | null }> };
    assert.equal(show.total_episodes, 4);
    assert.equal(show.episodes.length, 6);

    const marked = await built.app.request("/api/shows/1/seasons/2/episodes/1/watched", { method: "POST" });
    assert.equal(marked.status, 200);
    const page = await (await built.app.request("/shows/1")).text();
    assert.match(page, /Hello, Ms\. Cobel/);
    assert.match(page, /image\.tmdb\.org\/t\/p\/w185\/pPHpeI2X1qEd1CS1SeyrdhZ4qnT\.jpg/);
    const form = new URLSearchParams({ action: "watch-episode-number", id: "1", season: "1", number: "3", back: "/shows/1" });
    const response = await built.app.request("/actions", {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: form.toString(),
    });
    assert.equal(response.status, 303);
    assert.equal((await ctx.queries.show(1))?.watched_count, 2);
  });
});
