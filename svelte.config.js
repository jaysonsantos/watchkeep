import adapter from "@sveltejs/adapter-node";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  kit: {
    // The server reads WATCHKEEP_HTTP_ORIGIN and the other adapter variables. The adapter rejects unknown
    // variables with its prefix, so the prefix cannot be WATCHKEEP_. `main.ts serve` maps WATCHKEEP_HOST and WATCHKEEP_PORT.
    adapter: adapter({ envPrefix: "WATCHKEEP_HTTP_" }),
    // Plex posts multipart forms without an Origin header, so the built-in check must be off.
    // `src/hooks.server.ts` checks the origin of form posts to the UI pages instead.
    csrf: { trustedOrigins: ["*"] },
  },
};

export default config;
