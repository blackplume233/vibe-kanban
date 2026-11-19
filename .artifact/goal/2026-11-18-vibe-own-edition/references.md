# 参考记录

## 现状调研摘要（2026-11-17，scout 只读调研）

- 上游下线：提交 `97123d526`（#3387）把项目/看板路由换成 `packages/web-core/src/pages/kanban/ProjectSunsetPage.tsx`；README 声明 sunsetting。`KanbanContainer`、列表、多选、子议题、关系 UI 仍在仓库但无路由引用。
- 议题数据在 `crates/remote`（Postgres + ElectricSQL，`crates/remote/migrations/20260112000000_remote-projects.sql`），本地 `crates/db` SQLite 只有 workspaces/sessions 等。
- UI/UX：双 token 体系（新 `app/styles/new/index.css` + 旧 shadcn token 163 处）；字号 8/10/12/14px（`packages/local-web/tailwind.new.config.js:89`）；139 处 `[Npx]`；`framer-motion` 仅 2 文件使用；主 chunk 5.6MB（gzip 1.7MB），`autoCodeSplitting: false`；主题依赖 `--vscode-*` 变量。
- 交互：快捷键注册表 `shared/keyboard/registry.ts`，15 个语义 hook 仅 4 个被使用；`CommandBar.tsx:225` 子串匹配；看板/列表/侧栏无虚拟化；偏好存单个全局 scratch 行（`useUiPreferencesScratch.ts:30`）。
- GitHub：本地认证仅 `gh` CLI（`crates/git-host/src/github/cli.rs`）；无 GitLab；PR 60s 逐个轮询（`crates/services/src/services/pr_monitor.rs`）；无 CI checks；webhook 仅云端（`crates/remote/src/routes/github_app.rs`）；无 Issue 同步。
- ACP：已有 `AcpServer` 通用执行器、registry、harness v2、session/load、审批（`crates/executors/src/executors/acp/`）；缺全局并发、持久队列（`queued_message.rs:27` 单槽内存）、usage 丢弃（`acp/mod.rs:244`）、MCP 未注入（`harness_v2.rs:478`）、远程节点；ACP 依赖 0.8 与 3.4.0 双版本。
- NSH：Solid 式 signals，无 React hooks，不能直接转译；Tailwind 4 运行时解析（部分类不支持）；内建 `virtual_list`、`DiffView`、`Editor`、`dock`、`data_table`；无终端模拟器、无通用拖拽；Web 后端为 HTML/CSS + wasm（`website/docs/web.md`）；Windows 为 Win32 + Direct2D；WebView2 子窗口会盖住 NSH 弹层；无稳定版本承诺（`docs/compatibility.md`）。
- 可复用：AgentDispatch `packages/client-node/src/acp/acp-controller.ts`、`dispatch/dispatcher.ts`、`packages/acp/src/pass-runner.ts`；actant-next `packages/daemon/src/rpc-registry.ts`、`permission-gate.ts`。

## 环境记录

- pnpm 9.15 不读取 `pnpm-workspace.yaml` 的 `overrides` → `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH`；已全局升级 pnpm 10.13.1，`pnpm install --frozen-lockfile` 成功（31 分钟，网络重试）。
- `pnpm --filter @vibe/local-web run build` 通过（2m29s）。
- rustup 下载 `nightly-2026-08-01` 组件多次 TLS 握手失败；`RUSTUP_USE_CURL=1` 后仍出现 schannel 握手失败，需继续重试或换网络/镜像。
- `nightly-2026-08-01` 安装中断后缺 manifest（`rustup component list` 报 missing manifest），每次 cargo 调用都会触发组件下载并卡在 TLS。仓库无 `#![feature]`、`.cargo/config.toml` 无 `-Z` 参数，因此改用已安装的 `cargo +1.99.1` 做 M0 检查；网络恢复后再补装 nightly 并复核。
- `cargo check` 失败于 `libsqlite3-sys` 的 bindgen 找不到 libclang（`sqlx` 的 `sqlite-preupdate-hook` 特性 → `buildtime_bindgen`）。本机 VS 自带 LLVM：`C:/Program Files/Microsoft Visual Studio/18/Community/VC/Tools/Llvm/x64/bin`（含 `libclang.dll`、`clang.exe`），设置 `LIBCLANG_PATH` 即可；D4 可能无需另装 LLVM（wasm32 目标支持待 M1 验证）。
- corepack 在 Git Bash 下因 CRLF 脚本无法运行，改用全局 pnpm。
- loop-004 恢复了锁定 nightly。通过 USTC 镜像安装工具链，保存上次中断时遗留的 `rust-src` 目录后补齐仓库声明的组件。公开工作区通过 `cargo +nightly-2026-08-01 check --locked --workspace`。
- 从 `12af3d778b588c2837267ff6aaed678eb5ad72a2` 的提交快照用锁定 nightly 构建 `nsh-cli`，`nsh.exe --version` 输出 `nsh 0.4.0`。工具位于本机 `F:/vk-targets/main/nsh-cli/debug/nsh.exe`。两端正式包需在集成后的应用目录用该工具创建。

## 错误路径

- 不要用 pnpm 9 安装本仓库（overrides 不生效）。
- 不要并行运行两个 `rustup toolchain install`（锁竞争）。
- `pnpm run check` 在 Windows 的默认 `cmd.exe` 脚本 shell 中无法运行 `./scripts/check-legacy-frontend-paths.sh`。排查脚本时先核对解释器和工作区换行，再分析 TypeScript 或 Rust 输出。
- Clippy 是工具链组件。`pnpm run lint` 已运行两套前端 ESLint，随后因 `1.99.1` 缺少 `cargo-clippy.exe` 退出。`rustup component add` 未列入当前工具绑定。误启动的安装已在下载阶段取消，组件列表未变。
- `crates/remote` 是独立工作区。虽然 `vk-billing` 不在默认特性中，完整 `pnpm run check` 照样在解析 `billing` 时尝试访问私有 Git 依赖。此次日志明确报 SSH 认证失败。仅重试下载、切换 Cargo 的 Git 客户端或禁用默认特性无法提供缺少的访问授权。
- NSH 参考仓库的 `HEAD` 与锁定 SHA 一致，但 `git diff --stat HEAD` 显示 38 个已跟踪文件有工作区差异，包含 `Cargo.lock`。复核锁定版本时用 `git archive <SHA>` 读取提交树，避免把工作区补丁纳入验证。
- 首次原生探针已经取得正确标题的 Win32 窗口句柄，但 .NET `WaitForInputIdle` 在控制台子系统可执行文件上抛出异常。探针改用 Windows 子系统，取证脚本直接用窗口句柄调用 `PrintWindow`。
- 锁定 Rust 1.99.1 的 `wasm32-unknown-unknown` 下载在官方分发站受阻。`RUSTUP_USE_CURL=1` 报 schannel TLS 握手失败。取消该变量后，默认后端在下载 `rust-std` 时达到 100 秒时限。镜像副本应与 rustup 已锁定的 SHA-256 比对。
- 中科大镜像的直连 `rustup` 下载仍未返回，取消该尝试。使用 `curl -4` 将公开分发文件下载到 `target/`。`sha256sum -c` 返回失败，但独立输出的 SHA-256 与 rustup 锁定值一致，因此改用 Python `hashlib` 比对实际字节。安装入口仍是 `rustup target add`，分发地址指向本地缓存。
- 在 `nsh-platform-web` 私有模块调用 `rust_app!` 时，宏生成的 `pub` 初始化函数触发工作区的 `unreachable_pub`。在临时 crate 根重新导出 `nsh_app_init` 即可保留原 lint 与 WASM 导出。
- Python Playwright 会给事件处理器附加属性。传入内建 `list.append` 会在事件注册时触发 `AttributeError`，使用 Python 函数即可。

## 失败计划

-

## 有用模式

- 网络下载步骤：低并发 + 重试循环 + 日志落文件（`../.vk-install.log`），避免长输出回灌。
- `RUSTUP_TOOLCHAIN=1.99.1` 使 pnpm 根脚本内层的 `cargo` 也使用与独立 `cargo +1.99.1` 命令相同的工具链。
- 根脚本使用 `&&`。首项失败后，只能把已经运行的子检查记为通过。可独立运行后续验证以收集证据，并保留根脚本的失败结论。
- Windows 的 pnpm 用 `npm_config_script_shell="$(cygpath -w "$(type -P bash)")"` 指定 Git Bash。实测能保留根 `check` 的路径检查和后续步骤。
- 路径测试夹具要使用宿主系统认可的绝对路径。Windows 上 `/tmp/...` 有根目录分隔符但缺盘符，`Path::is_absolute()` 仍为假。测试用带盘符的合成路径，无需创建目录或改动运行时路径规则。
- `git_ops_safety` 的换行断言受 Git 客户端配置影响。测试仓库的 `Local` 配置层设 `core.autocrlf=false`，隔离宿主配置，并让原始字节断言保持有效。
- NSH 锁定版本的 `examples/gallery-rs/native/src/main.rs` 使用 `AppSource::Rust(Root::of::<组件>())`。M1 原生探针复用该入口，作为 `nsh-platform-windows` 的 Cargo example 构建，直接使用锁定提交内的 `Cargo.lock`。
- 锁定版本的 Web 探针可在 `nsh-platform-web` 的临时快照中编译 `rust_app!` 入口，随后用同版本 `nsh.js` 和 `player.runRust()` 挂载。Win32 与 Web 入口共同包含 `vibe-own-root.rs`。正式打包仍按 E1 独立验收。
- 浏览器探针用已安装的 Chrome 与 Python Playwright。静态 HTTP 服务仅绑定回环地址和临时端口，WASM MIME、组件可见性与页面异常由脚本断言。

## 待确认假设

- D1–D4 见 `goal.md` 执行指导。
- [推测] NSH Rust 侧原生端 WebSocket 需自接 `tungstenite`，Web 端用 `web-sys::WebSocket`；M1 验证。
- [推测] NSH Web 端文字由 Rust 排版，浏览器 Ctrl+F 可能不可用；M1 验证。

## 外部参考经验

- [Ponytail](https://github.com/DietrichGebert/ponytail)（2026-11-18）：最小探针优先复用已有宿主和组件。下一轮先通过真实的现有 API 验证两端连通，再扩展 UI。
- [SkillOpt](https://github.com/microsoft/SkillOpt)（2026-11-18）：按实测轨迹更新证据门。下一轮保留可复跑入口，并断言浏览器与原生窗口显示来自同一服务的工作区记录。
- Windows `cargo test --workspace` 的首个错误是页文件不足（`os error 1455`），随后出现读取 `.rlib` 失败和编译器内部错误。保留锁定 nightly，改用独立 target 目录、`--jobs 2`、`CARGO_INCREMENTAL=0` 和 `CARGO_PROFILE_TEST_DEBUG=0` 再验证。

## 子代理发现

- 调研报告原文见会话中 scout：UxLinearGap、GitHubIntegration、AcpAgentMgmt、NshNativePort（2026-11-17）。
