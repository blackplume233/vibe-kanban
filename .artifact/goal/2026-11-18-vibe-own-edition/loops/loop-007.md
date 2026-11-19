# 循环 007

## 子目标

验收 M3 展示页，合入 ACP SDK 升级，厘清正式包与本地界面的接线边界。

## 候选与决策

- 连同打包入口一次合入已经交付的后端和展示页，再统一验收。
- 分别合入可验证的展示页和 ACP 升级，从独立检出复验。正式包随共享主题和本地操作界面集成后验收。

采用第二种。M3 展示页和 E6 后端复验可独立执行，正式包证据绑定到最终入口。

## 执行与验证

- M3 集成提交 `ced0abaf4` 通过锁定 nightly 双端展示页复验。Win32 与 Chromium 各生成 8 张截图。Chromium 检查了密度、搜索、空态、键盘打开及失败后保留输入。见 [E4 证据](../evidence/m3-showcase.md)。
- `90d4f723b` 在锁定 nightly 通过 `cargo test --locked --workspace`，340 项通过，4 项忽略。见 [构建记录](../evidence/m0-build.md)。
- ACP SDK 升级与 LF 修复已合入 `4cb6f061c`。该提交的独立检出正在复验受影响的后端和 `session/new`、`session/load` 的 MCP 工具发现。实际 Claude/Codex 适配器和 NSH 管理入口仍待验证。
- M7 的独立分支已生成双端 CLI 包。打包验证新增 `VK_ASSET_DIR`，并确认最初误写到默认数据库的测试记录已按 UUID 清理。主题、正式入口及默认构建切换待集成。
- GitHub 后端通过隔离分支 72 项测试后合入 `efe17674f`，包含 PR 检查与修复派发、review 草稿、Issue 三方同步和持久队列接线。集成复验正在执行。Agent 推送与检查转绿仍是 E5 的必验步骤。

## 终止检查

- decision: continue
- reason: M1–M3 已通过。M0、M4–M7 仍需完整构建、功能与实机证据。
- remaining_evidence: E1 最终命令与双端正式包，E2–E3 本地产品流程和性能，E5 实际 GitHub 往返，E6 适配器验证与集成证据，E7 默认构建切换及最终审计。
