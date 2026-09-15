import adapter from "@sveltejs/adapter-static";

/** The frontend sources live here. The manifest and the tool configs live at the repository root. */
const FRONTEND = "frontend";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  kit: {
    // The Rust server serves `frontend/build` and answers every unknown path with `index.html`,
    // so the app runs as a single-page app that talks to `/api`.
    adapter: adapter({ pages: `${FRONTEND}/build`, assets: `${FRONTEND}/build`, fallback: "index.html" }),
    files: {
      appTemplate: `${FRONTEND}/src/app.html`,
      assets: `${FRONTEND}/static`,
      errorTemplate: `${FRONTEND}/src/error.html`,
      hooks: {
        client: `${FRONTEND}/src/hooks.client`,
        server: `${FRONTEND}/src/hooks.server`,
        universal: `${FRONTEND}/src/hooks`,
      },
      lib: `${FRONTEND}/src/lib`,
      routes: `${FRONTEND}/src/routes`,
      serviceWorker: `${FRONTEND}/src/service-worker`,
    },
  },
};

export default config;
