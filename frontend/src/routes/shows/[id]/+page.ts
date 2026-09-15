import { error } from "@sveltejs/kit";
import { ApiError, getJson } from "$lib/api.ts";
import type { ShowPage } from "$lib/types.ts";
import type { PageLoad } from "./$types";

/** Row ids are UUIDs. Other paths get a 404 before any request. */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export const load: PageLoad = async ({ fetch, params }) => {
  const id = params.id.toLowerCase();
  if (!UUID.test(id)) error(404, "Show not found");
  try {
    const { data } = await getJson<ShowPage>(fetch, `/api/shows/${id}`);
    const { episodes, on_watchlist, ...show } = data;
    return { show, episodes, onWatchlist: on_watchlist };
  } catch (cause) {
    if (cause instanceof ApiError && cause.status === 404) error(404, "Show not found");
    throw cause;
  }
};
