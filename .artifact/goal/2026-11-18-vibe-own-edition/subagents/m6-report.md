# M6 ACP Agent 管理交付报告

分支 `my/m6` 的实现提交为 `02e3c0e902ae1b7c381171a134b285bcc1a159a4`。本报告与验收记录位于其后的文档提交。

## 改动

- `crates/executors/src/executors/acp/`、`acp_server.rs` 和 `crates/executors/src/logs/mod.rs` 归一化 ACP `UsageUpdate` 与 `PromptResponse`。`UsageUpdate.used` 表示上下文占用，账单用量使用响应中的 token 分类和可用的美元成本。注册表执行器从既有 MCP 配置接口读取各 agent 独立配置，在 `session/new` 和 `session/load` 中传入所支持的 MCP transport。
- `crates/executors/src/executors/{claude.rs,codex/normalize_logs.rs,opencode/}` 将各执行器可提供的账单分类写入统一事件。Claude 的 result 数据可提供模型明细，Codex 采用 thread 累计 token，OpenCode 按已完成 step 记录。
- `crates/db/migrations/2026111930000{0,1}_*.sql`、`crates/db/src/models/{execution_usage,execution_queue}.rs` 和 `crates/services/src/services/{execution_process,queued_message}.rs` 新增 SQLite 用量、价目表、累计快照基线、排队记录和会话草稿。累计用量按 provider session 记录增量。缺失美元报价的样本由 `unpriced_samples` 标明。费率修改作用于后续入库样本。
- `crates/local-deployment/src/{container,lib}.rs`、`crates/services/src/services/container.rs` 和 `crates/server/src/startup.rs` 接入实际 Agent 启动与完成路径。SQLite 全局上限默认 `4`，同一工作区一次只调度一个 Agent。优先级数值越高越先派发，同优先级采用入队顺序。每个 session 的旧 follow-up 端点保留替换草稿语义，草稿以事务转换为全局队列项。重启会恢复等待项。已领取的运行项由既有 orphan 清理标为失败，避免已执行过的操作被自动重放。
- `crates/server/src/routes/agent_management.rs` 新增 `GET /api/execution-usage`（支持 `session_id`、`workspace_id`、RFC 3339 `from`/`to`，时间区间为 `[from,to)`）、`GET/PUT /api/execution-usage/prices`、`GET /api/execution-queue`、`GET/PUT /api/execution-queue/config` 和 `PATCH /api/execution-queue/{id}`。更新按 executor/model 设置精确或 `*` 通配费率。排队记录继续关联原 execution process ID。旧流程可通过现有停止端点取消等待项。路由注册及 `generate_types.rs` 使用追加块，并生成 `shared/types.ts`。没有新增应用依赖。

## 验证

命令在 `my/m6` 工作树运行，加载里程碑环境，使用 Rust `1.99.1` 与该里程碑专用 target 目录。

| 验证 | 退出码与结果 |
|---|---|
| `cargo +1.99.1 test --offline -p db -p executors -p services -p local-deployment -p server` | `0`，175 项通过，4 项忽略（含文档测试）。|
| `cargo +1.99.1 run --offline --bin generate_types -- --check` | `0`，`shared/types.ts is up to date`，10 个 schema 一致。|
| `cargo +1.99.1 build --offline -p server --bin server` + 两次回环 HTTP 服务启动 | `0`，真实路由返回用量、计算成本与代理报价，等待项以原 ID 在重启后执行，续接累计计数只记录增量。|
| `cargo +1.99.1 clippy --offline -p db -p executors -p services -p local-deployment -p server --all-targets -- -D warnings` | `101`，原工作树基线的 Windows `utils` 触发 `unused_imports`（`command_ext.rs:36`）与 `useless_conversion`（`shell.rs:337`）。|
| Clippy 补充定位 | `--no-deps` 遇到 `async_trait` 展开的既有 `double_must_use`。继续对它及 `services::analytics` 既有 Windows `collapsible_if`、`needless_borrows_for_generic_args` 加命令行豁免后，所涉 crate 的所有 target 检查退出 `0`。这些豁免仅用于定位，无法替代严格验收。|
| `node scripts/prepare-db.js` | `0`，用固定版本 `sqlx-cli 0.8.6` 应用两条 M6 迁移并再生成 `crates/db/.sqlx`。新增 SQL 使用运行时绑定，既有查询元数据没有内容差异。|

[E6 请求、响应和用例明细](../evidence/m6-acp.md) 覆盖上限、优先级 FIFO、重启恢复、MCP 请求和用量查询。E1 仅取得表中涉及 crate 的局部证据。E2–E5、E7 由其他里程碑负责。

## 兼容性与待决事项

- Claude/Codex 保留专用执行器和现有 `profiles.json` 键。Claude 依赖 CLI 的 router 参数、权限选项、hooks 和按消息恢复（`claude.rs` 的 command builder 与 `spawn_follow_up`）。Codex 依赖 app-server 的 sandbox/审批设置、线程 fork、native review 及恢复（`codex.rs` 的 review、thread 启动和续接）。通用 ACP 目前只有通用会话、模型、模式、权限配置。统一切换需要这些能力的 ACP 对等实现与已有会话迁移，0.8 桥接仍由旧事件与 harness 使用。无需为当前切片调整已有 profiles。
- 可选远程执行接缝为全局队列的领取/派发边界。未来由服务端选择节点，签发带租期与运行 ID 的凭据，将 action、工作区定位与节点能力交给受信节点。节点回传日志、用量、退出状态和心跳。租期收回时先确认上一节点运行的处置结果，再决定是否重派发。当前仅本机派发。
- 建议 `plan.md` 的 M6 状态为 `in-progress`。E6 已取得本地 HTTP/ACP 和请求级 MCP 证据。严格 Clippy 需在 Windows 基线修复后复核，Claude/Codex 统一 ACP 受配置与会话兼容性约束。相关决策是接受现有专用执行器作为里程碑兼容路径，或要求补齐对等能力后关闭统一 ACP 项。

## 与 M2 及 Windows 基线整合

`my/m6` 在合并提交 `45535d286` 纳入 `my/main` 的 `7af50286a`。

- `crates/db/src/models/mod.rs` 同时导出 M2 的 `issue`、`issue_board`、`issue_links` 和 M6 的 `execution_queue`、`execution_usage`。两组迁移按时间戳独立保留。
- `crates/server/src/routes/mod.rs` 同时挂载 `agent_management::router()` 与 `issues::router()`，两者继续使用现有 relay middleware。
- `generate_types.rs` 保留两组追加声明。顺序采用现有分支的 M6 段在前，随后追加 M2 段。`shared/types.ts` 以合并后的 Rust 声明为准，使用仓库锁定工具链生成。
- `.gitattributes` 使用 `my/main` 的 LF 约定。M2、M6 的 DB 新查询均使用运行时绑定，合并沿用现有 `.sqlx` 元数据。
- `cargo +nightly-2026-08-01 run --offline -p server --bin generate_types` 生成后，`generate_types.exe --check` 退出 `0`，确认 TypeScript 与 10 个 schema 字节一致。合并后的 `cargo +nightly-2026-08-01 clippy --offline -p db -p executors -p services -p local-deployment -p server --all-targets -- -D warnings` 编译检查完成。
