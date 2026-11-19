# M1 NSH Spike 交付记录

`crates/nsh-app` 提供一个共享 NSH 根组件。Windows 用 Win32 + Direct2D 宿主，Chrome 用 Release WASM 和锁定版 NSH Web 宿主。两个客户端读取同一 vibe server 的工作区和最新会话，订阅执行进程及其原始日志，并把工作区 diff 流接到 NSH `DiffView`。

## 文件与取舍

- `crates/nsh-app/Cargo.toml`、`Cargo.lock`、`build.rs` 固定 NSH 提交 `12af3d778b588c2837267ff6aaed678eb5ad72a2`，并设定 Web 链接栈。独立 workspace 保留各自锁文件。现有后端锁文件固定 `wasm-bindgen 0.2.115`，NSH Web 及其配套 CLI 锁定 `0.2.129`，因此本轮无需改动后台依赖解析。
- `src/app.rs` 是共享根组件，`src/lib.rs` 注册 Web 入口，`src/main.rs` 启动 native 窗口并接受 `--fps`。`src/api.rs` 消费 HTTP 与 WS，`src/socket.rs` 给 native 提供 `tungstenite 0.30.0`，给浏览器提供 `web-sys 0.3.106` 的 `WebSocket`。`futures-channel` 与 `futures-util 0.3.34` 把平台消息送回 NSH 任务，并并发等待会话与进程流。`json-patch 3.0.1` 复用后台的 JSON Patch 格式，另用 `serde`/`serde_json` 解码和 `log`/`env_logger` 记录采样。
- `harness/tools/m1-gate.sh` 构建应用及后台。`m1-runtime.py` 为单次运行启动回环后台，通过真实 API 创建临时 Git 工作区和执行脚本。`m1-web-browser.py` 与 `m1-native-window.ps1` 操作实际界面。`m1-native-smoke.sh`、`m1-web-smoke.sh` 可分别复跑。

## 进程内后台方案

本轮 native 指向同一外部后台。进程内方案可复用 `crates/tauri-app/src/main.rs` 的启动顺序。Windows 使用 Tokio 独立运行时，初始化 rustls crypto provider，调用 `server::startup::start_with_bind` 取得回环地址并在运行时内开 `ServerHandle::serve`。应用等待后台就绪后，把端口传给同一个 HTTP/WS 客户端。窗口退出时取消服务的 `shutdown_token` 并等待清理。该改动需链接后台 Rust 依赖、处理本地资源和发布包的生命周期。本轮的 Release 程序与后台通过回环验收。

Web 的开发宿主仅代理 `/api/workspaces` 和 `/api/sessions` 的 HTTP GET。当前后台的 `VK_ALLOWED_ORIGINS` 只校验 Origin，HTTP 响应不包含 CORS 许可。浏览器的三个 WebSocket 直接连接后台，并由 `VK_ALLOWED_ORIGINS` 校验来源。部署时可把静态资源和 `/api` 放在同源入口。

## M7 正式打包入口

在 `crates/nsh-app/nsh.json` 指定导出的 `App` 根组件。

```json
{
  "name": "vibe-own",
  "id": "io.vibe-own.desktop",
  "rust": ".",
  "root": "App"
}
```

正式 CLI 可使用 `F:/vk-targets/main/nsh-cli/debug/nsh.exe`，分别运行 `nsh package --target windows` 和 `nsh package --target web`。Web 需要 `wasm-bindgen-cli 0.2.129`，可用 `NSH_WASM_BINDGEN` 指向对应可执行文件。CLI 为两个目标生成宿主。接入时应让 CLI 生成的 Web 入口独占 `nsh_app_init`，用 Cargo feature 隔离当前 smoke 的 `rust_app!` 导出。按锁定 NSH Rust gallery 的入口约定提供 crate 注册函数，并统一 CLI 生成宿主与组件 crate 的 NSH 依赖源。

## 验证与证据范围

复跑命令、截图和各闸门的实测结果见 [M1 闸门](../evidence/m1-gate.md)。

- E1 获得真实后台、Win32 Release 和 Web Release 构建证据。正式 `nsh package` 发布包仍按 E1 验收。
- E2 获得回环服务、临时 Git 工作区、实时日志和工作区 diff 的证据。目标中的议题流程、Agent 和 PR 不在本切片。
- E3 获得 5000 行滚动的 Win32 `frame` 和 Chrome rAF 测量。全流程键盘、快速搜索及冷启动尚待对应里程碑验证。
- E4–E7 由对应里程碑取证。

## 建议里程碑状态

`M1 功能验收通过（Win32 / Chrome 读取同一真实工作区，实时日志和 DiffView 已验证，Win32 5,000 行 Release 活跃滚动 p95 ≤16 ms）。以协调者的锁定 nightly 共同编译证据结清路线闸门。`

协调者负责锁定 nightly 交叉编译及路线的最终判定。实现与实测截图提交 `d3ef5a2c21d301bf22716af8a75cfff3bbe41bac`。当前报告提交同时收录复跑主入口、适配工具与最终判定表，分支为 `my/m1`。
