---
status: complete
updated_at: 2026-11-19
artifact_language: zh-CN
execution_mode: simulated-file-goal
workflow_mode: light
termination_reason: 锁定 nightly 下双端展示页、主题生成、截图矩阵和设计规范均已验收。
handoff_summary: E4 已通过，M7 负责在正式包复验同一主题与字体。
---

# Auto Goal 任务

## 目标

用单一 token 源生成 NSH 主题与 Tailwind 配置。Win32 与 Web 使用同一展示组件和离线字体，保留亮暗、舒适与紧凑四个组合的截图及设计规范。父目标引用 `../../goal.md` 的 E4。

## 约束

锁定 NSH 和 nightly。遵循已有设计 token 及 `docs/design-system.md`。开发工作树为 `F:/vk-wt/m3`，最终打包入口由 M7 持有。

## 执行指导

M3DesignContract 已交付，展示页组件和双端证据见集成提交 `203a6ea60`、`ced0abaf4`。已在独立工作树使用锁定 nightly 复跑 `harness/tools/m3-showcase.sh both`。命令退出 `0`，两端共生成 16 张截图。见 `../../evidence/m3-showcase.md`。
