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

## 错误路径

- 不要用 pnpm 9 安装本仓库（overrides 不生效）。
- 不要并行运行两个 `rustup toolchain install`（锁竞争）。

## 失败计划

-

## 有用模式

- 网络下载步骤：低并发 + 重试循环 + 日志落文件（`../.vk-install.log`），避免长输出回灌。

## 待确认假设

- D1–D4 见 `goal.md` 执行指导。
- [推测] NSH Rust 侧原生端 WebSocket 需自接 `tungstenite`，Web 端用 `web-sys::WebSocket`；M1 验证。
- [推测] NSH Web 端文字由 Rust 排版，浏览器 Ctrl+F 可能不可用；M1 验证。

## 外部参考经验

-

## 子代理发现

- 调研报告原文见会话中 scout：UxLinearGap、GitHubIntegration、AcpAgentMgmt、NshNativePort（2026-11-17）。
