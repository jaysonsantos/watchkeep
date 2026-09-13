import type { LayoutServerLoad } from "./$types";

export const load: LayoutServerLoad = ({ locals }) => ({ images: locals.ctx.config.imageBaseUrl });
