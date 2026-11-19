# M6 NSH 代理管理面板交付报告

## 挂载

在 `crates/nsh-app/src/lib.rs` 声明 `pub mod agent_management;` 后，可在共享根使用 `agent_management::agent_management()`。需要跟随会话时，使用 `agent_management::agent_management().session_id(move || selected_session.get())`，其中 `selected_session` 提供 `Option<SharedString>`，值为本地 session UUID。也可使用 `AgentManagement` 及 `AgentManagementProps { session_id: ... }`。会话参数只筛选用量，执行队列保留全局顺序，并标记当前会话的等待项。

面板在 `crates/nsh-app/src/agent_management/` 复用现有 `Api::request` 和语义主题。绑定更新时清空上一会话的用量，并取消其读取任务。刷新同一范围保留上一份数据，读取失败时提示可能过期。

## API 与单位

| 界面 | 读写契约 |
| --- | --- |
| Token 用量 | `GET /api/execution-usage`，按需传 `session_id`。分开显示输入、输出、缓存读取和缓存写入数量。费用用 USD，单独显示未计价样本数，缺失价格显示「未计价」。 |
| 并发配额 | `GET/PUT /api/execution-queue/config`，范围为正 `u32`。 |
| 等待队列 | `GET /api/execution-queue` 保留服务端顺序，`PATCH /api/execution-queue/{id}` 使用有符号 `i32`。队列返回等待项的执行、会话和工作区 UUID 及入队时间。 |
| 模型价目表 | `GET/PUT /api/execution-usage/prices`。四项费率为有限非负数，单位为 USD / 百万 token，模型 `*` 表示默认费率，执行器与模型 ID 的组合唯一。 |

价格 API 使用整表替换。写入前对照最新表和本次读取的版本，有变化则提示先刷新核对。该 API 当前没有原子版本条件，并发客户端在读取和写入之间仍可能发生覆盖。新增、编辑、删除及调度写入后，面板再从服务端 GET 回读。费用以样本入库时的金额为准。

## 验证环境

- 使用 `nightly-2026-08-01` 和锁定的 NSH `12af3d778b588c2837267ff6aaed678eb5ad72a2`。目标目录使用 `F:/vk-targets/m6-nsh/{server,ui,wasm}`。
- `cargo +nightly-2026-08-01 build --offline -p server --bin server` 编译隔离工作树的真实 server。以 `HOST=127.0.0.1 BACKEND_PORT=59467` 运行，迁移出的数据库为工作树内 `dev_assets/db.v2.sqlite`。
- 启动后在 SQLite 插入同工作区的三个会话，两条完成执行各含一条用量样本，另有一条占槽执行与两条等待执行。所有时间均为 `2026-11-19T12:34:56Z`，工作区 UUID 尾号 `010`，会话尾号 `011`、`012`、`013`，排队执行尾号 `024`、`025`。占槽执行与等待项在同一工作区，使调度器回读期间保持等待状态。
- 使用临时独立 NSH example 直接挂载生产组件，通过 Windows UI Automation 触发按钮及设置字段。截图为 Win32 原生窗口，外框像素为 1116 × 889，窗口 DPI 为 96。共享入口由集成方接线。

## 检查结果

| 检查 | 结果 |
| --- | --- |
| `node crates/nsh-app/design/generate-tokens.mjs --check` | 通过。 |
| `rustfmt +nightly-2026-08-01 --edition 2024 --check crates/nsh-app/src/agent_management/mod.rs` | 通过。 |
| 使用独立 example 的 `cargo test --example agent_management_probe agent_management::` | 2 项通过，覆盖 UUID、配额和优先级边界、费率非负且有限、组合键唯一。 |
| 使用独立 example 的 `cargo check --example agent_management_probe --target wasm32-unknown-unknown` | 通过。 |
| 全局与会话切换 | 全局输入、输出、缓存读、缓存写为 `203/52/17/7`，已计价 `$0.000337 USD`，另有 1 个未计价样本。会话 `011` 为 `80/20/10/3`、`$0.000337 USD`，会话 `012` 为 `123/32/7/4`、未计价，`013` 显示明确空态。快速切换到 `013` 后未出现上一会话的模型记录。 [全局](../evidence/m6-nsh-usage-global.png) · [会话](../evidence/m6-nsh-usage-session.png) · [空态](../evidence/m6-nsh-usage-empty.png) |
| 配额与优先级 UI 写入 | 配额 `0` 在界面报错，独立 GET 仍为 `4`。保存 `2` 后读回 `2`。将执行 `025` 的优先级从 `1` 改到 `9` 后排到执行 `024`（优先级 `3`）之前。独立 API 与 SQLite 查询一致。 [队列回读](../evidence/m6-nsh-queue-readback.png) |
| 新增与编辑价格 | 用界面新增 `COPILOT / *`，费率为 `2/8/0.5/4`。将输入费率改到 `2.25`，独立 API 与 SQLite 获得 `2.25/8/0.5/4`。 [价格空态](../evidence/m6-nsh-prices-empty.png) · [编辑回读](../evidence/m6-nsh-prices-readback.png) |
| 删除价格与历史金额 | 编辑同一行后通过二次确认删除。独立 API 返回 `[]`，SQLite 价目表为空。会话 `011` 的已存费用仍为 `$0.000337 USD`。 |
| 队列空态 | 从夹具移除两条等待记录，再在 Win32 界面刷新。显示「没有等待执行的任务」。 [队列空态](../evidence/m6-nsh-queue-empty.png) |

完整 Win32 验证脚本退出码为 `0`。WebAssembly 已通过编译检查，界面操作记录来自 Win32。
