<script lang="ts">
  import { percent } from "$lib/format.ts";
  import { fmtClock, livePosition, TICK_MS } from "$lib/nowplaying.ts";
  import type { ProgressView } from "$lib/types.ts";
  import ActionButton from "./ActionButton.svelte";
  import MediaTitle from "./MediaTitle.svelte";
  import ProgressBar from "./ProgressBar.svelte";
  import Thumb from "./Thumb.svelte";

  let { entries, images }: { entries: ProgressView[]; images: string } = $props();

  // The server writes a progress row only on a Plex event, so the widget moves the
  // position itself. The page reloads the rows on a timer.
  let now = $state(Date.now());

  $effect(() => {
    const tick = setInterval(() => {
      now = Date.now();
    }, TICK_MS);
    return () => clearInterval(tick);
  });
</script>

<section class="now-playing">
  <h2><span class="dot" aria-hidden="true"></span>Now playing</h2>
  <div class="np-grid">
    {#each entries as entry (`${entry.target_kind}:${entry.target_id}`)}
      {@const position = livePosition(entry, now)}
      <article class="np-card">
        <Thumb {images} path={entry.poster_path} />
        <div class="info">
          <div class="t"><MediaTitle {entry} /></div>
          <ProgressBar value={position} total={entry.duration_ms} />
          <div class="row">
            <span class="time">
              {fmtClock(position)}{entry.duration_ms ? ` / ${fmtClock(entry.duration_ms)}` : ""}
              <span class="muted">· {percent(position, entry.duration_ms)}%</span>
            </span>
            {#if entry.duration_ms}<span class="muted">{fmtClock(entry.duration_ms - position)} left</span>{/if}
          </div>
          <div class="row">
            <span class="muted">{[entry.player, entry.account].filter(Boolean).join(" · ")}</span>
            <ActionButton action="clear-progress" id={entry.target_id} label="Clear" small fields={{ kind: entry.target_kind }} />
          </div>
        </div>
      </article>
    {/each}
  </div>
</section>
