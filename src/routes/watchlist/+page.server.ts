import { act } from "$lib/form-actions.server.ts";
import { watchlistPage } from "$lib/server/pages.ts";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals }) => watchlistPage(locals.ctx);

export const actions: Actions = { act };
