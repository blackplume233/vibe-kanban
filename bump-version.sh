#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

bump="${1:---patch}"
case "$bump" in
  --prerelease) npm version prerelease --preid="${2:-rc}" --no-git-tag-version ;;
  --major|--minor|--patch) npm version "${bump#--}" --no-git-tag-version ;;
  *) echo "Usage: $0 [--patch|--minor|--major|--prerelease [identifier]]" >&2; exit 2 ;;
esac

version=$(node -p "require('./package.json').version")
tag="v$version.$(date +%s)"
npm --prefix npx-cli version "$version" --no-git-tag-version --allow-same-version
cargo set-version --workspace "$version"
cargo metadata --format-version 1 >/dev/null
cargo metadata --manifest-path crates/nsh-app/Cargo.toml --format-version 1 >/dev/null

git add package.json npx-cli/package.json npx-cli/package-lock.json Cargo.lock crates/nsh-app/Cargo.lock
# cargo set-version --workspace changes the root workspace manifests only.
cargo metadata --no-deps --format-version 1 | node -e '
  const fs = require("node:fs");
  const { spawnSync } = require("node:child_process");
  const { packages } = JSON.parse(fs.readFileSync(0, "utf8"));
  const result = spawnSync("git", ["add", "--", ...packages.map(pkg => pkg.manifest_path)], { stdio: "inherit" });
  process.exit(result.status ?? 1);
'
git commit -m "chore: bump version to $version"
git tag -a "$tag" -m "Release $tag"
