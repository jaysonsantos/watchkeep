# Changelog

Every notable change of Watchkeep. The entries come from the commit messages.

## [0.6.0](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.6.0) - 2026-09-20

### Documentation

- Show the web UI with a demo recording in the README (#32) ([d7e25b4](https://github.com/jaysonsantos/watchkeep/commit/d7e25b433789cd0966cb25ca94fa85fe97f0b0a9))

### Features

- **telemetry:** Name the database server on every statement span (#31) ([2a6a408](https://github.com/jaysonsantos/watchkeep/commit/2a6a4080c01667605904f6461a50a82d8cb88846))
- Show and set user ratings on media (#35) ([0bb3d05](https://github.com/jaysonsantos/watchkeep/commit/0bb3d05ab146d040412c84a105fbae174a7b65f7))

### Fixes

- **recommend:** Cap the cards of one genre set and one collection (#23) ([d6dde2f](https://github.com/jaysonsantos/watchkeep/commit/d6dde2f91b20abce9d494ea65640416ac4fda57a))

## [0.5.0](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.5.0) - 2026-09-18

### Continuous integration

- End the claude workflow files with one newline ([680203f](https://github.com/jaysonsantos/watchkeep/commit/680203fe75d06ee2171fb5aa2c0233520a853b06))

### Features

- **telemetry:** Make a span from every sqlx statement (#30) ([6ec3bbe](https://github.com/jaysonsantos/watchkeep/commit/6ec3bbe263eedcd3d9e56c1ac111c62159608db1))

## [0.4.0](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.4.0) - 2026-09-18

### Continuous integration

- Split the release into a tag workflow and a tag answer (#28) ([356eb8d](https://github.com/jaysonsantos/watchkeep/commit/356eb8ddd3bb1fbaa0c1a8adbb7bf99908c19001))

### Features

- **webhook:** Add a generic scrobble endpoint (#6) ([347398f](https://github.com/jaysonsantos/watchkeep/commit/347398f6b83523118489afa31402f7d3bbab95f7))

## [0.3.0](https://github.com/jaysonsantos/watchkeep/releases/tag/v0.3.0) - 2026-09-17

### Build

- **release:** Bump the version and write the changelog with git-cliff (#15) ([71f34f9](https://github.com/jaysonsantos/watchkeep/commit/71f34f92a496adb8aa4e5bde508eb1533ee06e63))

### Continuous integration

- Run the frontend tests and build in their own job (#21) ([9009e77](https://github.com/jaysonsantos/watchkeep/commit/9009e7784ddd4a5728c4e7688f17c4c5b6346c15))
- Run the backend tests in their own job (#20) ([1e3834f](https://github.com/jaysonsantos/watchkeep/commit/1e3834f1fbbed367a0df32c505274e6a8507a9d8))
- Run the linters in their own job (#19) ([dc20fb5](https://github.com/jaysonsantos/watchkeep/commit/dc20fb54acaddfea14a1482041795ab3626ed66f))
- **release:** Start a release from a manual workflow run (#22) ([7b342bf](https://github.com/jaysonsantos/watchkeep/commit/7b342bfed088d1536aa39aecab2b6a577153e95e))
- Add a check guard job for the split pipeline (#25) ([5caef60](https://github.com/jaysonsantos/watchkeep/commit/5caef601af1c7cf8ffe854cc9d4a1f596e42431b))
- Push the release commit with a personal access token (#26) ([7f916c9](https://github.com/jaysonsantos/watchkeep/commit/7f916c95e768aa2af0f8ae407cee49b6c6cc75f0))
- Push the release with the deploy key (#27) ([1dae854](https://github.com/jaysonsantos/watchkeep/commit/1dae8549a5ea28a54546ac2aca3fc716f6c92e23))

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


