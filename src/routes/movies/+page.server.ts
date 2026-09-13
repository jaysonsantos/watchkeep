import { act } from "$lib/form-actions.server.ts";
import { moviesPage } from "$lib/server/pages.ts";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals, url }) => moviesPage(locals.ctx, url.searchParams);

export const actions: Actions = { act };
