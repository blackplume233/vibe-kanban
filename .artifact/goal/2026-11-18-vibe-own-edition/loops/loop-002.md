# 循环 002

## 基础目标切片

M1 路线闸门的第一步，验证锁定 NSH 版本的 Windows 原生编译与渲染。

## 子目标

使用 Rust 1.99.1 和锁定的 NSH 源码编译最小应用，启动真实 Win32 窗口并保存构建证据与截图。

## HTN 分解

- 无。单个探针由本轮执行者完成。

## 仓库基准检查

- 文档与回复使用中文。
- 构建前加载 `harness/env.sh`，使用 `cargo +1.99.1` 和 NSH 提交 `12af3d778b588c2837267ff6aaed678eb5ad72a2`。
- 脚本存于 `harness/tools/`，截图与构建摘要存于 `evidence/`，长日志存于仓库外 `../.vk-*.log`。
- 参考仓库只读，提交限 `my/main` 上的本轮路径。
- 已读 Goal、计划、上一轮记录、工具绑定、控制面板和环境配置。

## 发散候选

| 方案 | 核心方法 | 优点 | 风险与代价 | 预期证据 |
|---|---|---|---|---|
| A | 用锁定 Git 树提取干净快照，把最小 Rust 示例作为 `nsh-platform-windows` 的 Cargo example 构建 | 复用上游锁文件与原生宿主，依赖和输入明确 | 快照需要本地 Git 对象，且仍需编译 Win32 依赖 | 锁定 SHA、编译日志、真实窗口截图 |
| B | 编译 NSH CLI 后启动上游 TSX 计数器 | 可复用已有示例，也能验证 CLI 链路 | 增加 TSX 打包、QuickJS 和 CLI 构建，证据偏离计划采用的 Rust 根组件 | CLI 日志、真实窗口截图 |

## 收敛决策

- 选择 A。参考仓库的 `HEAD` 与锁定提交一致，工作树有 38 个文件与该提交不同。仅从提交对象生成构建快照。
- 若依赖下载失败，依据日志有限重试。若窗口取证方式无法捕获 Direct2D 渲染，改用前台窗口区域捕获，人工检查像素内容。M1 的路线切换仍按 `plan.md`。

## 计划

- 提取锁定源码，构建独立探针，启动并捕获真实窗口。
- Eval 命令 `bash .artifact/goal/2026-11-18-vibe-own-edition/harness/tools/m1-native-smoke.sh`。构建和截图脚本均须退出 0，PNG 中需可见窗口标题 `vibe-own | NSH M1` 与客户区文字 `NSH 原生窗口已启动`。
- 核验 PNG 显示 NSH 渲染的文字，记录构建和格式化结果。

## 参考审查

- 复用 NSH Rust Gallery 的 `Root::of` + `AppSource::Rust` 入口，以及 Windows 后端关闭默认特性后的 Rust 路径。探针置于任务工具目录，不进入产品工作区。

## 执行记录

- 初始严格校验返回 `valid: true`。`goal_state.py new-loop` 创建 `loop-002`。
- 从锁定提交提取干净快照，复用其原始 `Cargo.lock`。新增探针示例与窗口截图脚本，提交范围限于 Goal 目录。
- 第一轮编译成功。取证拿到正确标题的 Win32 窗口句柄，随后 `.NET WaitForInputIdle` 抛出异常。将探针改为 Windows 子系统，改用句柄捕获首帧后的窗口。

## 验证

- 修正后 `m1-native-smoke.sh` 退出 `0`，Rust 1.99.1 编译成功，截图显示 `vibe-own | NSH M1` 和「NSH 原生窗口已启动」。证据见 `../evidence/m1-native-build.md` 和 `../evidence/m1-native-window.png`。
- `bash -n` 校验入口脚本通过。仓库 `pnpm run format` 退出 `0`。

## 结果

result: 完成

## 终止检查


- decision: continue
- 本轮原生编译、启动与渲染通过。M1 保持进行中，里程碑完成后再推送。
- 下轮用同一锁定 NSH 提交和同一个 Rust 根组件构建 Web 静态站点，在真实浏览器验证渲染并保存日志与截图。
- M1 后续还需两端接入真实 HTTP 和实时流，显示工作区 diff，并验证 5000 行滚动 p95。本轮公共 Rust 工具链证据对应 1.99.1。

## 更新

- 用 `goal_state.py set` 更新 `next_sub_goal` 与 `last_verified_loop`，收尾运行严格校验。
- `plan.md` 与 `harness/status.md` 记录 Windows 探针通过和下一项浏览器验证。`references.md` 记录干净快照和窗口捕获的排查经验。
