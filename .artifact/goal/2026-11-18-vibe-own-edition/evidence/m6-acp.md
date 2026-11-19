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

## 集成复验

独立检出 `4cb6f061ccdcf40afadf92a9fb8969a4d0cd1abf`，加载 `harness/env.sh`，设置 `SQLX_OFFLINE=true`。锁定 nightly 上执行 `cargo test --locked --offline -p db -p executors -p services -p local-deployment -p server`，185 项通过，5 项忽略。另行构建 `vibe-kanban-mcp` 并运行上述 MCP 测试，两个请求均发现 `list_repos`。原始日志位于 `F:/vk-targets/main/m6-acp-upgrade-4cb6f061c.log`。已安装适配器的单独认证结果见 [实际联调](m6-real-adapters.md)。

## 真实本地模型

OpenCode 1.19.3 通过 `opencode acp --pure` 使用回环 Ollama 的 `gemma4:latest`。独立 ACP 请求返回 `end_turn`，用量为 2,282 输入、144 输出 token。

产品验收通过隔离的 `COPILOT/OPENCODE_PROBE` 预设覆盖启动命令。`POST /api/workspaces/start` 从真实 Git 仓库创建工作区，执行状态为 `completed`。`GET /api/execution-usage?session_id=9ad16c62-ffa1-46a2-8c9d-96e5b2c993cd` 返回 2,046 输入、38 输出 token、`cost_usd=0.0` 和 `unpriced_samples=0`。探针在正成本断言上返回 1。正费率及适配器成本累计量由上面的受控测试覆盖。当前 ACP Registry 的 OpenCode 条目使用二进制分发，本次采用本机已安装的可执行文件。

## 受信远程节点

仅设置 `trusted_node` 的 ACP 预设连接配置节点。Windows `cargo test --locked -p executors trusted_node -- --include-ignored` 通过两个用例，验证会话绑定、路径转换和带用量的取消回执。`cargo clippy --locked -p executors --all-targets -- -D warnings` 退出 0。

Ubuntu 上的真实 Python 子系统使用两个独立进程，确认共享工作目录和 Git 元数据、拒绝未验证的目录，并在通道关闭后回收 Agent PID。独立命名空间内启动真实 OpenSSH 服务，使用临时身份和主机密钥。错误主机密钥被拒绝。正确密钥连接到 `vibe-acp` 子系统，ACP v1 握手成功，断连后的 Agent PID 已消失。SSH 探针使用受控协议对端。实际模型与跨进程会话恢复由 [已安装适配器联调](m6-real-adapters.md) 验证。
