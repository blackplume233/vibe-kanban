# M7 NSH 正式打包切片
## 共享产品入口复验

共享入口 `7cbc13b483` 的 Win32 ZIP 和 Web 包均由锁定 NSH CLI 构建。`verify-packages.py` 通过 Win32 UI Automation 与 Chrome 157 选择工作区，再进入日志与变更。两端读到实际工作树 `review.txt` 从 `status=planned` 改成 `status=packaged` 的 diff 与运行中的日志。浏览器建立 4 条同源 WebSocket。关闭 Win32 窗口后，窗口进程正常退出，API 端口关闭，子进程心跳停止。脚本退出 0。[Win32 截图](../evidence/product-m7-native.png) · [Chrome 截图](../evidence/product-m7-browser.png)

- Windows ZIP 为 `F:/vk-targets/main/product-windows/vibe-own.zip`，SHA-256 `19c71bfa917d6b1eac4d71eb4b35707cdd06081ec0c99620df209039fdb5e4f0`。11 个条目通过 ZIP CRC 检查。
- Web 包的 WASM 为 `F:/vk-targets/main/product-web/vibe-own/nsh.1d92c475ff.wasm`，SHA-256 `1d92c475ff563ea78f38ebfdb3658d6d6035d2aa9edfac8704e004c66fe61c9f`。
- Win32 计时包用 `VK_NSH_BENCHMARK=1` 记录分段，5,005 条议题下九次首滚耗时 650–810 ms，包含内嵌后端启动。Chrome 同规模首个键盘操作为 573.6 ms，滚动渲染切片 p95 为 3.17 ms。 [Win32 测量](../evidence/product-native-cold-stages.json) · [Chrome 测量](../evidence/product-web-results.json)



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

## 实机验收

- `nsh package --target windows` 和 `nsh package --target web` 在锁定 nightly 下生成正式产物。M1 的 `--features m1-web-smoke` 直编通过，`wasm-bindgen-cli 0.2.129` 处理后的 WASM 导出 `nsh_app_init`。
- 隔离 Win32 与 Chrome 复验退出 `0`，日志见 `F:/vk-targets/m7-package/base-isolated-runtime.log`。脚本验证数据库在 `vibe-own-m7-rko2n3fo/data/db.v2.sqlite`，数据库内仅有刚创建的工作区，`git worktree list` 返回同一临时根下的工作树。Win32 的 HTTP、diff 和执行日志操作从包内服务读取，该服务的两个监听仅位于回环地址并属于窗口 PID。Chrome 157 通过相同 origin 请求 WASM、HTTP 及三个 WebSocket。关闭 Win32 后进程正常退出，回环连接失败，活跃任务心跳停止。截图存于 `F:/vk-targets/m7-package/base-isolated-runtime/`。
- M3 正式宿主注册 Source Sans 3、IBM Plex Mono 和许可证。主题生成器同时投影 `nsh.json` 中的 `appearance`、`theme` 和 `tailwind`，`--check` 检测产物偏离。Win32 与 Web 运行 `PackagedApp` 并挂载同一个 `App`，保留 M4 的共享 API 请求层。
- 主题产物的四个字体和许可证文件在 Windows、Web 各自的产物中均与源文件 SHA-256 一致。主题双端实机命令退出 `0`。日志及截图位于 `F:/vk-targets/m7-package/themed-runtime.log` 与 `F:/vk-targets/m7-package/themed-runtime/`。长期保留这一目录内的测试数据库与截图可复核隔离结果。
- 完整复验覆盖 `e82085774a482f77f86794f6bf6607611fbe8696`，该提交同步 `0f0f8eb6c5` 的 ACP 与 GitHub 后端并更新应用锁文件。`windows-integrated.log`、`web-integrated.log`、`integrated-runtime-smoke.log` 与 `integrated-runtime/` 位于 `F:/vk-targets/m7-package`。两个正式打包命令及实机脚本均退出 `0`。测试数据库和 worktree 位于 `integrated-runtime/vibe-own-m7-97uadx6i/`，工作区 UUID 为 `d62d81bb-bf72-4f49-bbb1-4d098ebd9c3a`。Chrome 在 `127.0.0.1:65485` 读取同源 API，Win32 PID `92664` 退出并关闭 `127.0.0.1:50433`，同时停止实时任务 `16c3d7b5-affc-48e2-906a-3f7073f56490`。Vite 在浏览器关闭时记录 WebSocket `ECONNABORTED`，此前的 HTTP、WebSocket 和 diff 断言均通过。
- 最终 Windows 产物 `integrated-windows/vibe-own.zip`，SHA-256 `cb14034b4cd56a67516bb28ad34ad548ded1fec9c543c41d6eeccd88cf6d93aa`。最终 Web 产物 `integrated-web/vibe-own/`，其中 `index.html` 的 SHA-256 为 `b557e985d2204bdea583d119208f279edaa117e580ff8a5da960828d5dc294ee`，`nsh.2e56a98d1b.wasm` 的 SHA-256 为 `2e56a98d1b327ec9f605fcc4b01f8320c76d7263487708f535aeff7c0585c01f`。

## E7 默认命令切换清单

- Cargo 根 workspace 已排除 `crates/remote` 和 `crates/relay-tunnel`，它仍包含 `crates/tauri-app`。`pnpm run check` 经 `backend:check` 显式检查 `crates/remote/Cargo.toml`，`pnpm run lint` 经 `backend:lint` 显式执行 remote clippy。`remote:generate-types:check` 同样直接加载该清单。独立 remote 工作区解析时可能拉取其可选私有 Git 依赖 `billing`。将这些显式 remote 命令移出公共默认验证入口，`backend:format` 也单独执行 remote 格式化。
- `pnpm run generate-types:check` 直接运行本地 `server` 的 `generate_types`，应继续检查本地共享类型。远端类型由 `remote:generate-types:check` 和 CI 的 `backend-remote` 作业负责。该作业还解析、检查 `crates/relay-tunnel` 并检查 remote SQLx 元数据。
- 根 `check` 还调用 `local-web` 路径检查、两个 React 入口的类型检查和 `web-core`、`ui` 的检查。根 `lint` 调用 `local-web`、`ui` 和未使用 i18n 键检查。根 `format` 调用两个 React 入口和 `web-core`。对等验收后将这些默认入口切换到 NSH 构建和 smoke，调整 `pnpm-workspace.yaml` 与锁文件及 `.github/workflows/test.yml` 的前端任务。
- 替换 `dev`、`dev:qa`、`prepack` 调用的前端及 `local-build.sh`、`Dockerfile`、`.github/workflows/pre-release.yml` 中的 React 构建。发布命令和 Tauri 更新交付改用 NSH Windows 与 Web 产物。服务端 `crates/server/build.rs` 与 `routes/frontend.rs` 目前从 `packages/local-web/dist` 准备和嵌入页面，切到 NSH 发布路径后再移除 Tauri workspace 成员、`ProjectKanban.tsx` 的 `ProjectSunsetPage` 路由以及 React 入口。
