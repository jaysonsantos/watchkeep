<script lang="ts">
  import { episodeCode } from "$lib/format.ts";

  interface Props {
    entry: {
      target_kind: string;
      title: string;
      show_id: number | null;
      show_title: string | null;
      season: number | null;
      number: number | null;
      year?: number | null;
    };
  }

  let { entry }: Props = $props();
</script>

{#if entry.target_kind === "movie"}
  {entry.title}{#if entry.year}&nbsp;<span class="muted">({entry.year})</span>{/if}
{:else}
  <a href="/shows/{entry.show_id}">{entry.show_title}</a>
  <span class="ep-code">{episodeCode(entry.season, entry.number)}</span><span class="muted">{entry.title}</span>
{/if}
