# 控制面板

更新时间：2026-11-19（协调者模式启动）

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 本会话负责拆分、集成与验收。GPT-6.2-Sol 子 agent 在 `F:/vk-wt/<m>` 的独立 Git worktree 开发，编译输出放在 `F:/vk-targets/<m>`。M0–M7 和 E1–E7 全部通过后才推送 `origin/my/main`。loop-001–003 由已停止的无头循环执行。
- 当前切片覆盖 M3 展示页、M4 议题界面、M5 GitHub 验收、M6 ACP 迁移和管理，以及 M7 打包、工作区、会话、终端与设置。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | 锁定 nightly 的公开 check、严格 Clippy 和生成一致性通过。完整 check 仍涉及私有依赖。 |
| M1 NSH Spike | verified | 锁定 nightly 构建的 Win32 与 Chrome 通过真实服务 HTTP、日志追加与 diff。Win32 `frame` p95 为 5.77 ms。 |
| M2 本地议题底座 | verified | SQLite、真实 HTTP API、集成的 32 项测试、类型生成和严格 Clippy 均通过。 |
| M3 设计系统 | verified | 锁定 nightly 通过双端展示页复验，生成检查和截图矩阵满足 E4。 |
| M4 Linear 交互 | in-progress | 已合入动作、搜索、议题 API 与看板状态。双端操作界面开发中。 |
| M5 GitHub 联动 | in-progress | PR 与 Issue 同步后端及 M6 队列接线已合入 `efe17674f`，正在复验。测试仓库已验证 Issue 往返、review 草稿与失败检查派发。Agent 推送和转绿待验证。 |
| M6 ACP 管理 | in-progress | SDK 升级、Claude/Codex ACP 选项和 LF 修复已合入。正在复验后端与实际适配器。 |
| M7 对等与退役 | in-progress | 已实测独立分支的双端 CLI 包及 ConPTY。继续验证数据目录隔离、主题接入及终端退出，退役依赖 M4–M6 和本地操作对等。 |

## Run

- 当前 loop 为 `loop-007`。M1–M3 已验收。ACP 升级已合入 `4cb6f061c`，正在从独立检出复验。

## Gate

- M0 已通过本地 Web 构建、公开 Rust 检查、前端类型、lint 和翻译键检查。锁定 nightly 的集成工作区在 `90d4f723b` 通过 340 项测试。完整命令在默认构建切换后复验。
- Windows LF 检出规则已合入，实测 `generate_types --check` 退出 0。
- 锁定 NSH 版本的打包 CLI 在 nightly 下编译通过，输出 `nsh 0.4.0`。正式应用包仍在 E1 验收范围。
- M1 已通过锁定 nightly 的路线闸门，见 `../evidence/m1-gate.md`。Chrome rAF 间隔 p95 为 19.60 ms，E3 的产品列表帧耗时与启动测量待验收。
- M6 集成验证中 180 项测试通过、4 项忽略，生成检查和所涉 crate 的严格 Clippy 均退出 0。完整迁移仍待同一验收版本验证。
- E2 的离线部分验证本地议题、Agent 和 diff。真实开 PR 在 E5 的授权测试仓库联网完成，两段均保留证据。
- E5 在私有测试仓库 `blackplume233/vibe-own-sandbox` 验收，先用本机 `gh` 凭据和测试进程内的 PAT。若仓库权限不足，记录具体 API 和响应。

## Handoff

- 共享入口为 `crates/nsh-app/src/app.rs`。`harness/tools/m1-gate.sh` 使用独立数据库和回环端口复跑，操作界面使用已合入的 `Api::request` 和 `BoardState`。
- M3–M6 的隔离工作、契约及验收边界见 `../child-goals/`。M7 共享入口由集成方接线，各切片报告写入 `../subagents/`。

## 待人工决策

- M0 完整 `pnpm run check` 需要读取 `BloopAI/vibe-kanban-private` 的锁定提交 `020091913ce5608d6c8bdb667d124fd22ab10561`。推荐由用户确认具备该仓库只读权限的 SSH 身份，或在不删除 `crates/remote` 的前提下决定该私有依赖的处理方案。
- E5 测试凭据可写提交状态，写入 Actions 工作流返回 404。可用外部 CI 实际运行同一 PR 提交上的测试并上报，若需 GitHub Actions 托管执行则补充 sandbox 的 workflow 写权限。
