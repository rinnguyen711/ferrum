# syntax=docker/dockerfile:1.7

# ─── Stage 1: build UI ─────────────────────────────────────────────
FROM node:22-alpine AS ui
RUN corepack enable
WORKDIR /ui
COPY ui/package.json ui/pnpm-lock.yaml ./
# `packageManager: pnpm@10.x` in package.json pins the version corepack uses.
# CI=true disables interactive prompts; pnpm 10 ignores its own ignored-scripts
# gate when the package is the workspace root and esbuild is hoisted.
ENV CI=true
RUN pnpm install --frozen-lockfile --ignore-scripts \
 && pnpm rebuild esbuild
COPY ui/ ./
RUN pnpm build

# ─── Stage 2: build Rust binary ────────────────────────────────────
FROM rust:1.88-slim-bookworm AS rust
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
# Cache only the registry (deps); not /src/target — the target cache mount
# survives across builds and Docker can't reliably bust it when source
# changes, leading to stale binaries shipped into the image.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p rustapi \
 && cp target/release/rustapi /tmp/rustapi

# ─── Stage 3: runtime ──────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=rust /tmp/rustapi /usr/local/bin/rustapi
COPY --from=ui /ui/dist /app/studio
ENV RUSTAPI_BIND=0.0.0.0:8080 \
    RUSTAPI_STUDIO_DIR=/app/studio \
    RUSTAPI_LOG=info
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/usr/local/bin/rustapi"]
