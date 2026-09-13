import { webhooksPage } from "$lib/server/pages.ts";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ locals }) => webhooksPage(locals.ctx);
