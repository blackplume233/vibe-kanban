#!/usr/bin/env bash
# Build the showcase with the existing NSH app manifest and lock in a staging directory.
set -euo pipefail

tools_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(git -C "$tools_dir" rev-parse --show-toplevel)
goal=$(dirname -- "$(dirname -- "$tools_dir")")
source <(tr -d '\r' < "$goal/harness/env.sh")
cd "$repo"

toolchain=nightly-2026-08-01
revision=12af3d778b588c2837267ff6aaed678eb5ad72a2
target=$(cygpath -am "${CARGO_TARGET_DIR:-F:/vk-targets/m3}")
ref_repo=${NSH_REFERENCE_REPO:-"$repo/../nsh-ue-ui/vendor/nsh-src"}
bindgen=${NSH_WASM_BINDGEN:-F:/vk-targets/m1/tools/bin/wasm-bindgen.exe}
mode=${1:-both}
case "$mode" in windows|web|both) ;; *) echo "Usage: $0 [windows|web|both]" >&2; exit 2;; esac

node crates/nsh-app/design/generate-tokens.mjs --check
staging="$target/showcase-app"
mkdir -p "$staging/src" "$staging/design"
cp -- crates/nsh-app/{Cargo.toml,Cargo.lock,build.rs} "$staging/"
cp -- crates/nsh-app/src/design_showcase.rs "$staging/src/"
cp -r -- crates/nsh-app/src/design_showcase "$staging/src/"
cp -r -- crates/nsh-app/design/fonts crates/nsh-app/design/tokens.json crates/nsh-app/design/theme-*.generated.json "$staging/design/"
cat > "$staging/src/main.rs" <<'RUST'
#![cfg_attr(windows, windows_subsystem = "windows")]
use nsh_platform_windows::{AppOptions, AppSource, run_app, set_ui_sets};
use vibe_own_nsh::design_showcase::{ShowcaseOptions, root};
fn main() -> impl std::process::Termination {
    let name = std::env::args().nth(1).expect("showcase fixture");
    let options = ShowcaseOptions::fixture(&name).expect("known showcase fixture");
    set_ui_sets(vec![nsh_base::registry()]);
    run_app(AppSource::Rust(root(options)), AppOptions {
        title: format!("vibe-own | M3 {name}"),
        appearance: options.appearance(),
        size: options.size(),
        ..AppOptions::default()
    })
}
RUST
cat > "$staging/src/lib.rs" <<'RUST'
pub mod design_showcase;

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::Cell;
    use wasm_bindgen::prelude::wasm_bindgen;
    use crate::design_showcase::{ShowcaseOptions, root};
    thread_local! { static OPTIONS: Cell<ShowcaseOptions> = Cell::new(ShowcaseOptions::default()); }
    #[wasm_bindgen]
    pub fn configure_showcase(name: &str) {
        OPTIONS.set(ShowcaseOptions::fixture(name).expect("known showcase fixture"));
    }
    nsh_platform_web::rust_app!(vec![nsh_base::registry()], root(OPTIONS.get()));
}
RUST
export CARGO_TARGET_DIR="$target"
export CARGO_PROFILE_DEV_DEBUG=0
build() {
    local log=$1
    shift
    if cargo "+$toolchain" build --offline --locked --manifest-path "$staging/Cargo.toml" "$@" > "$log" 2>&1; then
        tail -n 2 "$log"
    else
        tail -n 70 "$log" >&2
        return 1
    fi
}

if [[ $mode != web ]]; then
    build "$target/showcase-native-build.log" --bin vibe-own
fi
if [[ $mode != windows ]]; then
    build "$target/showcase-web-build.log" --lib --target wasm32-unknown-unknown
    site="$target/showcase-site"
    mkdir -p "$site"
    git -C "$ref_repo" archive "$revision" crates/nsh-platform-web/web | tar -xf - -C "$site" --strip-components=3
    "$bindgen" --target web --out-name nsh-bindgen --out-dir "$site" "$target/wasm32-unknown-unknown/debug/vibe_own_nsh.wasm"
    python "$tools_dir/m1-glue.py" "$site/nsh-bindgen.js"
    cat > "$site/index.html" <<'HTML'
<!doctype html>
<html lang="zh-CN">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>vibe-own | NSH M3</title>
<div id="app" style="position:fixed;inset:0"></div>
<script type="module">
import { createPlayer } from "./nsh.js";
import { configure_showcase } from "./nsh-bindgen.js";
const fixture = new URLSearchParams(location.search).get("fixture") || "light-comfortable";
const player = await createPlayer(document.querySelector("#app"), {
  wasm: "./nsh-bindgen_bg.wasm", glue: "./nsh-bindgen.js"
});
configure_showcase(fixture);
player.runRust();
</script>
</html>
HTML
fi
python "$tools_dir/m3-showcase.py" "$mode" "$target" "$(cygpath -am "$goal/evidence/m3-showcase")"
