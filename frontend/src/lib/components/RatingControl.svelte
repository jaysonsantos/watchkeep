<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { clearRating, setRating } from "$lib/api.ts";
  import { fmtRating, MAX_USER_RATING, RATING_PICKS } from "$lib/format.ts";
  import type { Id, RatingKind } from "$lib/types.ts";

  interface Props {
    kind: RatingKind;
    id: Id;
    value: number | null;
  }

  let { kind, id, value }: Props = $props();

  // One page holds many controls, so each one names its own popover and its own anchor.
  const uid = $props.id();
  const popoverId = `rating-${uid}`;
  const anchorName = `--rating-${uid}`;

  let popover: HTMLDivElement | undefined = $state();
  let busy = $state(false);

  async function run(work: () => Promise<void>) {
    popover?.hidePopover();
    busy = true;
    try {
      await work();
      await invalidateAll();
    } catch (cause) {
      window.alert(cause instanceof Error ? cause.message : String(cause));
    } finally {
      busy = false;
    }
  }

  function pick(rating: number) {
    void run(() => setRating(kind, id, rating));
  }

  function clear() {
    void run(() => clearRating(kind, id));
  }
</script>

<!-- One button shows the rating. The picks live in a popover, so a card or a table row stays small. -->
<span class="rating">
  <button
    type="button"
    class="rating-trigger"
    class:rated={value !== null}
    style:anchor-name={anchorName}
    popovertarget={popoverId}
    disabled={busy}
    title={value === null ? "Rate" : "Change the rating"}
    aria-label={value === null ? "Rate" : `Rated ${value} out of ${MAX_USER_RATING}. Change the rating`}
  >
    {#if busy}
      …
    {:else if value === null}
      <span class="star">☆</span><span class="word">Rate</span>
    {:else}
      <span class="star">★</span>{value}
    {/if}
  </button>
  <div bind:this={popover} id={popoverId} class="rating-pop" popover="auto" style:position-anchor={anchorName}>
    <div class="head">
      <span>Your rating</span>
      <span class="score">{value === null ? "None" : fmtRating(value)}</span>
    </div>
    <div class="picks">
      {#each RATING_PICKS as rating (rating)}
        <button
          type="button"
          class:on={value === rating}
          class:below={value !== null && rating < value}
          aria-label="Rate {rating} out of {MAX_USER_RATING}"
          aria-pressed={value === rating}
          onclick={() => pick(rating)}>{rating}</button
        >
      {/each}
    </div>
    {#if value !== null}
      <button type="button" class="small clear" onclick={clear}>Clear the rating</button>
    {/if}
  </div>
</span>
