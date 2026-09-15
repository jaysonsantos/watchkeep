import { getJson } from "$lib/api.ts";
import type { WebhookLogEntry } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch }) => ({
  events: (await getJson<WebhookLogEntry[]>(fetch, "/api/webhooks")).data,
});
