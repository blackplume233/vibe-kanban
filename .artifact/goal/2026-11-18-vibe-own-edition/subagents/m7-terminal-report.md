# M7 工作区终端交付报告

## 根组件接入

由集成切片在 `crates/nsh-app/src/lib.rs` 的 Windows/Wasm 条件模块区添加 `mod terminal;`，在工作区详情列挂载 `crate::terminal::terminal_panel(backend_origin, workspace_id, terminal_open, obscured)`。

- `backend_origin: String` 是应用配置的可信 HTTP(S) API origin。原生传入实际启动的本地服务地址。Web 同源部署时传入 `location.origin()`，独立 Web 静态站点需要服务端按现有 `VK_ALLOWED_ORIGINS` 配置允许该页面的 origin。
- `workspace_id` 是读取当前工作区 UUID 字符串的 `Fn() -> Option<String>`。`terminal_open` 是父组件与命令菜单可共用的 `Signal<bool>`，默认关闭。组件以工作区 ID 为键挂载会话，切换工作区会销毁旧 WebView。
- `obscured` 是读取覆盖终端区域的命令菜单、对话框等状态的 `Fn() -> bool`。终端被遮挡时隐藏 WebView、移开终端输入焦点并保留 shell。合上面板、切换工作区或正常退出 shell 均销毁会话。首次显示时聚焦 xterm 输入，Windows WebView2 的系统键盘焦点也可通过点击终端正文进入。

## 传输与打包

- Windows NSH WebView 加载 `/api/terminal/view`。Web NSH 通过 `srcdoc` 使用同一份 HTML。Web 的终端只接受 `event.source === window.parent` 的消息，Windows 只接受 NSH WebView2 在窗口本地合成的 `source === null` 事件。父端从该 WebView 的 NSH `on_message` 接收消息，浏览器端使用和 iframe 绑定的 `MessageChannel`。Rust 根据应用提供的服务地址和已选工作区构造 WebSocket URL。Windows 与浏览器均由 Rust 管理连接生命周期，输出通过 NSH 任务作用域递交。
- 服务端终端路由通过 `include_str!` 将 HTML、CSS 和 JS 嵌入服务，可随 standalone server 或链接 server 的 Windows 包分发。Web 静态包依赖所连接服务的 `/api/terminal/` 路由。无需生成 React bundle、访问 CDN 或复制终端文件到 NSH 静态包。Windows 需已安装 WebView2 Runtime。
- xterm `5.5.0` 和 Fit addon `0.10.0` 与 `pnpm-lock.yaml` 一致，附带各自许可证。更新时用冻结的依赖安装后执行 `node crates/nsh-app/src/terminal/vendor.mjs`。新版本必须同时更新脚本中锁定的版本。
- 复用 WS 的 Origin、relay 签名、工作区与目录检查，继续使用 Base64 输入与输出和现有 resize 帧。PTY 监测子进程退出，移出会话并关闭 ConPTY master，输出读取线程排空后结束 WebSocket。手动关闭移出会话后在线程池终止子进程，避免在共享会话锁内关闭 ConPTY。

## 实测证据

- 使用仓库声明的 `nightly-2026-08-01` 构建真实 `server` 与 Windows、Wasm 独立挂载验证程序。Rust 格式检查及两个 JS 源文件的 `node --check` 通过。验证使用真实 SQLite 记录、两个 Git worktree 和服务端启动的 PowerShell ConPTY。构建、日志、截图与验证入口都在 `F:/vk-targets/m7-terminal/`。
- Web Chromium 中，从工作区 A 读回 A 标记，窗口变窄后 ConPTY 从 `144×16` 变为 `99×16`。浮层遮挡时 iframe 隐藏，关闭浮层后同一个 PowerShell PID 继续交互。切到 B 后旧 PID 退出，手动收起 B 后新 PID 退出。重新打开 B 后以一条命令打印末尾标记并退出。完整标记和唯一 `exited` 帧到达、iframe 移除、焦点回到打开按钮。见 `web-interaction.log`、`web-terminal.png` 和 `web-overlay.png`。
- Windows WebView2 中，从工作区 A 读回 A 标记，窗口变窄后 ConPTY 从 `141×16` 变为 `101×16`。浮层关闭后同一 PID 恢复交互。切换到 B 和手动收起分别终止对应的旧 PID。再次打开 B 后，新的 PowerShell PID 打印 `PTY-FINAL-OUTPUT-END` 并执行 `exit`。WebView 收到完整末尾输出和唯一 `exited` 帧，面板自动收起。见 `windows-terminal.png`、`windows-overlay.png` 和 `windows-clean-exit.log`。
- Win32 探针在退出链路加了分段日志，并用 `WM_CLOSE` 正常关闭验证窗口。重新验证时，采集器正常退出，NSH 原生进程退出码为 `0`，Playwright 驱动正常停止，两套临时工作区与仓库删除成功，服务停止，整条命令退出码为 `0`。本轮未复现早先的清理超时，无法确定早先具体卡在哪个调用。重开终端期间 Playwright 出现长等待并自行恢复，完整日志保留在 `F:/vk-targets/m7-terminal/windows-clean-exit.log`。
