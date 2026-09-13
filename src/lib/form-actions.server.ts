import { applyAction } from "./server/pages.ts";

/** The `act` form action of every page that has watched, watchlist, or progress buttons. */
export async function act({ locals, request }: { locals: App.Locals; request: Request }): Promise<void> {
  await applyAction(locals.ctx, await request.formData());
}
