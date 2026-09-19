/** Display helpers for the UI. */

/** Two digits, as a date and a time need. */
const PAD = 2;

/** `getMonth` counts from zero. */
const FIRST_MONTH = 1;

/** The length of `YYYY-MM-DD`. */
const DAY_LENGTH = 10;

function pad(value: number): string {
  return String(value).padStart(PAD, "0");
}

/** `YYYY-MM-DD HH:MM` in the time zone of the browser. The API sends UTC. */
function localStamp(date: Date): string {
  const day = `${date.getFullYear()}-${pad(date.getMonth() + FIRST_MONTH)}-${pad(date.getDate())}`;
  return `${day} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function fmtDate(value: string | null | undefined): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return localStamp(date);
}

export function fmtDay(value: string | null | undefined): string {
  return fmtDate(value).slice(0, DAY_LENGTH);
}

/** Today in the time zone of the browser, as `YYYY-MM-DD`, to compare with an air date. */
export function today(): string {
  return localStamp(new Date()).slice(0, DAY_LENGTH);
}

export function fmtDuration(ms: number | null | undefined): string {
  if (!ms) return "";
  const minutes = Math.round(ms / 60_000);
  return minutes >= 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`;
}

export function episodeCode(season: number | null, number: number | null): string {
  if (season === null || number === null) return "";
  return `S${String(season).padStart(2, "0")}E${String(number).padStart(2, "0")}`;
}

export function percent(position: number, total: number | null): number {
  return total ? Math.min(100, Math.round((position / total) * 100)) : 0;
}

/** A share or a score from 0.0 to 1.0 as a whole percent. */
export function fmtShare(value: number): number {
  return Math.round(Math.max(0, Math.min(1, value)) * 100);
}

const LANGUAGE_NAMES = new Intl.DisplayNames(undefined, { type: "language" });

/** An ISO 639-1 code as a language name, or the code when there is no name. */
export function languageName(code: string): string {
  try {
    return LANGUAGE_NAMES.of(code) ?? code;
  } catch {
    return code;
  }
}

export function initials(title: string): string {
  return title
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word.charAt(0).toUpperCase())
    .join("");
}

// region: ratings

/** The bottom of the 0 to 10 scale that Trakt and Plex write. */
export const MIN_USER_RATING = 0;

/** The top of that scale. */
export const MAX_USER_RATING = 10;

/** The picker starts at 1. Zero is a stored Plex value; Clear removes the rating. */
export const MIN_RATING_PICK = 1;

/** The whole numbers the UI offers, from `MIN_RATING_PICK` to `MAX_USER_RATING`. */
export const RATING_PICKS: readonly number[] = Array.from(
  { length: MAX_USER_RATING - MIN_RATING_PICK + 1 },
  (_, index) => MIN_RATING_PICK + index,
);

/** `★ 8/10`, or an empty string when there is no rating. */
export function fmtRating(rating: number | null | undefined): string {
  if (rating === null || rating === undefined) return "";
  return `★ ${rating}/${MAX_USER_RATING}`;
}

// endregion: ratings

// region: statistics

const MS_PER_MINUTE = 60_000;
const MINUTES_PER_HOUR = 60;
const HOURS_PER_DAY = 24;
const MINUTES_PER_DAY = HOURS_PER_DAY * MINUTES_PER_HOUR;

/** Two digits, as an hour of the day needs. */
const HOUR_DIGITS = 2;

/** The digits of the century, which a short year label drops. */
const CENTURY_DIGITS = 2;

/** The `YYYY-MM` periods count their months from one. */
const PERIOD_FIRST_MONTH = 1;

/** Short weekday names, in the order that the API sends the buckets: Sunday first. */
const WEEKDAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

const MONTH_NAMES = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"] as const;

/**
 * A long watch time, for example `12d 7h`. Below one day it reads like
 * `fmtDuration`, so that a small total stays exact.
 */
export function fmtWatchTime(ms: number | null | undefined): string {
  if (!ms) return "0m";
  const minutes = Math.round(ms / MS_PER_MINUTE);
  if (minutes < MINUTES_PER_DAY) return fmtDuration(ms);
  const days = Math.floor(minutes / MINUTES_PER_DAY);
  const hours = Math.floor((minutes % MINUTES_PER_DAY) / MINUTES_PER_HOUR);
  return `${days}d ${hours}h`;
}

/** The whole hours of a watch time, for the title of a tile. */
export function fmtHours(ms: number | null | undefined): string {
  const hours = Math.round((ms ?? 0) / (MS_PER_MINUTE * MINUTES_PER_HOUR));
  return `${hours.toLocaleString()} hours`;
}

/** A `YYYY-MM` period as `Jan 26`. Text that is not a period stays as it is. */
export function fmtMonth(period: string): string {
  const [year, month] = period.split("-");
  const name = MONTH_NAMES[Number(month) - PERIOD_FIRST_MONTH];
  return name && year ? `${name} ${year.slice(CENTURY_DIGITS)}` : period;
}

/** The weekday bucket of the API, where Sunday is zero. */
export function fmtWeekday(bucket: number): string {
  return WEEKDAY_NAMES[bucket] ?? String(bucket);
}

/** The hour bucket of the API, as `00`. */
export function fmtHour(bucket: number): string {
  return String(bucket).padStart(HOUR_DIGITS, "0");
}

/** A count with the thousands separator of the browser. */
export function fmtCount(value: number): string {
  return value.toLocaleString();
}

// endregion: statistics
