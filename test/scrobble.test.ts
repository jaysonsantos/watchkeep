import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { parsePlexPayload, type PlexEvent } from "../src/lib/server/plex/payload.ts";
import { episodePayload, moviePayload, testContext, type TestContext } from "./helpers.ts";

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

describe("Scrobbler", () => {
  it("tracks progress for play, pause and resume", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(episodePayload({ event: "media.play" })));
    let progress = await ctx.library.getProgress("episode", 1);
    assert.equal(progress?.state, "playing");
    assert.equal(progress?.position_ms, 120_000);

    await ctx.scrobbler.apply(event(episodePayload({ event: "media.pause" }, { viewOffset: 900_000 })));
    progress = await ctx.library.getProgress("episode", 1);
    assert.equal(progress?.state, "paused");
    assert.equal(progress?.position_ms, 900_000);

    await ctx.scrobbler.apply(event(episodePayload({ event: "media.resume" }, { viewOffset: undefined })));
    progress = await ctx.library.getProgress("episode", 1);
    assert.equal(progress?.state, "playing");
    assert.equal(progress?.position_ms, 900_000, "keeps the last position when Plex sends none");
    assert.equal(await ctx.library.playCount("episode", 1), 0);
  });

  it("records a play on media.scrobble and clears progress", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(episodePayload({ event: "media.play" })));
    const result = await ctx.scrobbler.apply(event(episodePayload({ event: "media.scrobble" })));
    assert.equal(result.action, "play");
    assert.equal(await ctx.library.playCount("episode", 1), 1);
    assert.equal(await ctx.library.getProgress("episode", 1), null);
    const play = await ctx.library.lastPlay("episode", 1);
    assert.equal(play?.source, "plex-scrobble");
    assert.equal(play?.account, "jayson");
    assert.equal(play?.player, "Plex Web");
  });

  it("records a play on media.stop past the threshold", async () => {
    ctx = await testContext({ watchedThresholdPercent: 85 });
    const stopped = await ctx.scrobbler.apply(event(moviePayload({ event: "media.stop" }, { viewOffset: 4_000_000 })));
    assert.equal(stopped.action, "progress");
    assert.equal((await ctx.library.getProgress("movie", 1))?.state, "stopped");
    assert.equal(await ctx.library.playCount("movie", 1), 0);

    const finished = await ctx.scrobbler.apply(event(moviePayload({ event: "media.stop" }, { viewOffset: 9_000_000 })));
    assert.equal(finished.action, "play");
    assert.equal((await ctx.library.lastPlay("movie", 1))?.source, "plex-stop");
    assert.equal(await ctx.library.getProgress("movie", 1), null);
  });

  it("does not count scrobble plus stop as two plays", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(moviePayload({ event: "media.scrobble" })));
    const second = await ctx.scrobbler.apply(event(moviePayload({ event: "media.stop" }, { viewOffset: 9_900_000 })));
    assert.equal(second.action, "duplicate-play");
    assert.equal(await ctx.library.playCount("movie", 1), 1);

    ctx.clock.advanceMinutes(60 * 24);
    const rewatch = await ctx.scrobbler.apply(event(moviePayload({ event: "media.scrobble" })));
    assert.equal(rewatch.action, "play");
    assert.equal(await ctx.library.playCount("movie", 1), 2);
  });

  it("matches the same episode across guid and season/number", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(episodePayload({ event: "media.scrobble" })));
    await ctx.scrobbler.apply(
      event(episodePayload({ event: "media.play" }, { guid: undefined, Guid: undefined, grandparentGuid: undefined })),
    );
    assert.equal((await ctx.queries.shows()).length, 1);
    assert.equal((await ctx.queries.episodes(1)).length, 1);
  });

  it("stores ratings", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(moviePayload({ event: "media.rate", rating: 9 })));
    assert.equal(await ctx.library.getRating("movie", 1), 9);
    assert.equal((await ctx.queries.movie(1))?.rating, 9);
  });

  it("ignores accounts outside the allow list", async () => {
    ctx = await testContext({ plexAccounts: ["someone-else"] });
    const result = await ctx.scrobbler.apply(event(moviePayload({ event: "media.scrobble" })));
    assert.equal(result.action, "ignored-account");
    assert.equal((await ctx.queries.stats()).movies, 0);
  });
});

describe("Actions", () => {
  it("marks episodes and whole shows watched and unwatched", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(episodePayload({ event: "media.play" })));
    await ctx.scrobbler.apply(
      event(episodePayload({ event: "media.play" }, { index: 2, guid: "plex://episode/ep2", title: "Half Loop", Guid: [] })),
    );
    assert.equal(await ctx.actions.markShowWatched(1), 2);
    assert.equal((await ctx.queries.show(1))?.watched_count, 2);
    assert.equal((await ctx.queries.inProgress()).length, 0);
    assert.ok(await ctx.actions.markUnwatched("episode", 2));
    assert.equal((await ctx.queries.show(1))?.watched_count, 1);
    assert.equal(await ctx.actions.markShowUnwatched(1), 1);
    assert.equal((await ctx.queries.show(1))?.watched_count, 0);
    assert.equal(await ctx.actions.markShowWatched(999), -1);
    assert.equal(await ctx.actions.markWatched("movie", 999), false);
  });

  it("filters movies by watch state", async () => {
    ctx = await testContext();
    await ctx.scrobbler.apply(event(moviePayload({ event: "media.scrobble" })));
    await ctx.scrobbler.apply(
      event(moviePayload({ event: "media.play" }, { title: "Collateral", year: 2004, guid: "plex://movie/collateral", Guid: [] })),
    );
    assert.deepEqual((await ctx.queries.movies("watched")).map((m) => m.title), ["Heat"]);
    assert.deepEqual((await ctx.queries.movies("unwatched")).map((m) => m.title), ["Collateral"]);
    assert.deepEqual((await ctx.queries.movies("all", "coll")).map((m) => m.title), ["Collateral"]);
  });
});
