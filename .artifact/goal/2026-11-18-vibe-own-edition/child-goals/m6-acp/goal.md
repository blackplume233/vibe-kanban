---
status: active
updated_at: 2026-11-19
artifact_language: zh-CN
execution_mode: simulated-file-goal
workflow_mode: light
---

# Auto Goal 任务

## 目标

查询 ACP 会话用量和成本，设置全局并发与跨重启的优先级队列，并在代理内发现 MCP 工具。验证 Claude 与 Codex 的通用 ACP 路径及已有会话兼容，移除 0.8 桥接。父目标 `../../goal.md` 的 E6 以实际进程和 API 为准。

## 约束

协议升级工作树 `F:/vk-wt/m6`。管理与配置面板分别在 `F:/vk-wt/m6-nsh`、`F:/vk-wt/m6-registry-ui` 开发。仅在隔离应用数据目录中安装适配器、编辑 profiles 和 MCP 配置。

## 执行指导

用量、队列和 MCP 请求已合入，ACP 升级和 LF 修复见 `4cb6f061c`。切片通过 185 项测试，MCP 测试代理通过实际 `vibe-kanban-mcp` 发现 `list_repos`。集成复验进行中。补齐实际 Claude/Codex 路径、配置读写及 NSH 查询和编辑证据。参见 `../../evidence/m6-acp.md` 和 `../../subagents/m6-report.md`。
