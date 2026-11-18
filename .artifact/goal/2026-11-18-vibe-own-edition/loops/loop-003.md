# 循环 003

## 基础目标切片

M1 路线闸门的第二步，复用 Windows 探针的 Rust 根组件验证 WebAssembly 静态站点。

## 子目标

从同一 NSH 锁定 Git 提交构建同一个 Rust 根组件供 Web 使用，用真实 Chromium 加载并验证可见文字，保存构建摘要与截图。

## HTN 分解


- 无。构建与浏览器验证由本轮执行者完成。

## 仓库基准检查

- 读过目标、计划、控制面板、工具绑定、上一轮记录和上游 Web 文档。
- 中文记录。构建前加载 `harness/env.sh`，使用 `cargo +1.99.1`。只读参考仓库，仅提交 `my/main` 上的本轮明确路径。
- 使用 `harness/tools/` 保存复跑入口，`evidence/` 保存截图和摘要，仓库外保存完整构建日志。

## 发散候选


| 方案 | 核心方法 | 优点 | 风险与代价 | 预期证据 |
|---|---|---|---|---|
| A | 使用锁定树的 `nsh-platform-web` Rust 入口、页面宿主和加载器直接生成静态站点 | 复用现有组件和 Cargo 锁文件，只构建 Web 运行时 | 需配好挂载入口与静态文件布局，不覆盖正式打包器的压缩与清单 | WASM 构建日志、HTTP 响应、可见文字和截图 |
| B | 编译锁定树的 `nsh-cli`，通过 Rust Gallery 配置运行 `nsh package --target web` | 直接演练 E1 正式打包入口，并覆盖资源指纹和预压缩 | 增加主机编译与生成工程的依赖解析。仍要安装 wasm32 目标 | 正式打包日志与浏览器截图 |

## 收敛决策

- 选择 A。两种方式都能运行共享根组件。本轮验证 Web 运行时渲染，直接复用上游宿主和现有锁文件，避免把正式打包链引入探针。
- 遇到阻塞先区分 wasm32 工具链、运行时编译和页面挂载。M1 路线切换条件仍以 `plan.md` 为准。

## 计划

- 构建 Release WASM 并从本机回环地址加载产物。浏览器须收到成功的 WASM 响应、显示共享组件文字且无页面异常，再保存 PNG。
- Eval 命令为 `bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m1-web-smoke.sh`。预期退出码为 `0`，输出含「Web 评测通过」，截图无裁切并显示共享根组件文字。
- 用 `bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m1-native-smoke.sh` 复核组件移出平台入口后 Win32 仍能渲染。

## 参考审查

- 按 Ponytail 的最小实现原则复用 NSH 宿主、原生组件与本机 Chromium。按 SkillOpt 的证据门原则保留浏览器可见性和 WASM 响应断言。

## 执行记录

- 初始严格校验返回 `valid: true`。`goal_state.py new-loop` 创建 `loop-003`。
- 将 Win32 已验证的组件移至 `vibe-own-root.rs`，Win32 和 Web 入口均编译此文件。Web 探针复用锁定树内的宿主加载器。
- 官方分发站的两种后端均未完成下载。经中科大镜像取得的 `rust-std` 文件通过锁定摘要核验。指向本地分发缓存的 `rustup target add` 确认目标已就绪。
- 首次编译触发 NSH 的 `unreachable_pub`，在生成的临时 crate 根重新导出 `nsh_app_init` 后，Release WASM 编译通过。Playwright 注册内建 `list.append` 回调时失败，改用可包装的 Python 回调。

## 验证

- 原生探针退出 `0`。独立 Win32 窗口显示标题 `vibe-own | NSH M1` 和「NSH 原生窗口已启动」，截图见 `../evidence/m1-native-window.png`。两份 Bash 脚本的语法检查、探针 Rust 格式检查和仓库 `pnpm run format` 均通过。
- Web 评测退出 `0`。Chrome 157.0.8103.50 以 `application/wasm` 加载 HTTP 200 的 Release WASM，组件文字可见，未捕获页面异常。查看 `../evidence/m1-web-browser.png` 确认文字完整。日志摘要见 `../evidence/m1-web-build.md`。

## 结果

result: 完成

## 终止检查

- decision: continue
- reason: 双端同源构建和真实界面验证通过，M1 继续按原路线推进。
- remaining_evidence: 双端 HTTP 与实时流、真实工作区 diff、5000 行滚动 p95、锁定 nightly 共编译。正式双端 `nsh package` 仍计入 E1。
- 下一轮先让两个平台通过真实本地 vibe server 的 HTTP API 读取并展示同一条工作区记录。

## 更新

- `goal.md` 更新本轮验证状态与下一子目标。`plan.md`、`harness/status.md` 同步双端渲染证据。
- `references.md` 记录分发失败、镜像核验、WASM 入口导出和 Playwright 回调问题。
