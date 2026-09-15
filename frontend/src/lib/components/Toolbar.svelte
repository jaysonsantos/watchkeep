<script lang="ts">
  import { DEFAULT_FILTER, type ListState, listUrl, QUERY, SORT_ORDERS, type WatchFilter } from "$lib/lists.ts";

  let { base, list }: { base: string; list: ListState } = $props();

  const filters: Array<[WatchFilter, string]> = [
    ["all", "All"],
    ["watched", "Watched"],
    ["unwatched", "Unwatched"],
  ];
</script>

<div class="toolbar">
  <div class="seg">
    {#each filters as [value, label] (value)}
      <a href={listUrl(base, list, 1, value)} class:active={list.filter === value}>{label}</a>
    {/each}
  </div>
  <form method="GET" action={base} data-sveltekit-keepfocus>
    {#if list.filter !== DEFAULT_FILTER}<input type="hidden" name={QUERY.status} value={list.filter} />{/if}
    <input type="search" name={QUERY.search} placeholder="Filter by title" value={list.search} />
    <label class="sort">
      Sort
      <select name={QUERY.sort} onchange={(event) => event.currentTarget.form?.requestSubmit()}>
        {#each SORT_ORDERS as [value, label] (value)}
          <option {value} selected={value === list.sort}>{label}</option>
        {/each}
      </select>
    </label>
    <noscript><button type="submit" class="small">Apply</button></noscript>
  </form>
</div>
