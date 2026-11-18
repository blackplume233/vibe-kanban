# M1 Windows 原生探针证据

## 复跑

从仓库根目录在有图形会话的 Windows Git Bash 中执行。脚本从本地参考库提取锁定 Git 树，加载 `harness/env.sh`，然后用该树的 `Cargo.lock` 构建探针。

```bash
bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m1-native-smoke.sh
```

通过条件为退出码 `0`，且截图显示窗口标题 `vibe-own | NSH M1` 和客户区文字「NSH 原生窗口已启动」。

## 结果

- `cargo +1.99.1 build --locked -p nsh-platform-windows --no-default-features --example vibe-own-smoke` 首次编译完成，用时 48.19 秒。随后取证脚本在 `WaitForInputIdle` 报错。改用 Windows 子系统和按窗口句柄取证后，完整命令退出 `0`，增量编译用时 3.31 秒。
- 实际读取了 `m1-native-window.png`。969 × 484 像素的截图显示预期标题与 NSH 渲染的中文正文。截图 [m1-native-window.png](m1-native-window.png)
- `pnpm run format` 退出 `0`。本地完整日志 `../.vk-m1-format.zlWcpe.log`。

## 构建与运行日志摘录

```text
NSH 提交 12af3d778b588c2837267ff6aaed678eb5ad72a2
rustc 1.99.1 (58cf8f9c3 2026-10-13)
host: x86_64-pc-windows-msvc
release: 1.99.1
LLVM version: 23.1.1
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.31s
已捕获窗口 vibe-own | NSH M1 (969x484)
```

本地完整构建日志为 `../.vk-m1-native-build.Bngt2r.log` 和 `../.vk-m1-native-build.gxYv2I.log`，路径相对于仓库根目录。M1 下一道门是浏览器运行。双端 HTTP 与实时流、真实 diff、滚动 p95 以及正式打包均待各自验收。
