import { showsPage } from "$lib/server/pages.ts";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals, url }) => showsPage(locals.ctx, url.searchParams);
