import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

// `pnpm dev` proxies the API to a running Rust server. Set WATCHKEEP_API_URL to change the target.
const api = process.env.WATCHKEEP_API_URL ?? "http://127.0.0.1:8484";

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: { "/api": api, "/webhook": api, "/healthz": api },
  },
});
