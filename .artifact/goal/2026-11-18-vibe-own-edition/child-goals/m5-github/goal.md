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

先验收持久队列接线和 NSH 控件的真实 HTTP 请求。再记录对应 PR 提交的失败检查、启动一个修复任务、Agent 推送以及该检查恢复。分别验证 review 评论触发 follow-up 与一次真实 Issue 双向往返。测试凭据写 Actions 工作流返回 404。可重复的外部 CI 用实际测试退出码和精确提交上报同一检查。
