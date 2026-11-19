# 循环 005

## 基础目标切片

集成 M0–M6 已交付的后端和共享界面切片，启动 M7 正式打包及本地终端验证。

## 子目标

在已合并的分支上验证 M2 与 M6 的组合，定位 NSH 锁定工具链交叉构建失败，准备真实 GitHub 验收环境。

## HTN 分解

| 切片 | 隔离 worktree | 交付证据 |
|---|---|---|
| M1 nightly 兼容 | `F:/vk-wt/m1` | 锁定 NSH 和 Rust nightly 下双端构建、HTTP、WebSocket、diff 与滚动测量 |
| M3 设计展示 | `F:/vk-wt/m3` | 真实双端亮暗和两档密度截图 |
| M4 界面接入 | `F:/vk-wt/m4` | 议题请求与动作，接入共享根后的运行证据 |
| M5 GitHub | `F:/vk-wt/m5` | 与持久队列集成的测试，以及私有仓库的 CI、评审及议题往返 |
| M6 ACP | `F:/vk-wt/m6` | 保持既有 profiles 与会话可用的 ACP 兼容迁移 |
| M7 打包和终端 | `F:/vk-wt/m7-package` 与 `F:/vk-wt/m7-terminal` | 两个正式 NSH 包、进程内本地服务和真实 PTY 交互 |

## 发散候选

| 方案 | 方法 | 证据要求 |
|---|---|---|
| A | 独立分支通过后直接记录里程碑完成 | 分支证据只能覆盖各自切片 |
| B | 子任务先同步共享契约，再集成和复验关键行为 | 独立证据和集成证据都能追溯到对应版本 |

## 收敛决策

选 B。M2、M5、M6 共用路由、生成类型和队列调用方。协调者核对分支范围，在合入后运行受影响的测试。

## 执行与验证

- M2 本地 API 已验收。锁定 nightly 下数据库和服务端 32 项测试通过。Windows 的 LF 检出规则使类型生成检查通过。
- M6 已合入用量、队列与 MCP 注入。集成后共有 180 项测试通过、4 项忽略，生成类型和所涉 crate 的严格 Clippy 均通过。
- M1 已合入 Win32 和 Chrome 的真实 HTTP、实时日志与 DiffView。锁定 nightly 的复验重试使用 Windows Git Bash，Win32 编译停在 `filetime 0.2.30` 的 E0658。M1 和 M7 正在验证兼容的依赖锁定。
- M3 已合入单一 token 源和生成器，生成器的 `--check` 通过。M4 已合入共用动作和搜索核心。M7 已使用锁定提交编出 `nsh 0.4.0` CLI。
- 为 E5 创建私有 `blackplume233/vibe-own-sandbox`。产品分支的推送仍在完整验收后执行。

## 终止检查

- decision: continue
- reason: M2 已验证，M1、M3–M7 仍有实现与集成证据待补。
- remaining_evidence: 双端锁定 nightly 运行、UI 完整路径、真实 GitHub 往返、ACP 兼容、正式打包与产品对等检查、E1–E7 终止审计及远端核对。
