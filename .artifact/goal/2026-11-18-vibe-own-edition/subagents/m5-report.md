# M5 GitHub 后端报告

分支 `my/m5`。实现及测试证据提交为 `0ca73853d5`，本报告随其后的文档提交交付。

## 变更文件与使用方法

- `crates/git-host/Cargo.toml`、`Cargo.lock`、`crates/git-host/src/{lib,types,azure/cli}.rs` 和 `crates/git-host/src/github/`。GitHub.com 按 `VK_GITHUB_TOKEN`、`GITHUB_TOKEN` 的顺序选择 PAT，两者均未配置时使用 `gh`。保留 `GitHostProvider` 接口。HTTP 凭据仅存在进程内并作为敏感请求头发送。沿用 workspace 已解析到 `0.13.2` 的 `reqwest`，为现有 REST 及 GraphQL 请求复用 HTTP 客户端。
- `crates/services/src/services/pr_monitor.rs` 按每批最多 20 个 PR 查询 GitHub 状态、当前 HEAD 的检查和可选的 inline review，超出响应页容量时读取后续页。轮询基础间隔为 60 秒，失败后指数退避并加抖动，按 `Retry-After`、限额重置和 GraphQL `rateLimit` 延后请求。GitHub Enterprise 和 Azure DevOps 沿用各自的 CLI 状态路径。
- `crates/services/src/services/{mod,pr_follow_up,queued_message}.rs`、`crates/db/src/models/pull_request.rs` 和迁移 `20261119200000_github_review_follow_ups.sql`。设置 `VK_GITHUB_REVIEW_AUTO_FOLLOW_UP=1` 或 `true` 后重启后端，已跟踪 PR 上来自仓库所有者、成员或协作者的新 inline review 评论会形成 `DRAFT_FOLLOW_UP` 会话草稿，由用户查看并确认。已有草稿保留。草稿和去重 ID 通过同一个 SQLite 事务保存，数据库复用现有 Scratch 接口。新增 SQL 为运行时参数化查询，无需生成 SQLx 宏缓存。
- `crates/server/src/routes/workspaces/pr.rs`、`crates/server/src/bin/generate_types.rs` 和生成的 `shared/types.ts`。`GET /api/workspaces/{id}/pull-requests/detail?repo_id={id}` 返回 PR 明细和 `checks`。用户确认后调用 `POST /api/workspaces/{id}/pull-requests/fix-ci?repo_id={id}`，后端重新读取失败项并发送给最近执行过的工作区 Agent。该 Agent 正在运行且现有队列有空位时返回 `queued`，空闲时启动并返回 `started`。CI 日志和评论当作外部数据处理，自动 review 分支的输出仅为草稿。
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

- E1 增加四个 crate 的测试与共享类型一致性结果。严格 Clippy 仍待集成复验。
- E5 增加 PAT 传输、批量轮询、CI 修复 prompt 和 review 草稿去重的离线证据。真实 GitHub 演示和 Issue 双向同步均待完成。
- 建议 `plan.md` 中 M5 标为 `in-progress`，状态文本可写为「PR 后端已实现 PAT、CI 汇总及人工确认修复、review 草稿、GraphQL 批量轮询，离线测试与类型检查通过。待严格 Clippy 集成复验、Issue 双向同步和 E5 真实仓库演示。」

## E5 演示条件

1. 由用户准备授权的测试仓库，配置可读 PR、review 和 checks 的 PAT 或 `gh` 登录，安排一个已跟踪 PR 和一个已有 Agent 会话。Git 推送使用测试仓库授权的 Git 凭据。
2. 配置可控的失败 CI。通过 PR 明细读到失败项，人工确认修复，检查 Agent 输出并授权测试分支推送，再观察同一 PR 的 CI 通过。
3. 开启 review 草稿功能，由仓库协作者提交一条 inline review，查看会话草稿并手动发送。重复轮询验证同一条评论只创建一次草稿。
4. 在 M2 的本地议题 API 与后续 GitHub Issue 同步接通后，使用获授权的 Issue 写权限完成一次往返。
