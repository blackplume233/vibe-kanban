# Auto Goal 规划

## 发散候选

| 方案 | 核心方法 | 优点 | 风险与代价 | 完成证据适配 |
|---|---|---|---|---|
| A 不做 UI 迁移（基线） | 保留 React 前端 + Tauri 打包；在 React 上做设计系统、Linear 交互；后端做本地议题/GitHub/ACP | 风险最低；现有 ~530 个前端文件全复用；生态成熟（Radix、xterm、pierre） | 不满足“NSH 原生化”诉求；首屏包 5.6MB 需大量拆分；双 token 体系清理成本仍在 | E3 Web 可达，E1 的 NSH 产物与 E7 退役不满足 |
| B React 先行，NSH 后迁 | 先在 React 做 M3/M4 的设计与交互，再整体迁到 NSH | 早期就能用；交互方案先在成熟栈验证 | UI 做两遍；React 上的交互投入大部分在迁移时作废；周期最长 | 全部可达，但成本约为 C 的 1.5 倍以上 |
| C NSH 为唯一新 UI（推荐） | Spike 验证后，新 UI 直接用 NSH Rust 实现，同一 crate 出原生 exe 与 Web；后端能力在 Rust crate 中独立推进；React 只做过渡参照，达到对等后退役 | UI 只做一次；原生 + Web 共用代码；动画在 UI 线程执行，`virtual_list` 原生虚拟化，利于 Linear 级手感；类型可复用 `crates/api-types` | NSH 无稳定版；缺终端模拟器、通用拖拽；Web 端 Rust WebSocket 需 `web-sys` 分支；需 LLVM | 全部可达；风险集中在 M1 Spike，可提前暴露 |
| D 进程内直连（无 HTTP） | NSH 原生 exe 直接调用 `LocalDeployment` trait | 零序列化、类型同源 | 无法服务 Web 端（后端不能进 wasm），两条数据通路 | 与“同一 crate 出 Web”冲突，拒绝 |

## 收敛决策

- selected: C（NSH 为唯一新 UI，统一走 HTTP/WS；原生 exe 进程内启动 axum server）
- reason: 用户明确要求 NSH 原生化且需 Web 版；C 让 UI 只实现一次，四个方向中三个（本地议题、GitHub、ACP）在后端独立推进，不受 UI 路线风险影响。
- rejected: A 不满足 NSH 诉求；B 交互做两遍；D 无法覆盖 Web。
- switch_trigger: M1 Spike 任一硬指标不达标且两轮修正无改善时，切换到 B（React 先行）并请用户确认：
  - 原生窗口 + Web 页面任一端无法连通 vibe server（HTTP + 实时流）；
  - 5000 行 `virtual_list` 滚动 p95 帧时间 > 16ms（release，`--fps`）；
  - `DiffView` 无法展示真实工作区 diff；
  - vibe 后端与 NSH 无法在同一工具链（nightly-2026-08-01）下编译。

## 里程碑

| ID | 可观察结果 | 预期证据 | 依赖 | 状态 |
|---|---|---|---|---|
| M0 基线 | fork 在 `my/main` 上完整构建通过，Goal 目录已提交 | E1 构建与测试日志 | 网络、工具链 | verified（默认检查、Clippy、生成校验、363 项工作区测试和双端包运行均通过。） |
| M1 NSH Spike（路线闸门） | `crates/nsh-app` 在 Windows 原生窗口与浏览器中都能列工作区、看会话日志流、显示 diff | 两端截图；`--fps` 日志；switch_trigger 逐项判定记录 | M0、D4（LLVM） | verified（锁定 nightly 双端实机通过，Win32 `frame` p95 为 5.77 ms。见 `evidence/m1-gate.md`。） |
| M2 本地议题底座 | 不设置 `VK_SHARED_API_BASE` 时议题 CRUD、状态、标签、子议题、关系全部在 SQLite 中可用；API 与类型生成通过 | 迁移 + `cargo test`；`generate-types:check`；API 调用记录 | M0、D1 | verified（真实本地 API 流程见 `evidence/m2-api.md`，集成分支 `db` 和 `server` 共 32 项测试、生成类型和严格 Clippy 通过。） |
| M3 设计系统 | 单一 token JSON → NSH Tailwind 配置；主题生成（亮/暗、强调色）；舒适/紧凑密度；动效规范；组件展示页 | 展示页截图矩阵；`docs/design-system.md` | 组件接入依赖 M1 | verified（锁定 nightly 通过 Win32 与 Chromium 展示页复验。见 `evidence/m3-showcase.md`。） |
| M4 Linear 级交互与议题能力 | NSH 中的看板/列表、拖拽、Cmd+K 模糊搜索、动作注册表、行内编辑、peek、triage、cycle、保存视图 | E2、E3 录屏与性能日志 | M2、M3 | verified（原生拖放已落库，Web 键盘流程与沙箱草稿提交通过。滚动和冷启动满足实测渲染预算。见 `evidence/product-interaction.md`。） |
| M5 GitHub 联动 | CI 状态 + 一键修复、review 评论自动 follow-up、GraphQL 批量轮询、Issue 双向同步、PAT 认证 | E5 测试仓库演示记录 | Issue 双向同步依赖 M2 | verified（私有沙箱实测 CI 修复、Agent 推送、绿色检查读回、文本模式自动评审续接和 Issue 双向往返。见 `evidence/m5-sandbox.md`。） |
| M6 ACP Agent 管理 | 用量/成本、全局调度器 + 持久队列、MCP 注入、Claude/Codex 统一 ACP、可选远程节点 | E6 测试与 API 输出 | M0 | verified（真实模型用量、队列恢复、MCP 和适配器续接通过。受信 SSH 节点经 Windows 协议测试及 Ubuntu 回环验证。见 `evidence/m6-acp.md`。） |
| M7 对等与退役 | NSH 功能覆盖本地 React 工作流，终端可用，默认构建切换，原生与 Web 发布包 | E1、E7，发布包路径与哈希 | M4、M5、M6 | verified（工作区、终端、会话和设置已实测。默认构建使用 NSH，两个最终包均通过实机验收。） |

M5、M6 与 M3/M4 可并行（后端与 UI 互不阻塞）。

E1–E7 通过终止审计。

| 闸门 | 证据 |
|---|---|
| E1 构建 | [构建记录](evidence/m0-build.md#集成候选复验)记录检查、363 项测试及双端最终包的构建、哈希和运行结果。 |
| E2 本地 | [交互验收](evidence/product-interaction.md)记录原生拖放、本地 Agent 日志和 diff，以及联网草稿 PR #3。 |
| E3 交互 | [键盘与性能](evidence/product-interaction.md)记录议题至草稿 PR #4 的键盘操作、两端冷启动、渲染时间和浏览器呈现间隔。 |
| E4 设计 | [组件展示](evidence/m3-showcase.md)记录主题、密度和两端截图，规范见 [设计系统](../../../docs/design-system.md)。 |
| E5 GitHub | [私有沙箱](evidence/m5-sandbox.md)记录 Issue 双向同步、CI 修复转绿和自动评审续接。 |
| E6 ACP | [本地与 SSH 验收](evidence/m6-acp.md)及[真实适配器](evidence/m6-real-adapters.md)记录用量、队列恢复、MCP 与会话恢复。 |
| E7 退役 | `6731b8402` 将默认入口切到 NSH，移除 `ProjectSunsetPage`，默认 Cargo 工作区排除 `crates/remote`，旧数据导出页通过类型检查。 |

## 终止规则

- success: E1–E7 全部通过，最终审计无未声明的关键缺口。此后才推送 `origin/my/main`，并记录远端验证结果。
- pause_or_block: 外部依赖（网络、NSH 上游缺陷、GitHub 测试仓库/凭据）暂不可用，但目标仍有效且存在明确恢复条件。
- abandon: 目标不可行、收益不足以覆盖代价，或用户明确取消。
- supersede: M1 触发 switch_trigger 且用户选择 B 时，以新 plan 替代本路线并记录。
- retry_budget: 同一方法连续失败两次后不得无变化重试；先切换方法、重新规划、请求决策或终止。

## 终止记录

- outcome: complete
- reason: E1–E7 证据均已归档。363 项工作区测试通过，最终 Win32 与 Web 包已实机验证。
- evidence: 上方 E1–E7 证据表
- remote_head: `50a3c2c7b2dd9644b3d940fc9b72fc06d3859bb4`（`origin/my/main` 发布提交，经 GitHub SSH 读回验证。此记录随后以文档提交追加。）
- unresolved: 远程节点经回环 SSH 验证，跨主机挂载需按实际部署配置验证。
- successor_goal: 无
