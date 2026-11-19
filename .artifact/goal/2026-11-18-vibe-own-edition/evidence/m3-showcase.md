# M3 设计展示页 · 双端证据

## 集成复验

在 `ced0abaf4` 的独立工作树中通过 Windows Git Bash 执行上述命令，退出码为 `0`。两端分别生成 8 张截图，Chromium 操作检查通过。日志位于 `F:/vk-targets/main/m3-showcase/verification.log`。正式包的主题和字体接入由 E1 复验。

Win32 和 Chromium 挂载同一个 `DesignShowcase` Rust 组件，使用同一组议题、token、字体及状态。展示页提供看板和列表、议题检查栏、标题编辑、命令菜单、外观切换、密度切换以及空态和保存错误示例。

## 复现

在仓库根目录打开 Windows Git Bash。准备 `nightly-2026-08-01` 的 `wasm32-unknown-unknown` 目标、锁定的 NSH 源码、`wasm-bindgen` 0.2.129、Node 20 或更新版本，以及装有 `pywin32`、Pillow 和 Playwright 的 Python 和本地 Chrome。

```bash
NSH_REFERENCE_REPO=<本地 NSH 源码路径> \
NSH_WASM_BINDGEN=<wasm-bindgen.exe 路径> \
bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m3-showcase.sh both
```

脚本先运行 `generate-tokens.mjs --check`。暂存入口使用应用的 `Cargo.toml`、`Cargo.lock`、`build.rs` 和锁定 NSH `12af3d778b588c2837267ff6aaed678eb5ad72a2`，默认为 `F:/vk-targets/m3`。Win32 从应用客户区抓帧。Chromium 从本地回环静态站加载同一 Rust 模块的 WASM 构建。两份构建日志在同一目标目录下，分别为 `showcase-native-build.log` 和 `showcase-web-build.log`。本次 token 源的 SHA-256 是 `e09b8bd5d569c601a55c4ae0c876277f1fde5d1053b1156f217a1a65e66b7fd9`。

## 基线与补充截图

Win32 客户区为 1280 × 800 像素，96 DPI，系统文本缩放 100%。Chromium 157.0.8103.50 的基线视口为 1280 × 800 CSS 像素，DPR 1。窄容器均使用 900 × 720。

| 场景 | Win32 | Chromium |
| --- | --- | --- |
| 亮色 · 舒适 | [看板与详情](m3-showcase/win-light-comfortable.png) | [看板与详情](m3-showcase/web-light-comfortable.png) |
| 亮色 · 紧凑 | [缩减行高](m3-showcase/win-light-compact.png) | [缩减行高](m3-showcase/web-light-compact.png) |
| 暗色 · 舒适 | [命令菜单](m3-showcase/win-dark-comfortable.png) | [命令菜单](m3-showcase/web-dark-comfortable.png) |
| 暗色 · 紧凑 | [紧凑命令菜单](m3-showcase/win-dark-compact.png) | [紧凑命令菜单](m3-showcase/web-dark-compact.png) |
| 琥珀 · 亮色 | [选中与焦点](m3-showcase/win-amber-light.png) | [选中与焦点](m3-showcase/web-amber-light.png) |
| 琥珀 · 暗色 | [选中与焦点](m3-showcase/win-amber-dark.png) | [选中与焦点](m3-showcase/web-amber-dark.png) |
| 窄容器 | [单栏详情与错误](m3-showcase/win-narrow.png) | [单栏详情与错误](m3-showcase/web-narrow.png) |
| 空态 | [筛选无结果](m3-showcase/win-empty-reduced.png) | [筛选无结果与浏览器减少动效偏好](m3-showcase/web-empty-reduced.png) |

Chromium 检查覆盖了密度切换、空态恢复、`Ctrl+K` 打开命令菜单、搜索 `retry`、`Enter` 打开议题，以及保存失败时保留中文输入。浏览器截图确认搜索框获得焦点，且减少动效场景的媒体偏好处于开启状态。

锁定版 Win32 在挂载对话框时先将其透明度设为零。为使窗口可靠显示，展示页在 Win32 采用 NSH 的 200 ms 原生淡入。Chromium 使用 token 中的 120 ms，并在减少动效偏好下立即显示。

补充验收还包括 Windows 150% 显示缩放、Chromium 125% 页面缩放、Win32 系统减少动效开关、两端真实中文 IME 组合提交与模态焦点恢复。现有 Win32 空态截图的系统动画开关为开启。正式应用入口和 M7 的 NSH 打包接线见设计规范。 [设计系统](../../../../docs/design-system.md)
