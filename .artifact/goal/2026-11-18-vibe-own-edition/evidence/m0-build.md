# M0 构建证据 (2026-11-18)

- 前端：pnpm --filter @vibe/local-web run build 通过（2m29s，主 chunk 5,622 kB / gzip 1,698 kB）
- 后端：LIBCLANG_PATH=VS LLVM，cargo +1.99.1 check --workspace 通过（2m07s）

```
    Checking mcp v0.1.46 (G:\Workspace\AgentWorkSpace\vibe-kanban\crates\mcp)
    Checking services v0.1.46 (G:\Workspace\AgentWorkSpace\vibe-kanban\crates\services)
    Checking relay-hosts v0.1.46 (G:\Workspace\AgentWorkSpace\vibe-kanban\crates\relay-hosts)
    Checking deployment v0.1.46 (G:\Workspace\AgentWorkSpace\vibe-kanban\crates\deployment)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 07s
```

## loop-001 补充验证

在仓库根目录的 `my/main` 上复跑。构建前加载 `harness/env.sh`，设置 `RUSTUP_TOOLCHAIN=1.99.1`。实测 pnpm `10.13.1`、Node `22.17.1`。

| 命令 | 结果 | 依据 |
|---|---|---|
| `pnpm run format` | 通过（0） | Rust 与 Web 格式化脚本执行完毕。完整日志 `../.vk-m0-format.log`。 |
| `pnpm run check`（Windows 默认脚本 shell） | 失败（1） | `cmd.exe` 无法执行 `./scripts/check-legacy-frontend-paths.sh`。完整日志 `../.vk-m0-check.log`。 |
| `pnpm run check`（Git Bash 脚本 shell） | 部分通过（总退出码 101） | 路径检查、四个前端包的类型检查及 `cargo check --workspace` 通过。随后 `crates/remote` 解析可选的 `billing` Git 依赖时，无法从 `BloopAI/vibe-kanban-private` 认证获取锁定版本。完整日志 `../.vk-m0-check-bash.log`。 |
| `env SHELLOPTS=igncr bash ./scripts/check-legacy-frontend-paths.sh` | 通过（0） | 五项路径与导航检查通过。完整日志 `../.vk-m0-path-guard.log`。 |
| `pnpm run lint` | 部分通过（总退出码 1） | `@vibe/local-web` 与 `@vibe/ui` 的 ESLint 均通过。后端入口报 `cargo-clippy.exe` 未安装。完整日志 `../.vk-m0-lint.log`。 |
| `node scripts/check-unused-i18n-keys.mjs` | 通过（0） | 独立运行了因 Clippy 失败而跳过的翻译键检查。完整日志 `../.vk-m0-i18n-lint.log`。 |
| `cargo +1.99.1 test --workspace`（修复前） | 失败（101） | 编译完成。执行器单测 116 通过、3 失败、3 忽略。LS、相对路径转换、Amp 文件编辑别名测试将 Unix `/tmp/...` 用作绝对路径。完整日志 `../.vk-m0-test.log`。 |
| `cargo +1.99.1 test --workspace`（修复路径夹具后） | 失败（101） | 执行器单测 119 通过、3 忽略。随后 `git_ops_safety` 中 26 通过、3 失败，三项失败均展示 checkout 产物的 CRLF 与期望 LF 不符。完整日志 `../.vk-m0-test-after.log`。 |
| `cargo +1.99.1 test --workspace --no-fail-fast`（最终） | 通过（0） | 64 个测试目标合计 322 项通过、4 项忽略。执行器单测 119 通过，`git_ops_safety` 29 通过，工具 crate 的 `test_make_path_relative` 通过。完整日志 `../.vk-m0-test-final.log`。 |
| `pnpm run format`（最终） | 通过（0） | 测试夹具调整后重新格式化。完整日志 `../.vk-m0-format-final.log`。 |

日志路径相对于仓库根目录。

Windows 下使用以下方式运行完整检查，保留原脚本全部检查项：

```bash
source .artifact/goal/2026-11-18-vibe-own-edition/harness/env.sh
export RUSTUP_TOOLCHAIN=1.99.1
npm_config_script_shell="$(cygpath -w "$(type -P bash)")" pnpm run check
```

完整通过要求命令退出码为 0。若停在 `billing` Git 依赖的认证错误，说明公开工作区检查已通过，但完整检查仍受阻。
