<script lang="ts">
  import ActionButton from "$lib/components/ActionButton.svelte";
  import Pager from "$lib/components/Pager.svelte";
  import Poster from "$lib/components/Poster.svelte";
  import RatingControl from "$lib/components/RatingControl.svelte";
  import Toolbar from "$lib/components/Toolbar.svelte";
  import { fmtDuration, fmtShortDay } from "$lib/format.ts";
  import { listUrl, pageCount } from "$lib/lists.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();
</script>

<svelte:head><title>Movies · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Movies</h1>
    <p class="sub">{data.total} {data.list.filter === "all" ? "movies" : `${data.list.filter} movies`}</p>
  </div>
</div>

<Toolbar base="/movies" list={data.list} />

{#if data.movies.length === 0}
  <div class="empty">No movies match. Add one, run a Plex sync, or play a movie.</div>
{:else}
  <div class="grid">
    {#each data.movies as movie (movie.id)}
      <div class="card">
        <Poster images={data.images} path={movie.poster_path} title={movie.title}>
          {#snippet badge()}
            {#if movie.play_count > 0}
              <span class="badge ok">watched{movie.play_count > 1 ? ` ×${movie.play_count}` : ""}</span>
            {/if}
            <RatingControl kind="movie" id={movie.id} value={movie.rating} />
          {/snippet}
        </Poster>
        <div class="body">
          <span class="t" title={movie.title}>{movie.title}</span>
          <span class="m">
            {movie.year ?? ""}{movie.year && movie.duration_ms ? " · " : ""}{fmtDuration(movie.duration_ms)}
          </span>
          {#if movie.last_watched_at}<span class="m">Watched {fmtShortDay(movie.last_watched_at)}</span>{/if}
          <div class="actions">
            {#if movie.play_count > 0}
              <ActionButton action="unwatch-movie" id={movie.id} label="Unwatch" small />
            {:else}
              <ActionButton action="watch-movie" id={movie.id} label="Watched" primary small />
            {/if}
            <ActionButton action="watchlist-add-movie" id={movie.id} label="+ List" small />
          </div>
        </div>
      </div>
    {/each}
  </div>
{/if}

<Pager page={data.list.page} pages={pageCount(data.total, data.list.pageSize)} href={(page) => listUrl("/movies", data.list, page)} />
