#!/usr/bin/env bash
# Run the maintained CI commands locally. The Web toolchain is needed for `nsh`.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

backend_schema() {
  pnpm run generate-types:check
  pnpm run prepare-db:check
}

backend_clippy() {
  cargo metadata --locked --format-version 1 >/dev/null
  cargo fmt --all -- --check
  pnpm run backend:check
  pnpm run backend:lint
}

nsh_checks() {
  local manifest=crates/nsh-app/Cargo.toml
  cargo fmt --manifest-path "$manifest" -- --check
  cargo clippy --locked --manifest-path "$manifest" --target wasm32-unknown-unknown -- -D warnings
  pnpm run build:web
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      cargo clippy --locked --manifest-path "$manifest" -- -D warnings
      pnpm run build:windows ;;
  esac
}

if [[ $# -eq 0 ]]; then
  set -- backend nsh npm-launcher
fi

for job in "$@"; do
  case "$job" in
    backend)
      backend_schema
      backend_clippy
      cargo test --locked --workspace ;;
    backend-schema) backend_schema ;;
    backend-clippy) backend_clippy ;;
    backend-test) cargo test --locked --workspace ;;
    nsh) nsh_checks ;;
    npm-launcher)
      pnpm run build:npx-cli
      node npx-cli/bin/cli.js --help ;;
    backend-remote)
      # Explicit historical workspace check. Its Cargo graph requires private access.
      for manifest in crates/remote/Cargo.toml crates/relay-tunnel/Cargo.toml; do
        cargo metadata --locked --manifest-path "$manifest" --format-version 1 >/dev/null
        cargo fmt --all --manifest-path "$manifest" -- --check
        cargo clippy --locked --manifest-path "$manifest" --all-targets -- -D warnings
      done
      pnpm run remote:generate-types:check
      pnpm run remote:prepare-db:check ;;
    *)
      echo 'Usage: scripts/ci-local.sh [backend|backend-schema|backend-clippy|backend-test|nsh|npm-launcher|backend-remote] ...' >&2
      exit 2 ;;
  esac
done
