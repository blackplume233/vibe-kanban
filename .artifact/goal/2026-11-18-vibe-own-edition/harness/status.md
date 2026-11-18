# 控制面板

更新时间：2026-11-18（loop-001）

## Plan

- 目标：见 `../goal.md`；路线：NSH 为唯一新 UI（`../plan.md` 方案 C）。
- 执行者：GPT-6.2-Sol（`omp -p`），每次调用一个 loop。

## Queue

| 里程碑 | 状态 | 下一步 |
|---|---|---|
| M0 基线 | in-progress | 公开工作区检查和测试通过。完整 check 等私有依赖访问，完整 lint 等 Clippy。 |
| M1 NSH Spike | pending | 先验证锁定版本的 Windows 最小原生应用能编译并启动。 |
| M2 本地议题底座 | pending | 可与 M1 并行 |
| M3 设计系统 | pending | 依赖 M1 |
| M4 Linear 交互 | pending | 依赖 M2、M3 |
| M5 GitHub 联动 | pending | 依赖 M2；远端写操作待授权 |
| M6 ACP 管理 | pending | 依赖 M0，可并行 |
| M7 对等与退役 | pending | 依赖 M4–M6 |

## Run

- 当前 loop：`loop-001`
- 最近 eval：`cargo +1.99.1 test --workspace --no-fail-fast` 通过，初始 Goal 严格校验通过。

## Gate

- M0 已通过本地 Web 构建、公开 Rust 工作区检查与测试、前端类型检查和 lint、翻译键 lint。完整根 check/lint 分别受私有依赖认证和 Clippy 缺失阻塞。证据在 `../evidence/m0-build.md`。

## Handoff

- 已修复 Windows 下的绝对路径夹具和 Git 测试仓库换行夹具，工作区测试通过。
- 下轮验证 M1 的 Windows 原生最小应用。M0 的两项外部条件具备后，重跑完整 check/lint。

## 待人工决策

- E5 需要一个 GitHub 测试仓库和 token，用于演示 CI 失败修复、评论跟进、Issue 同步。GitHub 远端写操作目前未授权。推荐：用户创建 `blackplume233/vibe-own-sandbox`，并提供只限该仓库权限的 fine-grained PAT（放在环境变量 `VK_GITHUB_TOKEN`）。
- M0 完整 `pnpm run check` 需要读取 `BloopAI/vibe-kanban-private` 的锁定提交 `020091913ce5608d6c8bdb667d124fd22ab10561`。推荐由用户确认具备该仓库只读权限的 SSH 身份，或在不删除 `crates/remote` 的前提下决定该私有依赖的处理方案。
- M0 完整 `pnpm run lint` 需要 `1.99.1` 的 Clippy。推荐用户安装该组件，或明确将 `rustup component add clippy --toolchain 1.99.1` 纳入工具绑定。该命令曾误启动，已取消，组件未安装。
