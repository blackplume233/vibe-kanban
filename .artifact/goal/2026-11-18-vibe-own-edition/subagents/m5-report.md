# M5 GitHub 后端报告

分支 `my/m5`。PR 后端提交 `0ca73853d5`，Issue 同步提交 `daf9caba5e`，M6 队列接线提交 `f3ad6df4a7`。

## 变更文件与使用方法

- `crates/git-host/Cargo.toml`、`Cargo.lock`、`crates/git-host/src/{lib,types,azure/cli}.rs` 和 `crates/git-host/src/github/`。GitHub.com 按 `VK_GITHUB_TOKEN`、`GITHUB_TOKEN` 的顺序选择 PAT，两者均未配置时使用 `gh`。保留 `GitHostProvider` 接口。HTTP 凭据仅存在进程内并作为敏感请求头发送。沿用 workspace 已解析到 `0.13.2` 的 `reqwest`，为现有 REST 及 GraphQL 请求复用 HTTP 客户端。
- `crates/services/src/services/pr_monitor.rs` 按每批最多 20 个 PR 查询 GitHub 状态、当前 HEAD 的检查和可选的 inline review，超出响应页容量时读取后续页。轮询基础间隔为 60 秒，失败后指数退避并加抖动，按 `Retry-After`、限额重置和 GraphQL `rateLimit` 延后请求。GitHub Enterprise 和 Azure DevOps 沿用各自的 CLI 状态路径。
- `crates/services/src/services/{mod,pr_follow_up,queued_message}.rs`、`crates/db/src/models/pull_request.rs` 和迁移 `20261119200000_github_review_follow_ups.sql`。设置 `VK_GITHUB_REVIEW_AUTO_FOLLOW_UP=1` 或 `true` 后重启后端，已跟踪 PR 上来自仓库所有者、成员或协作者的新 inline review 评论会形成 `DRAFT_FOLLOW_UP` 会话草稿，由用户查看并确认。已有草稿保留。草稿和去重 ID 通过同一个 SQLite 事务保存，数据库复用现有 Scratch 接口。新增 SQL 为运行时参数化查询，无需生成 SQLx 宏缓存。
- `crates/server/src/routes/workspaces/pr.rs`、`crates/server/src/bin/generate_types.rs` 和生成的 `shared/types.ts`。`GET /api/workspaces/{id}/pull-requests/detail?repo_id={id}` 返回 PR 明细和 `checks`。用户确认后调用 `POST /api/workspaces/{id}/pull-requests/fix-ci?repo_id={id}`。后端重新读取失败项。Agent 正在运行时，SQLite 仅在会话队列为空时插入修复消息并返回 `queued`。空闲时交由 M6 调度器并返回 `scheduled` 和执行 ID。自动 review 的输出保存在草稿中，由用户确认后发送。
- `crates/git-host/src/github/fixtures/` 存放依照公开接口结构编写的离线夹具。证据见 [m5-core.md](../evidence/m5-core.md)。

## 验证结果

在 M5 worktree 中加载 `harness/env.sh` 并配置专属 `CARGO_TARGET_DIR`。下表为实际退出码。

| 命令 | 退出码 | 关键结果 |
| --- | ---: | --- |
| `cargo +1.99.1 test --offline -p git-host -p db -p services -p server` | 0 | 61 项单元和集成测试通过，包含批量 GraphQL、REST、限流退避、CI prompt 和 SQLite 去重 |
| `cargo +1.99.1 run --offline --bin generate_types` | 0 | 已生成 `shared/types.ts` |
| `cargo +1.99.1 run --offline --bin generate_types -- --check` | 0 | `shared/types.ts is up to date` |
| `cargo +1.99.1 clippy --offline -p git-host -p db -p services -p server --all-targets -- -D warnings` | 101 | M5 基线的 `utils/command_ext.rs:36` 有未使用导入，`utils/shell.rs:336` 有同类型转换。协调者已在集成分支处理 Windows 基线，严格 Clippy 待集成复验 |
| 上一条添加 `--no-deps` | 101 | `git-host` 现有 `GitHostProvider` 的 `#[async_trait]` 展开触发 `double_must_use` |
| 上一条添加 `-A clippy::double_must_use` | 101 | 通过 `db` 与 `git-host` 的 lint，随后 `services/analytics.rs:139-140` 现有 Windows 分支触发 `collapsible_if` 和 `needless_borrows_for_generic_args` |
| `gh auth status --hostname github.com >/dev/null 2>&1` | 1 | 只读查询的登录条件未满足，未查询真实 PR |

## E1–E7 与里程碑建议

- E1 增加四个 crate 的测试、锁定 nightly 严格 Clippy 与共享类型一致性结果。
- E5 已实测 Issue 往返、外部 CI 失败上报、review 草稿和人工确认修复派发。自动 review 已观察到实际调度，Claude 续接返回失败。[Sandbox 验收](../evidence/m5-sandbox.md)
- 建议 `plan.md` 中 M5 标为 `in-progress`，状态文本可写为「GitHub 后端已实现 PAT、CI 汇总及人工确认修复、review 草稿、GraphQL 批量轮询和 Issue 双向同步。离线测试通过，待 E5 真实仓库演示。」

## E5 演示条件

1. 使用授权私有 sandbox 继续真实验收，其中 PR #2 已记录失败的 `m5/controlled-ci`。
2. 配置可控的失败 CI。通过 PR 明细读到失败项，人工确认修复，检查 Agent 输出并授权测试分支推送，再观察同一 PR 的 CI 通过。
3. 开启 review 草稿功能，由仓库协作者提交一条 inline review，查看会话草稿并手动发送。重复轮询验证同一条评论只创建一次草稿。
4. 使用获授权的 Issue 写权限调用同步 API。先导入，再修改本地议题并显式开启 `write_back`，最后验证 GitHub 上的变更。

## Issue 双向同步与集成基线

合入 `my/main` 的 `cff4d5b64a307ee3d452bc44fcdc44d6e5799e22`，合并提交为 `d8c06c0df`。补齐 `generate_types.rs` 中两个里程碑的注册块。`PullRequestDetail.checks` 的字段文档触发了 ts-rs 在换行前生成空格。生成器逐行清理声明末尾空白，保留字段文档。

- `crates/git-host/src/github/{issues,cli,http,mod}.rs` 扩展现有 PAT 与 `gh` 客户端，支持 Issue 读取和字段级回写。`fixtures/rest-issue*.json` 为本地 HTTP 测试响应。
- `crates/db/src/models/{issue,issue_sync,mod}.rs` 与 `20261119200001_github_issue_sync.sql` 沿用 M2 的 `github` 外部标识并保存上次同步值。导入和本地合并与基线一起提交。
- `crates/services/src/services/{github_issue_sync,mod}.rs` 执行标题、描述和开关状态的三方比较。若同一字段冲突，两端保留各自内容。`crates/server/src/routes/issues.rs` 提供同步 API。
- `cargo +nightly-2026-08-01 test --offline -p git-host -p db -p services -p server` 退出码 `0`，通过 69 项。API 示例、隔离 HTTP 和 SQLite 测试见 [m5-issue-sync.md](../evidence/m5-issue-sync.md)。

## M6 队列集成

合入 `my/main` 的 `dd3372b3d`，合并提交为 `75cb2d1c3`。队列回归测试验证已有编辑器消息被保留、运行中的会话只在空位时插入、运行结束后拒绝插入。`cargo +nightly-2026-08-01 test --offline -p git-host -p db -p services -p server` 退出码 `0`，72 项通过。相同 crate 的 `clippy --all-targets -- -D warnings`、`generate_types`、`generate_types -- --check` 和 `cargo +nightly-2026-08-01 build --offline -p server --bin server` 均退出 `0`。

## 自动 review 讨论

基线为 `76ffbcc8e`。`VK_GITHUB_REVIEW_DISPATCH_ALLOWLIST` 以逗号分隔 `owner/repo@login`，要求 GitHub 仓库和有受信任关联的评审者均匹配。对现有 Claude 会话开启纯文本续接。Claude 启动参数关闭工具、MCP、技能和 hooks。SQLite 事务同时记录评论 ID 与 M6 调度条目，保留现有用户草稿和消息。原草稿模式仍可独立开启。

锁定 nightly 下 75 项测试、严格 Clippy、类型生成及一致性检查、server 构建均退出 `0`。真实 sandbox 的一次行内评论成功触发自动调度。Claude 续接以 `failed` 结束。Agent 修改及推送和外部 CI 成功上报仍待实测。
