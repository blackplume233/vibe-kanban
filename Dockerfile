# syntax=docker/dockerfile:1.6

FROM node:22-bookworm-slim AS node
FROM rust:1.93-slim-bookworm AS build-base

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_TARGET_DIR=/app/target
ENV SQLX_OFFLINE=true

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential ca-certificates clang git libclang-dev libssl-dev llvm pkg-config \
  && rm -rf /var/lib/apt/lists/*

COPY rust-toolchain.toml ./
RUN cargo --version

FROM build-base AS web-builder
COPY --from=node /usr/local/bin/node /usr/local/bin/node
ENV CC_wasm32_unknown_unknown=clang
ENV AR_wasm32_unknown_unknown=llvm-ar
COPY package.json ./
COPY scripts/nsh.mjs scripts/nsh.mjs
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    node scripts/nsh.mjs install && node scripts/nsh.mjs install-web

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY assets/ assets/
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    node scripts/nsh.mjs web /out

FROM build-base AS builder

ARG POSTHOG_API_KEY=""
ARG POSTHOG_API_ENDPOINT=""
ARG SENTRY_DSN=""
ARG VK_SHARED_API_BASE=""

ENV POSTHOG_API_KEY=${POSTHOG_API_KEY}
ENV POSTHOG_API_ENDPOINT=${POSTHOG_API_ENDPOINT}
ENV SENTRY_DSN=${SENTRY_DSN}
ENV VK_SHARED_API_BASE=${VK_SHARED_API_BASE}

COPY Cargo.toml Cargo.lock ./
COPY crates/api-types/ crates/api-types/
COPY crates/client-info/ crates/client-info/
COPY crates/db/ crates/db/
COPY crates/desktop-bridge/ crates/desktop-bridge/
COPY crates/deployment/ crates/deployment/
COPY crates/embedded-ssh/ crates/embedded-ssh/
COPY crates/executors/ crates/executors/
COPY crates/git/ crates/git/
COPY crates/git-host/ crates/git-host/
COPY crates/local-deployment/ crates/local-deployment/
COPY crates/mcp/ crates/mcp/
COPY crates/preview-proxy/ crates/preview-proxy/
COPY crates/relay-client/ crates/relay-client/
COPY crates/relay-control/ crates/relay-control/
COPY crates/relay-hosts/ crates/relay-hosts/
COPY crates/relay-protocol/ crates/relay-protocol/
COPY crates/relay-types/ crates/relay-types/
COPY crates/relay-tunnel-core/ crates/relay-tunnel-core/
COPY crates/relay-webrtc/ crates/relay-webrtc/
COPY crates/relay-ws/ crates/relay-ws/
COPY crates/remote-info/ crates/remote-info/
COPY crates/review/ crates/review/
COPY crates/server/ crates/server/
COPY crates/services/ crates/services/
COPY crates/trusted-key-auth/ crates/trusted-key-auth/
COPY crates/utils/ crates/utils/
COPY crates/workspace-manager/ crates/workspace-manager/
COPY crates/worktree-manager/ crates/worktree-manager/
COPY crates/ws-bridge/ crates/ws-bridge/
COPY assets/ assets/

RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=workspace-target,target=/app/target \
    cargo build --locked --release --bin server \
 && cp /app/target/release/server /usr/local/bin/server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    openssh-client \
    tini \
    wget \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --create-home --uid 10001 appuser

WORKDIR /repos

COPY --from=builder /usr/local/bin/server /usr/local/bin/server
COPY --from=web-builder /out/vibe-own/ /usr/local/bin/web/

RUN mkdir -p /repos \
  && chown -R appuser:appuser /repos

USER appuser

ENV HOST=0.0.0.0
ENV BACKEND_PORT=3000

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/bin/sh", "-c", "wget --spider -q http://127.0.0.1:${BACKEND_PORT:-3000}/api/health"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/server"]
