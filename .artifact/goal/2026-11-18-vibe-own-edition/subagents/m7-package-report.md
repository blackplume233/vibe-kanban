# M7 NSH 正式打包切片

## 启动与部署

`crates/nsh-app/nsh.json` 指定 `PackagedApp`。锁定提交 `12af3d778b588c2837267ff6aaed678eb5ad72a2` 的 NSH CLI 调用应用 crate 的 `#[native_module]` 注册入口，生成 Windows 和 Web 宿主。两个宿主从 `PackagedApp` 挂载同一个 `App`。Windows 宿主在自己的进程内启动 Tokio 和 `server::startup`，为 API 和预览代理分别申请回环端口。窗口卸载时取消后端并等待 `serve()` 完成清理。Web 由浏览器 origin 确定 HTTP 和 WebSocket 地址，部署入口须在同源发布静态产物并把 `/api` 的 HTTP 请求和 WebSocket 升级转发给 vibe server。

`m1-web-smoke` 保留 M1 的独立 WASM 入口和测试地址。直接构建 M1 WASM 时启用此 feature。正式打包时，生成的 Web 宿主提供 `nsh_app_init`。

使用非空 `VK_ASSET_DIR` 可让桌面包或 standalone server 将数据库、配置和运行日志写入指定目录。相对路径以进程工作目录解析。验证脚本还将当前进程的 `TEMP` 与 `TMP` 指向同一个临时 profile，以隔离 Git 工作树。

首次验收时 `APPDATA` 环境变量覆盖未改变 Win32 Known Folder。默认 DB 内新增 3 条测试工作区和 3 条对应仓库记录。已通过各次 API 返回的 UUID、测试名称、隔离 repo 路径及关联关系核对，在启用外键级联的事务中精确删除这 6 条记录并回读验证。验收脚本现在确认 SQLite 文件位于指定目录且起始工作区为空，再创建测试数据。

首次关闭窗口已经进入 `server::startup` 的执行进程清理，随后 Tokio runtime 等待后台阻塞任务导致退出超时。修改为在 `serve()` 清理完成后调用 `shutdown_background()`。后续 Win32 用例在活跃日志流下发出 `WM_CLOSE`，验证正常退出、端口关闭及测试任务停止更新。

## 依赖冲突与复现

锁定 CLI 先执行 `cargo metadata` 获取应用声明的 NSH Git checkout，再生成 `.nsh/rust/Cargo.toml`。生成工作区有自己的锁文件。应用和后端的 `Cargo.lock` 不传播给这个工作区。固定同一个 NSH Git 来源后，CLI 会把应用的 Git NSH 依赖和生成宿主的 path 依赖统一到同一 checkout。将 `nsh-platform-web` 声明为未启用的可选 Git 依赖时，`cargo metadata` 没有该依赖的活动解析边，CLI 会将它误判为 crates.io 来源。当前将依赖保留在 `wasm32` 目标下，由 feature 控制独立导出的符号。

从未携带 `.nsh/rust/Cargo.lock` 的 checkout 用锁定 CLI 打包时，生成工作区也会自行解析后端传递依赖。最初的解析出现下列失败。

| 初次解析 | 观察到的失败 | 应用清单约束 |
| --- | --- | --- |
| Rama alpha.4 与 `rama-error 0.3.0` | `rama_core` 无法导入 `OpaqueError` | Rama error、macros 和 utils 固定到 `0.3.0-alpha.4` |
| Starlark `0.13.0` 与 `allocative 0.3.6` | `HashTable`、`RawTable` 没有 `Allocative` 实现 | `allocative = 0.3.4`，与 Starlark 共用 `hashbrown 0.14.5` |
| `russh-sftp 2.4.0` | `Handler::Error` 的新 `Into<StatusReply>` 约束与现有 `SftpError` 不兼容 | `russh-sftp = 2.1.1` |
| `filetime 0.2.30` | M1 在锁定 nightly 上复现 `fs_set_times` 不稳定特性错误 | `filetime = 0.2.27`，沿用后端已锁定版本 |

这些约束位于 Windows 应用清单。原始 app 工作区和每次重新生成的 host 工作区均会读取，且可以与 `wasm-bindgen 0.2.129` 一起解析。原始构建失败分别记录在 `F:/vk-targets/m7-package/windows-online.log`、`windows-final.log`、`web-build.log`、`windows-pinned.log`。

## 构建命令

在 `crates/nsh-app` 下使用 `nightly-2026-08-01` 并设置 `CARGO_TARGET_DIR=F:/vk-targets/m7-package`。`NSH_WASM_BINDGEN` 指向 `F:/vk-targets/m1/tools/bin/wasm-bindgen.exe`。`LIBCLANG_PATH`、`CC_wasm32_unknown_unknown`、`AR_wasm32_unknown_unknown` 沿用 M1 的 Visual Studio LLVM 路径。构建时 CLI 项目目录与目标参数如下。

```bash
nsh=F:/vk-targets/main/nsh-cli/debug/nsh.exe
node design/generate-tokens.mjs --check
"$nsh" package --target windows -o F:/vk-targets/m7-package/package-windows
"$nsh" package --target web -o F:/vk-targets/m7-package/package-web
cargo +nightly-2026-08-01 build --locked --release --lib --features m1-web-smoke --target wasm32-unknown-unknown
```

## E7 默认命令切换清单

- Cargo 根 workspace 已排除 `crates/remote` 和 `crates/relay-tunnel`，它仍包含 `crates/tauri-app`。`pnpm run check` 经 `backend:check` 显式检查 `crates/remote/Cargo.toml`，`pnpm run lint` 经 `backend:lint` 显式执行 remote clippy。`remote:generate-types:check` 同样直接加载该清单。独立 remote 工作区解析时可能拉取其可选私有 Git 依赖 `billing`。将这些显式 remote 命令移出公共默认验证入口，`backend:format` 也单独执行 remote 格式化。
- `pnpm run generate-types:check` 直接运行本地 `server` 的 `generate_types`，应继续检查本地共享类型。远端类型由 `remote:generate-types:check` 和 CI 的 `backend-remote` 作业负责。该作业还解析、检查 `crates/relay-tunnel` 并检查 remote SQLx 元数据。
- 根 `check` 还调用 `local-web` 路径检查、两个 React 入口的类型检查和 `web-core`、`ui` 的检查。根 `lint` 调用 `local-web`、`ui` 和未使用 i18n 键检查。根 `format` 调用两个 React 入口和 `web-core`。对等验收后将这些默认入口切换到 NSH 构建和 smoke，调整 `pnpm-workspace.yaml` 与锁文件及 `.github/workflows/test.yml` 的前端任务。
- 替换 `dev`、`dev:qa`、`prepack` 调用的前端及 `local-build.sh`、`Dockerfile`、`.github/workflows/pre-release.yml` 中的 React 构建。发布命令和 Tauri 更新交付改用 NSH Windows 与 Web 产物。服务端 `crates/server/build.rs` 与 `routes/frontend.rs` 目前从 `packages/local-web/dist` 准备和嵌入页面，切到 NSH 发布路径后再移除 Tauri workspace 成员、`ProjectKanban.tsx` 的 `ProjectSunsetPage` 路由以及 React 入口。
