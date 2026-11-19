---
status: active
updated_at: 2026-11-19
artifact_language: zh-CN
execution_mode: simulated-file-goal
workflow_mode: light
---

# Auto Goal 任务

## 目标

实现可见的 PR 检查状态、CI 修复派发、review 自动续接、批量 GraphQL 轮询与退避、Issue 双向同步及 PAT 认证。使用父目标 `../../goal.md` 的 E5 路径验收完整产品。

## 约束

仅在私有 `blackplume233/vibe-own-sandbox` 写远端测试数据，凭据仅在环境或系统密钥链。后端由 M5GitHubCore-2 在 `F:/vk-wt/m5` 持有。M5GitHubPanel 在 `F:/vk-wt/m5-nsh` 使用共享 API 并避免并发写同一远端数据。

## 执行指导

后端与 M6 队列接线已合入 `efe17674f`，隔离分支通过 72 项测试，集成复验正在执行。测试仓库已验证 Issue 往返、review 草稿、失败检查和 CI 修复派发。还需验证 Agent 推送后的检查恢复、review 自动续接以及 NSH 控件的真实 HTTP 请求。测试凭据写入 Actions 工作流返回 404，外部 CI 使用实际测试退出码和对应提交上报同一检查。
