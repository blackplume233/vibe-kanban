# M5 NSH GitHub 面板交付报告

## 挂载

在共享 `lib.rs` 按现有的 Windows/WASM 条件声明 `mod github_integration;`。两项挂载均返回 NSH `View`。

```rust
workspace_pull_requests(api: Api, workspace_id: SharedString, shortcuts_active: impl Fn() -> bool + Clone + 'static) -> View
project_issue_sync(api: Api, project_id: SharedString, statuses: &[Status], on_synced: impl Fn(Issue) + 'static) -> View
```

工作区和项目的挂载分别以对应 ID 为键。NSH 在卸载时取消该面板的任务。工作区内切换仓库时，PR 查询的 effect 会取消旧查询。模态打开时让 `shortcuts_active` 返回 `false`。项目同步回调接收服务端返回的本地议题，可更新看板记录并保留当前位置。冲突留在面板中，跳过该回调。

## 行为与接口

- 工作区读取仓库列表、PR 详情和 CI 检查汇总，显示失败检查名称。仅开放且存在失败检查的 PR 可以提交修复。修复调用区分已启动、已排队和失败，并在请求完成前阻止重复提交。PR 和修复分别调用 `/api/workspaces/{id}/pull-requests/detail` 和 `/api/workspaces/{id}/pull-requests/fix-ci`，通过 `repo_id` 指定仓库。
- Agent 跟进区读取会话现有的 `DRAFT_FOLLOW_UP` 草稿。服务端打开 `VK_GITHUB_REVIEW_AUTO_FOLLOW_UP=1` 后可以自动生成 GitHub review 草稿。面板预览草稿，完整内容在 Agent 会话查看和发送。
- 项目表单支持选择开放与关闭状态的本地映射。通过独立操作分别发送 `write_back: false` 和 `write_back: true` 到 `/api/projects/{project_id}/github/issues/sync`。展示导入、同步、无变更、待回写、冲突及失败反馈。
- Win32 仓库和状态映射使用有输入焦点的 NSH combobox。工作区快捷键为 `Ctrl+Shift+J` 和 `Ctrl+Shift+F`。项目议题编号字段中 `Enter` 拉取，`Ctrl+Enter` 双向同步。Web 端可用 Tab 聚焦动作按钮并用 Enter 激活。反馈留在面板内，沿用 M3 的主题语义色与焦点色。
- UI 只通过 `Api::request` 请求本地服务。GitHub PAT 由服务端读取，面板没有凭据字段。

## 验证

使用 `nightly-2026-08-01` 与锁定的 NSH `12af3d778b588c2837267ff6aaed678eb5ad72a2` 构建临时 Win32 与 WASM 入口。两端使用隔离的回环 HTTP 夹具，响应结构取自 M5 后端属主确认的契约。夹具记录了准确的请求方法、路径、议题编号、映射状态和回写标志。

| 环境 | 实测操作与结果 | 截图 |
| --- | --- | --- |
| Win32 | 窗口显示中英文 PR、CI 汇总与跟进草稿。点击修复后显示排队。Tab 从仓库输入框进入议题编号，Enter 提交一次拉取并收到 `VO-1`，回调收到该记录。 | [初始](../evidence/m5-nsh/win-panel.png) · [键盘焦点](../evidence/m5-nsh/win-focus.png) · [导入](../evidence/m5-nsh/win-import.png) |
| Chromium 157.0.8103.50 | Tab 聚焦修复按钮，Enter 提交，验证进行中、禁用、排队和 409 失败。键盘拉取、显式双向同步与冲突分别命中期望请求，验证看板回调。 | [键盘焦点](../evidence/m5-nsh/web-focus.png) · [失败与冲突](../evidence/m5-nsh/web-conflict.png) |

Win32 合成窗口的前台切换被 Windows 拒绝，因此 Win32 键盘证据覆盖窗口定向 Tab 和 Enter。真实 GitHub sandbox 的 Agent 推送、CI 转绿及远端同步写入由后端属主及集成验收执行。
