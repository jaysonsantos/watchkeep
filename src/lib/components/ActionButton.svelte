<script lang="ts">
  import { enhance } from "$app/forms";
  import { page } from "$app/state";

  interface Props {
    action: string;
    id: number;
    label: string;
    primary?: boolean;
    small?: boolean;
    fields?: Record<string, string | number>;
  }

  let { action, id, label, primary = false, small = false, fields = {} }: Props = $props();

  // Post to the `act` action of the current page, and keep its query string so the page stays the same.
  const target = $derived.by(() => {
    const params = new URLSearchParams(page.url.search);
    for (const name of [...params.keys()]) if (name.startsWith("/")) params.delete(name);
    const query = params.toString();
    return query ? `?${query}&/act` : "?/act";
  });
</script>

<form class="inline" method="POST" action={target} use:enhance>
  <input type="hidden" name="action" value={action} />
  <input type="hidden" name="id" value={id} />
  {#each Object.entries(fields) as [name, value] (name)}
    <input type="hidden" {name} {value} />
  {/each}
  <button type="submit" class:primary class:small>{label}</button>
</form>
