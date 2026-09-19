<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { clearRating, setRating } from "$lib/api.ts";
  import { fmtRating, MAX_USER_RATING, RATING_PICKS } from "$lib/format.ts";
  import type { Id, RatingKind } from "$lib/types.ts";

  interface Props {
    kind: RatingKind;
    id: Id;
    value: number | null;
    compact?: boolean;
  }

  let { kind, id, value, compact = false }: Props = $props();

  let busy = $state(false);

  async function run(work: () => Promise<void>) {
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

<div class="rating-picker" class:compact>
  {#if value !== null && !compact}
    <span class="score">{fmtRating(value)}</span>
  {/if}
  {#each RATING_PICKS as rating (rating)}
    <button
      type="button"
      class:on={value === rating}
      disabled={busy}
      aria-label="Rate {rating} out of {MAX_USER_RATING}"
      aria-pressed={value === rating}
      onclick={(event) => {
        event.stopPropagation();
        pick(rating);
      }}>{rating}</button>
  {/each}
  {#if value !== null}
    <button
      type="button"
      class="small"
      disabled={busy}
      onclick={(event) => {
        event.stopPropagation();
        clear();
      }}>Clear</button
    >
  {/if}
</div>
