# M6 ACP 验收记录

## 本地 HTTP 验证

Windows 上构建 `server.exe`，启动两次独立的回环 HTTP 服务，共用同一个工作树中的 SQLite 数据库。Copilot 测试预设通过 `base_command_override` 启动本地 Node.js ACP 模拟进程，使用真实的 ACP v2 harness、归一化日志、用量写入和 API。Git 仓库夹具只有一个空提交。最终运行退出码为 `0`。

1. `PUT /api/execution-queue/config` 提交 `{"max_concurrent":1}`。响应为 `{"max_concurrent":1}`。
2. `PUT /api/execution-usage/prices` 配置 `COPILOT` 的 `*` 模型价目。每百万普通输入、输出、缓存读取和写入 token 的美元费率分别为 `2`、`8`、`0.5`、`4`。
3. 用 `POST /api/workspaces/start` 发出首轮 ACP prompt。模拟进程通过 `session/update` 上报上下文占用，并在 `session/prompt` 响应中返回普通输入 80、输出 20、缓存读取 10、缓存写入 3。首次 `GET /api/execution-usage?session_id=<priced-session>` 返回以下 `data`。

   ```json
   [{"executor":"COPILOT","model":null,"input_tokens":80,"output_tokens":20,"cache_read_tokens":10,"cache_write_tokens":3,"cost_usd":0.000337,"unpriced_samples":0}]
   ```

4. 保持另一个模拟 ACP 进程的 prompt 打开，再发出第二个任务。`GET /api/execution-queue` 返回一个等待项，且其 session 用量为空。随后终止服务进程并重新启动。

   ```json
   [{"execution_process_id":"8a218bd6-78b4-43fe-ba60-48f363b0e0bd","session_id":"5c3bbfed-69db-43cc-bdb1-50f04b4ef14c","workspace_id":"ccfcb52b-5b3f-4bf3-9cda-9b1fc839d538","priority":0,"queued_at":"2026-11-19T13:54:23.153938200Z"}]
   ```

5. 服务恢复了上一步的等待任务。任务执行后队列为空，并发配置仍为 `1`。该 ACP 进程上报的累积成本为 `$0.0125`，用量 API 返回以下 `data`。

   ```json
   [{"executor":"COPILOT","model":null,"input_tokens":80,"output_tokens":20,"cache_read_tokens":10,"cache_write_tokens":3,"cost_usd":0.0125,"unpriced_samples":0}]
   ```

6. 对同一会话调用 `POST /api/sessions/{session_id}/follow-up`。模拟进程接受 `session/load` 并上报更新后的累计值。用量 API 返回以下 `data`，证明加载后的第二个累计快照只记入一次增量。

   ```json
   [{"executor":"COPILOT","model":null,"input_tokens":160,"output_tokens":40,"cache_read_tokens":20,"cache_write_tokens":6,"cost_usd":0.025,"unpriced_samples":0}]
   ```

## 自动化回归

- `db::models::execution_queue::tests::waits_at_capacity_then_dispatches_by_priority_and_fifo` 验证上限为 `2` 时第三项等待，更高优先级先出队，同优先级 FIFO。
- `db::models::execution_queue::tests::restart_reclaims_only_dispatched_slots_and_draft_is_promoted_once` 验证恢复时保留等待项，以及同一草稿仅成功转换一次。
- `executors::executors::acp::harness_v2::mcp_requests::new_and_loaded_sessions_send_configured_mcp_servers` 用双向 ACP JSON-RPC 测试 `session/new` 和 `session/load` 的实际请求，断言配置的 MCP 描述及 transport 能力筛选。此项证据对应 MCP 配置到达 ACP agent 的请求边界。
- ACP 测试覆盖 `UsageUpdate` 上下文及美元成本映射、`PromptResponse` 用量和两者合并，同时覆盖 Claude、Codex、OpenCode 的归一化计数。

## ACP Agent 发现本地 MCP 工具

`new_and_loaded_sessions_discover_local_mcp_tools` 让 ACP 测试代理接收两条真实的 ACP JSON-RPC 请求。每次测试代理从请求的 `mcpServers` 取得命令及环境，启动仓库的 `vibe-kanban-mcp` 进程，通过 stdio 完成 MCP `initialize`、`notifications/initialized` 和 `tools/list`。测试使用 `--mode global`，可在独立工作树内运行。

```bash
source .artifact/goal/2026-11-18-vibe-own-edition/harness/env.sh
export CARGO_TARGET_DIR=F:/vk-targets/m6 SQLX_OFFLINE=true
cargo +nightly-2026-08-01 build --locked --offline -p mcp --bin vibe-kanban-mcp
VK_TEST_MCP_BIN=F:/vk-targets/m6/debug/vibe-kanban-mcp.exe \
  cargo +nightly-2026-08-01 test --locked --offline -p executors \
  new_and_loaded_sessions_discover_local_mcp_tools -- --ignored --nocapture
```

使用上述 nightly 构建 MCP 可执行文件，并用同一工具链构建的测试二进制执行指定用例。退出码为 `0`，两个 MCP 子进程均在读完工具清单后正常退出。

```text
"session/new" -> MCP tools/list contains list_repos
"session/load" -> MCP tools/list contains list_repos
test result: ok. 1 passed; 0 failed
```
