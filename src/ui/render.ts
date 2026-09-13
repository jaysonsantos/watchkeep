import type { HistoryEntry, MovieView, ProgressView, Stats, WatchFilter, WatchlistItem } from "../queries.ts";
import type { MergedEpisode, ShowListItem } from "../views.ts";

const CSS = `
:root { color-scheme: light dark; --bg: #f6f5f2; --fg: #1d1d1b; --muted: #6b6a66; --card: #ffffff; --line: #e2e0da; --accent: #2f6f4e; --accent-fg: #ffffff; --warn: #b3541e; }
@media (prefers-color-scheme: dark) { :root { --bg: #161615; --fg: #ececea; --muted: #a09f9a; --card: #1f1f1d; --line: #33322f; --accent: #6bbf8e; --accent-fg: #0f1f16; --warn: #e0894d; } }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg); font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; }
header { display: flex; flex-wrap: wrap; align-items: center; gap: 1rem; padding: .8rem 1.25rem; border-bottom: 1px solid var(--line); background: var(--card); }
header .brand { font-weight: 700; font-size: 1.15rem; text-decoration: none; color: var(--fg); }
header nav a { color: var(--muted); text-decoration: none; margin-right: .9rem; }
header nav a.active, header nav a:hover { color: var(--fg); }
main { max-width: 1100px; margin: 0 auto; padding: 1.25rem; }
h1 { font-size: 1.5rem; margin: 0 0 1rem; }
h2 { font-size: 1.1rem; margin: 1.6rem 0 .6rem; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: .75rem; }
.card { background: var(--card); border: 1px solid var(--line); border-radius: 10px; padding: .9rem 1rem; }
.card .n { font-size: 1.6rem; font-weight: 700; }
.card .l { color: var(--muted); font-size: .85rem; }
table { width: 100%; border-collapse: collapse; background: var(--card); border: 1px solid var(--line); border-radius: 10px; overflow: hidden; }
th, td { text-align: left; padding: .55rem .75rem; border-bottom: 1px solid var(--line); vertical-align: middle; }
th { font-size: .8rem; text-transform: uppercase; letter-spacing: .04em; color: var(--muted); }
tr:last-child td { border-bottom: 0; }
.muted { color: var(--muted); }
.badge { display: inline-block; padding: .1rem .5rem; border-radius: 999px; font-size: .78rem; border: 1px solid var(--line); color: var(--muted); }
.badge.ok { background: var(--accent); color: var(--accent-fg); border-color: transparent; }
.badge.part { border-color: var(--warn); color: var(--warn); }
.bar { height: 6px; background: var(--line); border-radius: 3px; overflow: hidden; min-width: 90px; }
.bar > span { display: block; height: 100%; background: var(--accent); }
form.inline { display: inline; }
button { font: inherit; font-size: .82rem; padding: .25rem .6rem; border-radius: 6px; border: 1px solid var(--line); background: var(--card); color: var(--fg); cursor: pointer; }
button:hover { border-color: var(--accent); }
button.primary { background: var(--accent); color: var(--accent-fg); border-color: transparent; }
.toolbar { display: flex; flex-wrap: wrap; gap: .5rem; align-items: center; margin-bottom: 1rem; }
.toolbar a { padding: .25rem .7rem; border-radius: 999px; border: 1px solid var(--line); text-decoration: none; color: var(--muted); }
.toolbar a.active { background: var(--accent); color: var(--accent-fg); border-color: transparent; }
.toolbar input { font: inherit; padding: .3rem .6rem; border-radius: 6px; border: 1px solid var(--line); background: var(--card); color: var(--fg); }
a { color: inherit; }
.empty { padding: 2rem; text-align: center; color: var(--muted); background: var(--card); border: 1px dashed var(--line); border-radius: 10px; }
.season { margin-top: 1rem; }
.poster { width: 34px; height: 51px; object-fit: cover; border-radius: 4px; background: var(--line); vertical-align: middle; margin-right: .6rem; }
.poster.big { width: 92px; height: 138px; float: left; margin: 0 1rem .5rem 0; }
.title-cell { display: flex; align-items: center; gap: .2rem; }
.future { opacity: .6; }
.pager { display: flex; gap: 1rem; margin-top: 1rem; }
code { font-size: .85em; }
@media (max-width: 640px) { th:nth-child(n+4), td:nth-child(n+4) { display: none; } }
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
    ["/webhooks", "Webhooks"],
  ];
  return html`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title} · Watchkeep</title>
<style>${raw(CSS)}</style>
</head>
<body>
<header>
  <a class="brand" href="/">Watchkeep</a>
  <nav>${links.map(([href, label]) => html`<a href="${href}" class="${href === active ? "active" : ""}">${label}</a>`)}</nav>
</header>
<main>${body}</main>
</body>
</html>`.html;
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

function actionForm(action: string, id: number, back: string, label: string, primary = false, extra: string | Raw = ""): Raw {
  return raw(html`<form class="inline" method="post" action="/actions">
    <input type="hidden" name="action" value="${action}">
    <input type="hidden" name="id" value="${id}">
    <input type="hidden" name="back" value="${back}">
    ${raw(extra)}
    <button type="submit" class="${primary ? "primary" : ""}">${label}</button>
  </form>`);
}

function toolbar(base: string, filter: WatchFilter, search: string): Raw {
  const link = (value: WatchFilter, label: string) =>
    html`<a href="${base}?status=${value}${search ? `&q=${encodeURIComponent(search)}` : ""}" class="${filter === value ? "active" : ""}">${label}</a>`;
  return raw(html`<div class="toolbar">
    ${raw(link("all", "All"))}${raw(link("watched", "Watched"))}${raw(link("unwatched", "Unwatched"))}
    <form method="get" action="${base}" class="inline">
      <input type="hidden" name="status" value="${filter}">
      <input type="search" name="q" placeholder="Search title" value="${search}">
    </form>
  </div>`);
}

function progressBar(positionMs: number, durationMs: number | null): Raw {
  const percent = durationMs ? Math.min(100, Math.round((positionMs / durationMs) * 100)) : 0;
  return raw(html`<div class="bar" title="${percent}%"><span style="width:${percent}%"></span></div>`);
}

function poster(images: string, path: string | null | undefined, big = false): Raw {
  if (!path) return raw(big ? "" : '<span class="poster"></span>');
  return raw(html`<img class="poster ${big ? "big" : ""}" src="${images}${big ? "w185" : "w92"}${path}" alt="" loading="lazy">`);
}

function progressTitle(entry: ProgressView): Raw {
  if (entry.target_kind === "movie") return raw(html`${entry.title}`);
  return raw(html`<a href="/shows/${entry.show_id}">${entry.show_title}</a> ${code(entry.season, entry.number)} <span class="muted">${entry.title}</span>`);
}

function historyTitle(entry: HistoryEntry): Raw {
  if (entry.target_kind === "movie") return raw(html`${entry.title}${entry.year ? html` <span class="muted">(${entry.year})</span>` : ""}`);
  return raw(html`<a href="/shows/${entry.show_id}">${entry.show_title}</a> ${code(entry.season, entry.number)} <span class="muted">${entry.title}</span>`);
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
  const body = html`<h1>Dashboard</h1>
<div class="cards">
  <div class="card"><div class="n">${stats.movies_watched}<span class="muted">/${stats.movies}</span></div><div class="l">Movies watched</div></div>
  <div class="card"><div class="n">${stats.episodes_watched}<span class="muted">/${stats.episodes}</span></div><div class="l">Episodes watched</div></div>
  <div class="card"><div class="n">${stats.shows}</div><div class="l">Shows</div></div>
  <div class="card"><div class="n">${stats.plays}</div><div class="l">Plays recorded</div></div>
</div>
${input.syncConfigured ? raw(html`<p>${actionForm("sync", 0, "/", "Sync Plex library")}</p>`) : ""}
${input.catalogConfigured ? "" : raw('<p class="muted">No TMDB catalog is configured. Episode lists show only what Plex reported. Set <code>WATCHKEEP_CATALOG_DATABASE_URL</code> to enable full episode lists.</p>')}
<h2>In progress</h2>
${
  input.inProgress.length === 0
    ? raw('<div class="empty">Nothing in progress. Play something on Plex.</div>')
    : raw(html`<table><thead><tr><th>Title</th><th>State</th><th>Progress</th><th>Player</th><th>Updated</th><th></th></tr></thead><tbody>
${input.inProgress.map(
  (entry) => html`<tr>
  <td><span class="title-cell">${poster(input.images, entry.poster_path)}<span>${progressTitle(entry)}</span></span></td>
  <td><span class="badge ${entry.state === "playing" ? "ok" : ""}">${entry.state}</span></td>
  <td>${progressBar(entry.position_ms, entry.duration_ms)} <span class="muted">${fmtDuration(entry.position_ms)}${entry.duration_ms ? ` / ${fmtDuration(entry.duration_ms)}` : ""}</span></td>
  <td class="muted">${entry.player ?? ""}</td>
  <td class="muted">${fmtDate(entry.updated_at)}</td>
  <td>${actionForm("clear-progress", entry.target_id, "/", "Clear", false, html`<input type="hidden" name="kind" value="${entry.target_kind}">`)}</td>
</tr>`,
)}
</tbody></table>`)
}
<h2>Recent history</h2>
${
  input.recent.length === 0
    ? raw('<div class="empty">No plays recorded yet.</div>')
    : raw(html`<table><thead><tr><th>Watched</th><th>Title</th><th>Source</th><th>Player</th></tr></thead><tbody>
${input.recent.map(
  (entry) => html`<tr><td class="muted">${fmtDate(entry.watched_at)}</td><td><span class="title-cell">${poster(input.images, entry.poster_path)}<span>${historyTitle(entry)}</span></span></td><td class="muted">${entry.source}</td><td class="muted">${entry.player ?? ""}</td></tr>`,
)}
</tbody></table>`)
}`;
  return layout("Dashboard", "/", body);
}

export function renderMovies(input: { images: string; movies: MovieView[]; filter: WatchFilter; search: string }): string {
  const back = `/movies?status=${input.filter}${input.search ? `&q=${encodeURIComponent(input.search)}` : ""}`;
  const body = html`<h1>Movies</h1>
${toolbar("/movies", input.filter, input.search)}
${
  input.movies.length === 0
    ? raw('<div class="empty">No movies match. Run a Plex sync or play a movie.</div>')
    : raw(html`<table><thead><tr><th>Title</th><th>Status</th><th>Last watched</th><th>Runtime</th><th></th></tr></thead><tbody>
${input.movies.map(
  (movie) => html`<tr>
  <td><span class="title-cell">${poster(input.images, movie.poster_path)}<span>${movie.title}${movie.year ? html` <span class="muted">(${movie.year})</span>` : ""}</span></span></td>
  <td>${movie.play_count > 0 ? raw(html`<span class="badge ok">watched${movie.play_count > 1 ? ` ×${movie.play_count}` : ""}</span>`) : raw('<span class="badge">unwatched</span>')}</td>
  <td class="muted">${fmtDate(movie.last_watched_at)}</td>
  <td class="muted">${fmtDuration(movie.duration_ms)}</td>
  <td>${movie.play_count > 0 ? actionForm("unwatch-movie", movie.id, back, "Mark unwatched") : actionForm("watch-movie", movie.id, back, "Mark watched", true)} ${actionForm("watchlist-add-movie", movie.id, back, "+ Watchlist")}</td>
</tr>`,
)}
</tbody></table>`)
}`;
  return layout("Movies", "/movies", body);
}

export function renderShows(input: { images: string; shows: ShowListItem[]; filter: WatchFilter; search: string }): string {
  const body = html`<h1>Shows</h1>
${toolbar("/shows", input.filter, input.search)}
${
  input.shows.length === 0
    ? raw('<div class="empty">No shows match. Run a Plex sync or play an episode.</div>')
    : raw(html`<table><thead><tr><th>Title</th><th>Progress</th><th>Episodes</th><th>Last watched</th></tr></thead><tbody>
${input.shows.map((show) => {
  const complete = show.total_episodes > 0 && show.watched_count >= show.total_episodes;
  return html`<tr>
  <td><span class="title-cell">${poster(input.images, show.poster_path)}<span><a href="/shows/${show.id}">${show.title}</a>${show.year ? html` <span class="muted">(${show.year})</span>` : ""}</span></span></td>
  <td>${progressBar(show.watched_count, show.total_episodes || null)}</td>
  <td><span class="badge ${complete ? "ok" : show.watched_count > 0 ? "part" : ""}">${show.watched_count} / ${show.total_episodes}</span>${show.hidden_at ? raw(' <span class="badge">hidden</span>') : ""}</td>
  <td class="muted">${fmtDate(show.last_watched_at)}</td>
</tr>`;
})}
</tbody></table>`)
}`;
  return layout("Shows", "/shows", body);
}

function episodeAction(showId: number, episode: MergedEpisode, back: string): Raw {
  if (episode.id !== null) {
    return episode.play_count > 0
      ? actionForm("unwatch-episode", episode.id, back, "Unwatch")
      : actionForm("watch-episode", episode.id, back, "Watched", true);
  }
  const extra = html`<input type="hidden" name="season" value="${episode.season}"><input type="hidden" name="number" value="${episode.number}">`;
  return actionForm("watch-episode-number", showId, back, "Watched", true, extra);
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
  const body = html`${poster(input.images, show.poster_path, true)}<h1>${show.title}${show.year ? html` <span class="muted">(${show.year})</span>` : ""}</h1>
<p class="muted">${show.watched_count} of ${show.total_episodes} episodes watched.${show.rating !== null ? ` Rated ${show.rating}/10.` : ""}</p>
${show.summary ? html`<p class="muted">${show.summary}</p>` : ""}
<p>${actionForm("watch-show", show.id, back, "Mark all watched", true)} ${actionForm("unwatch-show", show.id, back, "Mark all unwatched")}
${input.onWatchlist ? actionForm("watchlist-remove-show", show.id, back, "Remove from watchlist") : actionForm("watchlist-add-show", show.id, back, "Add to watchlist")}
${show.hidden_at ? actionForm("unhide-show", show.id, back, "Unhide") : actionForm("hide-show", show.id, back, "Hide from unwatched")}
<a href="/shows">← Shows</a></p>
${
  input.episodes.length === 0
    ? raw('<div class="empty">No episodes known yet. Configure the TMDB catalog or run a Plex sync to import the episode list.</div>')
    : [...seasons.entries()].map(
        ([season, episodes]) => html`<div class="season"><h2>${season === 0 ? "Specials" : `Season ${season}`}</h2>
<table><thead><tr><th>Episode</th><th>Status</th><th>Last watched</th><th>Aired</th><th></th></tr></thead><tbody>
${episodes.map(
  (episode) => html`<tr class="${episode.aired_at && episode.aired_at > today ? "future" : ""}">
  <td><strong>${code(episode.season, episode.number)}</strong> ${episode.title ?? ""}</td>
  <td>${
    episode.play_count > 0
      ? raw(html`<span class="badge ok">watched${episode.play_count > 1 ? ` ×${episode.play_count}` : ""}</span>`)
      : episode.position_ms
        ? raw(html`<span class="badge part">${episode.duration_ms ? `${Math.round((episode.position_ms / episode.duration_ms) * 100)}%` : "in progress"}</span>`)
        : raw('<span class="badge">unwatched</span>')
  }</td>
  <td class="muted">${fmtDate(episode.last_watched_at)}</td>
  <td class="muted">${episode.aired_at ?? ""}</td>
  <td>${episodeAction(show.id, episode, back)}</td>
</tr>`,
)}
</tbody></table></div>`,
      )
}`;
  return layout(show.title, "/shows", body);
}

export function renderHistory(input: { images: string; entries: HistoryEntry[]; page: number; pageSize: number; total: number }): string {
  const pages = Math.max(1, Math.ceil(input.total / input.pageSize));
  const back = `/history?page=${input.page}`;
  const body = html`<h1>History</h1>
<p class="muted">${input.total} plays.</p>
${
  input.entries.length === 0
    ? raw('<div class="empty">No plays recorded yet.</div>')
    : raw(html`<table><thead><tr><th>Watched</th><th>Title</th><th>Source</th><th>Account</th><th>Player</th><th></th></tr></thead><tbody>
${input.entries.map(
  (entry) => html`<tr>
  <td class="muted">${fmtDate(entry.watched_at)}</td>
  <td><span class="title-cell">${poster(input.images, entry.poster_path)}<span>${historyTitle(entry)}</span></span></td>
  <td class="muted">${entry.source}</td>
  <td class="muted">${entry.account ?? ""}</td>
  <td class="muted">${entry.player ?? ""}</td>
  <td>${actionForm("remove-play", entry.id, back, "Remove")}</td>
</tr>`,
)}
</tbody></table>`)
}
<div class="pager">
  ${input.page > 1 ? raw(html`<a href="/history?page=${input.page - 1}">← Newer</a>`) : ""}
  <span class="muted">Page ${input.page} of ${pages}</span>
  ${input.page < pages ? raw(html`<a href="/history?page=${input.page + 1}">Older →</a>`) : ""}
</div>`;
  return layout("History", "/history", body);
}

export function renderWatchlist(input: { images: string; items: WatchlistItem[] }): string {
  const back = "/watchlist";
  const body = html`<h1>Watchlist</h1>
${
  input.items.length === 0
    ? raw('<div class="empty">The watchlist is empty. Add movies and shows from their pages.</div>')
    : raw(html`<table><thead><tr><th>Title</th><th>Type</th><th>Status</th><th>Added</th><th></th></tr></thead><tbody>
${input.items.map(
  (item) => html`<tr>
  <td><span class="title-cell">${poster(input.images, item.poster_path)}<span>${
    item.kind === "show" ? html`<a href="/shows/${item.id}">${item.title}</a>` : item.title
  }${item.year ? html` <span class="muted">(${item.year})</span>` : ""}</span></span></td>
  <td class="muted">${item.kind}</td>
  <td>${item.watched_count > 0 ? raw(html`<span class="badge part">${item.kind === "movie" ? "watched" : `${item.watched_count} episodes watched`}</span>`) : raw('<span class="badge">unwatched</span>')}</td>
  <td class="muted">${fmtDate(item.listed_at)}</td>
  <td>${actionForm(item.kind === "movie" ? "watchlist-remove-movie" : "watchlist-remove-show", item.id, back, "Remove")}</td>
</tr>`,
)}
</tbody></table>`)
}`;
  return layout("Watchlist", "/watchlist", body);
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
  const body = html`<h1>Webhook log</h1>
<p class="muted">The last 100 webhook calls that Plex sent. Use this page to check that Plex reaches Watchkeep.</p>
${
  input.events.length === 0
    ? raw('<div class="empty">No webhook received yet. Add the webhook URL in Plex settings.</div>')
    : raw(html`<table><thead><tr><th>Received</th><th>Event</th><th>Outcome</th><th>Title</th><th>Account</th><th>Player</th></tr></thead><tbody>
${input.events.map(
  (event) => html`<tr>
  <td class="muted">${fmtDate(event.received_at)}</td>
  <td><code>${event.event}</code></td>
  <td>${event.outcome}</td>
  <td>${event.title ?? ""}${event.media_type ? html` <span class="muted">(${event.media_type})</span>` : ""}</td>
  <td class="muted">${event.account ?? ""}</td>
  <td class="muted">${event.player ?? ""}</td>
</tr>`,
)}
</tbody></table>`)
}`;
  return layout("Webhooks", "/webhooks", body);
}
