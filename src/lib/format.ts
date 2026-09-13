/** Display helpers for the UI. */

export function fmtDate(value: string | null | undefined): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toISOString().slice(0, 16).replace("T", " ");
}

export function fmtDay(value: string | null | undefined): string {
  return fmtDate(value).slice(0, 10);
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
    .map((word) => word[0]!.toUpperCase())
    .join("");
}
