---
status: active
updated_at: 2026-11-19
artifact_language: zh-CN
execution_mode: simulated-file-goal
workflow_mode: light
---

# Auto Goal 任务

## 目标

用单一 token 源生成 NSH 主题与 Tailwind 配置。Win32 与 Web 使用同一展示组件和离线字体，保留亮暗、舒适与紧凑四个组合的截图及设计规范。父目标引用 `../../goal.md` 的 E4。

## 约束

锁定 NSH 和 nightly。遵循已有设计 token 及 `docs/design-system.md`。开发工作树为 `F:/vk-wt/m3`，最终打包入口由 M7 持有。

## 执行指导

M3DesignContract 已交付。主题生成已合入，展示页组件和双端证据位于集成提交 `203a6ea60`、`ced0abaf4`。在 `F:/vk-wt/verify-m3` 重放 `harness/tools/m3-showcase.sh both`，以生成检查、实际窗口及浏览器证据验收。正式包嵌入同一主题和字体后复验实际入口。证据见 `../../evidence/m3-showcase.md`。
