# vibe-own 设计系统

这是 M3 的设计契约。Win32 和浏览器端共用 Rust 根组件、组件库和交互状态。M1 探针已经用同一份 `vibe-own-root.rs` 在两端显示中文。M1 通过路线闸门后，在该共享根路径接入主题生成和展示页。设计依据为 NSH `12af3d778b588c2837267ff6aaed678eb5ad72a2`。E4 以生成检查和两端实机截图为准。 [Goal][goal] · [路线闸门][gate] · [Win32 探针][m1-win] · [Web 探针][m1-web]

## 视觉方向与桌面布局

把界面做成工程工作台。浅色用暖纸色工作区和石墨文字，深色用带青色的石墨底。导航像项目索引，议题 ID、运行时长和分支名用等宽字对齐。当前项目左侧有一条窄强调线，选中议题同时有底色与标记。面板靠底色和细分隔线区分，阴影只用于浮层。默认强调色是铜绿，琥珀用于可选外观。Linear 只作为交互质量参照，具体表现为快速搜索、就地编辑、即时反馈和操作后回到原上下文。

| 层 | 位置与版式 |
| --- | --- |
| 工作区 | 顶栏容纳项目路径、视图切换和「搜索命令」。宽窗口左侧是 208 pt 的项目与保存视图索引。中间是主滚动区，看板只在自己的区域横向滚动。底部常驻应用状态行。 |
| 议题详情 | 宽度达到 `wide`（1200 pt）时右侧打开 360 pt 检查栏，保留看板和滚动位置。低于 `wide` 时使用共享根中的单栏详情视图。低于 `split`（960 pt）时项目索引收到顶栏按钮。切换布局时保留选中议题、未保存草稿和返回位置。 |
| 浮层 | 一级是菜单和提示，二级是命令菜单与确认对话框。模态期间背景不可操作。焦点轮廓应覆盖选中底色，滚动容器要留出轮廓空间。浮层放在 NSH 根内。Win32 需要区分 NSH 合成的 WebView2 视觉层与拥有独立窗口的原生控件，后者可以盖住 NSH。 [Win32 合成与限制][win] · [Web 渲染][web] |

## 单一 token 源

M3 将 `crates/nsh-app/design/tokens.json` 作为唯一手写设计值。`colors` 包含逐角色亮暗值。`accent` 保存两种强调色对，每种都提供实底、实底前景和选择底色。`space`、`type`、`shape`、`size`、`density` 与 `motion` 共用于所有主题。尺寸用 NSH 逻辑点，时长用毫秒。`schemaVersion` 固定语义，解析时检查必填键、枚举键、重复键和数值范围。

外观默认跟随系统，可显式改为亮色或暗色。默认 `verdigris`、`comfortable`，三个偏好独立持久化。换强调色时统一切换主按钮、链接、焦点和选中态，状态色保持原值。紧凑模式只缩减间距与行高，保留字号、焦点尺寸、键位和信息量。行高是单行最小值，换行及放大后的内容可扩展。虚拟列表必须按实际行高或固定单行规格重新计算，并按议题 ID 保持可见锚点。

下面的 JSON 是 v1 输入样例。颜色数组顺序固定为 `[light, dark]`。`type.scale` 的第二项是无单位行高倍率，`motion.easeOut` 是三次贝塞尔的四个控制参数。

```json
{
  "schemaVersion": 1,
  "defaults": { "appearance": "system", "accent": "verdigris", "density": "comfortable" },
  "colors": {
    "canvas": ["#F4F1E9", "#141B1D"],
    "rail": ["#EAE6DA", "#1B2427"],
    "panel": ["#FFFCF5", "#222D30"],
    "popover": ["#FFFDF8", "#283639"],
    "hover": ["#EEECE2", "#2D383A"],
    "ink": ["#232A2D", "#F2EFE5"],
    "muted": ["#515C60", "#AAB8B8"],
    "divider": ["#CBC7BA", "#3C4B4F"],
    "controlEdge": ["#68716E", "#849A9C"],
    "info": ["#2F506F", "#A0CAF3"],
    "ok": ["#285C38", "#A1D9A4"],
    "warn": ["#7A4803", "#F8CE92"],
    "error": ["#A12B23", "#FFA39A"]
  },
  "accent": {
    "verdigris": {
      "fill": ["#00695F", "#74D5C3"], "onFill": ["#FFFFFF", "#142320"],
      "selection": ["#DCEDE9", "#263F40"]
    },
    "amber": {
      "fill": ["#8A5009", "#F2BE67"], "onFill": ["#FFFFFF", "#142320"],
      "selection": ["#F5E7CC", "#423523"]
    }
  },
  "space": { "step": 4 },
  "type": {
    "body": ["Source Sans 3", "Microsoft YaHei UI", "Noto Sans CJK SC", "sans-serif"],
    "mono": ["IBM Plex Mono", "Microsoft YaHei UI", "Noto Sans CJK SC", "monospace"],
    "scale": { "meta": [12, 1.5], "body": [14, 1.5], "title": [18, 1.35], "page": [24, 1.25] },
    "weights": { "regular": 400, "emphasis": 600 }
  },
  "shape": { "control": 4, "card": 6, "overlay": 8, "focusWidth": 2, "focusOffset": 2 },
  "size": { "split": 960, "wide": 1200, "rail": 208, "detail": 360 },
  "density": {
    "comfortable": { "rowMin": 40, "controlMin": 36, "statusMin": 32, "gutter": 16 },
    "compact": { "rowMin": 32, "controlMin": 28, "statusMin": 28, "gutter": 12 }
  },
  "motion": { "feedbackMs": 80, "overlayMs": 120, "panelMs": 160, "reducedMs": 0, "easeOut": [0.2, 0, 0, 1] }
}
```

### 生成与安装

在仓库根目录用 Node 20 或更高版本生成产物。提交前运行 `--check`，它按字节检查每个派生文件，缺失或过期时返回失败并列出文件名。

```bash
node crates/nsh-app/design/generate-tokens.mjs
node crates/nsh-app/design/generate-tokens.mjs --check
```

产物在 `crates/nsh-app/design/`。将 `tailwind.generated.json` 中的 `appearance`、`theme` 和 `tailwind` 字段合入 `nsh.json`。其中 `theme` 根据默认强调色指向 `theme-verdigris.generated.json` 或 `theme-amber.generated.json`。共享根从 `tokens.json` 读取默认密度、完整字型回退和其余样式参数，并使用同一对派生主题文件切换强调色。

- 现有构建流程调用一次确定性生成步骤。它把 `space.step`、`type.scale`、`shape.card` 和 `size.split/wide` 写成 `nsh.json` 的 `tailwind.spacing`、`fontSize`、`radius.card` 和 `breakpoints`。同一步生成各强调色的 NSH `theme` JSON，按 `cssVars.theme/light/dark` 输出。所有派生文件由生成器管理，同一输入得到逐字节相同的输出。`nsh.json` 继续由 NSH CLI 打包。 [Tailwind 配置与主题][tw] · [配置解析器][tw-source]
- `canvas/rail/panel/popover` 映射到 `background/sidebar/card/popover`，`ink` 映射到对应前景和 `accent-foreground`，`muted` 映射 `muted-foreground`，`divider/controlEdge` 映射 `border/input`。`hover` 输出为 `surface-hover`，也供基础组件的 `secondary` 与 `muted` 底色使用，其中 `secondary-foreground` 取 `ink`。强调色的 `fill/onFill/selection` 映射 `primary`、`primary-foreground`、`accent`，`ring` 取 `fill`。`sidebar-*` 的动作、选中、边框和焦点角色映射到相应的全局角色。状态使用 `status-info/ok/warn/error`，其中 `error` 也供 `destructive` 使用。生成器校验映射覆盖实际用到的角色，禁止在 `tailwind.colors` 复制主色等语义名，因为调色板会覆盖同名语义类。 [默认主题字段][base-theme] · [主题优先级][tw]
- Rust 在两端共用 `nsh_base::theme()` 的响应式绑定读取颜色。`Root::tokens` 安装初始主题，CLI 生成的 Rust 入口从 `nsh.json` 的 `theme` 安装同一份默认初值。切换强调色时，共享根调用 `set_tokens` 安装对应派生主题。`set_color_scheme` 切换 `System/Light/Dark`，不会重建根。密度、字号、间距和动效取自同一份输入，作为共享 Rust 样式参数读取。类型化 Tailwind 方法使用内建量表，定制数值须显式传给 Rust 样式方法。语义类在 NSH 解析，构建入口继续使用现有 NSH 工具链。 [Rust 主题][rust-theme] · [CLI 根接线][cli-root] · [运行时主题接口][runtime-theme] · [外观设置][scheme] · [Tailwind 量表][tw]
- 生成检查应核对键集合、配对颜色、两个亮暗值、单位以及所有投影字段。用锁定 NSH 的 `tailwind::Theme::from_json` 与 `nsh_base::Theme::from_json` 解析各自输出，并断言所用 token 的类型和值。主题解析可接受省略字段或略去不能识别的值，单凭解析成功无法证明投影完整。按下列样例断言关键值，随后用 `--check` 比对所有产物。生成失败或输出过期时让打包失败。 [配置解析器][tw-source] · [语义主题解析器][theme-parser]

以下两个断言对象只列输出节选。第一个是 `nsh.json` 中的字段，第二个是默认强调色的语义主题文件。选用 `amber` 时，`primary` 和 `ring` 的亮暗值应变为 `#8A5009` 和 `#F2BE67`，字号和密度应保持原值。

```json
{
  "tailwind": {
    "spacing": 4,
    "fontSize": { "body": [14, 1.5] },
    "radius": { "card": 6 },
    "breakpoints": { "split": 960, "wide": 1200 }
  }
}
```

```json
{
  "cssVars": {
    "theme": { "radius": "4px", "font-sans": "Source Sans 3", "font-mono": "IBM Plex Mono" },
    "light": { "primary": "#00695F", "primary-foreground": "#FFFFFF", "ring": "#00695F" },
    "dark": { "primary": "#74D5C3", "primary-foreground": "#142320", "ring": "#74D5C3" }
  }
}
```

## 字型、动效与可访问性

- **字型与 CJK**。本地打包 Source Sans 3 的 400/600 和 IBM Plex Mono 的 400 字体及许可证。标题和正文沿用正文族，ID、日志、时长和按键提示用等宽族。中文跟随 JSON 中的 fallback 顺序。基础主题的字体解析器只保存第一字体名，完整回退链在共享根通过 `.font_family(..)` 应用到正文和等宽组件。Windows 按段落首个样式的列表回退，浏览器按自身字体机制补字。中英混排用完整标点、换行和双行议题标题检查字高与省略。重要 ID 保留可查找、可复制的完整值。 [字体资源与回退][fonts] · [字体 token 解析][theme-parser]
- **动效**。按压和悬停立即反馈，颜色变化最多 80 ms。浮层淡入最多 120 ms，检查栏位移最多 160 ms。焦点、状态文本和待保存提示即时更新。列表滚动与虚拟行换位保持直接响应。减少动效时全部时长归零，加载用静态图标与文字。共享组件读取 `reduce_motion()`。Win32 取系统动画开关，Web 取 `prefers-reduced-motion`。 [NSH 动效][motion] · [Win32 系统设置实现][win-env] · [Web 环境实现][web-env]
- **焦点与对比度**。焦点描边为 2 pt，外距 2 pt，放到与环色不同的相邻底色上。实底强调按钮的环与按钮之间保留面板色间隙。焦点、当前选中、按下、悬停分别表达，焦点移出时选中照常保留。普通正文、辅助字、状态文字和按钮文字至少 4.5:1。必要边框、焦点形状和信息图标与相邻底色至少 3:1。`divider` 仅供装饰。状态有文字或形状提示。注意 NSH 的 `focus-visible:` 与 `focus:` 表示同一种状态。 [焦点变体][tw] · [无障碍接口][a11y]
- **对比度回归**。按 WCAG sRGB 相对亮度检查最终叠加后的颜色。样例在 `canvas`、`rail`、`panel`、`popover`、`hover` 和两种 `selection` 上的最低正文对比度为 9.77:1，辅助字为 5.49:1，状态文字为 5.83:1。`controlEdge` 最低 3.79:1，两个强调色的焦点环最低 5.21:1，主按钮实底与前景最低 6.50:1。这些是样例颜色计算结果，截图和实际控件组合另行取证。
- **无障碍语义**。图标按钮提供动作名，列表提供选中状态，错误贴近触发处并使用受支持的通知机制。浏览器核对 ARIA 树，Windows 核对 UI Automation。锁定版的 Win32 原生 alert 与 toast 无 UI Automation 暴露，错误与保存结果要在内容区域或状态行保留可读取的文本。Windows 的平台说明只保证 `Tab` 在文本字段间移动，Web 支持遍历 pressable 和 input。以共享的控件或议题 ID 记录可操作的焦点位置，由两端适配器把键盘事件及焦点移动接入组件。验收需要证明 Win32 能用键盘进入列表、调用动作及退出浮层，画出的焦点环须对应实际接收键盘动作的控件或列表项。 [Win32 键盘和无障碍限制][win] · [Web 键盘与 ARIA][web] · [UI Automation 实现][win-a11y]

## 组件行为

| 组件 | 构成与状态 |
| --- | --- |
| 顶栏与议题行／卡 | 顶栏保持项目路径和主要动作可见。行从左到右排列状态图标、等宽 ID、标题、标签和次要元数据。卡使用相同顺序，标题最多两行，完整内容在详情可见。展示静止、悬停、按下、焦点、选中、禁用与保存中。侧栏与虚拟列表保持稳定 ID。 |
| 命令菜单 | 顶栏按钮与 `mod+k` 打开同一个 NSH 对话框。焦点进入搜索框，上下键改变高亮，回车提交高亮项，指针点选同一项。结果分为动作、议题、项目、工作区。结果行显示快捷键，禁用行显示原因。Esc 在结束输入法组合后关闭浮层并恢复触发控件或最近的有效列表锚点。用 `nsh_base` 的 `Command` 提供高亮和虚拟化。M4 使用 `.filterable(false)` 与共享查询结果替换其默认子串匹配，保证跨实体模糊查询只有一套排序。 [Command][command] · [Dialog][dialog] |
| 应用状态行 | 左侧显示 `本地已连接`、`正在重连`、`连接断开`，并展示失败原因及可用的「重试」。右侧显示 Agent 运行和排队数，足够宽时显示上一操作结果。窄窗口先收起次要字段。用固定区域呈现持续错误，瞬时提示不抢焦点。状态行由共享根绘制，使用普通控件与文本。 [Windows 状态栏限制][win] |
| 议题详情 | 首行显示返回、议题 ID 和更多操作。标题与描述后依次展示属性、子议题与关系，再展开关联工作区的运行、日志和 diff。就地编辑保存失败时保留输入并把错误留在字段旁。滚动区内部提供完整内容，关闭后按议题 ID 还原原视图与焦点。工作区可复用 NSH 的 `DiffView` 与 `Editor`。 [DiffView][diff] · [Editor][editor] |
| 空态与错误 | 空项目提供「新建议题」，筛选无结果提供「清除筛选」，命令无结果保留搜索框。首次加载显示静态布局和「正在加载」。首次数据失败提供原因与「重试」。已有数据后连接失败时保留当前可读内容并标明数据可能过期。禁用动作提供原因。状态文案与操作入口应随键盘和指针保持一致。 |

## M3 展示页与截图验收

使用同一份固定数据和展示页入口挂到两端的共享根，覆盖外观切换、基本输入、卡片、详情、状态行、浮层及空错态。示例议题统一使用 `VO-014 修复任务重试后的状态漂移`，再配一条很长的中文及英文混合标题、一个不可用动作和一个保存失败的输入。基线均用 1280 × 800 逻辑点的应用客户区／浏览器视口，Win32 记录 DPI 和文本缩放，浏览器记录 Chromium 版本、视口、DPR 与缩放。截图须显示正在运行的界面。

| 用例 | Win32 基线 | Chromium 基线 | 画面必须显示 |
| --- | --- | --- | --- |
| 亮色 · 舒适 | `win-light-comfortable.png` | `web-light-comfortable.png` | 默认铜绿、工作区三栏、选中与聚焦行、已连接状态。 |
| 亮色 · 紧凑 | `win-light-compact.png` | `web-light-compact.png` | 同样的数据、选中项和字级，行高缩减后状态行和焦点环均完整。 |
| 暗色 · 舒适 | `win-dark-comfortable.png` | `web-dark-comfortable.png` | 默认铜绿、暗色详情与命令浮层。搜索框聚焦、高亮项和禁用原因可辨。 |
| 暗色 · 紧凑 | `win-dark-compact.png` | `web-dark-compact.png` | 同一命令留在同高亮项，密度变化，正文、辅助文本和焦点保持清晰。 |
| 补充 · 琥珀 | `win-amber-light.png`、`win-amber-dark.png` | `web-amber-light.png`、`web-amber-dark.png` | 保持紧凑，比较两种亮暗外观的按钮、焦点与选中底色，状态色保持一致。 |
| 补充 · 窄容器 | `win-narrow.png` | `web-narrow.png` | 900 × 720 逻辑点，中文长标题、单栏详情、编辑失败和连接异常。Win32 设为 150% 显示缩放，浏览器检查 125% 页面缩放，记录实际逻辑视口。 |
| 补充 · 空态与减少动效 | `win-empty-reduced.png` | `web-empty-reduced.png` | 过滤无结果和清除按钮可见。另附交互记录，确认系统开关生效、浮层无位移、焦点在切换后可见。 |

以应用 Git 提交、NSH 锁定提交、token 输入摘要、生成检查日志、启动及操作步骤索引截图，存入 Goal 的 `evidence/`。按相同检查项比较两端，允许字体栅格和原生窗口边框差异。补录 Win32 和浏览器的键盘焦点、中文 IME 提交、状态文字及减少动效检查，因为截图单独无法验证这些行为。M3 的实际生成日志和截图仍待集成取证。 [Goal E4][goal]

## M4 动作接入边界

M3 提供组件、样式、状态槽和事件入口。M4 建立一份动作注册表，每项由稳定 `ActionId`、标签和别名、适用作用域、可用性及原因、快捷键提示和统一处理器组成。事件携带稳定实体 ID 与显式目标参数。来源只用作交互记录或焦点恢复。命令菜单、键位适配器、右键与更多菜单、按钮和拖放都调用同一派发入口。处理器负责校验、确认、提交、乐观更新和失败回滚，同一动作只提交一次。

| 动作 | 键盘入口 | 指针入口 | 共享输入与结果 |
| --- | --- | --- | --- |
| `command.open` | `mod+k` | 顶栏「搜索命令」 | 当前作用域、打开来源、焦点恢复位置。`mod` 在 Windows 为 Ctrl，在 Apple 键盘的受支持浏览器上为 Cmd。浏览器仅在收到该键时处理，顶栏入口始终存在。 |
| `issue.open` | 议题行或命令结果上按 Enter | 点击议题行、卡或搜索结果 | `issue_id`，相同详情视图及返回锚点。 |
| `issue.move` | 选中议题，从命令菜单或键盘打开的更多菜单选「移到状态」 | 从更多／右键菜单选目标，或卡片拖到目标列 | `issue_id` 与 `status_id`，排序时附锚点 ID。拖动只预览，落到有效目标才派发，取消和无效落点不写入。 |
| `issue.edit` | 在字段内确认编辑 | 点击字段后用确认按钮 | `issue_id` 与字段更新。两端复用校验、保存状态和失败信息。 |

键位通过 NSH `on_key_with` 按当前作用域注册。编辑器和输入法组合期间先处理文本操作，列表导航和破坏性命令只在对应作用域、非编辑状态启用。命令菜单优先使用组件自带的方向键和提交逻辑。Win32 与 Web 都要实测 Esc、Enter、重入和组合提交，回车确认候选词只提交组合文本。M4 用同一 fixture 对照每个入口的 `ActionId`、参数、调用次数、结果和返回位置。拖放可基于 NSH 的 pointer/pan 能力实现投放预览，同步提供无需拖动的状态选择。 [键位注册][keys] · [Win32 输入][win] · [Web 输入][web] · [手势与指针][gestures]

[goal]: ../.artifact/goal/2026-11-18-vibe-own-edition/goal.md
[gate]: ../.artifact/goal/2026-11-18-vibe-own-edition/plan.md#收敛决策
[m1-win]: ../.artifact/goal/2026-11-18-vibe-own-edition/evidence/m1-native-build.md
[m1-web]: ../.artifact/goal/2026-11-18-vibe-own-edition/evidence/m1-web-build.md
[tw]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/tailwind.md#theme
[tw-source]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-core/src/tailwind/theme.rs#L425-L455
[base-theme]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-base/src/default-theme.json
[rust-theme]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/rust.md#theme
[cli-root]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-cli/src/rust_app.rs#L2360-L2386
[runtime-theme]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-core/src/element/ambient.rs#L338-L354
[theme-parser]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-base/src/theme.rs#L298-L429
[scheme]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/device-and-environment.md#an-app-with-its-own-appearance
[fonts]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/fonts.md#shipping-a-font
[motion]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/animation.md#reduced-motion
[win-env]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-platform-windows/src/system.rs#L229-L245
[web-env]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-platform-web/web/nsh.js#L1677
[a11y]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/accessibility.md
[win-a11y]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-platform-windows/src/access/uia.rs#L429
[win]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/windows.md#what-differs-on-windows
[web]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/web.md#what-runs-where
[command]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/elements/command.md
[dialog]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/elements/dialog.md
[diff]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/elements/diff-view.md
[editor]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/elements/editor.md
[keys]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/crates/nsh-core/src/element/ambient.rs#L1132-L1211
[gestures]: https://github.com/longbridge/nsh/blob/12af3d778b588c2837267ff6aaed678eb5ad72a2/website/docs/touch-and-gestures.md
