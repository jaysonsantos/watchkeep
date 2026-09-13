import type { CatalogMovie, CatalogShow } from "../catalog/catalog.ts";
import type { HistoryEntry, MovieView, ProgressView, Stats, WatchFilter, WatchlistItem } from "../queries.ts";
import type { MergedEpisode, ShowListItem } from "../views.ts";

/**
 * The Watchkeep mark: a keeper's logbook with a ribbon bookmark and a check.
 * Fixed colours, so the logo looks the same in both themes and as a favicon.
 */
export const LOGO_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="Watchkeep">
  <defs>
    <linearGradient id="wk-g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#1f9a6a"/>
      <stop offset="1" stop-color="#0f5f44"/>
    </linearGradient>
  </defs>
  <rect width="64" height="64" rx="15" fill="url(#wk-g)"/>
  <!-- logbook: spine, cover, ribbon bookmark, check -->
  <rect x="15" y="13" width="7" height="38" rx="2.5" fill="#ffffff" fill-opacity=".55"/>
  <rect x="20" y="13" width="29" height="38" rx="4" fill="#ffffff"/>
  <path d="M38 13v15l3.5-3 3.5 3V13z" fill="#0f5f44"/>
  <path d="M27 24h7" stroke="#0f5f44" stroke-opacity=".35" stroke-width="2.5" stroke-linecap="round"/>
  <path d="M26.5 38.5l5 5 9-10.5" fill="none" stroke="#0f5f44" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>
</svg>`;

const LOGO_DATA_URI = `data:image/svg+xml,${encodeURIComponent(LOGO_SVG)}`;

const CSS = `
:root {
  color-scheme: light dark;
  --bg: #f4f5f2; --bg-2: #eceee9; --card: #ffffff; --line: #dfe2dc; --fg: #17201b; --muted: #66706a;
  --accent: #1f9a6a; --accent-2: #0f5f44; --accent-fg: #ffffff; --accent-soft: #dff3ea; --warn: #c7691f; --warn-soft: #fbe9d9;
  --radius: 14px; --shadow: 0 1px 2px rgba(20, 30, 25, .06), 0 8px 24px -16px rgba(20, 30, 25, .25);
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #111412; --bg-2: #171a18; --card: #1b1f1d; --line: #2b302d; --fg: #e9ede9; --muted: #98a19b;
    --accent: #3cc48c; --accent-2: #8fe0bb; --accent-fg: #07130d; --accent-soft: #163427; --warn: #f0a15a; --warn-soft: #3a2a17;
    --shadow: 0 1px 2px rgba(0, 0, 0, .4), 0 12px 32px -20px rgba(0, 0, 0, .8);
  }
}
* { box-sizing: border-box; }
html { scroll-behavior: smooth; }
body { margin: 0; background: var(--bg); color: var(--fg); font: 15px/1.5 ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; -webkit-font-smoothing: antialiased; }
a { color: inherit; }
img { max-width: 100%; }

/* Header */
header { position: sticky; top: 0; z-index: 10; display: flex; flex-wrap: wrap; align-items: center; gap: .6rem 1.2rem; padding: .6rem 1.25rem; background: color-mix(in srgb, var(--card) 88%, transparent); backdrop-filter: blur(10px); border-bottom: 1px solid var(--line); }
.brand { display: inline-flex; align-items: center; gap: .55rem; font-weight: 800; font-size: 1.15rem; letter-spacing: -.01em; text-decoration: none; }
.brand img { width: 30px; height: 30px; border-radius: 8px; }
header nav { display: flex; flex-wrap: wrap; gap: .15rem; }
header nav a { padding: .35rem .7rem; border-radius: 999px; color: var(--muted); text-decoration: none; font-weight: 500; font-size: .92rem; }
header nav a:hover { color: var(--fg); background: var(--bg-2); }
header nav a.active { color: var(--accent-fg); background: var(--accent); }
header .quick { margin-left: auto; }
header .quick input { width: 200px; max-width: 45vw; font: inherit; font-size: .9rem; padding: .4rem .8rem; border-radius: 999px; border: 1px solid var(--line); background: var(--bg); color: var(--fg); }
header .quick input:focus { outline: 2px solid var(--accent); outline-offset: 1px; border-color: transparent; }

main { max-width: 1180px; margin: 0 auto; padding: 1.5rem 1.25rem 3rem; }
.page-head { display: flex; flex-wrap: wrap; align-items: end; justify-content: space-between; gap: .5rem 1rem; margin-bottom: 1.1rem; }
h1 { font-size: 1.75rem; line-height: 1.15; letter-spacing: -.02em; margin: 0; }
h2 { font-size: 1.05rem; letter-spacing: -.01em; margin: 1.8rem 0 .7rem; display: flex; align-items: center; gap: .5rem; }
h2 .count { font-weight: 500; color: var(--muted); font-size: .85rem; }
.sub { color: var(--muted); margin: .15rem 0 0; }
.muted { color: var(--muted); }
code { font-size: .85em; background: var(--bg-2); padding: .05rem .35rem; border-radius: 5px; }

/* Stat tiles */
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr)); gap: .8rem; }
.tile { position: relative; overflow: hidden; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); padding: 1rem 1.1rem .9rem; box-shadow: var(--shadow); }
.tile .n { font-size: 1.9rem; font-weight: 800; letter-spacing: -.03em; line-height: 1.1; }
.tile .n small { font-size: .95rem; font-weight: 500; color: var(--muted); letter-spacing: 0; }
.tile .l { color: var(--muted); font-size: .85rem; margin-top: .15rem; }
.tile .bar { margin-top: .7rem; }

/* Progress bars, badges, buttons */
.bar { height: 6px; background: var(--bg-2); border-radius: 3px; overflow: hidden; min-width: 80px; }
.bar > span { display: block; height: 100%; background: linear-gradient(90deg, var(--accent), var(--accent-2)); border-radius: 3px; }
.badge { display: inline-flex; align-items: center; gap: .3rem; padding: .12rem .55rem; border-radius: 999px; font-size: .76rem; font-weight: 600; border: 1px solid var(--line); color: var(--muted); background: var(--card); white-space: nowrap; }
.badge.ok { background: var(--accent); color: var(--accent-fg); border-color: transparent; }
.badge.part { background: var(--warn-soft); color: var(--warn); border-color: transparent; }
.badge.soft { background: var(--accent-soft); color: var(--accent-2); border-color: transparent; }
form.inline { display: inline; }
button, .btn { font: inherit; font-size: .84rem; font-weight: 600; padding: .34rem .75rem; border-radius: 8px; border: 1px solid var(--line); background: var(--card); color: var(--fg); cursor: pointer; text-decoration: none; display: inline-flex; align-items: center; gap: .35rem; line-height: 1.3; transition: background .12s, border-color .12s, transform .06s; }
button:hover, .btn:hover { border-color: var(--accent); background: var(--bg-2); }
button:active { transform: translateY(1px); }
button.primary { background: var(--accent); color: var(--accent-fg); border-color: transparent; }
button.primary:hover { background: var(--accent-2); }
button.small { font-size: .78rem; padding: .22rem .55rem; }
.actions { display: flex; flex-wrap: wrap; gap: .4rem; align-items: center; }

/* Toolbar */
.toolbar { display: flex; flex-wrap: wrap; gap: .5rem; align-items: center; margin-bottom: 1.1rem; }
.seg { display: inline-flex; background: var(--bg-2); border: 1px solid var(--line); border-radius: 999px; padding: 3px; }
.seg a { padding: .3rem .85rem; border-radius: 999px; text-decoration: none; color: var(--muted); font-size: .88rem; font-weight: 500; }
.seg a.active { background: var(--card); color: var(--fg); box-shadow: var(--shadow); }
.toolbar input { font: inherit; padding: .4rem .8rem; border-radius: 999px; border: 1px solid var(--line); background: var(--card); color: var(--fg); min-width: 220px; }

/* Tables */
.table-wrap { overflow-x: auto; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: var(--shadow); }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: .6rem .85rem; border-bottom: 1px solid var(--line); vertical-align: middle; }
th { font-size: .74rem; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); font-weight: 600; background: var(--bg-2); }
tbody tr:hover { background: color-mix(in srgb, var(--bg-2) 60%, transparent); }
tr:last-child td { border-bottom: 0; }
.title-cell { display: flex; align-items: center; gap: .7rem; min-width: 220px; }
.thumb { flex: none; width: 36px; height: 54px; object-fit: cover; border-radius: 6px; background: var(--bg-2); }
.thumb.empty { display: inline-block; }
.future { opacity: .55; }

/* Poster grid */
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 1rem; }
.card { background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); display: flex; flex-direction: column; transition: transform .12s, box-shadow .12s; }
.card:hover { transform: translateY(-2px); }
.card .poster { position: relative; aspect-ratio: 2 / 3; background: linear-gradient(160deg, var(--bg-2), var(--line)); display: flex; align-items: center; justify-content: center; color: var(--muted); font-weight: 700; font-size: 1.6rem; }
.card .poster img { width: 100%; height: 100%; object-fit: cover; display: block; }
.card .poster .badge { position: absolute; left: .5rem; top: .5rem; box-shadow: var(--shadow); }
.card .poster .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 5px; border-radius: 0; background: rgba(0,0,0,.35); min-width: 0; }
.card .body { padding: .65rem .75rem .75rem; display: flex; flex-direction: column; gap: .35rem; flex: 1; }
.card .t { font-weight: 600; line-height: 1.25; text-decoration: none; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.card .m { color: var(--muted); font-size: .82rem; }
.card .actions { margin-top: auto; padding-top: .3rem; }

/* In progress cards */
.progress-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: .8rem; }
.progress-card { display: flex; gap: .8rem; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); padding: .7rem; box-shadow: var(--shadow); }
.progress-card .thumb { width: 56px; height: 84px; border-radius: 8px; }
.progress-card .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: .3rem; }
.progress-card .t { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.progress-card .row { display: flex; justify-content: space-between; align-items: center; gap: .5rem; font-size: .82rem; color: var(--muted); }

/* Show hero */
.hero { display: flex; gap: 1.4rem; align-items: flex-start; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); padding: 1.25rem; box-shadow: var(--shadow); margin-bottom: 1.4rem; }
.hero .art { flex: none; width: 150px; aspect-ratio: 2 / 3; border-radius: 10px; overflow: hidden; background: linear-gradient(160deg, var(--bg-2), var(--line)); }
.hero .art img { width: 100%; height: 100%; object-fit: cover; display: block; }
.hero .info { flex: 1; min-width: 0; }
.hero .chips { display: flex; flex-wrap: wrap; gap: .4rem; margin: .5rem 0 .8rem; }
.hero .summary { color: var(--muted); max-width: 70ch; margin: 0 0 1rem; }
.hero .bar { max-width: 360px; margin: .4rem 0 .9rem; }
.season { margin-top: 1.2rem; }
.season summary { cursor: pointer; list-style: none; display: flex; align-items: center; gap: .6rem; font-weight: 700; padding: .4rem 0; }
.season summary::-webkit-details-marker { display: none; }
.season summary::before { content: "▸"; color: var(--muted); font-size: .8rem; transition: transform .12s; }
.season[open] summary::before { transform: rotate(90deg); }
.season summary .bar { width: 120px; }
.ep-code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .8rem; color: var(--muted); margin-right: .5rem; }

/* Forms and misc */
.empty { padding: 2.2rem 1rem; text-align: center; color: var(--muted); background: var(--card); border: 1px dashed var(--line); border-radius: var(--radius); }
.notice { padding: .7rem .9rem; border-radius: 10px; background: var(--warn-soft); color: var(--warn); font-size: .9rem; margin: .8rem 0; }
.add-form { display: grid; gap: .6rem; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); align-items: end; max-width: 780px; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); padding: 1rem; box-shadow: var(--shadow); }
.add-form label { display: grid; gap: .25rem; font-size: .82rem; color: var(--muted); font-weight: 500; }
.add-form input, .add-form select { font: inherit; padding: .4rem .55rem; border-radius: 8px; border: 1px solid var(--line); background: var(--bg); color: var(--fg); }
.search { display: flex; gap: .5rem; max-width: 560px; margin-bottom: 1.2rem; }
.search input { flex: 1; font: inherit; font-size: 1rem; padding: .55rem .9rem; border-radius: 10px; border: 1px solid var(--line); background: var(--card); color: var(--fg); }
.search input:focus, .toolbar input:focus, .add-form input:focus, .add-form select:focus { outline: 2px solid var(--accent); outline-offset: 1px; border-color: transparent; }
.pager { display: flex; gap: 1rem; align-items: center; margin-top: 1rem; }
.list { display: flex; flex-direction: column; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: var(--shadow); overflow: hidden; }
.list .row { display: flex; align-items: center; gap: .8rem; padding: .55rem .85rem; border-bottom: 1px solid var(--line); }
.list .row:last-child { border-bottom: 0; }
.list .row .when { color: var(--muted); font-size: .82rem; min-width: 8.5rem; }
.list .row .what { flex: 1; min-width: 0; }
.list .row .src { color: var(--muted); font-size: .8rem; }
footer { text-align: center; color: var(--muted); font-size: .8rem; padding: 2rem 1rem 0; }
@media (max-width: 720px) {
  main { padding: 1rem .9rem 2.5rem; }
  .hero { flex-direction: column; }
  .hero .art { width: 120px; }
  th:nth-child(n+4), td:nth-child(n+4) { display: none; }
  .list .row .when { min-width: 0; }
  header .quick { display: none; }
}
`;

export function escapeHtml(value: unknown): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Trusted HTML. `html` templates return it, so nested templates are not escaped twice. */
export class Raw {
  constructor(readonly html: string) {}
  toString(): string {
    return this.html;
  }
}
export const raw = (value: string | Raw): Raw => (value instanceof Raw ? value : new Raw(value));

/** Tagged template that escapes interpolated values. Wrap trusted HTML with `raw()`. */
export function html(strings: TemplateStringsArray, ...values: unknown[]): Raw {
  let out = "";
  const render = (value: unknown): string => {
    if (value instanceof Raw) return value.html;
    if (Array.isArray(value)) return value.map(render).join("");
    if (value === null || value === undefined || value === false) return "";
    return escapeHtml(value);
  };
  strings.forEach((part, index) => {
    out += part;
    if (index < values.length) out += render(values[index]);
  });
  return new Raw(out);
}

function layout(title: string, active: string, body: Raw): string {
  const links: Array<[string, string]> = [
    ["/", "Dashboard"],
    ["/movies", "Movies"],
    ["/shows", "Shows"],
    ["/watchlist", "Watchlist"],
    ["/history", "History"],
    ["/add", "Add"],
    ["/webhooks", "Webhooks"],
  ];
  return html`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="theme-color" content="#1f9a6a">
<title>${title} · Watchkeep</title>
<link rel="icon" href="${LOGO_DATA_URI}" type="image/svg+xml">
<style>${raw(CSS)}</style>
</head>
<body>
<header>
  <a class="brand" href="/"><img src="/logo.svg" alt="" width="30" height="30">Watchkeep</a>
  <nav>${links.map(([href, label]) => html`<a href="${href}" class="${href === active ? "active" : ""}">${label}</a>`)}</nav>
  <form class="quick" method="get" action="/add"><input type="search" name="q" placeholder="Search catalog…" aria-label="Search the catalog"></form>
</header>
<main>${body}</main>
<footer>Watchkeep · self-hosted watch history</footer>
</body>
</html>`.html;
}

function pageHead(title: string | Raw, sub?: string | Raw, right?: Raw): Raw {
  return html`<div class="page-head"><div><h1>${title}</h1>${sub ? html`<p class="sub">${sub}</p>` : ""}</div>${right ?? ""}</div>`;
}

function fmtDate(value: string | null | undefined): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toISOString().slice(0, 16).replace("T", " ");
}

function fmtDuration(ms: number | null | undefined): string {
  if (!ms) return "";
  const minutes = Math.round(ms / 60_000);
  return minutes >= 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`;
}

function code(season: number | null, number: number | null): string {
  if (season === null || number === null) return "";
  return `S${String(season).padStart(2, "0")}E${String(number).padStart(2, "0")}`;
}

function percent(position: number, total: number | null): number {
  return total ? Math.min(100, Math.round((position / total) * 100)) : 0;
}

function actionForm(action: string, id: number, back: string, label: string, primary = false, extra: string | Raw = "", small = false): Raw {
  return raw(html`<form class="inline" method="post" action="/actions">
    <input type="hidden" name="action" value="${action}">
    <input type="hidden" name="id" value="${id}">
    <input type="hidden" name="back" value="${back}">
    ${raw(extra)}
    <button type="submit" class="${primary ? "primary" : ""} ${small ? "small" : ""}">${label}</button>
  </form>`);
}

function toolbar(base: string, filter: WatchFilter, search: string): Raw {
  const link = (value: WatchFilter, label: string) =>
    html`<a href="${base}?status=${value}${search ? `&q=${encodeURIComponent(search)}` : ""}" class="${filter === value ? "active" : ""}">${label}</a>`;
  return raw(html`<div class="toolbar">
    <div class="seg">${raw(link("all", "All"))}${raw(link("watched", "Watched"))}${raw(link("unwatched", "Unwatched"))}</div>
    <form method="get" action="${base}" class="inline">
      <input type="hidden" name="status" value="${filter}">
      <input type="search" name="q" placeholder="Filter by title" value="${search}">
    </form>
  </div>`);
}

function progressBar(positionMs: number, durationMs: number | null): Raw {
  const value = percent(positionMs, durationMs);
  return raw(html`<div class="bar" title="${value}%"><span style="width:${value}%"></span></div>`);
}

function thumb(images: string, path: string | null | undefined): Raw {
  if (!path) return raw('<span class="thumb empty"></span>');
  return raw(html`<img class="thumb" src="${images}w92${path}" srcset="${images}w185${path} 2x" alt="" loading="lazy">`);
}

function initials(title: string): string {
  return title
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word[0]!.toUpperCase())
    .join("");
}

function cardPoster(images: string, path: string | null | undefined, title: string, badge: Raw | "", bar?: Raw): Raw {
  const image = path
    ? html`<img src="${images}w185${path}" srcset="${images}w342${path} 2x" alt="" loading="lazy">`
    : html`<span>${initials(title)}</span>`;
  return raw(html`<div class="poster">${image}${badge}${bar ?? ""}</div>`);
}

function progressTitle(entry: ProgressView): Raw {
  if (entry.target_kind === "movie") return raw(html`${entry.title}`);
  return raw(html`<a href="/shows/${entry.show_id}">${entry.show_title}</a> <span class="ep-code">${code(entry.season, entry.number)}</span><span class="muted">${entry.title}</span>`);
}

function historyTitle(entry: HistoryEntry): Raw {
  if (entry.target_kind === "movie") return raw(html`${entry.title}${entry.year ? html` <span class="muted">(${entry.year})</span>` : ""}`);
  return raw(html`<a href="/shows/${entry.show_id}">${entry.show_title}</a> <span class="ep-code">${code(entry.season, entry.number)}</span><span class="muted">${entry.title}</span>`);
}

export function renderDashboard(input: {
  images: string;
  stats: Stats;
  inProgress: ProgressView[];
  recent: HistoryEntry[];
  syncConfigured: boolean;
  catalogConfigured: boolean;
}): string {
  const { stats } = input;
  const tile = (value: number, total: number | null, label: string) =>
    html`<div class="tile"><div class="n">${value}${total !== null ? html`<small> / ${total}</small>` : ""}</div><div class="l">${label}</div>${
      total !== null ? progressBar(value, total || null) : ""
    }</div>`;
  const body = html`${pageHead("Dashboard", "What you watched, what is playing, and what is left.", input.syncConfigured ? actionForm("sync", 0, "/", "Sync Plex library") : undefined)}
<div class="tiles">
  ${tile(stats.movies_watched, stats.movies, "Movies watched")}
  ${tile(stats.episodes_watched, stats.episodes, "Episodes watched")}
  ${tile(stats.shows, null, "Shows tracked")}
  ${tile(stats.plays, null, "Plays recorded")}
</div>
${input.catalogConfigured ? "" : raw('<div class="notice">No TMDB catalog is configured. Episode lists show only what Plex reported. Set <code>WATCHKEEP_CATALOG_DATABASE_URL</code> to enable full episode lists.</div>')}
<h2>In progress <span class="count">${input.inProgress.length}</span></h2>
${
  input.inProgress.length === 0
    ? raw('<div class="empty">Nothing in progress. Play something on Plex.</div>')
    : raw(html`<div class="progress-grid">
${input.inProgress.map(
  (entry) => html`<div class="progress-card">
  ${thumb(input.images, entry.poster_path)}
  <div class="info">
    <div class="t">${progressTitle(entry)}</div>
    <div class="row"><span class="badge ${entry.state === "playing" ? "ok" : ""}">${entry.state}</span><span>${percent(entry.position_ms, entry.duration_ms)}% · ${fmtDuration(entry.position_ms)}${entry.duration_ms ? ` / ${fmtDuration(entry.duration_ms)}` : ""}</span></div>
    ${progressBar(entry.position_ms, entry.duration_ms)}
    <div class="row"><span>${entry.player ?? ""}${entry.player ? " · " : ""}${fmtDate(entry.updated_at)}</span>${actionForm("clear-progress", entry.target_id, "/", "Clear", false, html`<input type="hidden" name="kind" value="${entry.target_kind}">`, true)}</div>
  </div>
</div>`,
)}
</div>`)
}
<h2>Recent history</h2>
${
  input.recent.length === 0
    ? raw('<div class="empty">No plays recorded yet.</div>')
    : raw(html`<div class="list">
${input.recent.map(
  (entry) => html`<div class="row">${thumb(input.images, entry.poster_path)}<div class="what">${historyTitle(entry)}</div><span class="src">${entry.source}${entry.player ? ` · ${entry.player}` : ""}</span><span class="when">${fmtDate(entry.watched_at)}</span></div>`,
)}
</div>
<p class="muted"><a href="/history">Full history →</a></p>`)
}`;
  return layout("Dashboard", "/", body);
}

export function renderMovies(input: { images: string; movies: MovieView[]; filter: WatchFilter; search: string }): string {
  const back = `/movies?status=${input.filter}${input.search ? `&q=${encodeURIComponent(input.search)}` : ""}`;
  const body = html`${pageHead("Movies", html`${input.movies.length} ${input.filter === "all" ? "movies" : `${input.filter} movies`}`)}
${toolbar("/movies", input.filter, input.search)}
${
  input.movies.length === 0
    ? raw('<div class="empty">No movies match. Add one, run a Plex sync, or play a movie.</div>')
    : raw(html`<div class="grid">
${input.movies.map((movie) => {
  const badge = movie.play_count > 0 ? raw(html`<span class="badge ok">watched${movie.play_count > 1 ? ` ×${movie.play_count}` : ""}</span>`) : "";
  return html`<div class="card">
  ${cardPoster(input.images, movie.poster_path, movie.title, badge)}
  <div class="body">
    <span class="t">${movie.title}</span>
    <span class="m">${movie.year ?? ""}${movie.year && movie.duration_ms ? " · " : ""}${fmtDuration(movie.duration_ms)}${movie.rating !== null ? ` · ★ ${movie.rating}` : ""}</span>
    ${movie.last_watched_at ? html`<span class="m">Watched ${fmtDate(movie.last_watched_at).slice(0, 10)}</span>` : ""}
    <div class="actions">${
      movie.play_count > 0 ? actionForm("unwatch-movie", movie.id, back, "Unwatch", false, "", true) : actionForm("watch-movie", movie.id, back, "Watched", true, "", true)
    }${actionForm("watchlist-add-movie", movie.id, back, "+ List", false, "", true)}</div>
  </div>
</div>`;
})}
</div>`)
}`;
  return layout("Movies", "/movies", body);
}

export function renderShows(input: { images: string; shows: ShowListItem[]; filter: WatchFilter; search: string }): string {
  const body = html`${pageHead("Shows", html`${input.shows.length} ${input.filter === "all" ? "shows" : `${input.filter} shows`}`)}
${toolbar("/shows", input.filter, input.search)}
${
  input.shows.length === 0
    ? raw('<div class="empty">No shows match. Add one, run a Plex sync, or play an episode.</div>')
    : raw(html`<div class="grid">
${input.shows.map((show) => {
  const complete = show.total_episodes > 0 && show.watched_count >= show.total_episodes;
  const left = Math.max(0, show.total_episodes - show.watched_count);
  const badge = complete
    ? raw('<span class="badge ok">complete</span>')
    : show.hidden_at
      ? raw('<span class="badge">hidden</span>')
      : left > 0 && show.watched_count > 0
        ? raw(html`<span class="badge part">${left} left</span>`)
        : "";
  return html`<a class="card" href="/shows/${show.id}" style="text-decoration:none">
  ${cardPoster(input.images, show.poster_path, show.title, badge, progressBar(show.watched_count, show.total_episodes || null))}
  <div class="body">
    <span class="t">${show.title}</span>
    <span class="m">${show.year ?? ""}${show.year ? " · " : ""}${show.watched_count} / ${show.total_episodes} episodes</span>
    ${show.last_watched_at ? html`<span class="m">Last ${fmtDate(show.last_watched_at).slice(0, 10)}</span>` : ""}
  </div>
</a>`;
})}
</div>`)
}`;
  return layout("Shows", "/shows", body);
}

function episodeAction(showId: number, episode: MergedEpisode, back: string): Raw {
  if (episode.id !== null) {
    return episode.play_count > 0
      ? actionForm("unwatch-episode", episode.id, back, "Unwatch", false, "", true)
      : actionForm("watch-episode", episode.id, back, "Watched", true, "", true);
  }
  const extra = html`<input type="hidden" name="season" value="${episode.season}"><input type="hidden" name="number" value="${episode.number}">`;
  return actionForm("watch-episode-number", showId, back, "Watched", true, extra, true);
}

export function renderShow(input: { images: string; show: ShowListItem; episodes: MergedEpisode[]; onWatchlist: boolean }): string {
  const { show } = input;
  const back = `/shows/${show.id}`;
  const today = new Date().toISOString().slice(0, 10);
  const seasons = new Map<number, MergedEpisode[]>();
  for (const episode of input.episodes) {
    const list = seasons.get(episode.season) ?? [];
    list.push(episode);
    seasons.set(episode.season, list);
  }
  const complete = show.total_episodes > 0 && show.watched_count >= show.total_episodes;
  const nextUp = input.episodes.find((episode) => episode.season > 0 && episode.play_count === 0 && (!episode.aired_at || episode.aired_at <= today));
  const body = html`<div class="hero">
  <div class="art">${show.poster_path ? html`<img src="${input.images}w185${show.poster_path}" srcset="${input.images}w342${show.poster_path} 2x" alt="">` : ""}</div>
  <div class="info">
    <p class="muted" style="margin:0"><a href="/shows">← Shows</a></p>
    <h1>${show.title}${show.year ? html` <span class="muted">(${show.year})</span>` : ""}</h1>
    <div class="chips">
      <span class="badge ${complete ? "ok" : show.watched_count > 0 ? "part" : ""}">${show.watched_count} / ${show.total_episodes} watched</span>
      ${show.rating !== null ? html`<span class="badge soft">★ ${show.rating}/10</span>` : ""}
      ${show.hidden_at ? raw('<span class="badge">hidden</span>') : ""}
      ${input.onWatchlist ? raw('<span class="badge soft">on watchlist</span>') : ""}
      ${nextUp ? html`<span class="badge">Next: ${code(nextUp.season, nextUp.number)}${nextUp.title ? ` ${nextUp.title}` : ""}</span>` : ""}
    </div>
    ${progressBar(show.watched_count, show.total_episodes || null)}
    ${show.summary ? html`<p class="summary">${show.summary}</p>` : ""}
    <div class="actions">
      ${actionForm("watch-show", show.id, back, "Mark all watched", true)}
      ${actionForm("unwatch-show", show.id, back, "Mark all unwatched")}
      ${input.onWatchlist ? actionForm("watchlist-remove-show", show.id, back, "Remove from watchlist") : actionForm("watchlist-add-show", show.id, back, "Add to watchlist")}
      ${show.hidden_at ? actionForm("unhide-show", show.id, back, "Unhide") : actionForm("hide-show", show.id, back, "Hide from unwatched")}
    </div>
  </div>
</div>
${
  input.episodes.length === 0
    ? raw('<div class="empty">No episodes known yet. Configure the TMDB catalog or run a Plex sync to import the episode list.</div>')
    : [...seasons.entries()].map(([season, episodes]) => {
        const watched = episodes.filter((episode) => episode.play_count > 0).length;
        const open = season > 0 && watched < episodes.length;
        return html`<details class="season" ${open ? "open" : ""}><summary>${season === 0 ? "Specials" : `Season ${season}`} <span class="muted" style="font-weight:500">${watched} / ${episodes.length}</span>${progressBar(watched, episodes.length)}</summary>
<div class="table-wrap"><table><thead><tr><th>Episode</th><th>Status</th><th></th><th>Last watched</th><th>Aired</th></tr></thead><tbody>
${episodes.map(
  (episode) => html`<tr class="${episode.aired_at && episode.aired_at > today ? "future" : ""}">
  <td><span class="ep-code">${code(episode.season, episode.number)}</span>${episode.title ?? ""}</td>
  <td>${
    episode.play_count > 0
      ? raw(html`<span class="badge ok">watched${episode.play_count > 1 ? ` ×${episode.play_count}` : ""}</span>`)
      : episode.position_ms
        ? raw(html`<span class="badge part">${episode.duration_ms ? `${percent(episode.position_ms, episode.duration_ms)}%` : "in progress"}</span>`)
        : episode.aired_at && episode.aired_at > today
          ? raw('<span class="badge">upcoming</span>')
          : raw('<span class="badge">unwatched</span>')
  }</td>
  <td>${episodeAction(show.id, episode, back)}</td>
  <td class="muted">${fmtDate(episode.last_watched_at)}</td>
  <td class="muted">${episode.aired_at ?? ""}</td>
</tr>`,
)}
</tbody></table></div></details>`;
      })
}`;
  return layout(show.title, "/shows", body);
}

export function renderHistory(input: { images: string; entries: HistoryEntry[]; page: number; pageSize: number; total: number }): string {
  const pages = Math.max(1, Math.ceil(input.total / input.pageSize));
  const back = `/history?page=${input.page}`;
  const body = html`${pageHead("History", html`${input.total} plays`)}
${
  input.entries.length === 0
    ? raw('<div class="empty">No plays recorded yet.</div>')
    : raw(html`<div class="table-wrap"><table><thead><tr><th>Watched</th><th>Title</th><th></th><th>Source</th><th>Account</th><th>Player</th></tr></thead><tbody>
${input.entries.map(
  (entry) => html`<tr>
  <td class="muted" style="white-space:nowrap">${fmtDate(entry.watched_at)}</td>
  <td><span class="title-cell">${thumb(input.images, entry.poster_path)}<span>${historyTitle(entry)}</span></span></td>
  <td>${actionForm("remove-play", entry.id, back, "Remove", false, "", true)}</td>
  <td class="muted">${entry.source}</td>
  <td class="muted">${entry.account ?? ""}</td>
  <td class="muted">${entry.player ?? ""}</td>
</tr>`,
)}
</tbody></table></div>`)
}
<div class="pager">
  ${input.page > 1 ? raw(html`<a class="btn" href="/history?page=${input.page - 1}">← Newer</a>`) : ""}
  <span class="muted">Page ${input.page} of ${pages}</span>
  ${input.page < pages ? raw(html`<a class="btn" href="/history?page=${input.page + 1}">Older →</a>`) : ""}
</div>`;
  return layout("History", "/history", body);
}

export function renderWatchlist(input: { images: string; items: WatchlistItem[] }): string {
  const back = "/watchlist";
  const body = html`${pageHead("Watchlist", html`${input.items.length} items to watch`, raw(html`<a class="btn" href="/add">+ Add</a>`))}
${
  input.items.length === 0
    ? raw('<div class="empty">The watchlist is empty. Add movies and shows from the Add page.</div>')
    : raw(html`<div class="grid">
${input.items.map((item) => {
  const badge = item.watched_count > 0 ? raw(html`<span class="badge part">${item.kind === "movie" ? "watched" : `${item.watched_count} seen`}</span>`) : "";
  const inner = html`${cardPoster(input.images, item.poster_path, item.title, badge)}
  <div class="body">
    <span class="t">${item.title}</span>
    <span class="m">${item.kind}${item.year ? ` · ${item.year}` : ""} · added ${fmtDate(item.listed_at).slice(0, 10)}</span>
    <div class="actions">${item.kind === "movie" ? actionForm("watch-movie", item.id, back, "Watched", true, "", true) : ""}${actionForm(item.kind === "movie" ? "watchlist-remove-movie" : "watchlist-remove-show", item.id, back, "Remove", false, "", true)}</div>
  </div>`;
  return item.kind === "show"
    ? html`<div class="card"><a href="/shows/${item.id}" style="text-decoration:none">${cardPoster(input.images, item.poster_path, item.title, badge)}</a><div class="body"><a class="t" href="/shows/${item.id}">${item.title}</a><span class="m">show${item.year ? ` · ${item.year}` : ""} · added ${fmtDate(item.listed_at).slice(0, 10)}</span><div class="actions">${actionForm("watchlist-remove-show", item.id, back, "Remove", false, "", true)}</div></div></div>`
    : html`<div class="card">${inner}</div>`;
})}
</div>`)
}`;
  return layout("Watchlist", "/watchlist", body);
}

type AddResult<T> = T & { localId: number | null };

function addForms(kind: "movie" | "show", tmdbId: number, query: string): Raw {
  const hidden = html`<input type="hidden" name="kind" value="${kind}"><input type="hidden" name="tmdb_id" value="${tmdbId}"><input type="hidden" name="q" value="${query}">`;
  return raw(html`<div class="actions"><form class="inline" method="post" action="/add">${hidden}<input type="hidden" name="watchlist" value="1"><button type="submit" class="primary small">+ Watchlist</button></form>
  <form class="inline" method="post" action="/add">${hidden}<button type="submit" class="small">Add to library</button></form></div>`);
}

export function renderAdd(input: {
  images: string;
  query: string;
  catalogConfigured: boolean;
  movies: AddResult<CatalogMovie>[];
  shows: AddResult<CatalogShow>[];
  error?: string;
}): string {
  const results = (kind: "movie" | "show", items: Array<AddResult<CatalogMovie> | AddResult<CatalogShow>>) =>
    items.length === 0
      ? ""
      : html`<h2>${kind === "movie" ? "Movies" : "Shows"} <span class="count">${items.length}</span></h2>
<div class="table-wrap"><table><thead><tr><th>Title</th><th>Year</th><th></th></tr></thead><tbody>
${items.map(
  (item) => html`<tr>
  <td><span class="title-cell">${thumb(input.images, item.posterPath)}<span>${
    item.localId !== null && kind === "show" ? html`<a href="/shows/${item.localId}">${item.title}</a>` : item.title
  }${item.localId !== null ? raw(' <span class="badge soft">in library</span>') : ""}</span></span></td>
  <td class="muted">${item.year ?? ""}</td>
  <td>${addForms(kind, item.tmdbId, input.query)}</td>
</tr>`,
)}
</tbody></table></div>`;
  const body = html`${pageHead("Add a movie or show", "Search the catalog, or add by hand.")}
${
  input.catalogConfigured
    ? html`<form class="search" method="get" action="/add"><input type="search" name="q" placeholder="Search the catalog by title" value="${input.query}" autofocus><button type="submit" class="primary">Search</button></form>
${input.query.length >= 2 && input.movies.length === 0 && input.shows.length === 0 ? raw('<div class="empty">No catalog match. Use the manual form below.</div>') : ""}
${results("movie", input.movies)}${results("show", input.shows)}`
    : raw('<div class="notice">No TMDB catalog is configured, so there is no search. Use the manual form.</div>')
}
<h2>Add by hand</h2>
${input.error ? raw('<div class="notice">A title is required.</div>') : ""}
<form class="add-form" method="post" action="/add">
  <label>Type <select name="kind"><option value="movie">Movie</option><option value="show">Show</option></select></label>
  <label>Title <input name="title" required placeholder="Title"></label>
  <label>Year <input name="year" type="number" min="1870" max="2100" placeholder="2024"></label>
  <label>TMDB id <input name="tmdb_id" type="number" min="1" placeholder="optional"></label>
  <label><span><input type="checkbox" name="watchlist" value="1" checked> Add to watchlist</span></label>
  <button type="submit" class="primary">Add</button>
</form>`;
  return layout("Add", "/add", body);
}

export function renderWebhooks(input: {
  events: Array<{
    id: number;
    received_at: string;
    event: string;
    account: string | null;
    player: string | null;
    media_type: string | null;
    title: string | null;
    outcome: string;
  }>;
}): string {
  const body = html`${pageHead("Webhook log", "The last 100 calls that Plex sent. Use this page to check that Plex reaches Watchkeep.")}
${
  input.events.length === 0
    ? raw('<div class="empty">No webhook received yet. Add the webhook URL in Plex settings.</div>')
    : raw(html`<div class="table-wrap"><table><thead><tr><th>Received</th><th>Event</th><th>Outcome</th><th>Title</th><th>Account</th><th>Player</th></tr></thead><tbody>
${input.events.map(
  (event) => html`<tr>
  <td class="muted" style="white-space:nowrap">${fmtDate(event.received_at)}</td>
  <td><code>${event.event}</code></td>
  <td><span class="badge ${event.outcome === "play" ? "ok" : event.outcome.startsWith("ignored") ? "" : "soft"}">${event.outcome}</span></td>
  <td>${event.title ?? ""}${event.media_type ? html` <span class="muted">(${event.media_type})</span>` : ""}</td>
  <td class="muted">${event.account ?? ""}</td>
  <td class="muted">${event.player ?? ""}</td>
</tr>`,
)}
</tbody></table></div>`)
}`;
  return layout("Webhooks", "/webhooks", body);
}
