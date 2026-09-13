import { act } from "$lib/form-actions.server.ts";
import { historyPage } from "$lib/server/pages.ts";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals, url }) => historyPage(locals.ctx, url.searchParams);

export const actions: Actions = { act };
