<script lang="ts">
  import ActionButton from "$lib/components/ActionButton.svelte";
  import ProgressBar from "$lib/components/ProgressBar.svelte";
  import RatingControl from "$lib/components/RatingControl.svelte";
  import { episodeCode, fmtDate, fmtRating, percent, today } from "$lib/format.ts";
  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();

  const day = today();
  const show = $derived(data.show);
  const complete = $derived(show.total_episodes > 0 && show.watched_count >= show.total_episodes);
  const nextUp = $derived(
    data.episodes.find(
      (episode) => episode.season > 0 && episode.play_count === 0 && (!episode.aired_at || episode.aired_at <= day),
    ),
  );
  const seasons = $derived.by(() => {
    const bySeason = new Map<number, typeof data.episodes>();
    for (const episode of data.episodes) {
      const list = bySeason.get(episode.season) ?? [];
      list.push(episode);
      bySeason.set(episode.season, list);
    }
    return [...bySeason.entries()];
  });
</script>

<svelte:head><title>{show.title} · Watchkeep</title></svelte:head>

<div class="hero">
  <div class="art">
    {#if show.poster_path}
      <img src="{data.images}w185{show.poster_path}" srcset="{data.images}w342{show.poster_path} 2x" alt="" />
    {/if}
  </div>
  <div class="info">
    <p class="muted" style="margin:0"><a href="/shows">← Shows</a></p>
    <h1>{show.title}{#if show.year}&nbsp;<span class="muted">({show.year})</span>{/if}</h1>
    <div class="chips">
      <span class="badge" class:ok={complete} class:part={!complete && show.watched_count > 0}>
        {show.watched_count} / {show.total_episodes} watched
      </span>
      {#if show.rating !== null}<span class="badge soft">{fmtRating(show.rating)}</span>{/if}
      {#if show.hidden_at}<span class="badge">hidden</span>{/if}
      {#if data.onWatchlist}<span class="badge soft">on watchlist</span>{/if}
      {#if nextUp}
        <span class="badge">Next: {episodeCode(nextUp.season, nextUp.number)}{nextUp.title ? ` ${nextUp.title}` : ""}</span>
      {/if}
    </div>
    <ProgressBar value={show.watched_count} total={show.total_episodes || null} />
    {#if show.summary}<p class="summary">{show.summary}</p>{/if}
    <div class="actions">
      <RatingControl kind="show" id={show.id} value={show.rating} />
      <ActionButton action="watch-show" id={show.id} label="Mark all watched" primary />
      <ActionButton action="unwatch-show" id={show.id} label="Mark all unwatched" />
      {#if data.onWatchlist}
        <ActionButton action="watchlist-remove-show" id={show.id} label="Remove from watchlist" />
      {:else}
        <ActionButton action="watchlist-add-show" id={show.id} label="Add to watchlist" />
      {/if}
      {#if show.hidden_at}
        <ActionButton action="unhide-show" id={show.id} label="Unhide" />
      {:else}
        <ActionButton action="hide-show" id={show.id} label="Hide from unwatched" />
      {/if}
    </div>
  </div>
</div>

{#if data.episodes.length === 0}
  <div class="empty">No episodes known yet. Configure the TMDB catalog or run a Plex sync to import the episode list.</div>
{:else}
  {#each seasons as [season, episodes] (season)}
    {@const watched = episodes.filter((episode) => episode.play_count > 0).length}
    <details class="season" open={season > 0 && watched < episodes.length}>
      <summary>
        {season === 0 ? "Specials" : `Season ${season}`}
        <span class="muted" style="font-weight:500">{watched} / {episodes.length}</span>
        <ProgressBar value={watched} total={episodes.length} />
      </summary>
      <div class="table-wrap">
        <table>
          <thead><tr><th>Episode</th><th>Status</th><th></th><th>Rating</th><th>Last watched</th><th>Aired</th></tr></thead>
          <tbody>
            {#each episodes as episode (`${episode.season}:${episode.number}`)}
              {@const upcoming = Boolean(episode.aired_at && episode.aired_at > day)}
              <tr class:future={upcoming}>
                <td><span class="ep-code">{episodeCode(episode.season, episode.number)}</span>{episode.title ?? ""}</td>
                <td>
                  {#if episode.play_count > 0}
                    <span class="badge ok">watched{episode.play_count > 1 ? ` ×${episode.play_count}` : ""}</span>
                  {:else if episode.position_ms}
                    <span class="badge part">{episode.duration_ms ? `${percent(episode.position_ms, episode.duration_ms)}%` : "in progress"}</span>
                  {:else if upcoming}
                    <span class="badge">upcoming</span>
                  {:else}
                    <span class="badge">unwatched</span>
                  {/if}
                </td>
                <td>
                  {#if episode.id === null}
                    <ActionButton
                      action="watch-episode-number"
                      id={show.id}
                      label="Watched"
                      primary
                      small
                      fields={{ season: episode.season, number: episode.number }}
                    />
                  {:else if episode.play_count > 0}
                    <ActionButton action="unwatch-episode" id={episode.id} label="Unwatch" small />
                  {:else}
                    <ActionButton action="watch-episode" id={episode.id} label="Watched" primary small />
                  {/if}
                </td>
                <td>
                  {#if episode.id !== null}
                    <RatingControl kind="episode" id={episode.id} value={episode.rating} />
                  {/if}
                </td>
                <td class="muted">{fmtDate(episode.last_watched_at)}</td>
                <td class="muted">{episode.aired_at ?? ""}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </details>
  {/each}
{/if}
