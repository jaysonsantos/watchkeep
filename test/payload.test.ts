import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { parseIds, parsePlexPayload } from "../src/plex/payload.ts";
import { episodePayload, moviePayload } from "./helpers.ts";

describe("parseIds", () => {
  it("reads modern plex guids and the Guid list", () => {
    const ids = parseIds("plex://movie/abc", [{ id: "imdb://tt1" }, { id: "tmdb://2" }, { id: "tvdb://3" }]);
    assert.deepEqual(ids, { plexGuid: "plex://movie/abc", imdb: "tt1", tmdb: "2", tvdb: "3" });
  });

  it("reads legacy agent guids", () => {
    assert.equal(parseIds("com.plexapp.agents.thetvdb://12345/1/5?lang=en", undefined).tvdb, "12345");
    assert.equal(parseIds("com.plexapp.agents.imdb://tt0113277?lang=en", undefined).imdb, "tt0113277");
    assert.equal(parseIds("com.plexapp.agents.themoviedb://949?lang=en", undefined).tmdb, "949");
  });
});

describe("parsePlexPayload", () => {
  it("parses an episode", () => {
    const result = parsePlexPayload(episodePayload());
    assert.ok(result.ok);
    assert.equal(result.event.event, "media.play");
    assert.equal(result.event.account, "jayson");
    assert.equal(result.event.viewOffsetMs, 120_000);
    assert.equal(result.event.media.type, "episode");
    if (result.event.media.type !== "episode") return;
    assert.equal(result.event.media.show.title, "Severance");
    assert.equal(result.event.media.show.ids.plexGuid, "plex://show/show1");
    assert.equal(result.event.media.season, 1);
    assert.equal(result.event.media.number, 1);
    assert.equal(result.event.media.ids.tvdb, "371980");
  });

  it("parses a movie", () => {
    const result = parsePlexPayload(moviePayload({ event: "media.stop" }));
    assert.ok(result.ok);
    assert.equal(result.event.media.type, "movie");
    if (result.event.media.type !== "movie") return;
    assert.equal(result.event.media.year, 1995);
    assert.equal(result.event.media.ids.imdb, "tt0113277");
  });

  it("derives show ids from a legacy episode guid", () => {
    const result = parsePlexPayload(
      episodePayload({}, { guid: "com.plexapp.agents.thetvdb://12345/1/5?lang=en", grandparentGuid: undefined, Guid: undefined }),
    );
    assert.ok(result.ok);
    if (result.event.media.type !== "episode") return;
    assert.equal(result.event.media.show.ids.tvdb, "12345");
  });

  it("rejects untracked events and media types", () => {
    const library = parsePlexPayload({ event: "library.new", Metadata: { type: "movie", title: "x" } });
    assert.ok(!library.ok);
    const track = parsePlexPayload(moviePayload({}, { type: "track" }));
    assert.ok(!track.ok);
    assert.ok(!parsePlexPayload("nope").ok);
    assert.ok(!parsePlexPayload({ Metadata: {} }).ok);
  });

  it("reads the rating for media.rate", () => {
    const result = parsePlexPayload(moviePayload({ event: "media.rate", rating: 8 }));
    assert.ok(result.ok);
    assert.equal(result.event.rating, 8);
  });
});
