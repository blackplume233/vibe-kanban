---
status: active
updated_at: 2026-11-19
artifact_language: zh-CN
execution_mode: simulated-file-goal
workflow_mode: full
native_goal_id: null
current_loop: loop-004
next_sub_goal: 验收首批独立 worktree 交付，集成通过的变更；M1 用真实本地服务验证 Win32 和 Web 数据通路。
last_verified_loop: loop-003
termination_reason: null
handoff_summary: null
references: references.md
---

# Auto Goal 任务

工作代号 vibe-own：基于 vibe-kanban 的个人版本。

## 目标

- 基础目标：在 fork `blackplume233/vibe-kanban` 的 `my/main` 分支上，把上游已下线看板（#3387 sunset）的 vibe-kanban 改造成一个**纯本地、可长期自维护**的 Agent 看板产品，覆盖用户提出的四个方向：
  1. **交互 + UI/UX 对标 Linear 或更好**：统一设计系统（单一 token 源、主题生成、密度、动效规范），键盘优先（统一动作注册表驱动 Cmd+K / 快捷键 / 右键菜单），即时响应（乐观更新、虚拟列表、首屏 < 1s），Linear 级议题能力（看板/列表、triage、cycle、保存视图、子议题与关系、行内编辑、全局模糊搜索）。
  2. **GitHub 深度联动**：PR 的 CI 检查状态可见并一键生成“修复 CI”任务；PR review 评论可自动作为 Agent follow-up；PR 状态用批量 GraphQL + 退避替代逐个 60s 轮询；GitHub Issue 与本地议题双向同步；支持 PAT 认证，不再强依赖 `gh` CLI。
  3. **NSH 原生化**：用 NSH（longbridge/nsh，Rust）构建新 UI，同一 crate 产出 **Windows 原生 exe** 与 **Web 静态站点（nsh.wasm）**，二者通过 HTTP/WS 连接 vibe server；原生 exe 进程内启动 server。功能达到 React 版对等后，React 前端退役。
  4. **基于扩展 ACP 的 Agent 管理**：ACP token 用量与成本入库并可查询；全局并发配额 + 持久化多槽优先级队列；ACP `session/new` 注入 MCP 配置；Claude/Codex 走通用 ACP 执行器并移除 0.8 桥接；可选远程执行节点（复用 AgentDispatch / actant-next 设计）。
- 完成证据（每项需当前状态的可复现证据，存于 `evidence/`）：
  - E1 构建：`cargo check --workspace`、`cargo test --workspace`、`pnpm run check`、`pnpm run lint`、`pnpm run generate-types:check` 全部通过；NSH app `nsh package --target windows` 与 `--target web` 均产出可运行产物。
  - E2 纯本地：断网且未设置 `VK_SHARED_API_BASE` 时，原生 exe 能完成“建议题 → 拖到进行中 → 启动 Agent 工作区 → 看日志流 → 看 diff → 开 PR”的端到端流程（录屏或截图序列 + 日志）。
  - E3 交互：键盘可完成上述全流程（无鼠标录屏）；Cmd+K 模糊搜索覆盖议题/项目/工作区；5000 条议题列表滚动 p95 帧时间 ≤ 16ms（`--fps` 日志）；冷启动到可交互 ≤ 1s（原生）/ ≤ 2s（Web）。
  - E4 设计系统：单一 token JSON 生成 NSH Tailwind 配置；组件展示页截图（亮/暗、舒适/紧凑）；设计规范文档。
  - E5 GitHub：真实测试仓库上演示 CI 失败 → 一键修复任务 → Agent 推送 → CI 转绿；review 评论 → 自动 follow-up；Issue 双向同步往返一次。
  - E6 ACP：一次 ACP 会话后 API 返回该会话 token/成本；并发上限为 N 时第 N+1 个任务排队且重启后不丢；MCP 工具在 ACP Agent 内可见。
  - E7 退役：React 前端与 `crates/remote` 云依赖从默认构建移除，`ProjectSunsetPage` 不再存在。

## 约束

- 文档语言：zh-CN；回复语言：中文；代码标识符、命令、路径保留原文。
- 技术要求：Rust `nightly-2026-08-01`（`rust-toolchain.toml`）、pnpm 10.13.1、Node ≥ 20；NSH 锁定 commit `12af3d778b588c2837267ff6aaed678eb5ad72a2`（与 `nsh-ue-ui/vendor.lock` 一致），升级需显式决策。
- 仓库基准（`AGENTS.md`）：共享类型改 `crates/server/src/bin/generate_types.rs` 后 `pnpm run generate-types`，禁止手改 `shared/types.ts`；SQLx 迁移后 `pnpm run prepare-db`；完成前 `pnpm run format`；Rust 单测与代码同处 `#[cfg(test)]`。
- 验证命令：见 E1；UI 必须在真实界面验证（原生窗口 / 浏览器），不得只以单测代替。
- 产物目录约定：本 Goal 目录 `.artifact/goal/2026-11-18-vibe-own-edition/` 随 `my/main` 提交；NSH app 放 `crates/nsh-app/`（待 D3 确认）。
- 依赖策略：优先复用已有依赖与 NSH 内建组件（`virtual_list`、`DiffView`、`Editor`、`dock`）；新增依赖需在 loop 中写理由；不得引入第二套设计 token 或状态管理约定。
- 安全边界：只推送到 `origin`（个人 fork）的 `my/*` 分支，绝不推送 `upstream`；不 force push `my/main`；GitHub token 只经环境变量或系统密钥链，不入库不入日志；删除上游代码（React 前端、`crates/remote`）仅在对应里程碑验收后进行；安装系统级工具（LLVM、wasm-bindgen-cli）前告知用户。
- 外部阻塞：网络对 static.rust-lang.org / registry.npmjs.org 不稳定（TLS 握手失败），下载类步骤需重试与日志；NSH 无稳定版本承诺。
- 非目标：恢复上游云服务（Postgres + ElectricSQL）；移动端（iOS/Android）；多人协作/团队权限；上游同步之外的上游贡献。

## 执行指导

- 每轮读本文件的状态摘要与当前子目标；完整重读仅在检查点、用户编辑或阶段切换时。
- 遵循仓库语言偏好、技术要求、验证命令和产物目录约定；所有持久文档使用 zh-CN。
- 完整模式：先按 `plan.md` 的里程碑推进；每轮在 `loops/loop-XXX.md` 比较本轮候选方法再收敛为一个有边界、可验证的子目标。
- M1（NSH Spike）是路线闸门：未通过 `plan.md` 的 `switch_trigger` 判定前，不启动 React 退役或大规模 NSH UI 开发。
- 后端能力（本地议题、GitHub、ACP）在 Rust crate 中实现并经 API 暴露，与 UI 技术无关；UI 只消费 API。
- 宽里程碑（M3 设计系统、M4 议题与交互、M5 GitHub、M6 ACP）各建 `child-goals/<slug>/` 子 Auto Goal；子代理只在隔离 worktree 内写入，报告写 `subagents/`。
- 每个子目标完成后：跑对应验证命令、截图/日志存 `evidence/`、更新 `plan.md` 里程碑状态，再用 `commit` 提交到 `my/main`。
- 本会话协调各独立 worktree，验收 E1–E7 并通过终止审计后推送 `origin/my/main`。中间阶段保留本地提交。
- 失败按类别（`bad-goal|bad-plan|bad-evidence|missing-prerequisite|external-blocker|wrong-direction`）写入 `references.md`，换方法再试，禁止无变化重试。
- 定期 `git fetch upstream`，仅在里程碑之间评估合并上游修复。
- 决策（2026-11-18 用户指示“安排 GPT-6.2-Sol 以 dynamic workflow 推进完成所有任务”，视为确认目标并采用以下推荐默认；用户可随时修改本节）：
  - D1 数据底座：推荐**纯本地 SQLite 议题**（移植 `crates/remote/migrations` 的 issues/statuses/tags/relations 到 `crates/db`）。
  - D2 最终 UI：推荐 **NSH 为唯一新 UI**（原生 + Web），React 仅作过渡期参照，不在 React 上做设计系统和交互大改。
  - D3 产品名与 NSH app 位置：工作代号 `vibe-own`，代码放 `crates/nsh-app/`。
  - D4 LLVM：无需另装。VS 自带 LLVM 22.1.3（`C:/Program Files/Microsoft Visual Studio/18/Community/VC/Tools/Llvm/x64/bin`）含 `clang`、`libclang.dll`、`llvm-ar`、`wasm-ld`，且支持 wasm32 目标；构建时设置 `LIBCLANG_PATH`、`CC_wasm32_unknown_unknown`、`AR_wasm32_unknown_unknown` 指向它。
