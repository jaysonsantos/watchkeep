import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { fmtShortDay } from "../src/lib/format.ts";

/** Noon in UTC is the same day in the time zone of every test machine. */
const NOW = new Date("2026-09-21T12:00:00Z");
const LOCALE = "en-US";

describe("fmtShortDay", () => {
  it("leaves the year out of a day of this year", () => {
    assert.equal(fmtShortDay("2026-09-10T12:00:00Z", NOW, LOCALE), "Sep 10");
  });

  it("writes the year of a day of another year", () => {
    assert.equal(fmtShortDay("2024-03-02T12:00:00Z", NOW, LOCALE), "Mar 2, 2024");
  });

  it("gives no text for no value and keeps a value that is no date", () => {
    assert.equal(fmtShortDay(null, NOW, LOCALE), "");
    assert.equal(fmtShortDay(undefined, NOW, LOCALE), "");
    assert.equal(fmtShortDay("soon", NOW, LOCALE), "soon");
  });
});
