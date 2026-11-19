# 产品键盘与 PR 验收

## Web 键盘运行

Chrome 157.0.8103.50 加载正式 NSH Web 包，在同源本地 Rust 服务与隔离 SQLite 上完成项目搜索、建议题、搜索议题、变更状态、启动工作区、搜索工作区、查看 Agent 日志与工作树 diff、打开 PR 表单并将焦点移入标题。全程仅键盘操作。本地 OpenCode 通过回环 Ollama 运行 Gemma 4 并实际修改 `release-state.txt`。测试夹具没有 Git remote，联网提交由独立的 Win32 验收覆盖。

[全程录像](product-keyboard-e3.webm) · [步骤与网络读数](product-keyboard-e3.json) · [日志与变更](product-keyboard-07-logs-and-diff.png) · [PR 表单](product-keyboard-08-open-pr-form.png)

从仓库根运行 `python .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/product-keyboard-e3.py --server <server.exe> --site <NSH Web 包目录> --out <新的证据目录> --channel chrome`。需预加载 `gemma4:latest` 的本机 Ollama 和已安装的 OpenCode。

## 滚动与冷启动

正式 Win32 包加载 5,005 条议题后，首次滚动用时 721.6 ms。NSH `--fps` 各滚动窗口最高 `frame` p95 为 6.84 ms。Chromium 测量加载 5,005 条议题并新增一条后滚动。表中渲染时间对应 `--fps` 的计算预算，呈现间隔另外记录。

| Chromium 运行方式 | 首次键盘操作 ms | 渲染切片 p95 ms | 呈现间隔 p95 ms | 同机滚动 rAF 基线 p95 ms |
| --- | ---: | ---: | ---: | ---: |
| 有头 | 685.9 | 2.712 | 19.557 | 19.6 |
| 无头 | 693.6 | 2.788 | 20.057 | 19.8 |

[Win32 数据](product-native-frame-results.json) · [有头 Chromium](product-web-frames-headed.json) · [无头 Chromium](product-web-frames-headless.json)

## Win32 看板拖放

正式 Win32 包在隔离 SQLite 与仅回环代理环境中启动。脚本从实际卡片拖动手柄发出鼠标事件，跨列拖入 `Drag started`，再通过原生进程内 API 读回目标状态。进程退出码为 0，监听端口已关闭。

[拖动前](product-native-drag-before.png) · [拖动后](product-native-drag-after.png)

## Win32 草稿 PR

由 Win32 包内的 `/api/workspaces/9e5c982c-7510-4145-9e8f-b971fdd7c6d8/pull-requests` 提交隔离的沙箱工作树。HTTP 200 返回草稿 PR #3。GitHub 读回 SHA `217874876122aadc54380c539d1033af0315287d`，与本地 HEAD 一致。SQLite 记录相同的 PR URL。Windows Git 在本轮使用 OpenSSL、HTTP/1.1 及覆盖新分支的 `origin` fetch refspec，GitHub API 使用进程内凭据。

[沙箱草稿 PR #3](https://github.com/blackplume233/vibe-own-sandbox/pull/3)
