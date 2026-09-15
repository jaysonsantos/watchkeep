<script lang="ts">
  import { percent } from "$lib/format.ts";

  let { value, total }: { value: number; total: number | null } = $props();

  const width = $derived(percent(value, total));
</script>

<div class="bar" title="{width}%"><span style:width="{width}%"></span></div>
