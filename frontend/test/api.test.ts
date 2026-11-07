import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { actionRequest } from "../src/lib/api.ts";

const SHOW = "0190f0a0-0000-7000-8000-000000000001";

describe("actionRequest", () => {
  it("calls the season route of the show for the season buttons", () => {
    assert.deepEqual(actionRequest("watch-season", SHOW, { season: 2 }), {
      method: "POST",
      path: `/api/shows/${SHOW}/seasons/2/watched`,
    });
    assert.deepEqual(actionRequest("unwatch-season", SHOW, { season: 0 }), {
      method: "DELETE",
      path: `/api/shows/${SHOW}/seasons/0/watched`,
    });
  });
});
