import { act } from "$lib/form-actions.server.ts";
import { dashboardPage } from "$lib/server/pages.ts";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals }) => dashboardPage(locals.ctx);

export const actions: Actions = { act };
