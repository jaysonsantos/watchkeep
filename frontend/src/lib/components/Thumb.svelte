<script lang="ts">
  let { images, path }: { images: string; path: string | null | undefined } = $props();
</script>

{#if path}
  <img class="thumb" src="{images}w92{path}" srcset="{images}w185{path} 2x" alt="" loading="lazy" />
{:else}
  <span class="thumb empty"></span>
{/if}
