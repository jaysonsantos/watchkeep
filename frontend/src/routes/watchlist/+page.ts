import { getJson } from "$lib/api.ts";
import type { WatchlistItem } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch }) => ({
  items: (await getJson<WatchlistItem[]>(fetch, "/api/watchlist")).data,
});
