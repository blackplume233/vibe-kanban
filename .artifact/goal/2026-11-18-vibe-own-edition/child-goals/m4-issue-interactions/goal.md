---
status: active
updated_at: 2026-11-19
artifact_language: zh-CN
execution_mode: simulated-file-goal
workflow_mode: light
---

# Auto Goal 任务

## 目标

NSH 提供真实 SQLite 议题的看板与列表、拖放、快速搜索、行内编辑、peek、triage、cycle、子议题、关系和保存视图。键盘与指针复用动作注册表，保留乐观更新、失败回滚及焦点。满足父目标 `../../goal.md` 的 E2、E3。

## 约束

UI 遵循 M3 token。M4InteractionCore 在 `F:/vk-wt/m4` 持有议题 UI 与共享 App，工作区、Agent 会话、终端与管理面板按对应模块契约接入。使用隔离数据库、仓库和目录。

## 执行指导

已合入动作、搜索、议题 API 和看板状态。状态切片的 19 项测试和 WASM 检查见 `../../subagents/m4-core-report.md`。在 Win32 与 Chromium 运行真实界面，记录完整鼠标与键盘路径、输入法、失败回滚和焦点。对 5000 条实际议题测 Release 帧耗时，并记录双端冷启动到可操作时间。
