import { redirect } from "@sveltejs/kit";
import { addFromForm, addPage } from "$lib/server/pages.ts";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals, url }) => addPage(locals.ctx, url.searchParams);

export const actions: Actions = {
  add: async ({ locals, request }) => redirect(303, await addFromForm(locals.ctx, await request.formData())),
};
