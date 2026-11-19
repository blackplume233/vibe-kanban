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

## loop-004 锁定 nightly 复核

- 经 USTC 镜像修复 `nightly-2026-08-01`，安装 Win32 与 `wasm32-unknown-unknown` 标准库及 Clippy。`cargo +nightly-2026-08-01 check --locked --workspace` 退出 0，日志为本机 `F:/vk-targets/main/m0-nightly-check.log`。
- `rust-src` 首次安装遇到中断残留。保留冲突目录后补装 `rust-src` 和 `rust-analyzer`，在仓库根直接运行 `cargo --version` 与 `rustc --version` 均识别到锁定的 `1.99.0-nightly`。
- 合入 Windows lint 修复与本地议题底座后，锁定 nightly 的公开工作区严格 Clippy 退出 101。诊断位于 `crates/services/src/services/analytics.rs`，详见本机 `F:/vk-targets/main/m0-m2-nightly-clippy.log`。完整根检查仍需处理 `crates/remote` 的私有 `billing` 依赖。
- 合入 Windows `analytics.rs` 修复后，`cargo clippy --locked --workspace --all-targets --features qa-mode -- -D warnings` 退出 0，日志为本机 `F:/vk-targets/main/m0-m2-nightly-clippy-after.log`。
- 锁定 nightly 的 M2 集成测试通过 `db` 8 项和 `server` 24 项。类型生成初次校验只因 Windows 检出换行失败。临时目录生成的所有文件逐字节等于仓库 LF 内容。合入 LF 检出规则后重建已有检出的生成文件，`generate_types --check` 退出 0。

## loop-006 集成工作区

- 在 `90d4f723bcd956b3bbb35b7dc1841f522bea4c4f` 的独立工作树，加载 `harness/env.sh` 后执行锁定 nightly 的 `cargo test --locked --workspace`。退出 `0`，64 个测试目标合计 340 项通过、4 项忽略。原始日志位于 `F:/vk-targets/main/e1-integrated-workspace-tests.log`。E1 在正式包与默认入口切换后复跑。

## loop-008 默认构建候选

在隔离工作树 `verify-product-cutover` 将 Web 托管和默认构建切换合入共享产品入口。使用锁定 nightly、pnpm 10.13.1 和离线 Cargo 缓存验证。

| 命令 | 结果 |
| --- | --- |
| `pnpm run check` | 通过。将独立工作树中 CRLF 的 `nsh.json` 按生成器重新生成后，主题字节校验通过。 |
| `cargo test --locked --workspace` | 361 项通过，5 项忽略。 |
| `pnpm run generate-types:check` | TypeScript 与 10 个 JSON schema 一致。 |
| `pnpm run lint` | Claude 评审方法与调用点共用 `qa-mode` 条件后通过。 |
| `cargo build --locked -p server --bin server` | 通过。修复空工作区的 Git 状态后，standalone 同源托管通过 Chromium 实测。 |

日志位于 `F:/vk-targets/main/cutover-*-v2.log`、`cutover-lint-v3.log` 和 `cutover-git-status-build.log`。这些结果来自候选工作树 `fe416bba2` 及上述两处修复，最终分支切换仍需对等验收。

## 集成候选复验

`my/m7-integrated-cutover` 在同一工作树合入终端依赖、Web 托管、默认发布命令和关机修复。用锁定 nightly 和离线依赖运行 `cargo check --locked --workspace`、`pnpm run check`、`pnpm run lint`、`pnpm run generate-types:check` 与 `cargo test --locked --workspace`，均退出 0。工作区测试通过 361 项，忽略 5 项。Cargo 元数据中的 NSH 默认图不含 `crates/remote`、`billing`、Tauri 或独立 relay 服务。原始日志在 `F:/vk-targets/main/integrated-cutover-*.log`。最终提交仍需在合入远程 ACP 后验收。

集成至 `my/main` 的提交为 `6731b8402`。带受信 ACP 节点的集成候选已经通过格式化、Cargo 工作区检查、默认 `check`、`lint` 及类型生成校验。首次运行工作区测试时，Windows 因页文件不足返回 `os error 1455`，低并发复验见 `F:/vk-targets/main/final-e1-clean-tests-j2.log`。

`pnpm run build:web` 生成 `F:/vk-targets/main/integrated-cutover-web/vibe-own/`，以同一候选的 standalone 服务托管。Chrome 键盘流程八步全通过。`index.html` SHA-256 为 `42c2bc673134d5aef751395f41dc1c74f9c6f98f21fb10cb16987078a8f0d298`，WASM SHA-256 为 `7f305be07be9eccd586a931a321e3123908777a746f313fa3b8624be083c60be`。根目录压缩副本全部解包并按字节匹配。

锁定 nightly 在独立目标目录执行 `cargo test --jobs 2 --locked --workspace`，363 项通过、5 项忽略，退出 0。构建使用 `CARGO_INCREMENTAL=0`、`CARGO_PROFILE_TEST_DEBUG=0` 和本地离线缓存。原始日志为 `F:/vk-targets/main/final-e1-clean-tests-j2.log`。

`pnpm run build:windows` 生成 `F:/vk-targets/main/final-windows/vibe-own.zip`。ZIP 内 11 项通过 CRC 校验，SHA-256 为 `eb7c1171b25d0da77a94a6523afab2c6cf7a55dd02d095f3e925ce66351e41e2`。`verify-packages.py` 用独立 SQLite 和 Git 工作树启动最终 Win32 包与对应 Web 包。双端显示实时日志与实际 diff，Chromium 建立 4 条同源 WebSocket。关闭 Win32 窗口后应用退出 0，API 端口关闭，子进程心跳停止。见 [Win32 截图](final-package-native.png) 与 [Web 截图](final-package-browser.png)。
