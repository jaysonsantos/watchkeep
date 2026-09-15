<script lang="ts">
  import { goto } from "$app/navigation";
  import { API_BASE, ApiError, send } from "$lib/api.ts";
  import Thumb from "$lib/components/Thumb.svelte";
  import { QUERY } from "$lib/lists.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();

  /** The form fields of the add forms. */
  const FIELD = {
    kind: "kind",
    tmdbId: "tmdb_id",
    title: "title",
    year: "year",
    watchlist: "watchlist",
    query: QUERY.search,
  } as const;

  /** The `error` query value that shows the "title is required" notice. */
  const TITLE_ERROR = "title";

  const groups = $derived([
    { kind: "movie" as const, heading: "Movies", items: data.movies },
    { kind: "show" as const, heading: "Shows", items: data.shows },
  ]);

  let busy = $state(false);

  /** Add form: kind, tmdb_id or title, year, and an optional watchlist flag. Then go to the item. */
  async function add(event: SubmitEvent) {
    event.preventDefault();
    const form = new FormData(event.currentTarget as HTMLFormElement);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const kind = field(FIELD.kind) === "show" ? "show" : "movie";
    const watchlist = field(FIELD.watchlist) === "1";
    busy = true;
    try {
      const row = await send<{ id: string }>("POST", kind === "show" ? `${API_BASE}/shows` : `${API_BASE}/movies`, {
        tmdb_id: Number(field(FIELD.tmdbId)) || undefined,
        title: field(FIELD.title) || undefined,
        year: Number(field(FIELD.year)) || undefined,
        watchlist,
      });
      await goto(watchlist ? "/watchlist" : kind === "show" ? `/shows/${row.id}` : "/movies?status=unwatched", {
        invalidateAll: true,
      });
    } catch (cause) {
      if (cause instanceof ApiError && cause.status === 400) {
        const params = new URLSearchParams({ [QUERY.search]: field(FIELD.query), error: TITLE_ERROR });
        await goto(`/add?${params}`, { invalidateAll: true });
      } else {
        window.alert(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>Add · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Add a movie or show</h1>
    <p class="sub">Search the catalog, or add by hand.</p>
  </div>
</div>

{#if data.catalogConfigured}
  <form class="search" method="GET" action="/add">
    <input type="search" name={QUERY.search} placeholder="Search the catalog by title" value={data.query} />
    <button type="submit" class="primary">Search</button>
  </form>
  {#if data.query.length >= 2 && data.movies.length === 0 && data.shows.length === 0}
    <div class="empty">No catalog match. Use the manual form below.</div>
  {/if}
  {#each groups as group (group.kind)}
    {#if group.items.length > 0}
      <h2>{group.heading} <span class="count">{group.items.length}</span></h2>
      <div class="table-wrap">
        <table>
          <thead><tr><th>Title</th><th>Year</th><th></th></tr></thead>
          <tbody>
            {#each group.items as item (item.tmdbId)}
              <tr>
                <td>
                  <span class="title-cell">
                    <Thumb images={data.images} path={item.posterPath} />
                    <span>
                      {#if item.localId !== null && group.kind === "show"}
                        <a href="/shows/{item.localId}">{item.title}</a>
                      {:else}
                        {item.title}
                      {/if}
                      {#if item.localId !== null}<span class="badge soft">in library</span>{/if}
                    </span>
                  </span>
                </td>
                <td class="muted">{item.year ?? ""}</td>
                <td>
                  <div class="actions">
                    <form class="inline" method="POST" onsubmit={add}>
                      <input type="hidden" name={FIELD.kind} value={group.kind} />
                      <input type="hidden" name={FIELD.tmdbId} value={item.tmdbId} />
                      <input type="hidden" name={FIELD.query} value={data.query} />
                      <input type="hidden" name={FIELD.watchlist} value="1" />
                      <button type="submit" class="primary small" disabled={busy}>+ Watchlist</button>
                    </form>
                    <form class="inline" method="POST" onsubmit={add}>
                      <input type="hidden" name={FIELD.kind} value={group.kind} />
                      <input type="hidden" name={FIELD.tmdbId} value={item.tmdbId} />
                      <input type="hidden" name={FIELD.query} value={data.query} />
                      <button type="submit" class="small" disabled={busy}>Add to library</button>
                    </form>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/each}
{:else}
  <div class="notice">No TMDB catalog is configured, so there is no search. Use the manual form.</div>
{/if}

<h2>Add by hand</h2>
{#if data.error}<div class="notice">A title is required.</div>{/if}
<form class="add-form" method="POST" onsubmit={add}>
  <label>Type <select name={FIELD.kind}><option value="movie">Movie</option><option value="show">Show</option></select></label>
  <label>Title <input name={FIELD.title} required placeholder="Title" /></label>
  <label>Year <input name={FIELD.year} type="number" min="1870" max="2100" placeholder="2024" /></label>
  <label>TMDB id <input name={FIELD.tmdbId} type="number" min="1" placeholder="optional" /></label>
  <label><span><input type="checkbox" name={FIELD.watchlist} value="1" checked /> Add to watchlist</span></label>
  <button type="submit" class="primary" disabled={busy}>Add</button>
</form>
