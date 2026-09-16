{
  description = "Watchkeep: Rust server with sqlx and Postgres, SvelteKit web UI";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              # backend/
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt
              pkgs.rust-analyzer
              pkgs.sqlx-cli
              # aws-lc-sys, the TLS provider, compiles C code
              pkgs.cmake
              pkgs.pkg-config
              # frontend/
              pkgs.nodejs_24
              pkgs.pnpm
              # psql, for the dev databases
              pkgs.postgresql
              # the changelog and the version bump (scripts/release.sh)
              pkgs.git-cliff
              # the client of the tokio-console layer (CONSOLE_SUBSCRIBER=<port>)
              pkgs.tokio-console
              # linters, run together by `prek run --all-files` (.pre-commit-config.yaml)
              pkgs.prek
              pkgs.taplo
              pkgs.shellcheck
              pkgs.hadolint
              pkgs.nixfmt
              pkgs.typos
            ];

            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            # Every SQL statement is a query macro; the offline data in backend/.sqlx
            # is the fallback when the two WATCHKEEP_*_DATABASE_URL variables are absent.
            RUST_LOG = "info";
          };
        }
      );
    };
}
