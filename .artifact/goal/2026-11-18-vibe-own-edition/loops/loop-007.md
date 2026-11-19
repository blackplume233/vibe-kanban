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
- ACP 升级 `4cb6f061c` 的独立检出通过 185 项测试，5 项忽略。MCP 发现用例单独通过，`session/new` 和 `session/load` 均从真实 MCP 进程找到 `list_repos`。Claude 与 Codex 的 [实际适配器验收](../evidence/m6-real-adapters.md) 遇到认证错误。
- M7 的独立分支已生成双端 CLI 包。打包验证新增 `VK_ASSET_DIR`，并确认最初误写到默认数据库的测试记录已按 UUID 清理。主题、正式入口及默认构建切换待集成。
- GitHub 后端 `efe17674f` 的集成复验通过 73 项测试，TypeScript 和 10 个 schema 均通过生成一致性检查。已合入 GitHub 和 Agent 管理面板及会话组件，等待正式入口接线。E5 仍需 Agent 推送后检查转绿，以及评审评论自动派发的新一轮实测。
- E7 的静态托管和默认构建入口已分派到独立工作树。运行约定是 standalone server 同源托管，默认查找可执行文件旁的 `web/`，可用 `VK_WEB_ROOT` 指定目录。对等验收后合入切换。
- 终端 `559999fdc` 已合入。隔离 Win32 和 Chromium 均验证真实 ConPTY 的输入、输出、缩放、切换、手动关闭和自然退出。Win32 脚本的清理阶段超时，复验待补。见 [终端报告](../subagents/m7-terminal-report.md)。
- 测试仓库 PR #2 的 `52da6ee380d64a52fb28c06c1a3583f8907983dc` 经 GitHub API 核对为 `m5/controlled-ci=failure`，描述记录 `grep -Fxq ok ci-state.txt` 退出 `1`。
- 本机 OpenCode 已连到回环 Ollama。CUDA 路径的两次模型加载超时，Gemma 4 用 CPU 和 4096 上下文通过回环 API 返回 `READY`。正在用同一 CPU 设置验证 OpenCode 的实际文件编辑。

## 终止检查

- decision: continue
- reason: M1–M3 已通过。M0、M4–M7 仍需完整构建、功能与实机证据。
- remaining_evidence: E1 最终命令与双端正式包，E2–E3 本地产品流程和性能，E5 实际 GitHub 往返，E6 适配器验证与集成证据，E7 默认构建切换及最终审计。
