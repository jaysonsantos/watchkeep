import { getJson, statsUrl } from "$lib/api.ts";
import { browserTimezone } from "$lib/stats.ts";
import type { Statistics } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch }) => ({
  stats: (await getJson<Statistics>(fetch, statsUrl(browserTimezone()))).data,
});
