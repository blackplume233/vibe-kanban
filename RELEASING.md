# 发布 NSH 桌面和 Web 包

在功能对等闸门通过后启用这些入口。GitHub Release 用同一个 tag 保存 Windows x64 原生包、Web 静态包、各平台 standalone/MCP/review 归档和 npm `.tgz`。npm 启动器从该 Release 下载对应平台归档，并用 `manifest.json` 校验下载内容。

## 构建环境

使用仓库的 `nightly-2026-08-01`、Node ≥ 20.19、pnpm 10.13.1 和 Python ≥ 3.11。Windows 需要 MSVC 与 Visual Studio LLVM，Web 需要带 wasm32 支持的 clang 和 `llvm-ar`。系统含有多个 LLVM 版本时，可通过 `LIBCLANG_PATH`、`CC_wasm32_unknown_unknown` 和 `AR_wasm32_unknown_unknown` 指定工具。

```bash
pnpm run nsh:install
pnpm run nsh:install-web  # 安装 wasm32 目标及 wasm-bindgen-cli 0.2.129
pnpm run build:web
pnpm run build:windows  # 在 Windows x64 上运行
```

`nsh:install` 按固定 Git 提交 `12af3d778b588c2837267ff6aaed678eb5ad72a2` 安装 CLI。也可通过绝对路径 `NSH_BIN` 使用已验证的同一提交所构建的 CLI。设定 `CARGO_TARGET_DIR` 时，工具和默认产物均在该目录下，默认目录为仓库根的 `target/`。

## 产物与部署

| 产物 | 启动或部署 |
| --- | --- |
| `package-windows/vibe-own.zip`，Release 中名为 `windows-x64-vibe-own.zip` | 完整解压后启动 `vibe-own/vibe-own.exe`，保留其同级 `nsh/` 资源。API server 在 GUI 进程内运行。 |
| `package-web/vibe-own/`，Release 中的 `vibe-own-web.zip` 解压到 `web/` | 将整个目录放在 standalone server 可执行文件旁，并命名为 `web/`。也可用 `VK_WEB_ROOT` 指向包含 `index.html` 的目录。保留哈希 JS/WASM、字体和压缩副本。 |
| `<platform>-vibe-kanban.zip` | 解压后直接启动其中的 standalone server 可执行文件。归档已包含相邻的 `web/`。 |
| `vibe-kanban-<version>.tgz` | npm 安装后默认启动 standalone server 和浏览器。Windows x64 可指定 `--desktop` 启动原生包。 |

Web 与 `/api` 的 HTTP 和 WebSocket 共用浏览器 origin。使用反向代理时，应把二者转发到同一 standalone server，并放行 WebSocket Upgrade。Docker 默认把 Web 安装到 `/usr/local/bin/web/`，容器端口为 `3000`。

## GitHub Actions

手动执行 `Create GitHub Pre-Release`，选定版本递增级别。工作流更新本地 Cargo workspace 与 npm 版本，从同一个 tag 生成 Windows 和 Web 包及六个平台的 standalone/MCP/review 归档，最后上传预发布。确认 Release 文件并通过部署验收后，可将其改为正式 Release 以触发 `Publish to npm`。该步骤需要 npm 发布权限及 GitHub OIDC trusted publisher 配置。发布目标仍使用现有 npm 包名 `vibe-kanban`。

## 从 macOS 构建与发布

使用 `gh auth login`、`npm login` 和 Docker（例如 `colima start --cpu 4 --memory 6 --disk 100`）。Docker 构建 Linux 和 Windows 的 standalone/MCP/review。原生 Windows UI 包在 Windows x64 上由锁定 CLI 生成，使用同一个 checkout/tag。默认 Release 仓库是 `blackplume233/vibe-kanban`，可通过 `VK_RELEASE_REPO` 指定发行仓库。

```bash
bash ./bump-version.sh             # 生成 v<version>.<timestamp> tag
# 在同一个 tag 的 Windows x64 checkout 运行 pnpm run build:windows
VK_WINDOWS_PACKAGE=/path/to/package-windows/vibe-own.zip bash scripts/release-local.sh build
bash scripts/release-local.sh smoke
```

可在未标记的 HEAD 上先试跑，结果位于 `release-out/v<version>-dryrun/`。`build macos-arm64` 等子集构建也会生成 Web 并刷新 manifest。全部平台都从同一干净提交构建后，可执行 `bash scripts/release-local.sh publish` 将 GitHub 预发布及 npm `next` 推出。`smoke` 会核对 npm 下载的 standalone 首页、WASM 和 `/api/health`。复验 `next` 后执行 `bash scripts/release-local.sh promote`。可用 `bash scripts/release-local.sh rollback <version>` 改回 npm `latest`，现有已发布版本不受影响。
