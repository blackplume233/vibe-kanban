#!/usr/bin/env bash
# 从锁定提交构建共享 Rust 根组件，使用 Chromium 验证静态站点。
set -euo pipefail

tools_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(git -C "$tools_dir" rev-parse --show-toplevel)"
goal="$(dirname -- "$(dirname -- "$tools_dir")")"
source "$goal/harness/env.sh"

ref_repo="$repo/../nsh-ue-ui/vendor/nsh-src"
revision=12af3d778b588c2837267ff6aaed678eb5ad72a2
triple=wasm32-unknown-unknown
target="$repo/target/goal-m1-web"
mkdir -p "$target"
snapshot="$(mktemp -d "$target/snapshot.XXXXXX")"
trap 'rm -r -- "$snapshot"' EXIT
git -C "$ref_repo" archive "$revision" | tar -xf - -C "$snapshot"
web_crate="$snapshot/crates/nsh-platform-web"
cp -- "$tools_dir/vibe-own-root.rs" "$web_crate/src/"
cat >>"$web_crate/src/lib.rs" <<'RUST'

mod vibe_own_smoke {
    include!("vibe-own-root.rs");
    use nsh_core::element::Root;
    crate::rust_app!(vec![nsh_base::registry()], Root::of::<Smoke>());
}

pub use vibe_own_smoke::nsh_app_init;
RUST

export CARGO_TARGET_DIR="$(cygpath -am "$target/build")"
log="$(mktemp "$repo/../.vk-m1-web-build.XXXXXX.log")"
printf '构建日志 %s\n' "$log"
{
    printf 'NSH 提交 %s\n' "$revision"
    rustc +1.99.1 -vV
    cd "$snapshot"
    cargo +1.99.1 build --locked --release --target "$triple" -p nsh-platform-web --no-default-features
} >"$log" 2>&1 || { tail -n 45 "$log"; exit 1; }

site="$(mktemp -d "$target/site.XXXXXX")"
tail -n 12 "$log"

cp -- "$CARGO_TARGET_DIR/$triple/release/nsh_platform_web.wasm" "$site/nsh.wasm"
cp -- "$tools_dir/m1-web-index.html" "$site/index.html"
cp -- "$web_crate/web/nsh.js" "$site/"
mkdir -p "$site/browser"
cp -- "$web_crate/web/browser/"*.js "$site/browser/"
printf '静态站点 %s\n' "$site"
python "$tools_dir/m1-web-browser.py" "$site" "$goal/evidence/m1-web-browser.png"
