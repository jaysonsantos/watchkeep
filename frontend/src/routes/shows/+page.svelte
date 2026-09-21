<script lang="ts">
  import Pager from "$lib/components/Pager.svelte";
  import Poster from "$lib/components/Poster.svelte";
  import ProgressBar from "$lib/components/ProgressBar.svelte";
  import RatingControl from "$lib/components/RatingControl.svelte";
  import Toolbar from "$lib/components/Toolbar.svelte";
  import { fmtShortDay } from "$lib/format.ts";
  import { listUrl, pageCount } from "$lib/lists.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();
</script>

<svelte:head><title>Shows · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Shows</h1>
    <p class="sub">{data.total} {data.list.filter === "all" ? "shows" : `${data.list.filter} shows`}</p>
  </div>
</div>

<Toolbar base="/shows" list={data.list} />

{#if data.shows.length === 0}
  <div class="empty">No shows match. Add one, run a Plex sync, or play an episode.</div>
{:else}
  <div class="grid">
    {#each data.shows as show (show.id)}
      {@const complete = show.total_episodes > 0 && show.watched_count >= show.total_episodes}
      {@const left = Math.max(0, show.total_episodes - show.watched_count)}
      {@const href = `/shows/${show.id}`}
      <div class="card">
        <Poster images={data.images} path={show.poster_path} title={show.title} {href}>
          {#snippet badge()}
            {#if complete}
              <span class="badge ok">complete</span>
            {:else if show.hidden_at}
              <span class="badge">hidden</span>
            {:else if left > 0 && show.watched_count > 0}
              <span class="badge part">{left} left</span>
            {/if}
            <RatingControl kind="show" id={show.id} value={show.rating} />
          {/snippet}
          {#snippet bar()}
            <ProgressBar value={show.watched_count} total={show.total_episodes || null} />
          {/snippet}
        </Poster>
        <div class="body">
          <a class="t" {href} title={show.title}>{show.title}</a>
          <span class="m">{show.year ?? ""}{show.year ? " · " : ""}{show.watched_count} / {show.total_episodes} episodes</span>
          {#if show.last_watched_at}<span class="m">Last {fmtShortDay(show.last_watched_at)}</span>{/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<Pager page={data.list.page} pages={pageCount(data.total, data.list.pageSize)} href={(page) => listUrl("/shows", data.list, page)} />
