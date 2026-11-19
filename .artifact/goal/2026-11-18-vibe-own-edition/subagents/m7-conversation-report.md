# M7 NSH Agent 会话

实现位于 `my/m7-conversation` 的 `a4ddb4d7bed6cdaf65d12f3f8f7dff2ef7b7d86f`。接线入口为 `crate::agent_conversation::agent_conversation(api, workspace_id)`。在共享根声明 `mod agent_conversation`，并用工作区 ID 作为挂载 key。隔离验证用 `each` 的工作区 ID key；浏览器在切换会话时收到旧会话进程 WS 的关闭事件，切换工作区时收到旧全局审批 WS 的关闭事件。组件退出后重进会从服务重新载入记录。同一工作区内按会话保留未发送草稿。

## 行为与接口

- `GET /api/sessions` 列出会话，`POST /api/sessions` 新建会话。Agent 选择读取 `/api/info` 的执行器配置。已有会话遵守服务端执行器绑定，新会话可选择执行器。
- `POST /api/sessions/{id}/follow-up` 提交后续消息。当前会话有正在执行的操作时，通过 `POST /api/sessions/{id}/queue` 保存同会话后续消息，使用 `DELETE` 取消。全局 `/api/execution-queue` 按会话和执行进程 ID 关联，显示 SQLite 调度等待项及优先级。运行中的操作和 SQLite 调度等待项均调用既有执行进程停止端点。
- 通过已有进程与归一化日志 WS 显示会话记录。审批 WS 的记录按运行中的执行进程归属过滤，批准与拒绝通过既有 `/api/approvals/{id}/respond` 提交。ACP 测试进程收到两种人工选择。问答型请求只显示等待状态，批准和拒绝仅适用于工具权限请求。

## 取证

在隔离工作树构建真实 `server.exe`，通过本地 HTTP 导入临时 Git 仓库、建立工作区和会话，设置 Gemini 使用通过 stdio 发送 ACP 测试消息的本地进程，并将 `yolo` 设为 `false`。Chrome 157.0.8103.50 的视口为 1180 × 900。使用隔离的 NSH 挂载入口和正式设计 token，HTTP 转发保留浏览器 Origin，WebSocket 直接连后端。核心 UI 流程退出码为 `0`，无浏览器页面错误。

| 实测场景 | 证据 |
| --- | --- |
| 正常 follow-up 进入执行中，收到日志和真实权限请求 | [执行中截图](../evidence/m7-conversation-running.png) |
| 等待批准时发送后续消息，SQLite 读回文本，取消后读回 `empty` | [审批与排队截图](../evidence/m7-conversation-queue.png) |
| 在另一个会话提交操作，同工作区配额触发 M6 调度等待，停止后全局队列为空 | [M6 调度等待截图](../evidence/m7-conversation-scheduled.png) |
| 用户批准和拒绝，ACP 记录分别返回 `allow` 和 `deny`，另一次等待权限中的执行通过停止端点终止 | [拒绝结果截图](../evidence/m7-conversation-denial.png) |
| 新建会话并选择 Gemini，切回后保留输入草稿和先前的批准与拒绝记录 | [Agent 选择截图](../evidence/m7-conversation-agent-selection.png) |
| 切换工作区时旧 WS 关闭，返回后从本地服务恢复已提交的记录 | [恢复截图](../evidence/m7-conversation-restored.png) |

[浏览器 HTTP 与 WebSocket 记录](../evidence/m7-conversation-browser.txt)

使用锁定 `nightly-2026-08-01` 和 `CARGO_TARGET_DIR=F:/vk-targets/m7-conversation` 执行以下检查。Windows 编译和两项单测通过，WASM 构建、`wasm-bindgen` 及浏览器用例通过。图形界面证据来自 Chrome。Win32 仅进行了编译和单元测试。

```bash
cargo +nightly-2026-08-01 build --offline --locked -p server --bin server
cargo +nightly-2026-08-01 build --manifest-path crates/nsh-app/Cargo.toml --locked --offline --target wasm32-unknown-unknown --lib
cargo +nightly-2026-08-01 test --manifest-path crates/nsh-app/Cargo.toml --locked --offline --lib agent_conversation
```

证据用的 `conversation_smoke.rs`、`acp_fixture.py`、`live_conversation.py`、`site/` 及构建和服务日志位于 `F:/vk-targets/m7-conversation`。取证完成后从应用工作树移除了临时挂载入口和测试数据。共享应用根的正式接线由 M7 集成处理。
