import { error } from "@sveltejs/kit";
import { act } from "$lib/form-actions.server.ts";
import { showPage } from "$lib/server/pages.ts";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = async ({ locals, params }) => {
  const data = await showPage(locals.ctx, Number(params.id));
  if (!data) error(404, "Show not found");
  return data;
};

export const actions: Actions = { act };
