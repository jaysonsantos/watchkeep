<script lang="ts">
  import ActionButton from "$lib/components/ActionButton.svelte";
  import MediaTitle from "$lib/components/MediaTitle.svelte";
  import Pager from "$lib/components/Pager.svelte";
  import Thumb from "$lib/components/Thumb.svelte";
  import { fmtDate } from "$lib/format.ts";
  import { pageCount } from "$lib/lists.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();
</script>

<svelte:head><title>History · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>History</h1>
    <p class="sub">{data.total} plays</p>
  </div>
</div>

{#if data.entries.length === 0}
  <div class="empty">No plays recorded yet.</div>
{:else}
  <div class="table-wrap">
    <table>
      <thead><tr><th>Watched</th><th>Title</th><th></th><th>Source</th><th>Account</th><th>Player</th></tr></thead>
      <tbody>
        {#each data.entries as entry (entry.id)}
          <tr>
            <td class="muted" style="white-space:nowrap">{fmtDate(entry.watched_at)}</td>
            <td><span class="title-cell"><Thumb images={data.images} path={entry.poster_path} /><span><MediaTitle {entry} /></span></span></td>
            <td><ActionButton action="remove-play" id={entry.id} label="Remove" small /></td>
            <td class="muted">{entry.source}</td>
            <td class="muted">{entry.account ?? ""}</td>
            <td class="muted">{entry.player ?? ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<Pager
  page={data.page}
  pages={pageCount(data.total, data.pageSize)}
  href={(page) => `/history?page=${page}`}
  previous="← Newer"
  next="Older →"
/>
