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

export function initials(title: string): string {
  return title
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word.charAt(0).toUpperCase())
    .join("");
}
