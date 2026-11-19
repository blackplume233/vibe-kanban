# M2 本地议题底座交付

## 实现

| 文件 | 变更 |
|---|---|
| `crates/db/migrations/20261119100000_local_issues.sql` | 在本地 `projects` 下增加状态、议题、标签、关系、评论、cycle 和保存视图。通过外键及触发器防止跨项目引用和循环父子关系。 |
| `crates/db/src/models/{issue,issue_board,issue_links,project,mod}.rs` | 实现议题 CRUD、筛选、批量更新、状态重排、看板资源和链接的持久化。包含 5 个 M2 测试，覆盖已有项目迁移、查找、排序、事务回滚和关系清理。 |
| `crates/server/src/routes/{issues,mod}.rs` | 注册 `/api/projects` 和项目下的议题及看板资源。 |
| `crates/server/src/bin/generate_types.rs`、`shared/types.ts` | 追加 23 个 ts-rs 声明并生成客户端类型。 |
| `evidence/m2-api.md` | API 契约和本地 `curl` 验收记录。 |

议题编号使用 SQLite `AUTOINCREMENT`，向整个本地库分配稳定的 `VO-` 短 ID。搜索采用项目范围内的 SQLite `LIKE` 字面子串搜索。triage 用独立布尔字段，cycle 以外键关联。M5 同步可用项目内唯一的 `(external_provider, external_id)` 定位议题。无新增项目依赖，SQLx CLI 0.8.6 只安装到本里程碑的临时工具目录执行仓库准备脚本，与项目 SQLx 版本一致。

## 验证

使用 Rust 1.99.1、独立 `CARGO_TARGET_DIR`，从 `harness/env.sh` 加载 LLVM 设置。该文件在 Windows 检出时带 CRLF，执行时先去除行尾 `\r`。提交文件使用 LF。

| 命令 | 退出码和关键输出 |
|---|---|
| `cargo +1.99.1 build -p server --bins --locked --offline` | `0`，生成 `server.exe` 和 `generate_types.exe`。 |
| `cargo +1.99.1 test --locked --offline -p db -p server` | `0`，`db` 8 通过（含 M2 新增 5 个），`server` 24 通过。 |
| `node scripts/prepare-db.js`，通过子进程 `PATH` 提供 SQLx CLI 0.8.6 | `0`，`Applied 20261119100000/migrate local issues` 和 `Database preparation complete!`。现有宏查询的 `.sqlx` 文件经重新生成后无差异。 |
| `cargo +1.99.1 run --locked --offline --bin generate_types` | `0`，生成 `shared/types.ts` 和 10 个 JSON schema。 |
| `cargo +1.99.1 run --locked --offline --bin generate_types -- --check` | `0`，SQLx 准备完成后串行执行，输出 `shared/types.ts is up to date`，schema 检查通过。 |
| `cargo +1.99.1 clippy --locked --offline -p db -p server --all-targets -- -D warnings` | `101`，停在既有 `crates/utils/src/command_ext.rs:36` 的 `unused_imports` 和 `crates/utils/src/shell.rs:336` 的 `clippy::useless_conversion`。 |
| `cargo +1.99.1 clippy --locked --offline -p db -p server --all-targets --no-deps -- -D warnings` | `0`，两个目标 crate 的全目标严格检查通过。 |
| 不设置 `VK_SHARED_API_BASE`，启动 `server.exe` 后用 `curl -fsS` 和 `jq` 发请求并断言结果 | `0`，`VK_SHARED_API_BASE not set; remote features disabled`，`All curl and jq assertions passed.` 请求证据见 [m2-api.md](../evidence/m2-api.md)。 |

## E1–E7 与里程碑建议

- **E1** 有 `db` 和 `server` 的构建、测试、共享类型证据。依赖链的严格 Clippy 仍受 M0 基线警告阻塞，由协调者在合并基线修复后复验。
- **E2** 有真实本地服务的议题 HTTP 证据。原生窗口拖拽、Agent 工作区、diff 和 PR 流程由 M4 与后续里程碑验收。
- **E3** 有筛选、搜索和重排 API。键盘操作、全局模糊搜索和性能数据由 M4 验收。
- **E5** 提供外部来源持久化和定位字段。真实 GitHub 双向同步由 M5 验收。
- 本切片不涉及 E4、E6、E7。无待人工决策事项。M4 可直接按 API 契约接入，M5 可用外部来源字段做同步。

建议 `plan.md` 状态为 `in-progress`（本地 SQLite 议题能力、真实 HTTP 路径、目标 crate 测试及本切片 Clippy 已通过，等待 M0 基线修复后通过带依赖链的严格 Clippy）。

实现提交为 `d337e6445a1f9e803bf756308075263a1fe2b398`。本报告提交位于 `my/m2` 分支末端，执行 `git rev-parse my/m2` 可取得包含报告的最终 HEAD。
