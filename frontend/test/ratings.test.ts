import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { ratingPath } from "../src/lib/api.ts";
import { fmtRating, MAX_USER_RATING, MIN_RATING_PICK, RATING_PICKS } from "../src/lib/format.ts";

describe("ratings", () => {
  it("builds the rating API path", () => {
    assert.equal(ratingPath("movie", "0199-movie"), "/api/ratings/movie/0199-movie");
    assert.equal(ratingPath("show", "0199-show"), "/api/ratings/show/0199-show");
    assert.equal(ratingPath("episode", "0199-episode"), "/api/ratings/episode/0199-episode");
  });

  it("offers the 1 to 10 picker on the stored scale", () => {
    assert.equal(RATING_PICKS[0], MIN_RATING_PICK);
    assert.equal(RATING_PICKS[RATING_PICKS.length - 1], MAX_USER_RATING);
    assert.equal(RATING_PICKS.length, MAX_USER_RATING - MIN_RATING_PICK + 1);
  });

  it("writes a rating for display", () => {
    assert.equal(fmtRating(8), "★ 8/10");
    assert.equal(fmtRating(8.5), "★ 8.5/10");
    assert.equal(fmtRating(null), "");
    assert.equal(fmtRating(undefined), "");
  });
});
