#!/usr/bin/env bash
# Build the pinned NSH targets and run both against one isolated vibe server fixture.
set -euo pipefail

tools_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(git -C "$tools_dir" rev-parse --show-toplevel)
goal=$(dirname -- "$(dirname -- "$tools_dir")")
source <(tr -d '\r' < "$goal/harness/env.sh")
cd "$repo"

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-F:/vk-targets/m1}
toolchain=${M1_RUST_TOOLCHAIN:-1.99.1}
target=$(cygpath -am "$CARGO_TARGET_DIR")
manifest="$repo/crates/nsh-app/Cargo.toml"
mode=${1:-both}
[[ $# -le 1 ]] || { echo "Usage: $0 [windows|web|both]" >&2; exit 2; }
case "$mode" in windows|web|both) ;; *) echo "Usage: $0 [windows|web|both]" >&2; exit 2;; esac
mkdir -p "$target"
run_logged() {
    local log=$1
    shift
    if "$@" >"$log" 2>&1; then
        tail -n 4 "$log"
    else
        tail -n 55 "$log" >&2
        return 1
    fi
}

if [[ $mode != web ]]; then
    run_logged "$target/nsh-native-build.log" cargo "+$toolchain" build --locked --release --manifest-path "$manifest"
fi
if [[ $mode != windows ]]; then
    run_logged "$target/nsh-web-build.log" cargo "+$toolchain" build --locked --release --lib --target wasm32-unknown-unknown --manifest-path "$manifest"
    bindgen=${NSH_WASM_BINDGEN:-"$target/tools/bin/wasm-bindgen.exe"}
    if [[ ! -x $bindgen ]]; then
        run_logged "$target/bindgen-build.log" cargo "+$toolchain" install wasm-bindgen-cli --version 0.2.129 --locked --root "$target/tools"
    fi
    ref_repo=${NSH_REFERENCE_REPO:-"$repo/../nsh-ue-ui/vendor/nsh-src"}
    revision=12af3d778b588c2837267ff6aaed678eb5ad72a2
    site="$target/m1-site"
    mkdir -p "$site"
    git -C "$ref_repo" archive "$revision" crates/nsh-platform-web/web | tar -xf - -C "$site" --strip-components=3
    cp -- "$tools_dir/m1-web-index.html" "$site/index.html"
    "$bindgen" --target web --out-name nsh-bindgen --out-dir "$site" "$target/wasm32-unknown-unknown/release/vibe_own_nsh.wasm"
    python "$tools_dir/m1-glue.py" "$site/nsh-bindgen.js"
fi
run_logged "$target/server-build.log" cargo "+$toolchain" build --locked --bin server
python "$tools_dir/m1-runtime.py" "$mode" "$(cygpath -am "$repo")" "$target" "$(cygpath -am "$goal/evidence")"
