# 控制面板

更新时间：2026-11-19（协调者模式启动）

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 本会话负责拆分、集成与验收。GPT-6.2-Sol 子 agent 在 `F:/vk-wt/<m>` 的独立 Git worktree 开发，编译输出放在 `F:/vk-targets/<m>`。M0–M7 和 E1–E7 全部通过后才推送 `origin/my/main`。loop-001–003 由已停止的无头循环执行。
- 当前切片包括 M1 锁定 nightly、M3 展示页、M4 议题操作、M5 GitHub 同步、M6 ACP 迁移、M7 打包和终端。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | 锁定 nightly 的公开 check、严格 Clippy 和生成一致性通过。完整 check 仍涉及私有依赖。 |
| M1 NSH Spike | in-progress | 已实测 Win32 与 Chrome 的真实服务、日志流和 diff。锁定 nightly 复验停在 `filetime 0.2.30`，正在验证兼容版本。 |
| M2 本地议题底座 | verified | SQLite、真实 HTTP API、集成的 32 项测试、类型生成和严格 Clippy 均通过。 |
| M3 设计系统 | in-progress | 规范与主题生成已合入，准备双端展示页。 |
| M4 Linear 交互 | in-progress | 已合入动作与搜索核心，议题 API 接入开发中。 |
| M5 GitHub 联动 | in-progress | PR 后端与 Issue 同步在独立分支验证。已创建私有测试仓库 `blackplume233/vibe-own-sandbox`。 |
| M6 ACP 管理 | in-progress | 用量、持久队列和 MCP 已合入，锁定 nightly 集成测试运行中。继续验证 Claude/Codex 迁移。 |
| M7 对等与退役 | in-progress | M7NshPackaging 在独立 worktree 实现正式包的启动与验证，退役依赖 M4–M6。 |

## Run

- 当前 loop 为 `loop-005`。M2 与 M6 的组合已通过锁定 nightly 验证。

## Gate

- M0 已通过本地 Web 构建、公开 Rust 检查与测试、前端类型、lint 和翻译键检查。锁定 nightly 严格 Clippy 退出 0。完整 check 的私有依赖仍待处理。
- Windows LF 检出规则已合入，实测 `generate_types --check` 退出 0。
- 锁定 NSH 版本的打包 CLI 在 nightly 下编译通过，输出 `nsh 0.4.0`。正式应用包仍在 E1 验收范围。
- M1 已合入共享 NSH 应用。Rust 1.99.1 下的双端 HTTP、实时日志与真实 diff 记录在 `../evidence/m1-gate.md`。锁定 nightly 的实机复验在独立工作树进行。Win32 绘制耗时 p95 已记录，Web 绘制耗时仍需测量。
- M6 集成验证中 180 项测试通过、4 项忽略，生成检查和所涉 crate 的严格 Clippy 均退出 0。完整迁移仍待同一验收版本验证。
- E2 的离线部分验证本地议题、Agent 和 diff。真实开 PR 在 E5 的授权测试仓库联网完成，两段均保留证据。
- E5 在私有测试仓库 `blackplume233/vibe-own-sandbox` 验收，先用本机 `gh` 凭据和测试进程内的 PAT。若仓库权限不足，记录具体 API 和响应。

## Handoff

- 共享入口为 `crates/nsh-app/src/app.rs`。复验通过 `harness/tools/m1-gate.sh` 使用独立数据库和回环端口。
- M5 继续开发本地议题与 GitHub Issue 同步。M6 正在合并用量、队列与 MCP 注入，并准备 Claude/Codex 的兼容迁移。M3、M4 和 M7 在独立工作树开发。

## 待人工决策

- M0 完整 `pnpm run check` 需要读取 `BloopAI/vibe-kanban-private` 的锁定提交 `020091913ce5608d6c8bdb667d124fd22ab10561`。推荐由用户确认具备该仓库只读权限的 SSH 身份，或在不删除 `crates/remote` 的前提下决定该私有依赖的处理方案。
