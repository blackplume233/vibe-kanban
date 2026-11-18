# 控制面板

更新时间：2026-11-18（初始化，由协调者写入）

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 执行者：GPT-6.2-Sol（`omp -p`），每次调用一个 loop。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | 补跑 `cargo +1.99.1 test --workspace`、`pnpm run check`、`pnpm run lint`，提交证据 |
| M1 NSH Spike | pending | 依赖 M0 |
| M2 本地议题底座 | pending | 可与 M1 并行 |
| M3 设计系统 | pending | 依赖 M1 |
| M4 Linear 交互 | pending | 依赖 M2、M3 |
| M5 GitHub 联动 | pending | 依赖 M2；远端写操作待授权 |
| M6 ACP 管理 | pending | 依赖 M0，可并行 |
| M7 对等与退役 | pending | 依赖 M4–M6 |

## Run

- 当前 loop：无
- 最近 eval：`validate_goal_file.py --strict` 通过（2026-11-18）

## Gate

- M0：前端 build 通过；`cargo +1.99.1 check --workspace` 通过（`../evidence/m0-build.md`）。

## Handoff

- 已完成：fork、clone、`my/main` 分支、依赖安装、前端构建、后端 check、Goal 文档。
- 未完成：cargo test、pnpm check/lint。

## 待人工决策

- E5 需要一个 GitHub 测试仓库和 token，用于演示 CI 失败修复、评论跟进、Issue 同步。GitHub 远端写操作目前未授权。推荐：用户创建 `blackplume233/vibe-own-sandbox`，并提供只限该仓库权限的 fine-grained PAT（放在环境变量 `VK_GITHUB_TOKEN`）。
