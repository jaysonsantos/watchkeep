<script lang="ts">
  import { fmtDate } from "$lib/format.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();
</script>

<svelte:head><title>Webhooks · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>Webhook log</h1>
    <p class="sub">The last 100 calls that Plex and other players sent. Use this page to check that they reach Watchkeep.</p>
  </div>
</div>

{#if data.events.length === 0}
  <div class="empty">No webhook received yet. Add the webhook URL in Plex settings, or point another player at /webhook/scrobble.</div>
{:else}
  <div class="table-wrap">
    <table>
      <thead><tr><th>Received</th><th>Event</th><th>Outcome</th><th>Title</th><th>Account</th><th>Player</th></tr></thead>
      <tbody>
        {#each data.events as event (event.id)}
          <tr>
            <td class="muted" style="white-space:nowrap">{fmtDate(event.received_at)}</td>
            <td><code>{event.event}</code></td>
            <td>
              <span class="badge" class:ok={event.outcome === "play"} class:soft={event.outcome !== "play" && !event.outcome.startsWith("ignored")}>
                {event.outcome}
              </span>
            </td>
            <td>{event.title ?? ""}{#if event.media_type}&nbsp;<span class="muted">({event.media_type})</span>{/if}</td>
            <td class="muted">{event.account ?? ""}</td>
            <td class="muted">{event.player ?? ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
