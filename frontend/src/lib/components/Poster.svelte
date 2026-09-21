<script lang="ts">
  import type { Snippet } from "svelte";
  import { initials } from "$lib/format.ts";

  interface Props {
    images: string;
    path: string | null | undefined;
    title: string;
    /** Where the image goes on a click. The badges stay outside the link, so a badge can hold a button. */
    href?: string | null;
    badge?: Snippet;
    bar?: Snippet;
  }

  let { images, path, title, href = null, badge, bar }: Props = $props();
</script>

{#snippet art()}
  {#if path}
    <img src="{images}w185{path}" srcset="{images}w342{path} 2x" alt="" loading="lazy" />
  {:else}
    <span>{initials(title)}</span>
  {/if}
{/snippet}

<div class="poster">
  {#if href}
    <!-- The title of the card is the link that a keyboard and a screen reader get. -->
    <a class="art" {href} tabindex="-1" aria-hidden="true">{@render art()}</a>
  {:else}
    {@render art()}
  {/if}
  {@render badge?.()}
  {@render bar?.()}
</div>
