# M7 本地设置切片

## 挂载与数据契约

- 实现提交 `793dc58751d13d46349175d98d02d1804d2af6dd`。集成时在 `lib.rs` 声明 `mod local_settings;`，路由分支挂载 `crate::local_settings::panel(move || …)`。签名为 `pub(crate) fn panel(on_back: impl Fn() + 'static) -> impl IntoElement`。组件持有草稿、窗口返回前确认和本地 API 请求生命周期。
- 产品页提供外观、语言、默认执行器与变体、发送快捷键、PR 与提交提示、通知偏好。编辑器页提供默认编辑器、自定义命令、适用编辑器的 SSH 和扩展选项，以及分支前缀、预览和工作区目录。使用 `GET /api/info` 及 `PUT /api/config`。保存前重新读取完整 `Config`，合入当前表单修改的字段路径，再提交完整 DTO。并发修改位于读回与写入之间时，现有服务端 API 没有版本检查。
- 仓库页提供注册、选择、确认移除、默认工作目录和目标分支，以及 setup、cleanup、archive、dev、复制文件和并行初始化。`PUT /api/repos/{id}` 只发送修改字段，空白可选值明确发 `null`。仓库目录选择使用现有 `/api/filesystem/directory`。
- 共享挂载由集成者接入。语言和发送快捷键在此面板编辑后端偏好，实际语言与消息输入行为由消费端接线决定。临时 Win32 与 Web 入口位于切片 target 目录，并随实现分开保留。

## 验证

在 `F:/vk-targets/m7-settings` 使用 `nightly-2026-08-01`、锁定 NSH Git 版本和各自新建的测试 profile。真实 vibe server 由 M7 已支持 `VK_ASSET_DIR` 的主题包启动。每次操作前确认隔离目录下 `data/db.v2.sqlite` 已创建。只对其中临时仓库与配置写入 fixture。Win32 通过真实窗口的输入、按钮和滚动事件操作。Web 使用 Chromium 157.0.8103.50 加载共享组件编译出的 WASM，通过同源代理调用服务端。

- Rust 单测 3 项通过，覆盖 `Config` 叶字段覆盖、仓库清空和省略、Git 分支前缀与路径编码。通过本切片 `rustfmt --check` 和 NSH 主题 token 生成检查。Win32 与 `wasm32-unknown-unknown` 的离线锁定构建均成功。
- Win32 输入非法前缀 `bad/slash` 后尝试保存，GET 确认原值保留。随后保存 `native-m7` 和工作区目录。保存前由另一请求修改 `host_nickname`，GET 确认该修改保留。仓库页保存 setup、cleanup、dev 脚本。GET 分别返回 `echo native-setup`、`echo native-cleanup`、`echo native-dev`。退出并新建 Win32 窗口后，重新打开两个表单，截图确认保存值。
- Web 在编辑器页保存 `web-m7` 与自定义编辑器命令 `web-fixture-editor`。在仓库页保存两行 setup 和 dev 命令并清空 cleanup。GET 回读确认 `cleanup_script: null`，保留未编辑的 archive、目标分支、复制文件及产品 `host_nickname`。刷新浏览器后再次打开两个表单，确认重新载入保存值。

证据目录 `F:/vk-targets/m7-settings/evidence`。Win32 保存与回读截图以 `win32-product-`、`win32-repo-` 和 `win32-dev-` 开头，Web 截图以 `web-product-` 和 `web-repo-` 开头。后端回读记录为 `win32-values.json` 与 `web-values.json`，HTTP 序列在 target 目录的 `native-evidence-run.log`、`web-evidence-run.log`。独立验证入口安装了语义主题与字体。标题字号、卡片圆角等额外 Tailwind 量表由正式包的应用配置安装。
