<script lang="ts">
  import { page } from "$app/state";
</script>

<svelte:head><title>{page.status} · Watchkeep</title></svelte:head>

<div class="page-head">
  <div>
    <h1>{page.status}</h1>
    <p class="sub">{page.error?.message}</p>
  </div>
</div>
<p class="muted"><a href="/">← Dashboard</a></p>
