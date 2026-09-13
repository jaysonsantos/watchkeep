import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, it } from "node:test";
import { deflateRawSync } from "node:zlib";
import { showPage, watchlistPage } from "../src/lib/server/pages.ts";
import { importTraktExport, sectionOf } from "../src/lib/server/trakt/import.ts";
import { readZip } from "../src/lib/server/trakt/zip.ts";
import { testApp, testContext, type TestContext } from "./helpers.ts";

/** Build a small ZIP in memory. Even entries are stored, odd entries deflated. */
function buildZip(files: Record<string, string>): Buffer {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let offset = 0;
  Object.entries(files).forEach(([name, content], index) => {
    const raw = Buffer.from(content, "utf8");
    const method = index % 2 === 0 ? 0 : 8;
    const data = method === 8 ? deflateRawSync(raw) : raw;
    const nameBuffer = Buffer.from(name, "utf8");
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(method, 8);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(raw.length, 22);
    local.writeUInt16LE(nameBuffer.length, 26);
    locals.push(local, nameBuffer, data);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(method, 10);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(raw.length, 24);
    central.writeUInt16LE(nameBuffer.length, 28);
    central.writeUInt32LE(offset, 42);
    centrals.push(central, nameBuffer);
    offset += local.length + nameBuffer.length + data.length;
  });
  const centralStart = offset;
  const centralBuffer = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(files).length, 8);
  end.writeUInt16LE(Object.keys(files).length, 10);
  end.writeUInt32LE(centralBuffer.length, 12);
  end.writeUInt32LE(centralStart, 16);
  return Buffer.concat([...locals, centralBuffer, end]);
}

const severance = {
  ids: { imdb: "tt11280740", plex: { guid: "5d9c0f1a" }, slug: "severance", tmdb: 95396, tvdb: 371980, trakt: 153017 },
  year: 2022,
  title: "Severance",
};
const heat = { ids: { imdb: "tt0113277", plex: { guid: "5d776825" }, slug: "heat-1995", tmdb: 949, trakt: 1 }, year: 1995, title: "Heat" };
const episode1 = { ids: { imdb: "tt1", plex: { guid: "aa1" }, tmdb: 1982925, tvdb: 1, trakt: 501 }, title: "Good News About Hell", number: 1, season: 1 };
const episode2 = { ids: { imdb: "tt2", plex: { guid: "aa2" }, tmdb: 3396429, tvdb: 2, trakt: 502 }, title: "Half Loop", number: 2, season: 1 };

function exportFiles(): Record<string, string> {
  return {
    "_errors.json": JSON.stringify([{ endpoint: "users/me/stats", error: "Unexpected end of JSON input" }]),
    "watched-history-1.json": JSON.stringify([
      { id: 9001, watched_at: "2026-08-21T06:41:00.000Z", action: "watch", type: "movie", movie: heat },
      { id: 9002, watched_at: "2026-07-21T19:19:00.000Z", action: "scrobble", type: "episode", episode: episode1, show: severance },
    ]),
    "watched-history-2.json": JSON.stringify([
      { id: 9003, watched_at: "2026-07-22T19:19:00.000Z", action: "checkin", type: "episode", episode: episode2, show: severance },
      { id: 9004, watched_at: "2020-01-01T00:00:00.000Z", action: "watch", type: "movie", movie: heat },
    ]),
    "ratings-movies.json": JSON.stringify([{ rated_at: "2026-06-30T19:55:52.000Z", rating: 9, type: "movie", movie: heat }]),
    "ratings-shows.json": JSON.stringify([{ rated_at: "2020-06-18T19:30:29.000Z", rating: 10, type: "show", show: severance }]),
    "ratings-episodes-1.json": JSON.stringify([{ rated_at: "2021-05-24T21:00:40.000Z", rating: 8, type: "episode", episode: episode1, show: severance }]),
    "watched-playback.json": JSON.stringify([
      { progress: 25, paused_at: "2026-08-19T21:09:06.000Z", id: 1, type: "episode", episode: { ids: { tmdb: 3396430, trakt: 503 }, title: "In Perpetuity", number: 3, season: 1 }, show: severance },
      { progress: 50, paused_at: "2026-08-19T21:09:06.000Z", id: 2, type: "movie", movie: { ids: { trakt: 77 }, year: 1999, title: "Unknown Film" } },
    ]),
    "lists-watchlist.json": JSON.stringify([
      { type: "movie", movie: { ids: { imdb: "tt0369339", tmdb: 4638, trakt: 2 }, year: 2004, title: "Collateral" }, rank: 1, id: 1, listed_at: "2026-01-04T14:02:03.000Z" },
      { type: "show", show: severance, rank: 2, id: 2, listed_at: "2026-01-04T14:02:03.000Z" },
    ]),
    "hidden-progress-watched.json": JSON.stringify([{ hidden_at: "2024-05-09T14:53:35.000Z", type: "show", show: severance }]),
    "comments-movies.json": "[]",
    "user-profile.json": JSON.stringify({ username: "someone" }),
  };
}

function writeExport(files = exportFiles()): string {
  const path = join(mkdtempSync(join(tmpdir(), "trakt-")), "export.zip");
  writeFileSync(path, buildZip(files));
  return path;
}

let ctx: TestContext | undefined;
afterEach(async () => {
  const current = ctx;
  ctx = undefined;
  await current?.close();
});

describe("readZip", () => {
  it("reads stored and deflated entries", () => {
    const entries = readZip(buildZip({ "a.json": "[1]", "dir/b.json": JSON.stringify({ long: "x".repeat(5000) }) }));
    assert.deepEqual(entries.map((entry) => entry.name), ["a.json", "dir/b.json"]);
    assert.equal(entries[0]!.read().toString(), "[1]");
    assert.equal(JSON.parse(entries[1]!.read().toString()).long.length, 5000);
    assert.throws(() => readZip(Buffer.from("not a zip")), /end of central directory/);
  });

  it("maps file names to sections", () => {
    assert.equal(sectionOf("watched-history-12.json"), "history");
    assert.equal(sectionOf("ratings-episodes-2.json"), "ratings");
    assert.equal(sectionOf("watched-playback.json"), "playback");
    assert.equal(sectionOf("lists-watchlist.json"), "watchlist");
    assert.equal(sectionOf("hidden-progress-watched.json"), "hidden");
    assert.equal(sectionOf("watched-movies-1.json"), null);
    assert.equal(sectionOf("ratings-seasons.json"), null);
  });
});

describe("importTraktExport", () => {
  it("imports plays, ratings, playback, watchlist and hidden shows with the catalog", async () => {
    ctx = await testContext({}, { withCatalog: true });
    const report = await importTraktExport(ctx.pool, ctx.catalog, writeExport(), { clock: ctx.clock });
    assert.equal(report.plays, 4);
    assert.equal(report.playsSkipped, 0);
    assert.equal(report.movies, 3);
    assert.equal(report.shows, 1);
    assert.equal(report.episodes, 3);
    assert.equal(report.ratings, 3);
    assert.equal(report.playback, 1);
    assert.equal(report.playbackSkipped, 1, "the unknown film has no runtime");
    assert.equal(report.watchlist, 2);
    assert.equal(report.hidden, 1);
    assert.deepEqual(report.ignoredFiles, ["comments-movies.json", "user-profile.json"]);
    assert.equal(report.exportErrors[0]?.endpoint, "users/me/stats");

    const movie = (await ctx.queries.movies("watched"))[0]!;
    assert.equal(movie.title, "Heat");
    assert.equal(movie.play_count, 2);
    assert.equal(movie.plex_guid, "plex://movie/5d776825");
    assert.equal(movie.tmdb_id, 949);
    assert.equal(movie.rating, 9);
    assert.equal(movie.last_watched_at, "2026-08-21T06:41:00.000Z");
    assert.equal((await ctx.library.lastPlay("movie", movie.id))?.source, "trakt");

    const show = (await ctx.views.shows())[0]!;
    assert.equal(show.tmdb_id, 95396);
    assert.equal(show.plex_guid, "plex://show/5d9c0f1a");
    assert.equal(show.watched_count, 2);
    assert.equal(show.hidden_at, "2024-05-09T14:53:35.000Z");
    assert.equal(show.rating, 10);
    assert.equal((await ctx.views.shows("unwatched")).length, 0, "hidden shows are not unwatched");
    const episodes = await ctx.queries.episodes(show.id);
    assert.equal(episodes[0]?.plex_guid, "plex://episode/aa1");
    assert.equal(episodes[2]?.progress_state, "stopped");
    assert.equal(episodes[2]?.position_ms, 56 * 60_000 * 0.25);
    assert.equal(await ctx.library.getRating("episode", episodes[0]!.id), 8);

    const watchlist = await ctx.queries.watchlist();
    assert.deepEqual(watchlist.map((item) => [item.kind, item.title, item.watched_count]), [["movie", "Collateral", 0], ["show", "Severance", 2]]);
    assert.equal((await ctx.queries.movies("unwatched")).find((m) => m.title === "Collateral")?.tmdb_id, 4638);
  });

  it("works without a catalog and is idempotent", async () => {
    ctx = await testContext();
    const path = writeExport();
    const first = await importTraktExport(ctx.pool, ctx.catalog, path, { clock: ctx.clock });
    assert.equal(first.plays, 4);
    assert.equal(first.playback, 0, "no runtime without a catalog");
    const second = await importTraktExport(ctx.pool, ctx.catalog, path, { clock: ctx.clock });
    assert.equal(second.plays, 0);
    assert.equal(second.playsSkipped, 4);
    assert.equal((await ctx.queries.stats()).plays, 4);
    assert.equal((await ctx.queries.shows()).length, 1);
    assert.equal((await ctx.queries.watchlist()).length, 2);
  });

  it("writes nothing on a dry run", async () => {
    ctx = await testContext();
    const report = await importTraktExport(ctx.pool, ctx.catalog, writeExport(), { clock: ctx.clock, dryRun: true });
    assert.equal(report.plays, 4);
    assert.equal(report.dryRun, true);
    assert.equal((await ctx.queries.stats()).plays, 0);
    assert.equal((await ctx.queries.stats()).movies, 0);
  });

  it("matches a Plex webhook to an imported item by plex guid", async () => {
    const built = await testApp();
    ctx = built.ctx;
    await importTraktExport(ctx.pool, ctx.catalog, writeExport(), { clock: ctx.clock });
    ctx.clock.advanceMinutes(60 * 24 * 365);
    await built.app.request("/webhook/plex", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        event: "media.scrobble",
        Metadata: { type: "movie", title: "Heat (Director's Cut)", guid: "plex://movie/5d776825", duration: 1 },
      }),
    });
    const movies = await ctx.queries.movies();
    assert.equal(movies.length, 3);
    assert.equal(movies.find((m) => m.tmdb_id === 949)?.play_count, 3);
  });
});

describe("bulk writes", () => {
  it("merges Trakt items that share an external id", async () => {
    ctx = await testContext();
    const twin = { ids: { imdb: "tt99", tmdb: 777, tvdb: 371980, trakt: 999 }, year: 2023, title: "Severance (duplicate)" };
    const files = exportFiles();
    files["watched-history-3.json"] = JSON.stringify([
      { id: 9101, watched_at: "2026-01-02T00:00:00.000Z", action: "watch", type: "episode", episode: { ids: { tmdb: 1982925, trakt: 601 }, title: "Same TMDB episode", number: 7, season: 3 }, show: twin },
      { id: 9102, watched_at: "2026-01-03T00:00:00.000Z", action: "watch", type: "movie", movie: { ids: { imdb: "tt0113277", tmdb: 424242, trakt: 55 }, year: 1995, title: "Heat (other entry)" } },
    ]);
    const report = await importTraktExport(ctx.pool, ctx.catalog, writeExport(files), { clock: ctx.clock });
    assert.equal(report.plays, 6);
    const shows = await ctx.queries.shows();
    assert.equal(shows.length, 1, "the same tvdb id merges into one show");
    assert.equal(shows[0]?.tmdb_id, 95396, "the first tmdb id is kept");
    assert.equal((await ctx.queries.episodes(shows[0]!.id)).length, 4);
    const movies = await ctx.queries.movies();
    assert.equal(movies.length, 3, "the same imdb id merges into one movie");
    assert.equal(movies.find((movie) => movie.imdb_id === "tt0113277")?.play_count, 3);
    const again = await importTraktExport(ctx.pool, ctx.catalog, writeExport(files), { clock: ctx.clock });
    assert.equal(again.plays, 0);
  });
});

describe("watchlist and hidden API", () => {
  it("adds and removes watchlist items and hides shows", async () => {
    const built = await testApp();
    ctx = built.ctx;
    await importTraktExport(ctx.pool, ctx.catalog, writeExport(), { clock: ctx.clock });
    const show = (await ctx.queries.shows())[0]!;
    assert.equal((await built.app.request(`/api/shows/${show.id}/hidden`, { method: "DELETE" })).status, 200);
    assert.equal((await ctx.queries.show(show.id))?.hidden_at, null);
    assert.equal((await built.app.request(`/api/shows/${show.id}/watchlist`, { method: "DELETE" })).status, 200);
    assert.equal((await ctx.queries.watchlist()).length, 1);
    assert.equal((await built.app.request(`/api/movies/${show.id}/watchlist`, { method: "POST" })).status, 404, "a show is not a movie");
    assert.ok((await watchlistPage(ctx)).items.some((item) => item.title === "Collateral"));
    const detail = await showPage(ctx, show.id);
    assert.equal(detail?.onWatchlist, false);
    assert.equal(detail?.show.hidden_at, null);
  });
});
