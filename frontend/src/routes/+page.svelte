<script lang="ts">
  import ActionButton from "$lib/components/ActionButton.svelte";
  import MediaTitle from "$lib/components/MediaTitle.svelte";
  import ProgressBar from "$lib/components/ProgressBar.svelte";
  import Thumb from "$lib/components/Thumb.svelte";
  import { fmtDate, fmtDuration, percent } from "$lib/format.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();

  const tiles = $derived([
    { label: "Movies watched", value: data.stats.movies_watched, total: data.stats.movies },
    { label: "Episodes watched", value: data.stats.episodes_watched, total: data.stats.episodes },
    { label: "Shows tracked", value: data.stats.shows, total: null },
    { label: "Plays recorded", value: data.stats.plays, total: null },
  ]);
</script>

<svelte:head><title>Dashboard · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Dashboard</h1>
    <p class="sub">What you watched, what is playing, and what is left.</p>
  </div>
  {#if data.syncConfigured}<ActionButton action="sync" label="Sync Plex library" />{/if}
</div>

<div class="tiles">
  {#each tiles as tile (tile.label)}
    <div class="tile">
      <div class="n">{tile.value}{#if tile.total !== null}<small> / {tile.total}</small>{/if}</div>
      <div class="l">{tile.label}</div>
      {#if tile.total !== null}<ProgressBar value={tile.value} total={tile.total || null} />{/if}
    </div>
  {/each}
</div>

{#if !data.catalogConfigured}
  <div class="notice">
    No TMDB catalog is configured. Episode lists show only what Plex reported. Set
    <code>WATCHKEEP_CATALOG_DATABASE_URL</code> to enable full episode lists.
  </div>
{/if}

<h2>In progress <span class="count">{data.inProgress.length}</span></h2>
{#if data.inProgress.length === 0}
  <div class="empty">Nothing in progress. Play something on Plex.</div>
{:else}
  <div class="progress-grid">
    {#each data.inProgress as entry (`${entry.target_kind}:${entry.target_id}`)}
      <div class="progress-card">
        <Thumb images={data.images} path={entry.poster_path} />
        <div class="info">
          <div class="t"><MediaTitle {entry} /></div>
          <div class="row">
            <span class="badge" class:ok={entry.state === "playing"}>{entry.state}</span>
            <span>
              {percent(entry.position_ms, entry.duration_ms)}% · {fmtDuration(entry.position_ms)}{entry.duration_ms ? ` / ${fmtDuration(entry.duration_ms)}` : ""}
            </span>
          </div>
          <ProgressBar value={entry.position_ms} total={entry.duration_ms} />
          <div class="row">
            <span>{entry.player ?? ""}{entry.player ? " · " : ""}{fmtDate(entry.updated_at)}</span>
            <ActionButton action="clear-progress" id={entry.target_id} label="Clear" small fields={{ kind: entry.target_kind }} />
          </div>
        </div>
      </div>
    {/each}
  </div>
{/if}

<h2>Recent history</h2>
{#if data.recent.length === 0}
  <div class="empty">No plays recorded yet.</div>
{:else}
  <div class="list">
    {#each data.recent as entry (entry.id)}
      <div class="row">
        <Thumb images={data.images} path={entry.poster_path} />
        <div class="what"><MediaTitle {entry} /></div>
        <span class="src">{entry.source}{entry.player ? ` · ${entry.player}` : ""}</span>
        <span class="when">{fmtDate(entry.watched_at)}</span>
      </div>
    {/each}
  </div>
  <p class="muted"><a href="/history">Full history →</a></p>
{/if}
