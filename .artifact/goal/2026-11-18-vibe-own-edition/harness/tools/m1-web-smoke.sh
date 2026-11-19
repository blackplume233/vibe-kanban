#!/usr/bin/env bash
# Build and capture the release Web application against the local fixture.
set -euo pipefail
exec bash "$(dirname -- "${BASH_SOURCE[0]}")/m1-gate.sh" web "$@"
