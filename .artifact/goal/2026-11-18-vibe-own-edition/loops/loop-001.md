# 循环 001

## 基础目标切片

M0 构建基线，补齐 E1 中尚无记录的工作区测试与仓库检查。

## 子目标

在 `my/main` 补跑 `cargo +1.99.1 test --workspace`、`pnpm run check` 和 `pnpm run lint`，补齐 M0 证据。修复可独立验证的基线问题，记录其余失败的复现命令和下一道验证门。

## HTN 分解

- 无。单工作区执行，避免多个进程争用依赖构建目录。

## 仓库基准检查

- 文档与回复使用中文。
- Rust 使用已安装的 `1.99.1`，pnpm 为 `10.13.1`，Node 为 `22.17.1`。构建前加载 `harness/env.sh`，pnpm 的 Cargo 子命令继承 `RUSTUP_TOOLCHAIN=1.99.1`。
- 使用仓库现有命令验收，日志落在仓库外，筛选后的结果写入 `evidence/m0-build.md`。
- 提交限 `my/main` 上的本轮路径，排除运行生成物以及已有的路由和 Tauri schema 改动。
- 已读 Goal、计划收敛与里程碑、工具绑定、控制面板、构建环境、既有证据和参考记录。

## 发散候选

两种方案均复用仓库现有的检查入口和依赖。

| 方案 | 核心方法 | 优点 | 风险与代价 | 预期证据 |
|---|---|---|---|---|
| A | 直接运行三个仓库验收命令，记录各自退出码并诊断首个失败 | 覆盖真实的根脚本、测试边界和工作区配置 | Rust 编译、远端 crate 依赖下载和 Clippy 组件可能受环境限制 | 三个命令各自的完整结果 |
| B | 先按 Rust crate 和前端包拆分运行，最后再合并跑根命令 | 更早定位失败所属包，适合已知失败复查 | 首跑重复编译和检查，拆包通过不能证明根脚本通过 | 细粒度诊断，仍需完整根命令验收 |

## 收敛决策

- 选择 A。对应 M0 尚缺的三道门，能在现有工作区复用依赖产物。
- 下载按日志实际故障有限重试。缺少组件且安装命令未列入绑定时，记录待人工决策。根命令失败后，转用 B 定位并验证可独立修复的原因。

## 计划

- [x] 校验 Goal，运行三道基线检查和仓库格式化命令。
- [x] 记录实际退出码与诊断，筛选本轮提交路径。
- [x] 确定下一子目标和本轮严格校验命令。

## 参考审查

- 复用已有 pnpm 脚本和 Cargo 测试入口。现有命令本身即可复跑，本轮无需新增任务局部脚本。

## 执行记录

- 初始 `validate_goal_file.py --strict` 返回 `valid: true`，由 `goal_state.py new-loop` 创建本文件。
- `pnpm run check` 首跑退出码 1。Windows 的默认脚本 shell 在 `./scripts/check-legacy-frontend-paths.sh` 处报错。下一次运行用进程级 `npm_config_script_shell` 指定 Git Bash。
- 执行者误启动未列在工具绑定中的 `rustup component add clippy --toolchain 1.99.1`。在下载阶段取消，随后确认工具链仍未安装 Clippy。本轮保留现有工具绑定。
- 指定 Git Bash 后，根 `pnpm run check` 完成路径检查、四个前端包的类型检查和公开 Rust 工作区检查。`crates/remote` 工作区在解析 `billing` 私有 Git 依赖时认证失败。
- 根 `pnpm run lint` 跑完本地 Web 和 UI 包的 ESLint，在 Clippy 入口退出。随后单独验证翻译键检查。
- `cargo +1.99.1 test --workspace` 首跑三项 Claude 路径测试失败。`make_path_relative` 用 `Path::is_relative()` 保留相对路径。Windows 将缺盘符的 `/tmp/...` 视为相对路径。Claude 与工具 crate 改用宿主系统认可的绝对路径夹具。
- 首次复验中三项 Claude 测试均通过，随后 Git 集成测试有三项因 CRLF 与 LF 不符而失败。仓库夹具现在仅在临时仓库的本地配置中设定 `core.autocrlf=false`，保留文件内容断言。

## 验证

- `cargo +1.99.1 test --workspace --no-fail-fast` 退出码 0，322 项通过、4 项忽略。此前失败的六项测试与工具 crate 的路径用例均通过。最终 `pnpm run format` 退出码 0。
- `pnpm run check` 在 Git Bash 下退出码 101，公开工作区检查通过。`pnpm run lint` 退出码 1，两套前端 ESLint 通过，独立翻译键检查通过。两项完整检查的解除条件见 `../harness/status.md`。
- 命令、通过信号和日志索引见 `../evidence/m0-build.md`。

## 结果

result: 部分完成

## 终止检查


- decision: continue
- M0 保持进行中。公开工作区达到基线，完整 check/lint 尚缺私有依赖访问权限与 Clippy。M1 可以先验证公开工具链的原生构建。
- 下一子目标是在锁定的 NSH 版本上验证 Windows 最小原生应用能编译并启动，保存日志与窗口截图。
- M0 的推送闸门仍待完整 check/lint 通过。本轮仅作本地提交。

## 更新

- `goal.md` 由状态脚本更新 `next_sub_goal` 与 `last_verified_loop`。收尾闸门为 `validate_goal_file.py --strict` 返回 `valid: true`。
- `plan.md` 与 `harness/status.md` 记录 M0 通过项、受阻项、解除条件和 M1 下一步。
- `references.md` 记录脚本解释器、私有依赖、工具绑定和跨平台测试夹具的排查结果。
