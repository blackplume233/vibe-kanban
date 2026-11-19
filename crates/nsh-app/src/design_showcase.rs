#[path = "design_showcase/tokens.rs"]
mod tokens;

use nsh_base::{CommandEntry, CommandItem, command, dialog};
use nsh_core::{
    LineHeight,
    element::{Appearance, Root, Text, View, apple_keys, input},
    keymap::When,
    prelude::*,
};
use tokens::{Accent, Density, Scheme, color, settings};

#[derive(Clone, Copy, PartialEq)]
pub struct ShowcaseOptions {
    scheme: Scheme,
    density: Density,
    accent: Accent,
    palette: bool,
    narrow: bool,
    empty: bool,
}

impl Default for ShowcaseOptions {
    fn default() -> Self {
        let defaults = &settings().defaults;
        Self {
            scheme: defaults.appearance,
            density: defaults.density,
            accent: defaults.accent,
            palette: false,
            narrow: false,
            empty: false,
        }
    }
}

impl ShowcaseOptions {
    pub fn fixture(name: &str) -> Option<Self> {
        let mut options = Self::default();
        match name {
            "light-comfortable" => options.scheme = Scheme::Light,
            "light-compact" => options.scheme = Scheme::Light,
            "dark-comfortable" | "dark-compact" => {
                options.scheme = Scheme::Dark;
                options.palette = true;
            }
            "amber-light" | "amber-dark" => {
                options.accent = Accent::Amber;
                options.scheme = if name == "amber-light" {
                    Scheme::Light
                } else {
                    Scheme::Dark
                };
                options.density = Density::Compact;
            }
            "narrow" => {
                options.scheme = Scheme::Light;
                options.narrow = true;
            }
            "empty-reduced" => {
                options.scheme = Scheme::Dark;
                options.empty = true;
            }
            _ => return None,
        }
        if name.ends_with("-compact") {
            options.density = Density::Compact;
        }
        Some(options)
    }

    pub fn appearance(self) -> Appearance {
        self.scheme.into()
    }

    pub fn size(self) -> (i32, i32) {
        if self.narrow { (900, 720) } else { (1280, 800) }
    }
}

pub fn root(options: ShowcaseOptions) -> Root {
    Root::new::<DesignShowcase>(move || DesignShowcaseProps {
        dark: options.scheme == Scheme::Dark,
        light: options.scheme == Scheme::Light,
        compact: options.density == Density::Compact,
        amber: options.accent == Accent::Amber,
        palette: options.palette,
        narrow: options.narrow,
        empty: options.empty,
    })
    .tokens(options.accent.theme().clone().into_tokens())
    .appearance(options.appearance())
    .font(&include_bytes!("../design/fonts/SourceSans3.ttf")[..])
    .font(&include_bytes!("../design/fonts/IBMPlexMono-Regular.ttf")[..])
}

#[derive(Clone, PartialEq, Props)]
pub struct DesignShowcaseProps {
    dark: bool,
    light: bool,
    compact: bool,
    amber: bool,
    palette: bool,
    narrow: bool,
    empty: bool,
}

#[derive(Clone, Copy)]
struct Issue {
    id: &'static str,
    title: &'static str,
    label: &'static str,
    code: &'static str,
    lane: usize,
}

const ISSUES: [Issue; 6] = [
    Issue {
        id: "VO-014",
        title: "修复任务重试后的状态漂移",
        label: "运行时",
        code: "runtime/retry",
        lane: 1,
    },
    Issue {
        id: "VO-019",
        title: "对齐断线重连后的 workspace event checkpoint 与中文日志溯源",
        label: "同步",
        code: "ws/reconnect",
        lane: 1,
    },
    Issue {
        id: "VO-022",
        title: "记录会话 token 用量",
        label: "Agent",
        code: "acp/usage",
        lane: 0,
    },
    Issue {
        id: "VO-024",
        title: "归档失效的本地工作区",
        label: "本地",
        code: "worktree/prune",
        lane: 0,
    },
    Issue {
        id: "VO-011",
        title: "校验离线 diff 导出",
        label: "审阅",
        code: "diff/export",
        lane: 2,
    },
    Issue {
        id: "VO-009",
        title: "精简项目切换路径",
        label: "导航",
        code: "nav/project",
        lane: 2,
    },
];
const LANES: [(&str, &str, &str); 3] = [
    ("待处理", "status-warn", "○"),
    ("进行中", "primary", "◐"),
    ("已完成", "status-ok", "✓"),
];

#[derive(Clone, Copy)]
struct State {
    density: Signal<Density>,
    scheme: Signal<Scheme>,
    accent: Signal<Accent>,
    palette: Signal<bool>,
    selected: Signal<usize>,
    cursor: Signal<usize>,
    detail: Signal<bool>,
    projects: Signal<bool>,
    list: Signal<bool>,
    empty: Signal<bool>,
    disconnected: Signal<bool>,
    draft: Signal<String>,
    error: Signal<bool>,
}

impl State {
    fn open(self, index: usize) {
        self.selected.set(index);
        self.cursor.set(index);
        self.detail.set(true);
        self.draft.set(ISSUES[index].title.to_owned());
        self.error.set(false);
    }

    fn move_cursor(self, delta: isize) {
        if self.board_active() {
            self.cursor.set(
                self.cursor
                    .get()
                    .saturating_add_signed(delta)
                    .min(ISSUES.len() - 1),
            );
        }
    }

    fn board_active(self) -> bool {
        !self.palette.get()
            && !self.empty.get()
            && !self.projects.get()
            && (!self.detail.get() || window().width >= px(settings().size.wide))
    }

    fn select_scheme(self, scheme: Scheme) {
        self.scheme.set(scheme);
        set_color_scheme(scheme.into());
    }

    fn select_accent(self) {
        let next = self.accent.get().toggle();
        self.accent.set(next);
        set_tokens(next.theme().clone().into_tokens());
    }

    fn toggle_density(self) {
        self.density.set(self.density.get().toggle());
    }
}

#[component]
pub fn DesignShowcase(props: DesignShowcaseProps) -> impl IntoElement {
    let scheme = if props.dark {
        Scheme::Dark
    } else if props.light {
        Scheme::Light
    } else {
        Scheme::System
    };
    let density = if props.compact {
        Density::Compact
    } else {
        Density::Comfortable
    };
    let accent = if props.amber {
        Accent::Amber
    } else {
        Accent::Verdigris
    };
    let state = State {
        density: signal(density),
        scheme: signal(scheme),
        accent: signal(accent),
        palette: signal(props.palette),
        selected: signal(0),
        cursor: signal(0),
        detail: signal(!props.empty),
        projects: signal(false),
        list: signal(false),
        empty: signal(props.empty),
        disconnected: signal(props.narrow),
        draft: signal(ISSUES[0].title.to_owned()),
        error: signal(props.narrow),
    };
    on_key("mod+k", move |_| state.palette.set(!state.palette.get()));
    let board_keys = KeyOptions {
        when: When::NotEditing,
        pass_through: true,
        ..Default::default()
    };
    on_key_with("j", board_keys, move |_| state.move_cursor(1));
    on_key_with("k", board_keys, move |_| state.move_cursor(-1));
    on_key_with("enter", board_keys, move |_| {
        if state.board_active() {
            state.open(state.cursor.get());
        }
    });
    view()
        .test_id("design-showcase")
        .flex_1()
        .min_h_0()
        .bg(|| color("background"))
        .font_family(settings().typography.body.as_str())
        .child(toolbar(state))
        .child(
            view()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(when(
                    move || window().width >= px(settings().size.split) || state.projects.get(),
                    move || sidebar(state),
                ))
                .child(when(
                    move || window().width >= px(settings().size.wide) || !state.detail.get(),
                    move || workbench(state),
                ))
                .child(when(state.detail, move || {
                    dynamic(move || inspector(state, window().width >= px(settings().size.wide)))
                })),
        )
        .child(status_line(state))
        .child(when(state.palette, move || palette(state)))
}

fn step(n: impl Into<f64>) -> Points {
    px(settings().space.step * n.into() as f32)
}
fn density(state: State) -> &'static tokens::DensityMetrics {
    state.density.get().metrics()
}

fn styled_text(
    value: impl IntoReactive<SharedString>,
    scale: (f32, f32),
    role: &'static str,
    bold: bool,
    mono: bool,
) -> Text {
    let typography = &settings().typography;
    let family = if mono {
        &typography.mono
    } else {
        &typography.body
    };
    text(value)
        .font_family(family.as_str())
        .text_color(move || color(role))
        .text_size(px(scale.0))
        .line_height(LineHeight::Multiple(scale.1))
        .font_weight(FontWeight(if bold {
            typography.weights.emphasis
        } else {
            typography.weights.regular
        }))
}

fn meta(value: impl IntoReactive<SharedString>, role: &'static str) -> Text {
    styled_text(value, settings().typography.scale.meta, role, false, false)
}
fn mono(value: impl IntoReactive<SharedString>, role: &'static str) -> Text {
    styled_text(value, settings().typography.scale.meta, role, false, true)
}
fn body(value: impl IntoReactive<SharedString>, bold: bool) -> Text {
    styled_text(
        value,
        settings().typography.scale.body,
        "foreground",
        bold,
        false,
    )
}
fn title(value: impl IntoReactive<SharedString>) -> Text {
    styled_text(
        value,
        settings().typography.scale.title,
        "foreground",
        true,
        false,
    )
}
fn rule() -> View {
    view().h(px(1.0)).flex_shrink_0().bg(|| color("border"))
}

fn duration(normal: f32) -> f32 {
    if reduce_motion() {
        settings().motion.reduced_ms
    } else {
        normal
    }
}

fn control(
    state: State,
    id: &'static str,
    label: impl IntoReactive<SharedString>,
    pressed: impl Fn() + 'static,
) -> View {
    let shape = &settings().shape;
    let [x1, y1, x2, y2] = settings().motion.ease_out;
    view()
        .test_id(id)
        .role(AccessibilityRole::Button)
        .on_press(pressed)
        .min_h(move || px(density(state).control))
        .px(step(2.5))
        .justify_center()
        .rounded(px(shape.control))
        .border_1()
        .border_color(|| color("input"))
        .transition_colors()
        .duration(|| duration(settings().motion.feedback_ms))
        .ease_bezier(x1, y1, x2, y2)
        .hover(|s| s.bg(|| color("surface-hover")))
        .active(|s| s.bg(|| color("accent")))
        .focus(|s| {
            s.outline(px(shape.focus_width))
                .outline_offset(px(shape.focus_offset))
                .outline_color(|| color("ring"))
        })
        .child(body(label, true).truncate())
}

fn toolbar(state: State) -> View {
    view()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(step(2))
        .px(step(4))
        .py(step(3))
        .border_b_1()
        .border_color(|| color("border"))
        .bg(|| color("card"))
        .child(
            view()
                .flex_row()
                .min_w_0()
                .items_center()
                .gap(step(2))
                .child(when(
                    || window().width < px(settings().size.split),
                    move || {
                        control(state, "projects", "项目", move || {
                            state.projects.set(!state.projects.get())
                        })
                    },
                ))
                .child(mono("V/O", "primary").font_weight(FontWeight::SEMIBOLD))
                .child(body("vibe-own", true).truncate())
                .child(meta("/", "muted-foreground"))
                .child(meta("本地工程", "muted-foreground").truncate()),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(step(2))
                .child(control(
                    state,
                    "accent-toggle",
                    move || match state.accent.get() {
                        Accent::Verdigris => "铜绿",
                        Accent::Amber => "琥珀",
                    },
                    move || state.select_accent(),
                ))
                .child(control(
                    state,
                    "scheme-toggle",
                    move || match state.scheme.get() {
                        Scheme::System => "跟随系统",
                        Scheme::Light => "亮色",
                        Scheme::Dark => "暗色",
                    },
                    move || {
                        state.select_scheme(match state.scheme.get() {
                            Scheme::System => Scheme::Light,
                            Scheme::Light => Scheme::Dark,
                            Scheme::Dark => Scheme::System,
                        })
                    },
                ))
                .child(control(
                    state,
                    "density-toggle",
                    move || match state.density.get() {
                        Density::Comfortable => "舒适",
                        Density::Compact => "紧凑",
                    },
                    move || state.toggle_density(),
                ))
                .child(control(
                    state,
                    "command-open",
                    if apple_keys() {
                        "搜索命令  ⌘K"
                    } else {
                        "搜索命令  Ctrl+K"
                    },
                    move || state.palette.set(true),
                )),
        )
}

fn sidebar(state: State) -> View {
    view()
        .w(px(settings().size.rail))
        .flex_shrink_0()
        .p(step(4))
        .gap(step(3))
        .bg(|| color("sidebar"))
        .border_r_1()
        .border_color(|| color("sidebar-border"))
        .child(meta("项目", "muted-foreground"))
        .child(
            view()
                .min_h(move || px(density(state).row))
                .justify_center()
                .px(step(2))
                .border_l(step(0.5))
                .border_color(|| color("sidebar-primary"))
                .bg(|| color("sidebar-accent"))
                .child(body("本地工程", true)),
        )
        .child(rule())
        .child(meta("保存视图", "muted-foreground"))
        .child(control(state, "all-issues", "全部议题", move || {
            state.empty.set(false);
            state.projects.set(false);
            state.detail.set(false);
        }))
        .child(control(state, "filter-empty", "仅我负责", move || {
            state.empty.set(true);
            state.projects.set(false);
            state.detail.set(false);
        }))
        .child(view().flex_1())
        .child(rule())
        .child(mono("workspace / local", "muted-foreground"))
}

fn workbench(state: State) -> View {
    view()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .p(move || px(density(state).gutter))
        .gap(step(3))
        .child(
            view()
                .flex_row()
                .justify_between()
                .items_center()
                .gap(step(2))
                .child(styled_text(
                    "工程看板",
                    settings().typography.scale.page,
                    "foreground",
                    true,
                    false,
                ))
                .child(
                    view()
                        .flex_row()
                        .gap(step(2))
                        .child(control(
                            state,
                            "view-toggle",
                            move || {
                                if state.list.get() {
                                    "切换看板"
                                } else {
                                    "切换列表"
                                }
                            },
                            move || state.list.set(!state.list.get()),
                        ))
                        .child(control(state, "filter-toggle", "仅我负责", move || {
                            state.empty.set(!state.empty.get())
                        })),
                ),
        )
        .child(rule())
        .child(dynamic(move || {
            if state.empty.get() {
                view()
                    .flex_1()
                    .gap(step(3))
                    .items_center()
                    .justify_center()
                    .child(title("筛选无结果"))
                    .child(meta("当前没有分配给你的议题。", "muted-foreground"))
                    .child(control(state, "clear-filter", "清除筛选", move || {
                        state.empty.set(false)
                    }))
            } else if state.list.get() {
                let rows = ISSUES
                    .iter()
                    .enumerate()
                    .fold(view().gap(step(2)), |rows, (index, _)| {
                        rows.child(issue_card(state, index, true))
                    });
                view()
                    .flex_1()
                    .min_h_0()
                    .child(scroll().flex_1().child(rows))
            } else {
                let lanes = LANES.iter().enumerate().fold(
                    view().flex_row().flex_1().gap(step(2)),
                    |lanes, (index, _)| lanes.child(lane(state, index)),
                );
                view()
                    .flex_1()
                    .min_h_0()
                    .child(scroll().flex_1().horizontal().child(lanes))
            }
        }))
        .child(when(
            move || !state.empty.get(),
            || mono("J / K  定位议题    Enter  打开详情", "muted-foreground"),
        ))
}

fn lane(state: State, index: usize) -> View {
    let (name, role, icon) = LANES[index];
    let cards = ISSUES
        .iter()
        .enumerate()
        .filter(|(_, issue)| issue.lane == index)
        .fold(view().gap(step(2)).p(step(1)), |cards, (index, _)| {
            cards.child(issue_card(state, index, false))
        });
    view()
        .min_w(step(50))
        .flex_1()
        .gap(step(2))
        .p(step(2))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(step(1.5))
                .child(mono(icon, role))
                .child(body(name, true))
                .child(mono("02", "muted-foreground")),
        )
        .child(rule())
        .child(cards)
}

fn issue_card(state: State, index: usize, list: bool) -> View {
    let issue = ISSUES[index];
    let shape = &settings().shape;
    let (_, role, icon) = LANES[issue.lane];
    let label = view()
        .flex_row()
        .items_center()
        .gap(step(1.5))
        .child(mono(icon, role))
        .child(mono(issue.id, "muted-foreground"));
    let row = view()
        .test_id(issue.id)
        .role(AccessibilityRole::Button)
        .on_press(move || state.open(index))
        .rounded(px(shape.card))
        .border_1()
        .border_color(move || {
            color(if state.selected.get() == index {
                "primary"
            } else {
                "border"
            })
        })
        .bg(move || {
            color(if state.selected.get() == index {
                "accent"
            } else {
                "card"
            })
        })
        .outline(px(shape.focus_width))
        .outline_offset(px(shape.focus_offset))
        .outline_color(move || {
            color("ring").opacity(if state.cursor.get() == index && !state.palette.get() {
                1.0
            } else {
                0.0
            })
        })
        .hover(|s| s.bg(|| color("surface-hover")))
        .active(|s| s.bg(|| color("accent")))
        .focus(|s| s.outline_color(|| color("ring")))
        .min_h(move || px(density(state).row))
        .px(step(2))
        .justify_center();
    if list {
        row.flex_row()
            .items_center()
            .justify_start()
            .gap(step(2))
            .child(label)
            .child(body(issue.title, true).flex_1().truncate())
            .child(meta(issue.label, "muted-foreground"))
    } else {
        row.py(move || px(density(state).gutter / 2.0))
            .gap(step(1.5))
            .child(label)
            .child(body(issue.title, true).number_of_lines(2))
            .child(meta(issue.label, "muted-foreground"))
    }
}

fn inspector(state: State, wide: bool) -> View {
    let field = input()
        .test_id("title-input")
        .label("议题标题")
        .value(move || state.draft.get())
        .on_change(move |value: &str| {
            state.draft.set(value.to_owned());
            state.error.set(false);
        })
        .on_submit(move |_: &str| state.error.set(true))
        .font_family(settings().typography.body.as_str())
        .text_color(|| color("foreground"))
        .text_size(px(settings().typography.scale.body.0))
        .min_h(move || px(density(state).control))
        .px(step(2))
        .rounded(px(settings().shape.control))
        .border_1()
        .border_color(move || {
            color(if state.error.get() {
                "status-error"
            } else {
                "input"
            })
        })
        .focus(|s| {
            s.outline(px(settings().shape.focus_width))
                .outline_offset(px(settings().shape.focus_offset))
                .outline_color(|| color("ring"))
        });
    let pane = view()
        .test_id("issue-detail")
        .min_h_0()
        .bg(|| color("card"))
        .border_l_1()
        .border_color(|| color("border"));
    let pane = if wide {
        pane.w(px(settings().size.detail)).flex_shrink_0()
    } else {
        pane.flex_1().min_w_0()
    };
    pane.child(
        scroll().flex_1().child(
            view()
                .p(move || px(density(state).gutter))
                .gap(step(3))
                .child(
                    view()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(mono(move || ISSUES[state.selected.get()].id, "primary"))
                        .child(control(state, "close-detail", "返回看板", move || {
                            state.detail.set(false);
                            state.cursor.set(state.selected.get());
                        })),
                )
                .child(title(move || ISSUES[state.selected.get()].title))
                .child(body(
                    "重放工作区事件后核对议题状态，避免恢复后的运行记录覆盖最近一次保存。",
                    false,
                ))
                .child(rule())
                .child(meta("属性 · 议题标题", "muted-foreground"))
                .child(field)
                .child(when(state.error, || {
                    meta("保存失败。工作区当前为只读，输入已保留。", "status-error")
                }))
                .child(control(state, "save-title", "保存标题", move || {
                    state.error.set(true)
                }))
                .child(rule())
                .child(
                    view()
                        .flex_row()
                        .justify_between()
                        .child(meta("状态", "muted-foreground"))
                        .child(body(
                            move || LANES[ISSUES[state.selected.get()].lane].0,
                            true,
                        )),
                )
                .child(
                    view()
                        .flex_row()
                        .justify_between()
                        .child(meta("子议题", "muted-foreground"))
                        .child(meta("暂无", "muted-foreground")),
                )
                .child(
                    view()
                        .flex_row()
                        .justify_between()
                        .child(meta("关系", "muted-foreground"))
                        .child(mono("VO-019", "primary")),
                )
                .child(rule())
                .child(meta("关联工作区", "muted-foreground"))
                .child(
                    view()
                        .p(step(3))
                        .gap(step(1))
                        .rounded(px(settings().shape.card))
                        .bg(|| color("muted"))
                        .child(mono(
                            move || ISSUES[state.selected.get()].code,
                            "foreground",
                        ))
                        .child(meta("最近运行 · 02:18 · 2 个文件变更", "muted-foreground")),
                ),
        ),
    )
}

fn status_line(state: State) -> View {
    view()
        .test_id("status-line")
        .min_h(move || px(density(state).status))
        .flex_shrink_0()
        .px(step(4))
        .flex_row()
        .items_center()
        .justify_between()
        .gap(step(2))
        .bg(|| color("sidebar"))
        .border_t_1()
        .border_color(|| color("border"))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(step(2))
                .child(dynamic(move || {
                    if state.disconnected.get() {
                        meta("× 连接断开 · 本地数据可能过期", "status-error")
                    } else {
                        meta("✓ 本地已连接", "status-ok")
                    }
                }))
                .child(when(state.disconnected, move || {
                    control(state, "retry-local", "重试", move || {
                        state.disconnected.set(false)
                    })
                })),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(step(3))
                .child(when(
                    || window().width >= px(settings().size.split),
                    || meta("最近操作 · 已读取工作区", "muted-foreground"),
                ))
                .child(mono("Agent  1 运行 / 2 排队", "muted-foreground")),
        )
}

fn palette(state: State) -> impl IntoElement {
    let items = vec![
        CommandItem::new("issue", "打开议题 VO-014")
            .group("议题")
            .keywords(["retry", "状态漂移"]),
        CommandItem::new("density", "切换列表密度").group("动作"),
        CommandItem::new("project", "返回本地工程").group("项目"),
        CommandItem::new("workspace", "推送关联工作区")
            .group("工作区")
            .disabled(true),
    ];
    let shape = &settings().shape;
    let enter = motion().entering(MotionPreset::FadeIn.spec().timing(|mut timing| {
        timing.duration_ms = duration(settings().motion.overlay_ms);
        timing
    }));
    let surface = view().test_id("command-panel").w(step(132))
        .max_w(|| px((window().width.0 - step(8).0).max(0.0))).p(step(4)).gap(step(2))
        .rounded(px(shape.overlay)).border_1().border_color(|| color("input")).bg(|| color("popover"))
        .shadow_lg()
        .bind_motion(&enter)
        .child(view().flex_row().items_center().justify_between().gap(step(2))
            .child(title("命令菜单"))
            .child(control(state, "command-close", "关闭  Esc", move || state.palette.set(false))))
        .child(command().items(items).placeholder("搜索议题、项目、工作区和动作")
            .accessibility_label("搜索命令和议题").auto_focus(true)
            .h(step(80))
            .search_class("text-foreground border border-input rounded-(--radius) px-2 min-h-9 focus:outline-2 focus:outline-ring focus:outline-offset-2 placeholder:text-muted-foreground")
            .item_class("rounded-(--radius) focus:bg-accent focus:outline-2 focus:outline-ring")
            .group_class("text-muted-foreground px-2")
            .separator_class("bg-border")
            .on_select(move |id: &SharedString| {
                match id.as_ref() {
                    "issue" => state.open(0),
                    "density" => state.toggle_density(),
                    "project" => { state.empty.set(false); state.detail.set(false); }
                    _ => return,
                }
                state.palette.set(false);
            })
            .empty(meta("没有匹配的命令", "muted-foreground"))
            .render_item(move |entry: &CommandEntry| view().flex_row().items_center().gap(step(2)).px(step(2))
                .min_h(move || px(density(state).row))
                .child(styled_text(entry.label.clone(), settings().typography.scale.body,
                    if entry.disabled { "muted-foreground" } else { "foreground" }, true, false).flex_1().truncate())
                .child(if entry.disabled { meta("暂无远端配置", "muted-foreground") } else { mono(entry.shortcut_label.clone().unwrap_or_default(), "muted-foreground") })))
        .child(rule()).child(mono("↑ ↓ 选择    Enter 执行    Esc 返回", "muted-foreground"));
    dialog()
        // The pinned Win32 backend sets the mounted portal's opacity to
        // zero before the animation config arrives. Its native fade resets it.
        .animation(if cfg!(windows) {
            ModalAnimation::Fade
        } else {
            ModalAnimation::None
        })
        .on_cancel(move || state.palette.set(false))
        .flex_1()
        .child(
            view()
                .absolute()
                .inset_0()
                .bg(|| color("background").opacity(0.72))
                .on_press(move || state.palette.set(false)),
        )
        .child(view().m(auto()).p(step(4)).child(surface))
}
