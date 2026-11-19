# 循环 004

## 基础目标切片

接续 M1 双端探针验证，同时推进 M0 严格检查、M2 本地议题、M3 设计规范、M5 GitHub 和 M6 ACP。

## 子目标

将执行迁入当前会话，并为每个子任务建立可独立验收的工作区和写入范围。

## HTN 分解

| 切片 | 子 agent | worktree | 交付证据 |
|---|---|---|---|
| M0 Windows lint | `M0WindowsLint` | `F:/vk-wt/m0` | 严格 Clippy 日志与变更范围 |
| M1 NSH 闸门 | `M1NshGate-2` | `F:/vk-wt/m1` | 两端连接真实本地 HTTP/实时流、diff 与滚动样本 |
| M2 本地议题 | `M2LocalIssues-2` | `F:/vk-wt/m2` | SQLite 迁移、API 与请求记录 |
| M3 设计规范 | `M3DesignContract` | `F:/vk-wt/m3` | `docs/design-system.md`，组件实现待 M1 接口稳定 |
| M5 PR 核心 | `M5GitHubCore-2` | `F:/vk-wt/m5` | CI 状态、鉴权、批量轮询与评论任务的集成记录 |
| M6 ACP | `M6AcpMgmt-2` | `F:/vk-wt/m6` | 会话用量、恢复队列、MCP 注入的证据 |

## 仓库基准检查

中文证据。各 worktree 独立，使用独立编译输出目录。协调者检查共享路由、类型生成与迁移，负责集成和发布。

## 发散候选

| 方案 | 核心方法 | 预期证据 |
|---|---|---|
| A | 从会话根使用内置隔离 | agent 可见、自动创建 worktree |
| B | 预建 Git worktree，再由会话中的子 agent 使用 | agent 可见、任务路径和写入边界可核对 |

## 收敛决策

选 B。内置隔离无法从会话根定位 Git 仓库，各子任务改用已绑定到 `vibe-kanban` 的独立 worktree。

## 执行记录

- 停止原无头循环。按用户顺序执行，完整终止审计通过后推送 `origin/my/main`。
- M6 的 SQLite pool 在本地部署层接线，队列恢复在服务器清理孤儿执行后启动。
- M5 将 CI 日志、评论和议题内容作为数据处理，评论生成的跟进任务由用户确认执行。外部验收在获得测试仓库权限后进行。

## 验证

- Clippy 0.1.99 已安装。`cargo +1.99.1 clippy --workspace --all-targets --features qa-mode -- -D warnings` 退出 101，首次报告 `utils`、`embedded-ssh` 和 Tauri 构建脚本的源码诊断。日志 `F:/vk-targets/main/m0-public-clippy.log`。
- 运行证据与合并结论由各任务返回后补录。

## 结果

result: 已启动并行实现，待验收。

## 终止检查

- decision: continue
- reason: M1 双端已渲染共享组件，仍需要真实服务场景。第一批实现与证据尚待集成。
- remaining_evidence: M0 严格检查及完整构建，M1 闸门，M2–M7，E1–E7 和最终分支引用。
