# M0 Windows 公开工作区 Clippy 证据

## 复现命令

在 `F:/vk-wt/m0` 的 `my/m0` 分支运行。`packages/local-web/dist` 从集成工作树的同名目录复制，供 `server` 嵌入。LLVM 路径取自 [Goal 构建环境](../harness/env.sh)。

```bash
export CARGO_TARGET_DIR=F:/vk-targets/m0
export LIBCLANG_PATH='C:/Program Files/Microsoft Visual Studio/18/Community/VC/Tools/Llvm/x64/bin'
cargo +1.99.1 clippy --workspace --all-targets --features qa-mode -- -D warnings \
  > F:/vk-targets/m0/m0-public-clippy.log 2>&1
```

## 诊断与修复

| 位置 | 诊断 | 修复 |
|---|---|---|
| `crates/utils/src/command_ext.rs` | `unused_imports` | 删除 Tokio `Command` 实现内多余的 Windows 扩展 trait 导入，保留标准库 `Command` 实现所需的导入。 |
| `crates/utils/src/shell.rs` | `clippy::useless_conversion` | 删除 `OsString` 到同型的转换。 |
| `crates/tauri-app/build.rs` | `clippy::collapsible_if` | 合并资源文件存在与复制成功的两个短路条件。 |
| `crates/embedded-ssh/src/sftp.rs` | `unused_variables` | 将权限处理限定于 Unix，并显式消费其他平台不用的 `setstat` 参数。 |
| `crates/git/tests/git_workflow.rs` | `unused_imports` | 将 `DiffChangeKind` 导入移入使用它的 Unix 权限测试。 |

原始集成日志为 `F:/vk-targets/main/m0-public-clippy.log`。直接加载 `harness/env.sh` 的首轮在编译前以 `101` 退出。该工作树的 CRLF 检出使 `CARGO_HTTP_TIMEOUT` 尾随 `\r`，Cargo 无法将其解析为整数。错误日志位于 `F:/vk-targets/m0/m0-public-clippy-harness.log`。

显式设置环境后，首次编译越过原始日志中的四处诊断，在 `git_workflow` 测试发现 Unix 专用导入未限定作用域，以 `101` 退出。该轮日志位于 `F:/vk-targets/m0/m0-public-clippy-first-build.log`。

## 最终结果

严格 Clippy 退出码为 `101`。最终日志未再出现表列的五处 Windows 诊断。编译推进到 `executors`，其库与库测试均报告同一组 19 处 `clippy::double_must_use`。诊断来自 `#[async_trait]` 展开，生成的方法带有不含消息的 `#[must_use]`，返回值是已经标记为 `must_use` 的装箱 Future。

| 文件（相对于 `crates/executors/src/`） | `#[async_trait]` 诊断位置 | 受影响的方法及行号 |
|---|---|---|
| `actions/mod.rs` | `74:15` | `spawn:77` |
| `approvals.rs` | `29:15` | `create_tool_approval:32`、`create_question_approval:35`、`wait_tool_approval:42`、`wait_question_answer:49` |
| `executors/acp/session_config.rs` | `417:15` | `set_config_option:420`、`set_mode:421`、`set_model:422` |
| `executors/codex/jsonrpc.rs` | `275:15` | `on_request:277`、`on_response:284`、`on_error:291`、`on_notification:298`、`on_non_json:305` |
| `executors/mod.rs` | `309:15` | `spawn:316`、`spawn_follow_up:324`、`spawn_review:333`、`get_setup_helper_action:360`、`discover_options:378`、`warm_options_cache:391` |

本次结果只覆盖 Windows 上的公开 Cargo 工作区 Clippy 命令。根 Goal 的 E1 仍待完整验证。
