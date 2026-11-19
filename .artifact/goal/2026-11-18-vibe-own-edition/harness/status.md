# 控制面板

更新时间：2026-11-19（协调者模式启动）

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 本会话负责拆分、集成与验收。GPT-6.2-Sol 子 agent 在 `F:/vk-wt/<m>` 的独立 Git worktree 开发，编译输出放在 `F:/vk-targets/<m>`。M0–M7 和 E1–E7 全部通过后才推送 `origin/my/main`。loop-001–003 由已停止的无头循环执行。
- 当前子 agent：M0WindowsLint、M1NshGate-2、M2LocalIssues-2、M3DesignContract、M5GitHubCore-2、M6AcpMgmt-2。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | 锁定 nightly 的公开 check、严格 Clippy 和生成一致性通过。完整 check 仍涉及私有依赖。 |
| M1 NSH Spike | in-progress | 独立分支已跑通 Win32 与 Chrome 的真实服务、日志流和 diff。正在准备锁定 nightly 的集成复验。 |
| M2 本地议题底座 | verified | SQLite、真实 HTTP API、集成的 32 项测试、类型生成和严格 Clippy 均通过。 |
| M3 设计系统 | in-progress | 规范与主题生成已合入，准备双端展示页。 |
| M4 Linear 交互 | in-progress | 已合入动作与搜索核心，议题 API 接入开发中。 |
| M5 GitHub 联动 | in-progress | 在 `my/m5` 实现 PR 核心。Issue 同步依赖 M2，真实验收等待测试仓库。 |
| M6 ACP 管理 | in-progress | 在 `my/m6` 实现。允许 `local-deployment/src/lib.rs` 接入 SQLite pool，并在 `server/src/startup.rs` 清理孤儿进程后启动队列 worker。 |
| M7 对等与退役 | in-progress | M7NshPackaging 在独立 worktree 实现正式包的启动与验证，退役依赖 M4–M6。 |

## Run

- 当前 loop：`loop-004`。前一轮 `harness/tools/m1-web-smoke.sh` 和 `harness/tools/m1-native-smoke.sh` 均退出 `0`。

## Gate

- M0 已通过本地 Web 构建、公开 Rust 检查与测试、前端类型、lint 和翻译键检查。锁定 nightly 严格 Clippy 退出 0。完整 check 的私有依赖仍待处理。
- Windows LF 检出规则已合入，实测 `generate_types --check` 退出 0。
- 锁定 NSH 版本的打包 CLI 在 nightly 下编译通过，输出 `nsh 0.4.0`。正式应用包仍在 E1 验收范围。
- M1 在 Rust 1.99.1 和锁定 NSH 提交下完成原生窗口和静态站点渲染，见 `../evidence/m1-web-build.md`、`../evidence/m1-web-browser.png` 和 `../evidence/m1-native-window.png`。双端 HTTP 与实时流、滚动 p95、真实 diff、锁定 nightly 共编译仍待验证。
- E2 的离线部分验证本地议题、Agent 和 diff。真实开 PR 在 E5 的授权测试仓库联网完成，两段均保留证据。

## Handoff

- Win32 与 Web 入口共同编译 `harness/tools/vibe-own-root.rs`。Web 产物位于本机 `target/goal-m1-web/site.*`。
- 第一波实现仍在各自 worktree。集成前检查文件范围和证据，集成后再验证组合行为。

## 待人工决策

- E5 需要一个 GitHub 测试仓库和 token，用于演示 CI 失败修复、评论跟进、Issue 同步。GitHub 远端写操作目前未授权。推荐：用户创建 `blackplume233/vibe-own-sandbox`，并提供只限该仓库权限的 fine-grained PAT（放在环境变量 `VK_GITHUB_TOKEN`）。
- M0 完整 `pnpm run check` 需要读取 `BloopAI/vibe-kanban-private` 的锁定提交 `020091913ce5608d6c8bdb667d124fd22ab10561`。推荐由用户确认具备该仓库只读权限的 SSH 身份，或在不删除 `crates/remote` 的前提下决定该私有依赖的处理方案。
