import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { statsUrl } from "../src/lib/api.ts";
import { fmtHour, fmtHours, fmtMonth, fmtWatchTime, fmtWeekday } from "../src/lib/format.ts";
import { peakBucket, sharePercent } from "../src/lib/stats.ts";
import type { BucketCount } from "../src/lib/types.ts";

const MS_PER_HOUR = 3_600_000;

function buckets(plays: number[]): BucketCount[] {
  return plays.map((count, bucket) => ({ bucket, plays: count, runtime_ms: 0 }));
}

describe("statistics formatting", () => {
  it("reads a long watch time in days and hours", () => {
    assert.equal(fmtWatchTime(0), "0m");
    assert.equal(fmtWatchTime(null), "0m");
    assert.equal(fmtWatchTime(90 * 60_000), "1h 30m", "below a day it stays exact");
    assert.equal(fmtWatchTime(25 * MS_PER_HOUR), "1d 1h");
    assert.equal(fmtWatchTime(24 * MS_PER_HOUR), "1d 0h");
    assert.equal(fmtHours(90 * MS_PER_HOUR), "90 hours");
  });

  it("labels the calendar buckets", () => {
    assert.equal(fmtMonth("2026-01"), "Jan 26");
    assert.equal(fmtMonth("2025-12"), "Dec 25");
    assert.equal(fmtMonth("not-a-month"), "not-a-month");
    assert.equal(fmtWeekday(0), "Sun");
    assert.equal(fmtWeekday(6), "Sat");
    assert.equal(fmtHour(0), "00");
    assert.equal(fmtHour(21), "21");
  });
});

describe("statistics helpers", () => {
  it("finds the busiest bucket", () => {
    assert.equal(peakBucket(buckets([1, 4, 2]))?.bucket, 1);
    assert.equal(peakBucket(buckets([4, 4, 2]))?.bucket, 0, "the first of a tie wins");
    assert.equal(peakBucket(buckets([0, 0, 0])), null);
    assert.equal(peakBucket([]), null);
  });

  it("gives a share as a whole percent", () => {
    assert.equal(sharePercent(1, 4), 25);
    assert.equal(sharePercent(1, 3), 33);
    assert.equal(sharePercent(3, 0), 0, "an empty whole has no share");
  });

  it("builds the statistics URL and leaves out an empty time zone", () => {
    assert.equal(statsUrl("Europe/Berlin"), "/api/statistics?tz=Europe%2FBerlin");
    assert.equal(statsUrl(""), "/api/statistics");
  });
});
