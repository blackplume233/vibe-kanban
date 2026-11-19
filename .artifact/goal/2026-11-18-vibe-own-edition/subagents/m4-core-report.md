# M4 交互核心交付报告

## Win32 冷启动分段复核
使用原隔离工作树和 5,005 条实际 SQLite 议题。后端和夹具在计时前就绪，同一夹具运行九个新建的 Release UI 进程。计时器从创建进程开始，通过 WinEvent 获取窗口显示事件，从 stderr 管道记录 HTTP 解码完成和首批议题可见。首批议题可见后只发送一次滚轮输入，以首次非零滚动偏移为响应。各样本前后检查编译进程，只接受 CPU 计数保持不变的两个闲置 `cl.exe`。有活跃编译时拒绝计时。

| 阶段 | 最快 | 中位 | 最慢 |
| --- | ---: | ---: | ---: |
| 启动至窗口显示 | 187.8 ms | 196.0 ms | 217.8 ms |
| 窗口显示至完整议题响应解码完成 | 255.0 ms | 383.5 ms | 567.2 ms |
| 议题解码完成至首批议题可见 | 3.4 ms | 3.5 ms | 3.8 ms |
| 发送滚轮至首个滚动事件 | 19.0 ms | 156.8 ms | 263.0 ms |
| 启动至首个滚动事件 | 467.3 ms | 739.2 ms | 996.0 ms |

九个样本均低于 1 秒。完整议题响应和滚轮派发贡献了主要波动。项目响应在窗口显示后 4.9–6.9 ms 完成，状态、周期和保存视图也都先于议题响应完成，因此保留现有数据加载逻辑。历史的 901.0 ms 和 1,250.5 ms 用整份日志轮询和重复输入测得。旧记录中的进程到窗口分别是 374.8 ms 和 490.1 ms，窗口到首滚分别是 526.2 ms 和 760.4 ms。旧日志没有保留足够精度的分段时间，无法把差值准确归因于某一次 API 或输入等待。

独立 Chromium 回归通过 SQLite 写入和滚动检查。首次可操作耗时 482.2 ms，渲染切片 p95 为 2.132 ms，rAF 间隔 p95 为 19.50 ms。原生 Release 可执行文件 SHA-256 为 `55aa3eabdd653f9e3ba25da4c708f302ddcb1405688664ce89bb5fa69cf45a90`，取证使用 `cargo +1.99.1`。

复现使用 `m4-runtime.py windows --cold-runs 9` 和该工作树构建的后端与 Release UI。[Win32 原始分段](../evidence/m4-native-cold-stages.json) · [Chromium 回归](../evidence/m4-cold-web-regression.json) · [Chromium 操作记录](../evidence/m4-cold-web-regression.log)

## 实现

- `crates/nsh-app/src/issue_actions.rs` 定义可用字符串保存的 `ActionId`、动作标签和别名、作用域、快捷键提示，以及带类型的项目、议题、状态和工作区 ID。议题的引用同时携带项目 ID，可直接定位 M2 的 `/api/projects/{project_id}/issues/{issue_id}`。
- `Invocation::dispatch` 是键盘、指针和拖放共用的派发入口。`ActionId::availability` 提供命令菜单所需的禁用原因，`Action::availability` 对实际参数追加校验。派发消费事件并调用一次处理器，保留来源和焦点恢复锚点。拖放预览复用同一校验，落点提供目标状态与可选的前置或后置议题锚点。M2 PATCH 处理器可根据最新列顺序把它们解析成 `status_id` 和 `sort_order`。
- `crates/nsh-app/src/command_search.rs` 在议题标题和短 ID、项目名、工作区名和分支、动作标签、别名和稳定键之间统一排序。匹配优先级依次是完全匹配、前缀、连续子串和有间隔的子序列。同分项优先展示可用结果，再按稳定 ID 排序。查询忽略 Unicode 空白并逐标量转小写，保留变音符号和标点。空查询只列出可从菜单启动的动作，任意查询最多返回 50 项。
- 查询字符与连续匹配表仅构建一次，候选借用 API 数据并以迭代器匹配，结果堆只保留前 K 项。搜索实体可生成与行、卡片和项目导航共用的 `Action::OpenEntity`。

## 验证

使用 `nightly-2026-08-01` 的 `rustfmt --edition 2024` 格式化两个模块。临时入口通过 `#[path]` 声明两个公开模块，用 `rustc --edition 2024 --test -D warnings` 编译并以 `--nocapture` 执行。11 项通过，0 项失败。

- 10 项模块单测覆盖稳定动作键、键盘/指针/拖放的同参单次派发、无效落点、输入法组合、连接与保存状态、字段校验、四类搜索、中文和非 ASCII 大小写、分支和短 ID 搜索、稳定排序、结果上限及空查询。
- 临时入口另用线程局部计数的全局分配器验证 1 和 5000 个匹配候选。相同查询和上限下，分别观察到 2 次和 2 次分配或重分配。候选数据在计数前构建。

## 集成边界

本次证据覆盖两个模块。M1 所属 crate 入口在集成时声明 `issue_actions` 与 `command_search`。NSH 根组件把实际键位、菜单与拖放事件转成 `Invocation`，在处理器内接入确认、HTTP 提交、乐观更新和回滚。Goal 的 E2/E3 仍需要真实界面的端到端操作、焦点和输入法检查，以及滚动与冷启动性能取证。Goal 目标和验收门无需修订。

## 议题 API 与看板状态

- `issues_api.rs` 复用 M1 的 NSH `fetch` 和响应封装。覆盖项目列表与创建、议题列表筛选与增删改查、批量 PATCH、子议题列表及创建、关系列表及增删、状态 CRUD 及重排，以及标签、cycle 和保存视图 CRUD。共享 HTTP 解码支持删除响应的 `data: null`，并将 M2 的错误消息传给调用方。
- `BoardState::dispatch` 通过已交付的动作派发校验处理新建、移动和编辑。移动按完整项目快照中的最新目标列和锚点排序。相邻排序值相同或没有可表示的中点时，通过一次 M2 批量 PATCH 重排列。一次仅提交一项看板变更，避免排序批次和字段编辑互相覆盖。
- `Submission::execute` 单独持有待提交的请求。`BoardState::finish` 用服务器返回记录更新状态，失败时只回滚本次变更的字段，并返回原焦点锚点。新建议题在服务器返回 ID 后加入看板。刷新版本与提交标识分别防止旧快照和其他看板的响应覆盖本地状态。选中项和滚动锚点均使用议题 ID。
- Windows 上运行 `CARGO_TARGET_DIR=F:/vk-targets/m4 cargo +1.99.1 test --offline --locked --manifest-path crates/nsh-app/Cargo.toml --lib`，19 项通过。新测试覆盖百分号与中文查询编码、PATCH 的省略与显式清空、删除响应、失效落点、隐含相邻议题、批量回滚、服务端 ID、错误响应和焦点锚点。
- 配置 Goal 的 LLVM 环境后，`CARGO_TARGET_DIR=F:/vk-targets/m4/web cargo +1.99.1 check --offline --locked --manifest-path crates/nsh-app/Cargo.toml --lib --target wasm32-unknown-unknown` 通过。
- NSH 界面接入时，从 `BoardEffect` 读取导航意图或启动异步提交，在请求成功、失败或取消时调用 `finish`。真实界面操作和性能验收属于下一切片。

## 共享议题界面

`App` 在同一运行时提供议题工作台和工作区视图。议题界面提供项目索引、虚拟列表、看板、命令搜索、右键和键盘菜单、拖放排序、详情、行内编辑、分诊、周期与保存视图。启动 Agent 的确认面板读取真实仓库、分支与执行器配置，提交后选中新工作区。

- `app.rs` 的 `selected` 保存共享工作区 ID，可在 `workspace_pane` 挂载工作区生命周期、PR、会话和终端。`selected_issue` 由 `board_view` 更新，生命周期组件可用该信号提供议题上下文。
- `board_ui.rs` 的 `Ui.project` 和 `Ui.board` 提供当前项目 ID 和状态列表。项目同步面板可挂在项目索引 `rail`，同步成功后递增 `Ui.reload` 触发重新加载。界面最外层的工作区和议题导航旁可挂载 Agent 管理。
- 使用同一工作树构建的调试后端和隔离 SQLite 启动 Win32 Release。5,000 条压力测试议题和 5 条具名议题通过 HTTP 校验。从启动原生进程到首次真实议题滚动耗时 901.0 ms。七个有效窗口中最高 `--fps` 主线程 `frame` p95 为 4.57 ms，滚动偏移从 288 到 6,000 pt。`gap` p95 约 264 ms，采样使用间歇滚轮输入，`gap` 包含等待绘制的时间。
- Win32 的独立键盘运行用窗口定向的方向键和 Enter 打开 `VO-1`，通过 UI Automation 核验详情标题。该运行首滚用时 1,250.5 ms，`frame` p95 为 7.14 ms。两次采样的原生冷启动结果分别低于和高于 1 秒目标。
- Chromium 157.0.8103.50 在 1360 × 900 视口验证键盘打开及返回、议题／项目／工作区模糊搜索、新建、行内编辑、右键菜单、键盘移动、拖放、分诊、周期分配和保存视图。写入后由 M2 API 核对 SQLite 数据。滚动 5,006 条议题时，482 个 `AnimationFrame::Render` 切片的 p95 为 3.007 ms，独立的 561 个 rAF 间隔的 p95 为 19.70 ms。以首次键盘打开议题后的绘制机会计，Web 冷启动用时 1,082.7 ms。前一次 Playwright 等待方式测得 2,661.3 ms。
- 浏览器用例未启动真实 Agent，Win32 本轮覆盖列表滚动及键盘打开详情。完整 Agent 到 PR 流程、IME 输入与重复冷启动验收仍需要集成实测。

证据见 [Win32 性能](../evidence/m4-native-results.json)、[Win32 键盘截图](../evidence/m4-native-keyboard-detail.png)、[Chromium 测量](../evidence/m4-web-results.json)、[操作日志](../evidence/m4-web-steps.log)、[看板拖放](../evidence/m4-web-drop.png)和[保存视图](../evidence/m4-web-saved-view.png)。复现入口为 `harness/tools/m4-runtime.py`。原始 Chromium 渲染 trace 位于 `F:/vk-targets/m4/proof-web-measured/web-scroll-trace.json`。
