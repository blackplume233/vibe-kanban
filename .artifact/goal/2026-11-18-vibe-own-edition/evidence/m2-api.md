# M2 本地议题 API 验收

## API 契约

所有路径以 `/api` 开头。成功响应使用现有的 `ApiResponse<T>` 封装，数据位于 `data`。下表以 `P` 表示项目 UUID，以 `I` 表示议题 UUID。

| 路径 | 方法和用途 |
|---|---|
| `/projects` | `GET` 列项目，`POST` 用 `{ "name": "…" }` 创建本地项目 |
| `/projects/P/issues` | `GET` 按字段筛选及搜索，`POST` 创建议题。至少提供 `title`、`status_id` |
| `/projects/P/issues/I` | `GET` 详情，`PATCH` 更新，`DELETE` 删除 |
| `/projects/P/issues/batch` | `PATCH` 提交 `[{ "issue_id": "…", "changes": { … } }]`。事务内全部成功或全部回滚 |
| `/projects/P/issues/I/children` | `GET` 列直接子议题，`POST` 使用议题创建结构创建直接子议题。移动或解除父子关系使用议题 `PATCH` |
| `/projects/P/issues/I/relationships` | `GET` 同时列入向和出向关系，`POST` 接收 `related_issue_id`、`relationship_type` |
| `/projects/P/issues/I/relationships/{id}` | `DELETE` 从任一端解除关系 |
| `/projects/P/issues/I/comments` | `GET` 列评论，`POST` 接收 `message` 和可选 `parent_id` |
| `/projects/P/issues/I/comments/{id}` | `PUT` 更新 `message`，`DELETE` 删除评论 |
| `/projects/P/{statuses,tags,cycles,saved-views}` | `GET` 列资源，`POST` 创建 |
| `/projects/P/{statuses,tags,cycles,saved-views}/{id}` | `GET` 读取单项，`PUT` 替换可编辑字段，`DELETE` 删除 |
| `/projects/P/statuses/reorder` | `PUT` 接收 `{ "status_ids": [...] }`，每个现有状态恰好出现一次 |

- `IssueFilter` 中的筛选条件以 AND 叠加，结果按 `sort_order`、`issue_number` 升序排列。`search` 用 SQLite `LIKE` 匹配标题、描述和短 ID 的字面子串，`%`、`_` 和反斜线会转义。默认采用 SQLite 的 ASCII 大小写折叠，中文按字面子串匹配。个人看板已有项目范围索引，使用 `LIKE` 省去维护全文索引，也支持搜索短 ID 片段。全局模糊排序及性能门由 M4 验收。
- `PATCH` 省略字段时保留原值，显式 `null` 可清空可空字段。`tag_ids` 替换整个标签集合，传 `[]` 清空。日期为 `YYYY-MM-DD`，保存视图 `filter` 为 JSON 对象。
- `VO-` 短 ID 在整个本地数据库中单调分配，删除已提交的议题后也不复用。M5 可以用 `external_provider` 和 `external_id` 按项目查找同步记录。GitHub 议题编号要结合仓库作为 `external_id`，例如 `owner/repo#7`。`external_url` 可记录来源链接。
- `blocking` 的方向为源阻塞目标，`duplicate` 的方向为源重复目标，`related` 对称并在数据库只存一次。删除父议题或 cycle 会清空对应引用。跨项目引用、循环父子关系和重复外部来源通过数据库约束拒绝。

## 实测请求与响应

在隔离 worktree 构建 `server.exe`，运行前执行 `unset VK_SHARED_API_BASE`。设置 `HOST=127.0.0.1`、`BACKEND_PORT=0` 和独立临时目录。服务分配端口 `57501`，`GET /api/health` 成功，启动日志包含 `VK_SHARED_API_BASE not set; remote features disabled`。下列请求通过 `curl -fsS` 发送，写入请求使用 `Content-Type: application/json`，响应由 `jq` 断言。脚本退出码为 `0`。

| 请求和重点输入 | 响应 `data` 摘要 |
|---|---|
| `POST /projects`，`name: M2 API evidence` | `id: 99453d19-1715-4df7-8bec-56595187c48d` |
| `POST /projects/P/statuses`，依次建 `Backlog` 和 `In Progress` | `backlog` 在位置 `0`，`started` 在位置 `1` |
| `POST /projects/P/tags`，`name: renderer` | 标签 `4ec6d2dd-ff4b-4133-9762-0cba28a9f193` |
| `POST /projects/P/issues`，`Repair renderer`、`description: Measured at 90%_`、`triage: true`、`external_provider: github`、`external_id: owner/repo#7` | `VO-1`，Backlog，来源及 triage 字段往返一致 |
| `POST /projects/P/issues/I/children`，`Reproduce renderer fault`，再 `GET` 同路径 | `VO-2` 的 `parent_issue_id` 等于 `VO-1` 的 UUID，列表为 `["VO-2"]` |
| `POST /projects/P/issues/I/relationships`，`related_issue_id` 指向 `VO-2`，类型 `blocking`，再从子议题 `GET` | `VO-1` 阻塞 `VO-2`，子议题返回 `["blocking"]` |
| `PUT /projects/P/statuses/reorder`，依次传 In Progress 和 Backlog 的 ID | `[{"name":"In Progress","sort_order":0},{"name":"Backlog","sort_order":1}]` |
| `PATCH /projects/P/issues/batch`，`VO-1` 改 In Progress 及顺序 `2`，`VO-2` 改顺序 `-1`、优先级 `high`、标签 `renderer` | 两项均返回新字段，标签 UUID 与创建结果一致 |
| `GET /projects/P/issues?search=RENDERER` | `["VO-2","VO-1"]`，按新顺序返回 |
| `GET /projects/P/issues?search=90%25_&status_category=started&triage=true` | `["VO-1"]`，字面 `90%_` 与叠加筛选匹配 |
| `POST /projects/P/cycles`，`November cycle`，日期 `2026-11-19` 至 `2026-11-26`，再 `PUT` 结束日期 `2026-11-27` | 创建返回 cycle ID，更新返回 `November cycle 2` 和 `2026-11-27` |
| `PATCH /projects/P/issues/I` 给 `VO-1` 设置 cycle ID，再 `GET /projects/P/issues?cycle_id={id}` | `["VO-1"]` |
| `POST /projects/P/saved-views`，过滤同一 cycle 和 `started`，再 `PUT` 更新并 `GET` 列表 | 首次响应保留 `cycle_id` 和 `status_category`，更新后为 `[{"name":"Triage","filter":{"triage":true}}]` |
| `DELETE` 保存视图、cycle、关系和 `VO-1`，再 `GET /projects/P/issues/{child_id}` | 四个删除均返回 `success: true`，`VO-2` 的 `parent_issue_id` 清空 |
