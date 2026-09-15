import { listParams, loadList } from "$lib/api.ts";
import type { MovieView } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch, url }) => {
  const list = await loadList<MovieView>(
    fetch,
    "/api/movies",
    listParams(url.searchParams),
    url.searchParams.get("page"),
  );
  return { movies: list.items, total: list.total, list: list.state };
};
