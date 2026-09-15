import { getJson } from "$lib/api.ts";
import type { ServerConfig } from "$lib/types.ts";
import type { LayoutLoad } from "./$types";

// The app is a single-page app: the Rust server serves the build and the pages load their data from `/api`.
export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ fetch }) => {
  const { data } = await getJson<ServerConfig>(fetch, "/api/config");
  return {
    images: data.image_base_url,
    syncConfigured: data.sync_configured,
    catalogConfigured: data.catalog_configured,
  };
};
