import type { AppContext } from "$lib/server/app.ts";

declare global {
  namespace App {
    interface Locals {
      ctx: AppContext;
    }
  }
}

export {};
