# M5 GitHub Issue 双向同步验证

集成基线为 `my/main` 的 `cff4d5b64a307ee3d452bc44fcdc44d6e5799e22`，在 `my/m5` 上对应合并提交 `d8c06c0df`。Rust 版本为 `1.99.0-nightly (ad3d0bc14 2026-07-31)`。

## API

`POST /api/projects/{project_id}/github/issues/sync` 接收仓库、Issue 编号和项目中的状态映射。使用现有 `ApiResponse` 封装。下面是一个请求示例，状态 UUID 应替换为项目中的实际值。

```json
{
  "repository": "owner/repo",
  "number": 42,
  "open_status_id": "00000000-0000-4000-8000-000000000001",
  "closed_status_id": "00000000-0000-4000-8000-000000000002",
  "write_back": false
}
```

首次请求按 `external_provider=github`、`external_id=owner/repo#42` 创建或识别本地记录。同步比较本地和 GitHub 的标题、描述和开关状态，并将上次同步的值持久化到 SQLite。请求显式传 `write_back: true` 时才可回写。响应 `outcome` 为 `imported`、`synced`、`unchanged`、`pending_write` 或 `conflict`。`fields` 为待回写、冲突或本次同步变更的字段名，具体含义取决于 `outcome`。

GitHub Issue REST API 缺少受支持的原子条件更新。回写会在发送字段级 PATCH 前再次读取 GitHub 版本，并在保存本地结果时检查本地快照。GitHub 侧复查与 PATCH 之间仍有竞态窗口。

## 夹具测试结果

```sh
cargo +nightly-2026-08-01 test --offline -p git-host -p db -p services -p server
```

退出码 `0`，共 69 项通过。`db` 9 项、`git-host` 25 项、`server` 24 项、`services` 6 项单测和 5 项集成测试。

| 测试 | 实测结果 |
| --- | --- |
| `fetches_and_patches_issue_against_local_fixture_server` | 临时 `127.0.0.1` HTTP 服务接收 `GET /repos/acme/repo/issues/42` 和同路径的 `PATCH`。夹具响应从 `open` 转为 `closed`，PATCH 仅包含 `state` 和用于清空描述的 `body` |
| `repository_input_is_canonical_and_constrained` | `Acme/ISSUES.tools` 规范化为 `acme/issues.tools`。URL、目录遍历段和多余路径均被拒绝 |
| `imports_syncs_both_directions_idempotently_and_preserves_conflicts` | 内存 SQLite 执行实际迁移。导入结果为 `acme/repo#42`，重复同步保留 UUID 和更新时间。只读同步合并远端关闭状态并保留待回写的本地标题和描述。显式回写一次，重复调用未新增 PATCH。双方分别改标题时返回 `conflict`，两边标题保留 |

GitHub API 写入仅发生在回环测试服务。E5 真实仓库演示仍需授权凭据和测试仓库。
