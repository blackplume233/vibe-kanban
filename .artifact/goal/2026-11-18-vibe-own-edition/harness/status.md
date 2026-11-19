# 控制面板

更新时间：2026-11-20

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 本会话负责拆分、集成与验收。GPT-6.2-Sol 子 agent 在 `F:/vk-wt/<m>` 的独立 Git worktree 开发，编译输出放在 `F:/vk-targets/<m>`。M0–M7 和 E1–E7 全部通过后才推送 `origin/my/main`。loop-001–003 由已停止的无头循环执行。
- 当前切片覆盖 M3 展示页、M4 议题界面、M5 GitHub 验收、M6 ACP 迁移和管理，以及 M7 打包、工作区、会话、终端与设置。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | `my/main` 已采用 NSH 默认构建。检查、Clippy 和类型生成通过，工作区测试正以低并发复验。 |
| M1 NSH Spike | verified | 锁定 nightly 构建的 Win32 与 Chrome 通过真实服务 HTTP、日志追加与 diff。Win32 `frame` p95 为 5.77 ms。 |
| M2 本地议题底座 | verified | SQLite、真实 HTTP API、集成的 32 项测试、类型生成和严格 Clippy 均通过。 |
| M3 设计系统 | verified | 锁定 nightly 通过双端展示页复验，生成检查和截图矩阵满足 E4。 |
| M4 Linear 交互 | verified | Win32 拖放已落库，Chromium 键盘流程覆盖 Agent、日志、diff 和联网草稿 PR #4。 |
| M5 GitHub 联动 | verified | 私有沙箱通过 Issue 往返、Codex 修复与推送、产品读回绿色检查，以及 Claude 文本模式自动续接。 |
| M6 ACP 管理 | verified | 实际 ACP 用量、持久队列、MCP、Claude/Codex 会话续接有证据。受信节点通过 Windows 协议测试与 Ubuntu 回环 SSH 验收。 |
| M7 对等与退役 | in-progress | NSH 默认入口和 sunset 页面移除已合入 `my/main`。历史导出路由的类型检查通过，正在复验发布包。 |

## Run

- `loop-008` 记录双端正式包、独立构建候选、实际 ACP 用量和空工作区修复。见 [循环记录](../loops/loop-008.md)。

## Gate

- [默认构建候选](../evidence/m0-build.md#集成候选复验) 已合入。新 Web 包已通过键盘探针。最终工作区测试及配套 Win32 包待复验。
- [共享产品包](../subagents/m7-package-report.md#共享产品入口复验)通过 Win32 和 Chrome 实机验收。
- Win32 5,005 条议题首次滚动 721.6 ms，最高主线程帧耗时 p95 6.84 ms。Chromium 前台首次键盘操作 685.9 ms，渲染 p95 2.712 ms。见 [产品交互证据](../evidence/product-interaction.md)。
- M6 协议升级在 `4cb6f061c` 通过 185 项测试，5 项忽略。单独复验 `session/new` 和 `session/load` 均从真实 MCP 进程发现 `list_repos`。M5 集成复验的 73 项测试及生成检查通过，原始日志为 `F:/vk-targets/main/m5-acp-integration.log`。
- E2 的离线部分验证了本地议题、拖放、Agent、日志和 diff。Win32 包在授权沙箱创建草稿 PR #3 并记录 URL。
- E5 在私有测试仓库 `blackplume233/vibe-own-sandbox` 验收，先用本机 `gh` 凭据和测试进程内的 PAT。若仓库权限不足，记录具体 API 和响应。

## Handoff

- 共享入口为 `crates/nsh-app/src/app.rs`。`harness/tools/m1-gate.sh` 使用独立数据库和回环端口复跑，操作界面使用已合入的 `Api::request` 和 `BoardState`。
- M3–M6 的隔离工作、契约及验收边界见 `../child-goals/`。M7 共享入口由集成方接线，各切片报告写入 `../subagents/`。

## 待人工决策

- 历史 `pnpm run check` 依赖私有 `billing` 仓库。新的默认构建候选已绕开云构建依赖，切换以功能对等证据为准。
- E5 测试凭据可写提交状态，写入 Actions 工作流返回 404。可用外部 CI 实际运行同一 PR 提交上的测试并上报，若需 GitHub Actions 托管执行则补充 sandbox 的 workflow 写权限。
- Claude 适配器的包内 CLI 在 `session/new` 返回 `-32603`，使用本机 `claude.exe` 的配置已复验通过。详见 [实测结果](../evidence/m6-real-adapters.md#现有登录复验)。
