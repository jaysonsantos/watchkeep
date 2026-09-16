import { getJson } from "$lib/api.ts";
import type { Recommendations } from "$lib/types.ts";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch, parent }) => {
  const { catalogConfigured } = await parent();
  if (!catalogConfigured) return { recommendations: null };
  const { data } = await getJson<Recommendations>(fetch, "/api/recommendations");
  return { recommendations: data };
};
