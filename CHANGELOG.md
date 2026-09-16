# Changelog

Every notable change of Watchkeep. The entries come from the commit messages.

## Unreleased

### Features

- **recommendations:** Add a recommendations page (#12) ([3d89c1b](https://github.com/jaysonsantos/watchkeep/commit/3d89c1b71a49e0654b5e68f07b9a27592fe5eebd))

## [0.2.0](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.2.0) - 2026-09-16

### Features

- **stats:** Add a statistics page (#11) ([f0fe045](https://github.com/jaysonsantos/watchkeep/commit/f0fe045ef2495fa2a45ec49d994b7754bf8ea487))
- **ui:** Show a now-playing widget on the dashboard (#10) ([2debc65](https://github.com/jaysonsantos/watchkeep/commit/2debc6500141a884d58acec17d010941448dd812))
- **telemetry:** Trace and measure the server with OpenTelemetry (#8) ([3fce17e](https://github.com/jaysonsantos/watchkeep/commit/3fce17e078ea5d4c6b7e2e569e8c8f41f4e7e067))

### Fixes

- **scrobble:** Keep a watched item out of the in-progress list ([7d67a6c](https://github.com/jaysonsantos/watchkeep/commit/7d67a6cb014c238fe124ff4d7642ceeea60687b5))

## [0.1.2](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.1.2) - 2026-09-15

### Fixes

- **docker:** Run as the numeric nonroot user ([63bbdc2](https://github.com/jaysonsantos/watchkeep/commit/63bbdc2b8d4fb2af7e4a6e388fb34a623d167e67))

## [0.1.1](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.1.1) - 2026-09-15

### Fixes

- **storage:** Create ids with the pg_uuidv7 extension (**breaking**) ([f3495c0](https://github.com/jaysonsantos/watchkeep/commit/f3495c01c5711ad2c60acbf8310d529caaec5bf3))
- **catalog:** Match the schema of the central TMDB database ([ab7c892](https://github.com/jaysonsantos/watchkeep/commit/ab7c8925de1c8ffaa3348b81805d156bc7e0a08f))

## [0.1.0](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.1.0) - 2026-09-15

### Build

- Add the nix flake, prek hooks, and Biome ([f538ab7](https://github.com/jaysonsantos/watchkeep/commit/f538ab77c267f2d53066a7af2183fae4375ebc6f))

### Continuous integration

- Run the checks on pull requests and push the image on tags ([426a024](https://github.com/jaysonsantos/watchkeep/commit/426a0249fa1fe8d6b1175c640856c818ee8dc660))

### Features

- Port the backend to Rust with axum and sqlx (**breaking**) ([226ef73](https://github.com/jaysonsantos/watchkeep/commit/226ef73f5e4fa65cd2e99b67e08d517a3ff66c93))

### Fixes

- **scripts:** Pass DATABASE_URL to sqlx-cli ([2fe7f67](https://github.com/jaysonsantos/watchkeep/commit/2fe7f67f201de70d652c84cdb408e3cc7e65824f))

### Refactoring

- **storage:** Start from one clean schema (**breaking**) ([170268f](https://github.com/jaysonsantos/watchkeep/commit/170268ff184855335e4a6ca115c4b41422f4b3a3))


