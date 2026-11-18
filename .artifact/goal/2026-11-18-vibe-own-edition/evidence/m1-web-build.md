# M1 Web 静态站点验收

## 复跑

从仓库根目录执行：

```bash
bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m1-web-smoke.sh
```

需要 Windows Git Bash、Python Playwright、本机 Chrome、NSH 参考仓库中的锁定 Git 对象，以及 Rust 1.99.1 的 `wasm32-unknown-unknown` 目标。脚本加载 `harness/env.sh`，从提交 `12af3d778b588c2837267ff6aaed678eb5ad72a2` 提取源码并沿用其 `Cargo.lock`。两个平台入口编译同一个 `harness/tools/vibe-own-root.rs`。

通过信号为退出码 `0`、输出「Web 评测通过」，且截图中可见共享根组件文字。

## 实测结果

| 验证项 | 实测 |
|---|---|
| Release WASM | `cargo +1.99.1 build --locked --release --target wasm32-unknown-unknown -p nsh-platform-web --no-default-features` 退出 `0`，构建耗时 49.69 秒。 |
| 静态产物 | `nsh.wasm` 为 19,258,798 字节，SHA-256 `3be15d57257aee2c80b64e3733501b5defabd6664b8d70709832d19e82e212df`。 |
| Chrome 157.0.8103.50 | `/`、`/nsh.js`、`/nsh.wasm` 均返回 HTTP 200。WASM MIME 为 `application/wasm`。页面无异常，浏览器自动请求的 `/favicon.ico` 返回 404。 |
| 屏幕取证 | 查看了 [Web 截图](m1-web-browser.png) 与 [Win32 截图](m1-native-window.png)。两端均完整显示「NSH 原生窗口已启动」。 |
| 其他检查 | 原生探针、两份 Bash 入口语法检查、探针 Rust 格式检查、仓库 `pnpm run format` 均退出 `0`。 |

本机完整 Web 构建日志为 `../.vk-m1-web-build.FovHnm.log`，评测日志为 `../.vk-m1-web-eval.42npMB.log`。路径相对于仓库根目录。

正式 E1 `nsh package`、两端 vibe server HTTP 与实时流、真实 diff、滚动性能及锁定 nightly 工具链证据仍待验收。
