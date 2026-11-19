# 控制面板

更新时间：2026-11-19（协调者模式启动）

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 本会话负责拆分、集成与验收。GPT-6.2-Sol 子 agent 在 `F:/vk-wt/<m>` 的独立 Git worktree 开发，编译输出放在 `F:/vk-targets/<m>`。M0–M7 和 E1–E7 全部通过后才推送 `origin/my/main`。loop-001–003 由已停止的无头循环执行。
- 当前子 agent：M0WindowsLint、M1NshGate-2、M2LocalIssues-2、M3DesignContract、M5GitHubCore-2、M6AcpMgmt-2。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | 公开工作区检查和测试已通过。已安装 Clippy，严格 lint 现因源码诊断失败。完整 check 仍有私有依赖。 |
| M1 NSH Spike | in-progress | 同一 Rust 根组件在 Win32 与 Chrome 渲染通过。下一步用真实本地 HTTP API 显示同一条工作区记录。 |
| M2 本地议题底座 | in-progress | 在 `my/m2` 实现 SQLite 数据与 API。 |
| M3 设计系统 | in-progress | 设计规范独立起草。组件展示页接入依赖 M1。 |
| M4 Linear 交互 | pending | 依赖 M2、M3 |
| M5 GitHub 联动 | in-progress | 在 `my/m5` 实现 PR 核心。Issue 同步依赖 M2，真实验收等待测试仓库。 |
| M6 ACP 管理 | in-progress | 在 `my/m6` 实现。允许 `local-deployment/src/lib.rs` 接入 SQLite pool，并在 `server/src/startup.rs` 清理孤儿进程后启动队列 worker。 |
| M7 对等与退役 | pending | 依赖 M4–M6 |

## Run

- 当前 loop：`loop-004`。前一轮 `harness/tools/m1-web-smoke.sh` 和 `harness/tools/m1-native-smoke.sh` 均退出 `0`。

## Gate

- M0 已通过本地 Web 构建、公开 Rust 工作区检查与测试、前端类型检查和 lint、翻译键 lint。Clippy 0.1.99 已安装，公开工作区严格 lint 退出 101。完整输出在本机 `F:/vk-targets/main/m0-public-clippy.log`，完整 check 的私有依赖仍待处理。
- M1 在 Rust 1.99.1 和锁定 NSH 提交下完成原生窗口和静态站点渲染，见 `../evidence/m1-web-build.md`、`../evidence/m1-web-browser.png` 和 `../evidence/m1-native-window.png`。双端 HTTP 与实时流、滚动 p95、真实 diff、锁定 nightly 共编译仍待验证。
- E2 的离线部分验证本地议题、Agent 和 diff。真实开 PR 在 E5 的授权测试仓库联网完成，两段均保留证据。

## Handoff

- Win32 与 Web 入口共同编译 `harness/tools/vibe-own-root.rs`。Web 产物位于本机 `target/goal-m1-web/site.*`。
- 第一波实现仍在各自 worktree。集成前检查文件范围和证据，集成后再验证组合行为。

## 待人工决策

- E5 需要一个 GitHub 测试仓库和 token，用于演示 CI 失败修复、评论跟进、Issue 同步。GitHub 远端写操作目前未授权。推荐：用户创建 `blackplume233/vibe-own-sandbox`，并提供只限该仓库权限的 fine-grained PAT（放在环境变量 `VK_GITHUB_TOKEN`）。
- M0 完整 `pnpm run check` 需要读取 `BloopAI/vibe-kanban-private` 的锁定提交 `020091913ce5608d6c8bdb667d124fd22ab10561`。推荐由用户确认具备该仓库只读权限的 SSH 身份，或在不删除 `crates/remote` 的前提下决定该私有依赖的处理方案。
