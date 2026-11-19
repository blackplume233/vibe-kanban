# M4 交互核心交付报告

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
