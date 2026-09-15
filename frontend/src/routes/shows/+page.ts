import { listParams, loadList } from "$lib/api.ts";
import type { ShowListItem } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch, url }) => {
  const list = await loadList<ShowListItem>(
    fetch,
    "/api/shows",
    listParams(url.searchParams),
    url.searchParams.get("page"),
  );
  return { shows: list.items, total: list.total, list: list.state };
};
