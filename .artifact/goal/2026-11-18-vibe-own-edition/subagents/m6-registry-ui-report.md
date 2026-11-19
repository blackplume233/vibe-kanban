# M6 NSH ACP Registry、Agent 预设与 MCP 配置

`crates/nsh-app/src/agent_settings.rs` 提供独立挂载入口 `pub(crate) fn agent_settings(api: Api, on_profiles_changed: impl Fn() + 'static) -> impl IntoElement`。根设置页可以挂载此入口，并在回调中使工作区启动器等处的预设列表重新载入。回调仅在安装、卸载或预设写入获服务确认后触发。

| 功能 | 行为 |
| --- | --- |
| ACP 适配器 | 搜索注册表，显示安装状态，安装与卸载分别确认。安装后选择新适配器的 `DEFAULT` 预设。 |
| Agent 预设 | 按执行器和预设选择，支持创建空预设、复制、编辑和确认删除。重选会重新读取服务端。保存前以最新完整文档为基准只修改目标预设并检查其是否发生变化，保留其他预设和最近使用记录。 |
| MCP 配置 | 按执行器读取配置路径、JSON 和内置模板。按服务端给出的 `servers_path` 校验 JSON，确认后提交服务映射，保留配置文件中的其他字段。取消确认不提交，切换前为草稿提供放弃确认。 |

MCP 传输沿用 `Api::request<T>`。NSH 侧的 MCP 错误信息不回显请求体、响应内容或解析片段。测试仅使用无凭据的合成配置，证据只记结果。

## 核验环境

- `nightly-2026-08-01`，NSH `12af3d778b588c2837267ff6aaed678eb5ad72a2`，`F:/vk-targets/m6-registry-ui` 专用构建目录。
- 后端从 `d2d1423ac9` 导出并离线构建，绑定 `127.0.0.1:59486`。数据库、`profiles.json` 和 ACP MCP 配置均位于该导出目录的 `dev_assets`。验证窗口使用现有 `Api` 访问该服务。
- 临时 Win32 入口位于目标目录，挂载生产面板并安装同一套设计 token。二进制构建成功。`agent_settings::data::tests` 三项通过，覆盖预设名称、ACP 标识、局部预设写入和 MCP 路径校验。

## Win32 交互

UI Automation 在实际 NSH 窗口操作。取消 Claude 安装后预设保持原样，确认分别安装 `claude-acp` 和 `codex-acp` 后，面板选中新适配器的本地预设。逐项比较原有执行器的完整预设，全部一致。复制 Codex ACP 预设为 `REVIEW_PASS` 保留 `registry_id`，编辑后的 `append_prompt` 经服务端读回。

另一次请求为 Codex ACP 新增预设后，重选同一执行器可见新预设并保持当前选中项。确认删除当前预设后，其他预设仍在。对 MCP 输入 `[]` 时显示字段校验错误。输入无凭据的服务配置后，取消确认保持文件内容，确认写入后由 API 读回。重选 MCP 页再次载入已保存配置，文件中的无关字段保持原值。分别确认卸载两个适配器后，原有执行器预设与初始值完全一致。

证据位于 `F:/vk-targets/m6-registry-ui/`。`native-flow.log` 的前三条记录对应安装，`native-flow-validated.log` 对应预设、MCP 和卸载。`win-codex-reselected.png` 与 `win-isolated-mcp-roundtrip.png` 是 1100 × 770 的实际 Win32 窗口截图。此切片的界面验证平台为 Win32。
