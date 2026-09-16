# Stage 1: the web UI. It is the same for every target, so it builds on the
# platform of the builder. The Rust server serves the static build.
FROM --platform=$BUILDPLATFORM node:24-alpine AS web
RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY svelte.config.js vite.config.ts tsconfig.json ./
COPY frontend ./frontend
RUN pnpm run build

# Stage 2: the server binary, cross-compiled on the platform of the builder for the
# platform of the image, so that an arm64 image needs no emulation. The query macros
# read the offline data in `.sqlx`, so the build needs no database. `catalog/schema.sql`
# is embedded at build time. The TLS provider (aws-lc-sys) compiles C code with cmake.
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS server
ARG TARGETARCH
# hadolint ignore=DL3008
RUN apt-get update \
  && apt-get install -y --no-install-recommends cmake \
  && if [ "$TARGETARCH" = "arm64" ] && [ "$(dpkg --print-architecture)" != "arm64" ]; then \
    apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross; \
  elif [ "$TARGETARCH" = "amd64" ] && [ "$(dpkg --print-architecture)" != "amd64" ]; then \
    apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu g++-x86-64-linux-gnu libc6-dev-amd64-cross; \
  fi \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# `.cargo/config.toml` carries the `tokio_unstable` flag that the named tasks need.
COPY .cargo ./.cargo
COPY .sqlx ./.sqlx
COPY catalog ./catalog
COPY backend ./backend
ENV SQLX_OFFLINE=true \
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
  CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ \
  AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
  CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
  CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++ \
  AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar
RUN case "$TARGETARCH" in \
    amd64) target=x86_64-unknown-linux-gnu ;; \
    arm64) target=aarch64-unknown-linux-gnu ;; \
    *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
  esac \
  && rustup target add "$target" \
  && cargo build --release --locked --target "$target" \
  && cp "target/$target/release/watchkeep" /watchkeep

# Stage 3: a distroless runtime. It has glibc, libgcc, CA certificates, and a
# time zone database, and nothing else: no shell, no package manager.
# The health check is the binary itself, because there is no curl.
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=server /watchkeep /usr/local/bin/watchkeep
COPY --from=web /app/frontend/build ./frontend/build
# The numeric id of `nonroot`. Kubernetes cannot verify `runAsNonRoot` for a user name.
USER 65532:65532
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=5s CMD ["watchkeep", "health"]
ENTRYPOINT ["watchkeep"]
CMD ["serve"]
