# M5 私有测试仓库验收

测试范围是 `blackplume233/vibe-own-sandbox`。在测试分支 `m5-ci-fixture` 创建 [PR #2](https://github.com/blackplume233/vibe-own-sandbox/pull/2)，创建 [Issue #1](https://github.com/blackplume233/vibe-own-sandbox/issues/1) 验证双向同步。GitHub 访问使用既有 `gh` 认证。

## 外部 CI 失败

在独立目录检出 PR 精确 HEAD `52da6ee380d64a52fb28c06c1a3583f8907983dc`，运行 `grep -Fxq ok ci-state.txt`，退出码 `1`。将该退出码通过同一提交的 GitHub Commit Statuses API 发布。

```json
{"context":"m5/controlled-ci","description":"grep -Fxq ok ci-state.txt exited 1","state":"failure"}
```

GitHub GraphQL 查询观察到 PR 状态 `OPEN` 与 `StatusContext(m5/controlled-ci, FAILURE)`。写入 `.github/workflows/m5-ci.yml` 的两种 REST 请求均返回 `404`，因此外部测试进程承担 CI 执行和状态上报。

## Issue 双向往返

本地服务使用 M5 的 `gh` 路径调用真实 GitHub。`POST /api/projects/{project_id}/github/issues/sync` 将 Issue #1 导入，重复调用返回 `unchanged`。本地 `PATCH` 议题标题后，默认同步返回 `pending_write`，此时 GitHub 标题保持原值。传入 `write_back: true` 后，GitHub REST GET 确认标题改为 `M5 local title synchronized to GitHub 870545ce`。再同步返回 `unchanged`。

分别修改本地与 GitHub 描述后，同步返回 `conflict` 和 `fields: ["description"]`。本地 API 与 GitHub REST GET 分别确认两份独立描述均保留。两侧操作均返回 HTTP 200。

## Review 草稿与人工确认修复

工作区与仓库通过本地 API 创建。`gh pr checkout` 遇到网络故障后，测试通过隔离数据库为已检出的分支关联 PR #2。PR 明细 API 返回 `total=1`、`failed=1` 与 `m5/controlled-ci`。测试提交行内评审 `5978347529` 后，`GET /api/scratch/DRAFT_FOLLOW_UP/78ca1db6-91b4-44eb-be29-3c19e368c618` 返回 HTTP 200，草稿包含评审正文并等待用户确认。

人工调用 `POST /api/workspaces/{id}/pull-requests/fix-ci?repo_id={id}` 返回 HTTP 200 和 `status=scheduled`，Claude 修复进程为 `5ff2e496-785e-4de0-a838-3a46a5b68257`。先前 Codex 实测模型连接超时，Claude 运行产生工具事件，但在验收窗口内未观察到完成的工作树修改。修复提交、Agent 推送和成功 CI 状态仍需验证。

## 自动评审讨论

为 sandbox 仓库和评审者显式开启 `VK_GITHUB_REVIEW_DISPATCH_ALLOWLIST`，提交行内评审 `5979177086`。在原 Claude 会话 `402ca15e-48bc-4feb-9758-3d19791b5f2f` 中，轮询自动创建并持久调度讨论进程 `1cc9fbb7-152a-4371-b46e-063214f845aa`。该进程最终返回 `failed`，本次验证退出码为 `1`。已观察到自动调度，Claude 续接完成仍待验证。

## Agent 修复、推送与 CI 转绿

产品从 PR #2 读到一项失败检查。`POST /api/workspaces/aadcec66-e325-402a-ad2a-3f99fdc662ff/pull-requests/fix-ci` 调度真实 Codex ACP，执行 `cbb27876-5498-4712-a285-1d5361deeb68` 完成。工作树新提交只将 `ci-state.txt` 的 `fail` 改为 `ok`。在同一会话明确请求推送后，由执行 `1462fcb6-607d-43fa-a6e3-76cf68fdc41f` 推送沙箱分支 `m5-ci-fixture`。

GitHub PR HEAD 为 `68bcbe297bac8ae7ed9e39bb0a7f573ed3f5f4bd`，唯一父提交是失败夹具，唯一改动文件是 `ci-state.txt`。通过 GitHub API 按 SHA 读取文件字节，`grep -Fxq ok ci-state.txt` 退出 0，GitHub `m5/controlled-ci` 状态变为 `success`。产品 PR 明细读回 `total=1`、`passed=1`、`failed=0`。

## 真实 Claude 自动续接

在同一沙箱启用 `VK_GITHUB_REVIEW_DISPATCH_ALLOWLIST`。隔离服务进程从现有 Claude 配置继承认证和模型环境，文本讨论继续禁用工具、Hooks 和 MCP。行内评审 `5981011247` 触发一次自动执行 `68af458e-71af-47bc-b269-1b173a913e8a`，在原会话 `88d80706-6cd3-4042-a4ef-5ec28f326eb8` 完成。日志核对 `tools=[]`、`mcp_servers=[]`，并读到对回归检查的回复。探针退出 0。
