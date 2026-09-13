import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { episodePayload, moviePayload, multipart, testApp, type TestContext } from "./helpers.ts";

let ctx: TestContext | undefined;
afterEach(async () => {
  const current = ctx;
  ctx = undefined;
  await current?.close();
});

describe("POST /webhook/plex", () => {
  it("rejects a wrong token", async () => {
    const built = await testApp({ webhookToken: "secret" });
    ctx = built.ctx;
    const denied = await built.app.request("/webhook/plex?token=wrong", multipart(episodePayload()));
    assert.equal(denied.status, 401);
    const allowed = await built.app.request("/webhook/plex?token=secret", multipart(episodePayload()));
    assert.equal(allowed.status, 200);
    const header = await built.app.request("/webhook/plex", multipart(episodePayload()));
    assert.equal(header.status, 401);
  });

  it("accepts the multipart form Plex sends", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const response = await built.app.request("/webhook/plex", multipart(episodePayload({ event: "media.scrobble" })));
    assert.equal(response.status, 200);
    const body = (await response.json()) as { action: string; title: string };
    assert.equal(body.action, "play");
    assert.equal(body.title, "Severance S01E01 Pilot");
    assert.equal((await ctx.queries.stats()).episodes_watched, 1);
    assert.equal((await ctx.queries.recentWebhooks())[0]?.outcome, "play");
  });

  it("accepts plain JSON", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const response = await built.app.request("/webhook/plex", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(moviePayload({ event: "media.pause" }, { viewOffset: 5_000_000 })),
    });
    assert.equal(response.status, 200);
    const body = (await response.json()) as { action: string; percent: number };
    assert.equal(body.action, "progress");
    assert.equal(body.percent, 50);
  });

  it("logs and ignores untracked events", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const response = await built.app.request("/webhook/plex", multipart({ event: "library.new", Metadata: { type: "movie", title: "New" } }));
    assert.equal(response.status, 200);
    assert.match((await ctx.queries.recentWebhooks())[0]?.outcome ?? "", /ignored/);
    assert.equal((await ctx.queries.stats()).movies, 0);
  });

  it("rejects bodies that are not JSON", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const response = await built.app.request("/webhook/plex", { method: "POST", body: "not json" });
    assert.equal(response.status, 400);
  });
});

describe("JSON API and UI", () => {
  it("serves lists, details and manual actions", async () => {
    const built = await testApp();
    ctx = built.ctx;
    const { app } = built;
    await app.request("/webhook/plex", multipart(episodePayload({ event: "media.play" })));
    await app.request("/webhook/plex", multipart(moviePayload({ event: "media.play" })));

    const shows = (await (await app.request("/api/shows")).json()) as Array<{ id: number; total_episodes: number }>;
    assert.equal(shows.length, 1);
    assert.equal(shows[0]?.total_episodes, 1);

    const marked = await app.request("/api/episodes/1/watched", { method: "POST" });
    assert.equal(marked.status, 200);
    const show = (await (await app.request("/api/shows/1")).json()) as { watched_count: number };
    assert.equal(show.watched_count, 1);

    const movies = (await (await app.request("/api/movies?status=unwatched")).json()) as Array<{ id: number }>;
    assert.equal(movies.length, 1);
    const movieId = movies[0]!.id;
    assert.equal((await app.request(`/api/movies/${movieId}/watched`, { method: "POST" })).status, 200);
    assert.equal((await app.request("/api/movies/999/watched", { method: "POST" })).status, 404);
    assert.equal((await app.request("/api/movies/1/watched", { method: "POST" })).status, 404, "id 1 is the show, not a movie");
    assert.equal(((await (await app.request("/api/history")).json()) as { total: number }).total, 2);

    const unwatched = await app.request(`/api/movies/${movieId}/watched`, { method: "DELETE" });
    assert.equal(unwatched.status, 200);
    assert.equal(((await (await app.request("/api/stats")).json()) as { movies_watched: number }).movies_watched, 0);

    const sync = await app.request("/api/sync", { method: "POST" });
    assert.equal(sync.status, 400);
    const health = (await (await app.request("/healthz")).json()) as { ok: boolean; catalog: boolean };
    assert.deepEqual(health, { ok: true, catalog: false });
  });

  it("renders every page", async () => {
    const built = await testApp();
    ctx = built.ctx;
    await built.app.request("/webhook/plex", multipart(episodePayload({ event: "media.play" })));
    for (const path of ["/", "/movies", "/shows", "/shows/1", "/history", "/webhooks", "/movies?status=watched&q=x"]) {
      const response = await built.app.request(path);
      assert.equal(response.status, 200, path);
      assert.match(await response.text(), /Watchkeep/);
    }
    assert.equal((await built.app.request("/shows/42")).status, 404);
  });

  it("applies form actions and redirects", async () => {
    const built = await testApp();
    ctx = built.ctx;
    await built.app.request("/webhook/plex", multipart(episodePayload({ event: "media.play" })));
    const form = new URLSearchParams({ action: "watch-episode", id: "1", back: "/shows/1" });
    const response = await built.app.request("/actions", {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: form.toString(),
    });
    assert.equal(response.status, 303);
    assert.equal(response.headers.get("location"), "/shows/1");
    assert.equal((await ctx.queries.show(1))?.watched_count, 1);

    const escaped = await built.app.request("/actions", {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ action: "noop", id: "1", back: "//evil.example" }).toString(),
    });
    assert.equal(escaped.headers.get("location"), "/");
  });

  it("escapes titles in HTML", async () => {
    const built = await testApp();
    ctx = built.ctx;
    await built.app.request("/webhook/plex", multipart(moviePayload({ event: "media.play" }, { title: "<script>alert(1)</script>" })));
    const page = await (await built.app.request("/movies")).text();
    assert.ok(!page.includes("<script>alert(1)</script>"));
    assert.ok(page.includes("&lt;script&gt;"));
  });
});
