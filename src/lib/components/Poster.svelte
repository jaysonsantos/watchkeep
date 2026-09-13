<script lang="ts">
  import type { Snippet } from "svelte";
  import { initials } from "$lib/format.ts";

  interface Props {
    images: string;
    path: string | null | undefined;
    title: string;
    badge?: Snippet;
    bar?: Snippet;
  }

  let { images, path, title, badge, bar }: Props = $props();
</script>

<div class="poster">
  {#if path}
    <img src="{images}w185{path}" srcset="{images}w342{path} 2x" alt="" loading="lazy" />
  {:else}
    <span>{initials(title)}</span>
  {/if}
  {@render badge?.()}
  {@render bar?.()}
</div>
