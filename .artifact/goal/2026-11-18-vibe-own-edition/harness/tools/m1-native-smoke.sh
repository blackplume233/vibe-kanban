#!/usr/bin/env bash
# 从只读参考仓库提取 M1 锁定源码，构建并捕获最小原生应用。
set -euo pipefail

tools_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(git -C "$tools_dir" rev-parse --show-toplevel)"
goal="$(dirname -- "$(dirname -- "$tools_dir")")"
source "$goal/harness/env.sh"

ref_repo="$repo/../nsh-ue-ui/vendor/nsh-src"
revision=12af3d778b588c2837267ff6aaed678eb5ad72a2
target="$repo/target/goal-m1-native"
mkdir -p "$target"
snapshot="$(mktemp -d "$target/snapshot.XXXXXX")"
trap 'rm -r -- "$snapshot"' EXIT

git -C "$ref_repo" archive "$revision" | tar -xf - -C "$snapshot"
example_dir="$snapshot/crates/nsh-platform-windows/examples"
mkdir -p "$example_dir"
cp -- "$tools_dir/vibe-own-smoke.rs" "$example_dir/"

export CARGO_TARGET_DIR="$(cygpath -am "$target/build")"
log="$(mktemp "$repo/../.vk-m1-native-build.XXXXXX.log")"
printf '构建日志 %s\n' "$log"
{
    printf 'NSH 提交 %s\n' "$revision"
    rustc +1.99.1 -vV
    cd "$snapshot"
    cargo +1.99.1 build --locked -p nsh-platform-windows --no-default-features --example vibe-own-smoke
} >"$log" 2>&1 || { tail -n 45 "$log"; exit 1; }
tail -n 12 "$log"

powershell.exe -NoProfile -NonInteractive -File "$(cygpath -aw "$tools_dir/m1-native-window.ps1")" \
    -Executable "$(cygpath -aw "$target/build/debug/examples/vibe-own-smoke.exe")" \
    -Screenshot "$(cygpath -aw "$goal/evidence/m1-native-window.png")"
