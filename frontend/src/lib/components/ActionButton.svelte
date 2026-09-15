<script lang="ts">
  import { invalidateAll } from "$app/navigation";
  import { type ActionName, act } from "$lib/api.ts";
  import type { Id } from "$lib/types.ts";

  interface Props {
    action: ActionName;
    /** The row the button acts on. `sync` has none. */
    id?: Id | null;
    label: string;
    primary?: boolean;
    small?: boolean;
    fields?: Record<string, string | number>;
  }

  let { action, id = null, label, primary = false, small = false, fields = {} }: Props = $props();

  let busy = $state(false);

  // Call the API and reload the data of the page, so the page stays the same.
  async function run() {
    busy = true;
    try {
      await act(action, id, fields);
      await invalidateAll();
    } catch (cause) {
      window.alert(cause instanceof Error ? cause.message : String(cause));
    } finally {
      busy = false;
    }
  }
</script>

<button type="button" class:primary class:small disabled={busy} onclick={run}>{busy ? "…" : label}</button>
