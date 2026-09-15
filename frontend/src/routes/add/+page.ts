import { getJson } from "$lib/api.ts";
import type { CatalogMovie, CatalogShow, SearchResult, SearchResults } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch, url, parent }) => {
  const query = (url.searchParams.get("q") ?? "").trim();
  const { catalogConfigured } = await parent();
  let movies: SearchResult<CatalogMovie>[] = [];
  let shows: SearchResult<CatalogShow>[] = [];
  if (catalogConfigured && query.length >= 2) {
    const { data } = await getJson<SearchResults>(fetch, `/api/search?q=${encodeURIComponent(query)}`);
    movies = data.movies;
    shows = data.shows;
  }
  return { query, movies, shows, error: url.searchParams.get("error") };
};
