<script lang="ts">
  import ActionButton from "$lib/components/ActionButton.svelte";
  import Poster from "$lib/components/Poster.svelte";
  import { fmtShortDay } from "$lib/format.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();
</script>

<svelte:head><title>Watchlist · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Watchlist</h1>
    <p class="sub">{data.items.length} items to watch</p>
  </div>
  <a class="btn" href="/add">+ Add</a>
</div>

{#if data.items.length === 0}
  <div class="empty">The watchlist is empty. Add movies and shows from the Add page.</div>
{:else}
  <div class="grid">
    {#each data.items as item (`${item.kind}:${item.id}`)}
      <div class="card">
        {#snippet badge()}
          {#if item.watched_count > 0}
            <span class="badge part">{item.kind === "movie" ? "watched" : `${item.watched_count} seen`}</span>
          {/if}
        {/snippet}
        <Poster
          images={data.images}
          path={item.poster_path}
          title={item.title}
          href={item.kind === "show" ? `/shows/${item.id}` : null}
          {badge}
        />
        <div class="body">
          {#if item.kind === "show"}
            <a class="t" href="/shows/{item.id}" title={item.title}>{item.title}</a>
          {:else}
            <span class="t" title={item.title}>{item.title}</span>
          {/if}
          <span class="m">{item.kind}{item.year ? ` · ${item.year}` : ""}</span>
          <span class="m">Added {fmtShortDay(item.listed_at)}</span>
          <div class="actions">
            {#if item.kind === "movie"}
              <ActionButton action="watch-movie" id={item.id} label="Watched" primary small />
              <ActionButton action="watchlist-remove-movie" id={item.id} label="Remove" small />
            {:else}
              <ActionButton action="watchlist-remove-show" id={item.id} label="Remove" small />
            {/if}
          </div>
        </div>
      </div>
    {/each}
  </div>
{/if}
