# M1 NSH 路线闸门

| `switch_trigger` | 判定 | 证据 |
|---|---|---|
| Win32 和 Web 连通真实 vibe server 的 HTTP 与实时流 | **PASS** | 同一临时 Git 工作区 `17a7d25a-7611-48c0-9a21-26145c1f0226`。两端 `/api/workspaces` 和 `/api/sessions` 均收到 HTTP 200。会话执行进程 `6c9b8b69-50de-4f4c-b056-2327489119aa` 的原始日志经 WS 追加，Win32 45 → 54 条，Chrome 11 → 13 条。[Win32 前](m1-native-live-before.png) · [Win32 后](m1-native-live-after.png) · [Chrome 前](m1-web-live-before.png) · [Chrome 后](m1-web-live-after.png) |
| Release 下 5000 行 `virtual_list` 滚动帧耗时 p95 ≤16 ms | **PASS（Win32 `frame`）** | Win32 用 `--fps` 记录五个活跃滚动采样窗口，最大的 `frame` p95 为 **5.69 ms**，滚动偏移达 4800 px。`frame` 指每个呈现帧在主线程花费的时间，扣除 VSync 等待。本次 NSH 日志同时报告 `fps 4/60` 和约 266 ms 的 `gap` p95，呈现频率仍需观察。Chrome 的 rAF 间隔 p95 为 **19.70 ms**，Web 绘制耗时尚待测量。[Windows 采样](m1-native-perf.md) · [滚动后截图](m1-native-scroll.png) · [Chrome 采样](m1-web-perf.md) |
| `DiffView` 渲染真实工作区 diff | **PASS** | 两端的 `m1-spike/review.txt` 均显示 `status=planned` 删除，`status=verified` 和 `log_transport=websocket` 新增。测试 dev 脚本在服务器为工作区建立的 Git worktree 中修改该文件，客户端从 `/api/workspaces/{id}/git/diff/ws` 读取变更。[Win32](m1-native-live-after.png) · [Chrome](m1-web-live-after.png) |
| vibe 后端和 NSH 使用锁定的 `nightly-2026-08-01` 编译 | **待验收** | 本轮后端及两个 NSH 目标统一使用 `1.99.1`，构建退出 `0`。锁定 nightly 的独立对照由协调者验证。 |

## 复跑

使用 Windows Git Bash，先从独立 worktree 的根目录执行。`NSH_REFERENCE_REPO` 只用于提取锁定版本的浏览器宿主文件。测试代码在回环端口启动本切片的 debug server，数据在该 worktree 的 `dev_assets` 下，新建的 Git 仓库和 worktree 在专用临时目录内。

```bash
export CARGO_TARGET_DIR=F:/vk-targets/m1
export NSH_REFERENCE_REPO='G:/Workspace/AgentWorkSpace/nsh-ue-ui/vendor/nsh-src'
export M1_BACKEND_PORT=59571 M1_WEB_PORT=59572
bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m1-gate.sh both
```

- 以上命令退出 `0`。Chrome `157.0.8103.50` 的 WASM 响应为 HTTP 200、`application/wasm`。浏览器断言三个 WS 端点连接并观察到输出追加。Win32 通过实际窗口点击选择工作区，用相同的工作区与执行进程 ID 验证日志，再发送滚轮消息采样。
- `cargo +1.99.1 test --release --locked --lib --manifest-path crates/nsh-app/Cargo.toml` 退出 `0`，`diff_snapshots_upsert_the_repo_and_keep_incremental_updates` 通过。
- 单端入口为同目录的 `m1-native-smoke.sh` 或 `m1-web-smoke.sh`。`M1_RUST_TOOLCHAIN=nightly-2026-08-01` 可切换本脚本中的 Rust 工具链。Chrome 验收需要可用的 Chrome 和 Python Playwright。
