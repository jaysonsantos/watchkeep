/**
 * The "Now playing" widget of the dashboard. Plex writes a progress row only when it
 * sends an event, so the widget moves the position of a playing row by the clock and
 * drops a row that an event never closed.
 */

import type { PlayState, ProgressView } from "./types.ts";

const SECOND_MS = 1000;
const SECONDS_PER_MINUTE = 60;
const MINUTES_PER_HOUR = 60;
const MINUTE_MS = SECONDS_PER_MINUTE * SECOND_MS;
const HOUR_MS = MINUTES_PER_HOUR * MINUTE_MS;

/** Two digits, as a clock needs. */
const PAD = 2;

/** The words of the `state` column of a progress row. */
export const PLAY_STATE = {
  playing: "playing",
  paused: "paused",
  stopped: "stopped",
} as const satisfies Record<PlayState, PlayState>;

/** How often the widget moves the position of a playing row. */
export const TICK_MS = SECOND_MS;

/** How often the dashboard reloads its data while a row plays, to see a pause or a stop. */
export const REFRESH_MS = 20 * SECOND_MS;

/** How often the dashboard reloads its data while nothing plays, to see a play that starts. */
export const IDLE_REFRESH_MS = 60 * SECOND_MS;

/** A playing row stays live until this long after its expected end. A crashed player sends no stop. */
const END_SLACK_MS = 30 * MINUTE_MS;

/** How long a playing row without a duration stays live. */
const UNKNOWN_END_MS = 4 * HOUR_MS;

function stampOf(value: string): number | null {
  const stamp = Date.parse(value);
  return Number.isNaN(stamp) ? null : stamp;
}

/** The position of a row at `now`: the stored one, plus the time that ran since the event. */
export function livePosition(entry: ProgressView, now: number): number {
  if (entry.state !== PLAY_STATE.playing) return entry.position_ms;
  const stamp = stampOf(entry.updated_at);
  if (stamp === null) return entry.position_ms;
  const position = entry.position_ms + Math.max(0, now - stamp);
  return entry.duration_ms ? Math.min(position, entry.duration_ms) : position;
}

/** True while a row plays and its title can still run. */
export function isLive(entry: ProgressView, now: number): boolean {
  if (entry.state !== PLAY_STATE.playing) return false;
  const stamp = stampOf(entry.updated_at);
  if (stamp === null) return true;
  const left = entry.duration_ms ? Math.max(0, entry.duration_ms - entry.position_ms) + END_SLACK_MS : UNKNOWN_END_MS;
  return now - stamp <= left;
}

/** The rows of the widget, and the rows that the "In progress" list keeps. */
export function splitProgress(entries: ProgressView[], now: number): { playing: ProgressView[]; rest: ProgressView[] } {
  const playing: ProgressView[] = [];
  const rest: ProgressView[] = [];
  for (const entry of entries) (isLive(entry, now) ? playing : rest).push(entry);
  return { playing, rest };
}

/** `M:SS`, or `H:MM:SS` from one hour on. */
export function fmtClock(ms: number | null | undefined): string {
  const total = Math.max(0, Math.round((ms ?? 0) / SECOND_MS));
  const seconds = total % SECONDS_PER_MINUTE;
  const minutes = Math.floor(total / SECONDS_PER_MINUTE) % MINUTES_PER_HOUR;
  const hours = Math.floor(total / (SECONDS_PER_MINUTE * MINUTES_PER_HOUR));
  const tail = String(seconds).padStart(PAD, "0");
  return hours ? `${hours}:${String(minutes).padStart(PAD, "0")}:${tail}` : `${minutes}:${tail}`;
}
