# M7 本地工作区生命周期

切片在 `F:/vk-wt/m7-workspaces` 的 `my/m7-workspaces`。构建使用 `nightly-2026-08-01`、锁定 NSH `12af3d778b588c2837267ff6aaed678eb5ad72a2` 和独立 `F:/vk-targets/m7-workspaces`。

## 集成接口

- 在共享根注册 `workspace_lifecycle`，以 `workspace_lifecycle::workspace_lifecycle(LifecycleContext::new(api, selected).for_issue(issue))` 挂载。使用单一 `Signal<Option<String>>` 工作区 ID 给终端、会话及 diff。`selected` 在用户从工作区列表选中条目、或创建成功后写入。归档、恢复和合并保持同一 ID。显示归档页签随对应状态变化。
- `issue` 为共享根拥有的本地议题 ID signal。列表依据 `local_issue_id` 即时筛选。打开创建表单时固定本次创建关联的议题，随后提交 `POST /api/workspaces/start`。`GET /api/workspaces` 和工作区详情返回可空的 `local_issue_id`。同步时可按该字段查找关联工作区。
- dev server 使用 session 执行进程流，通过 `Api::stream` 读取。停止只调用运行中的 `devserver` 进程的停止端点。Git 写操作沿用现有端点及权限逻辑。合并、中止和继续使用确认框，Rebase 和 PR 使用参数确认表单，PR 表单注明确认时会推送分支。结构化 API 错误保留后端的冲突及认证信息。
- workspaces 的 `local_issue_id` 引用 `issues(id)`，删除议题时置空。创建工作区与设置关联同事务写入。作为兼容字段，迁移前的工作区读回为 `null`。

## 本地验收

使用 Windows 上实跑的 `server.exe`、隔离工作区 `dev_assets/db.v2.sqlite`、独立 Git fixture 和 Chromium NSH 测试入口。测试入口加载同一组件和 M4 HTTP 通路。站点同源代理 HTTP，直接连接后端 WebSocket，后端显式允许测试站点的 Origin。

| 场景 | 已观察结果 |
| --- | --- |
| 创建和首轮 Agent | 浏览器登记实际本地 Git 目录，选择 `target`，提交提示词。后端返回工作区及关联，查询 session 确认 `CLAUDE_CODE` 首轮已提交。 [创建截图](../evidence/m7-workspaces-create.png) |
| dev server、编辑器和归档恢复 | NSH 启动实际 Python HTTP 服务并从监听端口收到 HTTP 200。点击停止后进程流显示已停止。编辑器 API 返回成功，归档和恢复分别由详情 API 读回。 [运行截图](../evidence/m7-workspaces-dev.png) |
| Git | 从同一基线分别改动工作区和目标分支的 `note.txt`。NSH 显示领先 1、落后 1。进入 Rebase 后展示冲突文件，确认中止。再次进入 Rebase，在真实工作树解决并暂存后确认继续。状态变为领先 1、落后 0。确认本地合并后，用 `git show target:note.txt` 读到解决后的内容。 [冲突截图](../evidence/m7-workspaces-conflict.png) [合并截图](../evidence/m7-workspaces-merge.png) |
| PR 入口 | 从 Git 状态打开实际 PR 表单，检查源分支、目标分支、标题和推送说明。通过滚动到达取消按钮，随后完成本地合并。远端操作由 E5 sandbox 验收。 [PR 表单截图](../evidence/m7-workspaces-pr.png) |
| 本地议题关联 | 浏览器设置已创建的本地议题并创建工作区。用列表按工作区 ID 和 `local_issue_id` 比对，停止并重启同一数据库的 `server.exe` 后再比对。调用议题 DELETE 后，工作区详情仍读到同一 ID 且 `local_issue_id` 变为 `null`。使用不存在的 UUID 提交时返回 HTTP 400，前后工作区 ID 集合相同。 |

独立后端 build、Web WASM build 和类型生成通过。新增 DB 回归测试的 Cargo 任务在重编译 `executors` 时达到 360 秒上限，未得到单测结果。对应有效关联、重启、无效关联回滚和删除行为已通过真实 HTTP 验证。组合跑编辑器和 Git 时，Windows 上已打开的工作树遇到现有后端 `os error 32`。Git 场景改用独立 fixture 复跑通过。
