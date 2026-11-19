# M5 GitHub 后端验证

## 离线测试

在 `my/m5` worktree 中加载 `harness/env.sh`，设置 `CARGO_TARGET_DIR=F:/vk-targets/m5` 后执行：

```sh
cargo +1.99.1 test --offline -p git-host -p db -p services -p server
```

退出码 `0`。`db` 通过 4 项，`git-host` 通过 23 项，`server` 通过 24 项，`services` 通过 5 项单测和 5 项集成测试。

| 验证点 | 关键输出 |
| --- | --- |
| 一次 GraphQL 查询为两个 PR 设置独立别名和变量，按配置包含 review | `builds_one_parameterized_batch_with_optional_reviews ... ok` |
| GraphQL fixture 包含一个不可访问的 PR。另一个 PR 汇总 5 项检查，其中 2 通过、2 失败、1 等待，解析失败链接，并过滤未提交的 review | `parses_mixed_check_states_review_comments_and_partial_results ... ok` |
| REST fixture 解析 inline review ID、行号和 PR 合并状态 | `parses_rest_review_comment_and_merge_status ... ok` |
| 等待时间采用 `Retry-After` 与主限额重置时间中较晚者。轮询失败后指数退避并附加抖动 | `honors_later_of_retry_after_and_primary_reset ... ok`、`polling_uses_capped_exponential_backoff_jitter_and_github_reset ... ok` |
| 修复 CI prompt 包含失败检查名称和可用链接。review 草稿合并新评论并过滤重复、过旧和非受信任协作者评论 | `ci_prompt_contains_only_failed_checks_and_their_links ... ok`、`review_prompt_groups_new_trusted_comments_once ... ok` |
| 实际 SQLite 迁移执行后，写入草稿与评论 ID。已有草稿保留，重复 ID 只记录一次 | `review_draft_records_ids_once_and_preserves_existing_draft ... ok` |

夹具按 GitHub GraphQL 和 REST 响应结构编写。测试完全离线运行。GraphQL review ID 字段和分支过滤参数对照了 GitHub 公布的 schema。[GitHub GraphQL schema](https://docs.github.com/public/fpt/schema.docs.graphql)

## 授权仓库的只读验证

`gh auth status --hostname github.com >/dev/null 2>&1` 退出码 `1`。本次未达到执行 `blackplume233/vibe-kanban` 只读 GraphQL 查询的登录前置条件。E5 的真实仓库演示仍待授权环境复验。
