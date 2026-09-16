import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { fmtClock, isLive, livePosition, splitProgress } from "../src/lib/nowplaying.ts";
import type { ProgressView } from "../src/lib/types.ts";

const MINUTE_MS = 60_000;
const NOW = Date.parse("2026-09-16T20:00:00Z");

function entry(overrides: Partial<ProgressView> = {}): ProgressView {
  return {
    target_kind: "movie",
    target_id: "0199-movie",
    position_ms: 10 * MINUTE_MS,
    duration_ms: 90 * MINUTE_MS,
    state: "playing",
    account: "jayson",
    player: "Living room",
    updated_at: new Date(NOW - 5 * MINUTE_MS).toISOString(),
    title: "A Movie",
    show_id: null,
    show_title: null,
    season: null,
    number: null,
    poster_path: null,
    ...overrides,
  };
}

describe("now playing", () => {
  it("moves the position of a playing row by the clock", () => {
    assert.equal(livePosition(entry(), NOW), 15 * MINUTE_MS);
    assert.equal(livePosition(entry({ state: "paused" }), NOW), 10 * MINUTE_MS);
    assert.equal(livePosition(entry({ updated_at: "not a date" }), NOW), 10 * MINUTE_MS);
  });

  it("never runs the position past the duration", () => {
    const long = entry({ updated_at: new Date(NOW - 5 * 60 * MINUTE_MS).toISOString() });
    assert.equal(livePosition(long, NOW), 90 * MINUTE_MS);
    assert.equal(livePosition({ ...long, duration_ms: null }, NOW), 310 * MINUTE_MS);
  });

  it("keeps only a row that can still run", () => {
    assert.ok(isLive(entry(), NOW));
    assert.ok(!isLive(entry({ state: "paused" }), NOW));
    assert.ok(!isLive(entry({ state: "stopped" }), NOW));
    const abandoned = entry({ updated_at: new Date(NOW - 8 * 60 * MINUTE_MS).toISOString() });
    assert.ok(!isLive(abandoned, NOW), "a row that no stop event closed drops out");
    assert.ok(!isLive({ ...abandoned, duration_ms: null }, NOW));
  });

  it("splits the progress rows into the widget and the list", () => {
    const paused = entry({ target_id: "0199-paused", state: "paused" });
    const split = splitProgress([entry(), paused], NOW);
    assert.deepEqual(
      split.playing.map((row) => row.target_id),
      ["0199-movie"],
    );
    assert.deepEqual(
      split.rest.map((row) => row.target_id),
      ["0199-paused"],
    );
  });

  it("writes a clock", () => {
    assert.equal(fmtClock(0), "0:00");
    assert.equal(fmtClock(65_000), "1:05");
    assert.equal(fmtClock(90 * MINUTE_MS), "1:30:00");
    assert.equal(fmtClock(null), "0:00");
    assert.equal(fmtClock(-1000), "0:00");
  });
});
