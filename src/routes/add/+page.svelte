<script lang="ts">
  import { enhance } from "$app/forms";
  import Thumb from "$lib/components/Thumb.svelte";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();

  const groups = $derived([
    { kind: "movie" as const, heading: "Movies", items: data.movies },
    { kind: "show" as const, heading: "Shows", items: data.shows },
  ]);
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
    <input type="search" name="q" placeholder="Search the catalog by title" value={data.query} />
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
                    <form class="inline" method="POST" action="?/add" use:enhance>
                      <input type="hidden" name="kind" value={group.kind} />
                      <input type="hidden" name="tmdb_id" value={item.tmdbId} />
                      <input type="hidden" name="q" value={data.query} />
                      <input type="hidden" name="watchlist" value="1" />
                      <button type="submit" class="primary small">+ Watchlist</button>
                    </form>
                    <form class="inline" method="POST" action="?/add" use:enhance>
                      <input type="hidden" name="kind" value={group.kind} />
                      <input type="hidden" name="tmdb_id" value={item.tmdbId} />
                      <input type="hidden" name="q" value={data.query} />
                      <button type="submit" class="small">Add to library</button>
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
<form class="add-form" method="POST" action="?/add" use:enhance>
  <label>Type <select name="kind"><option value="movie">Movie</option><option value="show">Show</option></select></label>
  <label>Title <input name="title" required placeholder="Title" /></label>
  <label>Year <input name="year" type="number" min="1870" max="2100" placeholder="2024" /></label>
  <label>TMDB id <input name="tmdb_id" type="number" min="1" placeholder="optional" /></label>
  <label><span><input type="checkbox" name="watchlist" value="1" checked /> Add to watchlist</span></label>
  <button type="submit" class="primary">Add</button>
</form>
