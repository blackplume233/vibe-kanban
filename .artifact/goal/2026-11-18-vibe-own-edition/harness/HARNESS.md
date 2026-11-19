# Dynamic Workflow Harness：vibe-own

## Objective

- Ship：按 `../goal.md` 与 `../plan.md` 完成 M0–M7，直到 E1–E7 全部有证据并通过终止审计。
- Do not ship：上游云服务恢复、移动端、团队协作；任何推送到 `upstream` 的改动。

## Inputs

- Repo：`G:/Workspace/AgentWorkSpace/vibe-kanban`，集成分支 `my/main`。
- 参考仓库（只读）：`../../../../nsh-ue-ui/vendor/nsh-src`（NSH 源码与文档）、`../../../../AgentDispatch`、`../../../../actant-next`。
- 外部系统：GitHub（`origin` = `blackplume233/vibe-kanban`），crates.io / npm registry / static.rust-lang.org（网络不稳定）。
- 凭据策略：GitHub token 只从环境变量或系统密钥链读取，不写入文件、提交或日志。
- Action-to-tool bindings：

| 操作 | 允许的工具 | 备注 |
|---|---|---|
| 读代码、文档、日志 | omp 读/搜索工具、git log/diff | 无限制 |
| 写代码与 Goal 文件 | omp 编辑/写工具 | 仅限 vibe-kanban 仓库与子代理的隔离 worktree |
| 本地提交 | eval `commit` 命名空间 | 子 agent 各自负责独立 worktree 的本地提交，协调者负责集成分支 |
| 推送 | `git push origin my/main` | M0–M7 和 E1–E7 全部通过并完成终止审计后推送 |
| 安装用户级工具链 | `rustup target add`、`cargo install`、`pnpm` | 允许；系统级软件（MSI、winget）未授权 |
| GitHub 远端写（建仓库、开 PR、建 Issue、配 webhook、改设置） | 无 | 未授权；记入 `status.md` 的「待人工决策」 |

- 实体范围：仅 `blackplume233/vibe-kanban` fork 与本机；不触碰其他仓库的工作区（参考仓库只读）。

## Executor

- 模型：`magpie/codex/gpt-6.2-sol:high`（`VK_GOAL_MODEL` 可覆盖），通过 `omp -p` 无人值守运行。
- 每次调用 = 一个 Auto Goal loop；提示词见 `worker-prompt.md`。
- 执行者可以在 `tools/` 下生成任务局部 harness（性能评测、端到端脚本、截图脚本等），每个都要带可复跑的 eval。

## Loop（`run.sh`）

1. 前置检查：`STOP` 文件、`goal.md` 状态必须为 `active`。
2. 调用执行者完成一个 loop（单轮超时默认 4h）。
3. 闸门：`validate_goal_file.py --strict` 必须通过；HEAD 或 `current_loop` 至少一项变化才算有进展。
4. 记录：`runs/runs.tsv` 一行摘要，`runs/iter-*.log` 完整输出。
5. 停止条件：状态不是 `active`；连续 2 轮无进展；连续 2 轮校验失败；达到最大轮数；出现 `STOP` 文件。

## Eval

- 命令：`python C:/Users/black/.agents/skills/auto-goal/scripts/validate_goal_file.py ../goal.md --strict`，加上每个子目标自己的验证命令（记在对应 loop 文件里）。
- 通过信号：校验 `valid: true`，并且子目标证据文件存在于 `../evidence/`。
- 失败归属：执行者在下一轮必须先修复，修复不了就把原因写进 `status.md`。

## Handoff

- 控制面板：`status.md`（Plan / Queue / Run / Gate / Handoff / 待人工决策），由执行者每轮更新。
- 运行记录：`runs/runs.tsv`（不提交，见 `.gitignore`）。
- 恢复方式：`bash .artifact/goal/2026-11-18-vibe-own-edition/harness/run.sh`；停止：在本目录创建 `STOP` 文件。
