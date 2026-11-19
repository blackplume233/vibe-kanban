mod api;
mod prices;
mod queue;

use nsh_core::{
    element::{Input, Prop, Text, View, input},
    prelude::*,
};

use crate::api::Api;

#[derive(Clone, PartialEq, Props)]
pub struct AgentManagementProps {
    /// The local Vibe Kanban session UUID. `None` shows usage for all sessions.
    pub session_id: Prop<Option<SharedString>>,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Tab {
    #[default]
    Usage,
    Queue,
    Prices,
}

#[component]
pub fn AgentManagement(props: AgentManagementProps) -> impl IntoElement {
    let api = Api::local();
    let tab = signal(Tab::default());
    let writing = signal(false);
    let usage = Read::new();
    let queue = queue::QueuePane::new();
    let prices = prices::PricePane::new();
    let revision = signal(0u64);
    let usage_session = signal(None::<SharedString>);
    let session = props.session_id;
    let load_api = api.clone();
    effect(move || {
        let id = session.get();
        revision.get();
        if id != usage_session.get_untracked() {
            usage.data.set(None);
            usage_session.set(id.clone());
        }
        let api = load_api.clone();
        // The effect owns the task. A session change cancels the previous GET.
        usage.load(async move { api::usage(&api, id.as_deref()).await });
    });
    queue.refresh(&api);
    prices.refresh(&api);

    let context = move || {
        usage_session.with(|session| match session {
            Some(id) => format!("会话 {id}"),
            None => "全部会话".into(),
        })
    };
    let nav = |label, target| {
        button(
            label,
            writing,
            move || tab.get() == target,
            move || tab.set(target),
        )
    };
    view()
        .flex_1()
        .min_h_0()
        .bg(|| nsh_base::theme().background)
        .child(
            view()
                .px_4()
                .py_3()
                .gap_3()
                .border_b_1()
                .border_color(|| nsh_base::theme().border)
                .child(heading("代理管理", 24.0))
                .child(
                    actions()
                        .child(nav("Token 用量", Tab::Usage))
                        .child(nav("执行队列", Tab::Queue))
                        .child(nav("模型价格", Tab::Prices)),
                ),
        )
        .child(dynamic(move || {
            let content = match tab.get() {
                Tab::Usage => usage_pane(usage, revision, context),
                Tab::Queue => queue.view(&api, usage_session, writing),
                Tab::Prices => prices.view(&api, writing),
            };
            scroll().flex_1().min_h_0().child(content)
        }))
}

fn usage_pane(
    usage: Read<Vec<api::UsageTotal>>,
    revision: Signal<u64>,
    context: impl Fn() -> String + 'static,
) -> View {
    pane()
        .child(
            actions()
                .child(heading("已记录用量", 18.0).flex_1())
                .child(button(
                    "刷新用量",
                    move || usage.busy(),
                    || false,
                    move || {
                        revision.update(|revision| *revision = revision.wrapping_add(1));
                    },
                )),
        )
        .child(code(context))
        .child(note("与服务端记录一致。费用以美元计，未计价样本单独标出。"))
        .child(usage.message())
        .child(dynamic(move || {
            usage.data.with(|data| {
                let Some(rows) = data else { return view() };
                if rows.is_empty() {
                    return view().child(note("此范围内尚无用量记录。"));
                }
                stack(rows.iter().map(|row| {
                    let cost = match (row.cost_usd, row.unpriced_samples) {
                        (Some(cost), 0) => format!("费用 · ${cost} USD"),
                        (Some(cost), n) => format!("已计价 · ${cost} USD    未计价样本 · {n}"),
                        (None, n) => format!("费用 · 未计价    未计价样本 · {n}"),
                    };
                    card()
                        .child(heading(
                            format!(
                                "{} / {}",
                                row.executor,
                                row.model.as_deref().unwrap_or("模型未报告")
                            ),
                            14.0,
                        ))
                        .child(token_metrics([
                            row.input_tokens,
                            row.output_tokens,
                            row.cache_read_tokens,
                            row.cache_write_tokens,
                        ]))
                        .child(code(cost))
                }))
            })
        }))
}

const TOKEN_LABELS: [&str; 4] = ["输入", "输出", "缓存读取", "缓存写入"];

fn token_metrics(values: impl IntoIterator<Item = impl ToString>) -> View {
    TOKEN_LABELS
        .into_iter()
        .zip(values)
        .fold(actions().gap_4(), |row, (label, value)| {
            row.child(
                view()
                    .min_w(px(96.0))
                    .gap_1()
                    .child(note(label))
                    .child(code(value.to_string())),
            )
        })
}

#[derive(Clone, Default, PartialEq)]
enum Phase {
    #[default]
    Loading,
    Ready,
    LoadFailed(String),
    Saving,
    Saved,
    SaveFailed(String),
    ReadbackFailed(String),
}

struct Read<T> {
    data: Signal<Option<T>>,
    phase: Signal<Phase>,
    owner: Owner,
}

impl<T> Clone for Read<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Read<T> {}

impl<T: 'static> Read<T> {
    fn new() -> Self {
        Self {
            data: signal_with(None, SignalEquals::Never),
            phase: signal(Phase::Loading),
            owner: Owner::current(),
        }
    }

    fn busy(self) -> bool {
        self.phase
            .with(|phase| matches!(phase, Phase::Loading | Phase::Saving))
    }

    fn writable(self) -> bool {
        self.phase
            .with(|phase| matches!(phase, Phase::Ready | Phase::Saved | Phase::SaveFailed(_)))
    }

    fn load(self, request: impl Future<Output = Result<T, String>> + 'static) {
        self.phase.set(Phase::Loading);
        spawn(async move {
            let response = request.await;
            batch(move || match response {
                Ok(data) => {
                    self.data.set(Some(data));
                    self.phase.set(Phase::Ready);
                }
                Err(error) => self.phase.set(Phase::LoadFailed(error)),
            });
        });
    }

    fn save<R: 'static>(
        self,
        writing: Signal<bool>,
        write: impl Future<Output = Result<R, String>> + 'static,
        readback: impl Future<Output = Result<T, String>> + 'static,
        accepted: impl FnOnce() + 'static,
    ) {
        batch(|| {
            writing.set(true);
            self.phase.set(Phase::Saving);
        });
        // Saving can close the form that dispatched it. Keep the readback
        // alive until the panel itself goes away.
        self.owner.run(|| {
            spawn(async move {
                let response = match write.await {
                    Ok(_) => Ok(readback.await),
                    Err(error) => Err(error),
                };
                batch(move || {
                    match response {
                        Ok(readback) => {
                            accepted();
                            match readback {
                                Ok(data) => {
                                    self.data.set(Some(data));
                                    self.phase.set(Phase::Saved);
                                }
                                Err(error) => self.phase.set(Phase::ReadbackFailed(error)),
                            }
                        }
                        Err(error) => self.phase.set(Phase::SaveFailed(error)),
                    }
                    writing.set(false);
                });
            });
        });
    }

    fn invalid(self, error: String) {
        self.phase.set(Phase::SaveFailed(error));
    }

    fn message(self) -> impl IntoElement {
        dynamic(move || {
            let line = self.phase.with(|phase| match phase {
                Phase::Loading => Some(("正在读取…".into(), false)),
                Phase::Ready => None,
                Phase::LoadFailed(error) => {
                    Some((format!("读取失败。已有数据可能过期。{error}"), true))
                }
                Phase::Saving => Some(("正在保存并回读…".into(), false)),
                Phase::Saved => Some(("已保存并从服务端回读。".into(), false)),
                Phase::SaveFailed(error) => Some((format!("未保存。{error}"), true)),
                Phase::ReadbackFailed(error) => {
                    Some((format!("已保存，回读失败。请刷新以核对。{error}"), true))
                }
            });
            line.map(|(message, error)| {
                note(message).text_color(move || {
                    if error {
                        nsh_base::theme().destructive
                    } else {
                        nsh_base::theme().muted_foreground
                    }
                })
            })
        })
    }
}

fn pane() -> View {
    view().p_4().gap_3()
}

fn card() -> View {
    view()
        .p_3()
        .gap_2()
        .rounded(px(6.0))
        .border_1()
        .border_color(|| nsh_base::theme().border)
        .bg(|| nsh_base::theme().card)
}

fn stack(children: impl IntoIterator<Item = View>) -> View {
    children
        .into_iter()
        .fold(view().gap_2(), |stack, child| stack.child(child))
}

fn actions() -> View {
    view().flex_row().flex_wrap().items_center().gap_2()
}

fn heading(value: impl IntoReactive<SharedString>, size: f32) -> Text {
    text(value)
        .text_size(px(size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(|| nsh_base::theme().foreground)
}

fn note(value: impl IntoReactive<SharedString>) -> Text {
    text(value)
        .text_size(px(12.0))
        .text_color(|| nsh_base::theme().muted_foreground)
}

fn code(value: impl IntoReactive<SharedString>) -> Text {
    text(value)
        .font_family("IBM Plex Mono, Microsoft YaHei UI, monospace")
        .text_size(px(14.0))
        .selectable(true)
        .text_color(|| nsh_base::theme().foreground)
}

fn button(
    label: &'static str,
    disabled: impl IntoReactive<bool>,
    selected: impl Fn() -> bool + Copy + 'static,
    action: impl Fn() + 'static,
) -> View {
    view()
        .role(AccessibilityRole::Button)
        .label(label)
        .disabled(disabled)
        .accessibility_state(move || nsh_core::AccessibilityState {
            selected: selected(),
            ..Default::default()
        })
        .min_h(px(36.0))
        .px_3()
        .justify_center()
        .rounded(|| nsh_base::theme().radius)
        .border_1()
        .border_color(|| nsh_base::theme().input)
        .bg(move || {
            if selected() {
                nsh_base::theme().accent
            } else {
                nsh_base::theme().card
            }
        })
        .hover(|s| s.bg(|| nsh_base::theme().secondary))
        .focus(|s| {
            s.outline(px(2.0))
                .outline_offset(px(2.0))
                .outline_color(|| nsh_base::theme().ring)
        })
        .disabled_style(|s| s.opacity(0.55))
        .on_press(action)
        .child(heading(label, 14.0))
}

fn field(
    label: &'static str,
    value: impl IntoReactive<String>,
    read_only: impl IntoReactive<bool>,
    on_change: impl Fn(&str) + 'static,
) -> Input {
    input()
        .label(label)
        .value(value)
        .readonly(read_only)
        .on_change(on_change)
        .auto_correct(false)
        .text_color(|| nsh_base::theme().foreground)
        .text_size(px(14.0))
        .min_h(px(36.0))
        .px_2()
        .rounded(|| nsh_base::theme().radius)
        .border_1()
        .border_color(|| nsh_base::theme().input)
        .focus(|s| {
            s.outline(px(2.0))
                .outline_offset(px(2.0))
                .outline_color(|| nsh_base::theme().ring)
        })
}
