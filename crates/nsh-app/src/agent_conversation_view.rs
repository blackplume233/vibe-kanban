use nsh_base::{SelectItem, SelectValue, button, input, select};
use nsh_core::prelude::*;

use super::{
    Action, Conversation, ProcessStatus, QueueStatus, State, create_session, run_action, submit,
};
use crate::api::Api;

#[path = "agent_conversation_forms.rs"]
mod forms;

pub(super) fn panel(api: Api, workspace_id: String, state: State) -> impl IntoElement {
    view()
        .app_class(Some(
            "flex-1 min-w-0 min-h-0 bg-card text-card-foreground p-4 gap-3".into(),
        ))
        .child(
            view()
                .app_class(Some("flex-row items-center gap-2".into()))
                .child(text("Agent 会话").app_class(Some("text-lg font-semibold flex-1".into())))
                .child(forms::review_button(state))
                .child(
                    action_button("重新连接", false)
                        .on_press(move || state.reconnect.update(|value| *value += 1)),
                )
                .child(
                    action_button("新建会话", false)
                        .disabled(move || state.workspace_loading.get() || state.creating.get())
                        .on_press({
                            let api = api.clone();
                            move || create_session(api.clone(), workspace_id.clone(), state)
                        }),
                ),
        )
        .child(
            view()
                .app_class(Some("flex-row gap-3".into()))
                .child(session_selector(state))
                .child(agent_selector(state)),
        )
        .child(when(
            move || state.workspace_loading.get(),
            || text("正在加载会话…").app_class(Some("text-muted-foreground".into())),
        ))
        .child(when(
            move || state.workspace_error.with(Option::is_some),
            move || error_text(move || state.workspace_error.get().unwrap_or_default()),
        ))
        .child(when(
            move || state.selected.with(Option::is_none) && !state.workspace_loading.get(),
            || {
                text("此工作区还没有会话。新建会话后可选择 Agent。")
                    .app_class(Some("text-muted-foreground".into()))
            },
        ))
        .child(when(
            move || state.selected.with(Option::is_some),
            move || {
                view()
                    .app_class(Some("flex-1 min-h-0 gap-3".into()))
                    .child(status_line(state))
                    .child(approvals(api.clone(), state))
                    .child(forms::review_form(api.clone(), state))
                    .child(history(api.clone(), state))
                    .child(composer(api.clone(), state))
            },
        ))
}

fn session_selector(state: State) -> impl IntoElement {
    view()
        .app_class(Some("flex-1 min-w-0 gap-1".into()))
        .child(text("会话").app_class(Some("text-xs text-muted-foreground".into())))
        .child(
            conversation_select("会话")
                .trigger(select_trigger(
                    move || {
                        state
                            .selected
                            .get()
                            .and_then(|id| state.session(id.as_ref()))
                            .map(|session| SharedString::from(session.label()))
                            .unwrap_or_else(|| "选择会话".into())
                    },
                    move || !state.sessions.with(Vec::is_empty),
                ))
                .items(move || {
                    state.sessions.with(|sessions| {
                        sessions
                            .iter()
                            .map(|session| SelectItem::new(session.id.clone(), session.label()))
                            .collect::<Vec<_>>()
                    })
                })
                .value(move || SelectValue::Controlled(state.selected.get()))
                .disabled(move || state.sessions.with(Vec::is_empty))
                .on_change(move |id: &Option<SharedString>| state.selected.set(id.clone())),
        )
}

fn agent_selector(state: State) -> impl IntoElement {
    let active = computed(move || {
        let id = state.selected.get()?;
        let session = state.session(id.as_ref())?;
        state.with(|data| {
            state.catalog.with(|catalog| {
                data?
                    .executor(&session, catalog)
                    .map(|agent| SharedString::from(agent.to_owned()))
            })
        })
    });
    let locked = computed(move || {
        let Some(id) = state.selected.get() else {
            return false;
        };
        let Some(session) = state.session(id.as_ref()) else {
            return false;
        };
        state.with(|data| data.is_some_and(|data| data.bound_executor(&session).is_some()))
    });
    view()
        .app_class(Some("flex-1 min-w-0 gap-1".into()))
        .child(
            text(move || {
                if locked.get() {
                    "Agent · 已绑定到此会话"
                } else {
                    "Agent"
                }
            })
            .app_class(Some("text-xs text-muted-foreground".into())),
        )
        .child(
            conversation_select("Agent")
                .trigger(select_trigger(
                    move || active.get().unwrap_or_else(|| "选择 Agent".into()),
                    move || !locked.get(),
                ))
                .items(move || {
                    if locked.get() {
                        active
                            .get()
                            .into_iter()
                            .map(|agent| SelectItem::new(agent.clone(), agent))
                            .collect::<Vec<_>>()
                    } else {
                        state.catalog.with(|catalog| {
                            catalog
                                .executors
                                .iter()
                                .map(|agent| {
                                    let agent = SharedString::from(agent.clone());
                                    SelectItem::new(agent.clone(), agent)
                                })
                                .collect::<Vec<_>>()
                        })
                    }
                })
                .value(move || SelectValue::Controlled(active.get()))
                .disabled(move || {
                    locked.get()
                        || state.with(|data| {
                            data.is_none_or(|data| data.editor.busy || !data.transcript.ready)
                        })
                })
                .on_change(move |value: &Option<SharedString>| {
                    let Some(id) = state.selected.get() else {
                        return;
                    };
                    state.update(&id, |data| {
                        data.editor.executor = value.as_ref().map(ToString::to_string)
                    });
                }),
        )
}

fn conversation_select(label: &'static str) -> nsh_base::SelectElement {
    select()
        .accessibility_label(label)
        .offset(px(4.0))
        .popup_class("bg-popover border border-input rounded-md shadow-md")
        .list_class("p-1 max-h-64")
        .item_class(
            "min-h-9 px-2 justify-center rounded-md text-sm focus:bg-muted selected:bg-accent",
        )
}

fn select_trigger(
    label: impl IntoReactive<SharedString>,
    openable: impl IntoReactive<bool>,
) -> impl IntoElement {
    view().app_class(Some("w-full flex-row items-center gap-2 border border-input rounded-md bg-background px-2 min-h-9 focus-visible:ring-2 focus-visible:ring-ring".into()))
        .child(text(label).app_class(Some("flex-1 min-w-0 text-sm".into())).number_of_lines(1))
        .child(when(openable, || text("▾").app_class(Some("text-muted-foreground".into()))))
}

fn status_line(state: State) -> impl IntoElement {
    view()
        .app_class(Some("border-y border-border py-2 gap-1".into()))
        .child(
            text(move || {
                state.with(|data| {
                    let Some(data) = data else {
                        return String::new();
                    };
                    let approvals = state.pending(&data.transcript, true);
                    let approval_count = approvals.len();
                    let scheduled_count = data.queue.scheduled.len()
                        + usize::from(data.queue.draft != QueueStatus::Empty);
                    let status = if !data.transcript.ready {
                        if data.transcript.error.is_some() {
                            "实时连接断开"
                        } else {
                            "正在连接会话"
                        }
                    } else if approvals.iter().any(|approval| approval.is_question) {
                        "等待回答"
                    } else if approval_count > 0 {
                        "等待权限"
                    } else if data.has_active_run() {
                        "正在执行"
                    } else if scheduled_count > 0 {
                        "已排队"
                    } else {
                        "就绪"
                    };
                    format!("{status}  ·  {approval_count} 项待审批  ·  {scheduled_count} 项排队")
                })
            })
            .app_class(Some("text-sm font-semibold".into())),
        )
        .child(when(
            move || state.approval_error.with(Option::is_some),
            move || {
                error_text(move || {
                    format!("审批流：{}", state.approval_error.get().unwrap_or_default())
                })
            },
        ))
        .child(when(
            move || state.with(|data| data.is_some_and(|data| data.transcript.error.is_some())),
            move || {
                error_text(move || {
                    state.with(|data| {
                        data.and_then(|data| data.transcript.error.clone())
                            .unwrap_or_default()
                    })
                })
            },
        ))
        .child(when(
            move || state.with(|data| data.is_some_and(|data| data.queue.error.is_some())),
            move || {
                error_text(move || {
                    state.with(|data| {
                        format!(
                            "队列：{}",
                            data.and_then(|data| data.queue.error.as_deref())
                                .unwrap_or_default()
                        )
                    })
                })
            },
        ))
}

fn approvals(api: Api, state: State) -> impl IntoElement {
    view().app_class(Some("gap-2".into())).child(each(
        move || {
            state.with(|data| {
                data.map(|data| state.pending(&data.transcript, true))
                    .unwrap_or_default()
            })
        },
        |approval| approval.approval_id.clone(),
        move |approval, _| {
            let question = move || approval.with(|approval| approval.is_question);
            let api_question = api.clone();
            let api_approve = api.clone();
            let api_deny = api.clone();
            view()
                .app_class(Some(
                    "bg-muted border border-border rounded-md px-3 py-2 gap-1".into(),
                ))
                .child(
                    text(move || {
                        approval.with(|row| {
                            if row.is_question {
                                format!("Agent 等待回答 · {}", row.tool_name)
                            } else {
                                format!("需要工具权限 · {}", row.tool_name)
                            }
                        })
                    })
                    .app_class(Some("font-semibold text-sm".into())),
                )
                .child(
                    text(move || {
                        approval.with(|row| {
                            format!(
                                "操作 {}  ·  到期 {}",
                                short_id(&row.execution_process_id),
                                row.timeout_at
                            )
                        })
                    })
                    .app_class(Some("text-xs text-muted-foreground".into())),
                )
                .child(when(question, move || {
                    forms::question_form(api_question.clone(), state, approval)
                }))
                .child(when(
                    move || !question(),
                    move || {
                        let approve_api = api_approve.clone();
                        let deny_api = api_deny.clone();
                        let disabled = move || state.approval_disabled();
                        view()
                            .app_class(Some("flex-row gap-2".into()))
                            .child(action_button("批准", true).disabled(disabled).on_press(
                                move || {
                                    if let Some(id) = state.selected.get() {
                                        run_action(
                                            approve_api.clone(),
                                            state,
                                            id,
                                            Action::Respond {
                                                approval: approval.get(),
                                                approve: true,
                                            },
                                        );
                                    }
                                },
                            ))
                            .child(action_button("拒绝", false).disabled(disabled).on_press(
                                move || {
                                    if let Some(id) = state.selected.get() {
                                        run_action(
                                            deny_api.clone(),
                                            state,
                                            id,
                                            Action::Respond {
                                                approval: approval.get(),
                                                approve: false,
                                            },
                                        );
                                    }
                                },
                            ))
                    },
                ))
        },
    ))
}

fn history(api: Api, state: State) -> impl IntoElement {
    scroll()
        .app_class(Some(
            "flex-1 min-h-0 border border-border rounded-md bg-background".into(),
        ))
        .child(
            view()
                .app_class(Some("gap-3 p-3".into()))
                .child(
                    text("会话记录")
                        .app_class(Some("text-xs font-semibold text-muted-foreground".into())),
                )
                .child(when(
                    move || {
                        state.with(|data| {
                            data.is_some_and(|data| {
                                data.transcript.ready && data.transcript.processes.is_empty()
                            })
                        })
                    },
                    || {
                        text("会话还没有消息。发送后续消息以启动 Agent。")
                            .app_class(Some("text-muted-foreground".into()))
                    },
                ))
                .child(each(
                    move || {
                        state.with(|data| {
                            data.map(|data| {
                                data.transcript
                                    .processes
                                    .iter()
                                    .map(|p| p.id.clone())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                        })
                    },
                    |id| id.clone(),
                    move |row_id, _| {
                        let api = api.clone();
                        let process = move || row_id.get();
                        let prompt =
                            computed(move || {
                                state.with(|data| {
                                    let data = data?;
                                    let id = process();
                                    if data.transcript.logs.get(&id).is_some_and(|logs| {
                                        logs.values().any(|entry| entry.is_user)
                                    }) {
                                        return None;
                                    }
                                    data.transcript
                                        .processes
                                        .iter()
                                        .find(|p| p.id == id)?
                                        .prompt()
                                        .map(str::to_owned)
                                })
                            });
                        let running = move || {
                            state.with(|data| {
                                data.is_some_and(|data| {
                                    data.transcript.processes.iter().any(|p| {
                                        p.id == process() && p.status == ProcessStatus::Running
                                    })
                                })
                            })
                        };
                        view()
                            .app_class(Some("gap-2 border-b border-border pb-3".into()))
                            .child(
                                view()
                                    .app_class(Some("flex-row items-center gap-2".into()))
                                    .child(
                                        text(move || {
                                            state.with(|data| {
                                                let id = process();
                                                let Some(data) = data else {
                                                    return String::new();
                                                };
                                                let Some(row) = data
                                                    .transcript
                                                    .processes
                                                    .iter()
                                                    .find(|row| row.id == id)
                                                else {
                                                    return String::new();
                                                };
                                                let status = if let Some(queued) =
                                                    data.scheduled(&id)
                                                {
                                                    format!("已排队 · 优先级 {}", queued.priority)
                                                } else if row.status == ProcessStatus::Running
                                                    && state.approvals.with(|items| {
                                                        items.iter().any(|item| {
                                                            item.execution_process_id == id
                                                        })
                                                    })
                                                {
                                                    "等待权限".to_owned()
                                                } else {
                                                    row.status.label().to_owned()
                                                };
                                                format!(
                                                    "{} {}  ·  {status}",
                                                    row.label(),
                                                    short_id(&id)
                                                )
                                            })
                                        })
                                        .app_class(Some("flex-1 text-xs font-semibold".into())),
                                    )
                                    .child(when(running, move || {
                                        let api = api.clone();
                                        action_button("停止操作", false)
                                            .disabled(move || {
                                                state.with(|data| {
                                                    data.is_none_or(|data| {
                                                        !data.transcript.ready || data.editor.busy
                                                    })
                                                })
                                            })
                                            .on_press(move || {
                                                if let Some(id) = state.selected.get() {
                                                    run_action(
                                                        api.clone(),
                                                        state,
                                                        id,
                                                        Action::Stop(process()),
                                                    );
                                                }
                                            })
                                    })),
                            )
                            .child(when(
                                move || prompt.with(Option::is_some),
                                move || message("你", move || prompt.get().unwrap_or_default()),
                            ))
                            .child(each(
                                move || {
                                    state.with(|data| {
                                        data.and_then(|data| data.transcript.logs.get(&process()))
                                            .map(|logs| logs.keys().copied().collect::<Vec<_>>())
                                            .unwrap_or_default()
                                    })
                                },
                                |index| *index,
                                move |index, _| {
                                    let content = move |label| {
                                        state.with(|data| {
                                            data.and_then(|data| {
                                                data.transcript.logs.get(&process())
                                            })
                                            .and_then(|logs| logs.get(&index.get()))
                                            .map(|entry| {
                                                if label {
                                                    entry.label.clone()
                                                } else {
                                                    entry.content.clone()
                                                }
                                            })
                                            .unwrap_or_default()
                                        })
                                    };
                                    message(move || content(true), move || content(false))
                                },
                            ))
                            .child(when(
                                move || {
                                    state.with(|data| {
                                        data.is_some_and(|data| {
                                            data.transcript.log_errors.contains_key(&process())
                                        })
                                    })
                                },
                                move || {
                                    error_text(move || {
                                        state.with(|data| {
                                            data.and_then(|data| {
                                                data.transcript.log_errors.get(&process())
                                            })
                                            .cloned()
                                            .unwrap_or_default()
                                        })
                                    })
                                },
                            ))
                    },
                )),
        )
}

fn composer(api: Api, state: State) -> impl IntoElement {
    let cancel_api = api.clone();
    view().app_class(Some("gap-2".into()))
        .child(when(move || state.with(|data| data.is_some_and(|data| data.queue.draft != QueueStatus::Empty)), move || {
            let api = cancel_api.clone();
            view().app_class(Some("border border-border rounded-md p-2 gap-1 bg-muted".into()))
                .child(view().app_class(Some("flex-row items-center gap-2".into()))
                    .child(text(move || state.with(|data| {
                        let Some(Conversation { queue: super::SessionQueue { draft: QueueStatus::Queued { message }, .. }, .. }) = data else { return String::new() };
                        format!("排队的后续消息 · {}", super::executor_name(&message.data.executor_config).unwrap_or("Agent"))
                    })).app_class(Some("flex-1 font-semibold text-xs".into())))
                    .child(action_button("取消排队", false)
                        .disabled(move || state.with(|data| data.is_none_or(|data| !data.queue.ready || data.editor.busy)))
                        .on_press(move || if let Some(id) = state.selected.get() {
                            run_action(api.clone(), state, id, Action::CancelDraft);
                        })))
                .child(text(move || state.with(|data| {
                    match data.map(|data| &data.queue.draft) {
                        Some(QueueStatus::Queued { message }) => message.data.message.clone(),
                        _ => String::new(),
                    }
                })).app_class(Some("text-sm".into())).number_of_lines(3))
        }))
        .child(text("后续消息").app_class(Some("text-xs font-semibold text-muted-foreground".into())))
        .child(input().app_class(Some("min-h-16 px-3 py-2 bg-background border border-input rounded-md text-sm focus-visible:ring-2 focus-visible:ring-ring".into()))
            .multiline(true)
            .label("后续消息")
            .placeholder("输入后续消息")
            .value(move || state.with(|data| data.map(|data| data.editor.draft.clone()).unwrap_or_default()))
            .on_change(move |value: &str| if let Some(id) = state.selected.get() {
                state.update(&id, |data| data.editor.draft = value.to_owned());
            }))
        .child(view().app_class(Some("flex-row gap-2 items-center".into()))
            .child(text(move || state.with(|data| data.and_then(|data| data.editor.notice.as_ref()).map(|(_, message)| message.clone()).unwrap_or_default()))
                .app_class(Some("flex-1 text-xs".into()))
                .text_color(move || if state.with(|data| data.and_then(|data| data.editor.notice.as_ref()).is_some_and(|(error, _)| *error)) {
                    token("destructive")
                } else { token("muted-foreground") }))
            .child(action_button(move || state.with(|data| {
                if data.is_some_and(|data| data.editor.busy) { "正在提交…" }
                else if data.is_some_and(Conversation::has_active_run) { "排队发送" }
                else { "发送后续消息" }
            }), true)
                .disabled(move || {
                    let Some(id) = state.selected.get() else { return true };
                    let Some(session) = state.session(id.as_ref()) else { return true };
                    state.with(|data| data.is_none_or(|data| !data.can_send() || state.catalog.with(|catalog| data.executor(&session, catalog).is_none())))
                })
                .on_press(move || submit(api.clone(), state))))
}

fn action_button(label: impl IntoReactive<SharedString>, primary: bool) -> nsh_base::ButtonElement {
    let background = if primary {
        "bg-primary text-primary-foreground"
    } else {
        "bg-card border border-input text-card-foreground"
    };
    let style = format!(
        "{background} rounded-md min-h-9 px-3 justify-center items-center focus-visible:ring-2 focus-visible:ring-ring hover:opacity-80 disabled:opacity-50"
    );
    button()
        .app_class(Some(style.into()))
        .child(text(label).app_class(Some("text-sm font-semibold".into())))
}

fn message(
    label: impl IntoReactive<SharedString>,
    content: impl IntoReactive<SharedString>,
) -> impl IntoElement {
    view()
        .app_class(Some("gap-1 min-w-0 rounded-md bg-card px-3 py-2".into()))
        .child(text(label).app_class(Some("text-xs text-muted-foreground font-semibold".into())))
        .child(
            text(content)
                .app_class(Some("text-sm".into()))
                .selectable(true),
        )
}

fn error_text(text_value: impl IntoReactive<SharedString>) -> impl IntoElement {
    text(text_value).app_class(Some("text-xs text-destructive".into()))
}

fn short_id(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

fn token(role: &str) -> AdaptiveColor {
    nsh_base::theme()
        .color(role)
        .expect("base theme semantic color")
}
