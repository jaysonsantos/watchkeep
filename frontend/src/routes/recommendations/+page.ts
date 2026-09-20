import { getJson } from "$lib/api.ts";
import { QUERY, recommendationsApiUrl, recommendInputOf } from "$lib/lists.ts";
import type { Recommendations } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch, parent, url }) => {
  const input = recommendInputOf(url.searchParams.get(QUERY.input));
  const { catalogConfigured } = await parent();
  if (!catalogConfigured) return { recommendations: null, input };
  const { data } = await getJson<Recommendations>(fetch, recommendationsApiUrl(input));
  return { recommendations: data, input };
};
