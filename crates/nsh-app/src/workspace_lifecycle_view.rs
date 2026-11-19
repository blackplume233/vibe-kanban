use std::rc::Rc;

use nsh_base::{SelectItem, SelectValue, button, select};
use nsh_core::element::input;

use super::*;

const SELECT: &str = "w-full border border-input rounded-md bg-background px-2 min-h-9 focus-visible:ring-2 focus-visible:ring-ring";
const FIELD: &str = "w-full border border-input rounded-md bg-background px-3 py-2 min-h-9 text-sm focus-visible:ring-2 focus-visible:ring-ring";

pub(super) fn panel(api: Rc<Api>, state: State) -> impl IntoElement {
    view()
        .app_class(Some(
            "flex-1 min-w-0 min-h-0 bg-card text-card-foreground p-4 gap-3 overflow-y-auto".into(),
        ))
        .child(
            view()
                .app_class(Some("flex-row flex-wrap items-center gap-2".into()))
                .child(text("本地工作区").app_class(Some("text-lg font-semibold flex-1".into())))
                .child(
                    action("刷新工作区", false)
                        .disabled(state.busy)
                        .on_press(move || {
                            state.revision.update(|value| *value += 1);
                            state.refresh_git();
                        }),
                )
                .child(
                    action("新建工作区", true)
                        .disabled(move || {
                            state.busy.get()
                                || state.create.with(Option::is_some)
                                || state.git_form.with(Option::is_some)
                        })
                        .on_press({
                            let api = api.clone();
                            move || open_create(api.clone(), state)
                        }),
                ),
        )
        .child(when(
            move || state.notice.with(Option::is_some),
            move || {
                text(move || {
                    state.notice.with(|notice| {
                        notice
                            .as_ref()
                            .map(|(_, text)| text.clone())
                            .unwrap_or_default()
                    })
                })
                .app_class(move || {
                    Some(
                        if state
                            .notice
                            .with(|notice| notice.as_ref().is_some_and(|(error, _)| *error))
                        {
                            "text-sm text-destructive"
                        } else {
                            "text-sm text-status-ok"
                        }
                        .into(),
                    )
                })
                .selectable(true)
            },
        ))
        .child(when(move || state.create.with(Option::is_some), {
            let api = api.clone();
            move || create_form(api.clone(), state)
        }))
        .child(when(
            move || state.create.with(Option::is_none),
            move || {
                view()
                    .app_class(Some("gap-3 min-w-0".into()))
                    .child(workspace_picker(state))
                    .child(when(move || state.selected_row().is_some(), {
                        let api = api.clone();
                        move || detail(api.clone(), state)
                    }))
            },
        ))
}

fn workspace_picker(state: State) -> impl IntoElement {
    let visible = computed(move || {
        let archived = state.show_archived.get();
        let issue = state.issue.and_then(|issue| issue.get());
        state.rows.with(|rows| {
            rows.iter()
                .filter(|row| {
                    row.archived == archived
                        && issue
                            .as_ref()
                            .is_none_or(|id| row.local_issue_id.as_ref() == Some(id))
                })
                .map(|row| SelectItem::new(row.id.clone(), row.label().to_owned()))
                .collect::<Vec<_>>()
        })
    });
    view()
        .app_class(Some("gap-2".into()))
        .child(
            view()
                .app_class(Some("flex-row flex-wrap gap-2 items-center".into()))
                .child(
                    action("活跃", false)
                        .selected(move || !state.show_archived.get())
                        .disabled(move || state.busy.get() || state.git_form.with(Option::is_some))
                        .on_press(move || state.show_archived.set(false)),
                )
                .child(
                    action("已归档", false)
                        .selected(state.show_archived)
                        .disabled(move || state.busy.get() || state.git_form.with(Option::is_some))
                        .on_press(move || state.show_archived.set(true)),
                )
                .child(
                    text(move || {
                        if state.loading.get() {
                            "正在加载…"
                        } else {
                            ""
                        }
                    })
                    .app_class(Some("text-xs text-muted-foreground".into())),
                ),
        )
        .child(
            picker("工作区", move || {
                let selected = state.selected.get();
                visible.with(|rows| {
                    rows.iter()
                        .find(|row| selected.as_ref().is_some_and(|id| row.value.as_ref() == id))
                        .map(|row| row.title.clone())
                })
            })
            .items(visible)
            .value(move || {
                let id = state.selected.get();
                let value = id.filter(|id| {
                    visible.with(|rows| rows.iter().any(|row| row.value.as_ref() == id))
                });
                SelectValue::Controlled(value.map(SharedString::from))
            })
            .disabled(move || {
                state.busy.get()
                    || visible.with(Vec::is_empty)
                    || state.git_form.with(Option::is_some)
            })
            .on_change(move |id: &Option<SharedString>| {
                state.returned_url.set(None);
                state.notice.set(None);
                state.selected.set(id.as_ref().map(ToString::to_string));
            }),
        )
        .child(when(
            move || !state.loading.get() && visible.with(Vec::is_empty),
            move || {
                text(move || {
                    if state.show_archived.get() {
                        "没有已归档的工作区。"
                    } else {
                        "没有活跃的工作区。"
                    }
                })
                .app_class(Some("text-sm text-muted-foreground".into()))
            },
        ))
}

fn create_form(api: Rc<Api>, state: State) -> impl IntoElement {
    view()
        .app_class(Some("border border-border rounded-card p-3 gap-3".into()))
        .child(text("创建工作区并启动 Agent").app_class(Some("font-semibold".into())))
        .child(when(
            move || {
                state
                    .create
                    .with(|draft| draft.as_ref().is_some_and(|draft| draft.issue.is_some()))
            },
            || {
                text("将关联打开此表单时的本地议题。")
                    .app_class(Some("text-xs text-muted-foreground".into()))
            },
        ))
        .child(labeled(
            "工作区名称（可选）",
            input()
                .app_class(Some(FIELD.into()))
                .label("工作区名称")
                .value(move || {
                    state.create.with(|draft| {
                        draft
                            .as_ref()
                            .map(|draft| draft.name.clone())
                            .unwrap_or_default()
                    })
                })
                .readonly(state.busy)
                .on_change(move |value: &str| state.edit(|draft| draft.name = value.into())),
        ))
        .child(labeled(
            "首轮提示词",
            input()
                .app_class(Some(FIELD.into()))
                .min_h(px(76.0))
                .multiline(true)
                .label("首轮提示词")
                .value(move || {
                    state.create.with(|draft| {
                        draft
                            .as_ref()
                            .map(|draft| draft.prompt.clone())
                            .unwrap_or_default()
                    })
                })
                .readonly(state.busy)
                .on_change(move |value: &str| state.edit(|draft| draft.prompt = value.into())),
        ))
        .child(labeled(
            "Agent 与预设",
            picker("Agent 与预设", move || {
                state.create.with(|draft| {
                    let draft = draft.as_ref()?;
                    let profile = draft.profiles.get(draft.profile?)?;
                    Some(
                        format!(
                            "{} · {}",
                            profile.executor,
                            profile.variant.as_deref().unwrap_or("DEFAULT")
                        )
                        .into(),
                    )
                })
            })
            .items(move || {
                state.create.with(|draft| {
                    draft
                        .as_ref()
                        .map(|draft| {
                            draft
                                .profiles
                                .iter()
                                .enumerate()
                                .map(|(index, profile)| {
                                    SelectItem::new(
                                        index.to_string(),
                                        format!(
                                            "{} · {}",
                                            profile.executor,
                                            profile.variant.as_deref().unwrap_or("DEFAULT")
                                        ),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            })
            .value(move || {
                SelectValue::Controlled(
                    state
                        .create
                        .with(|draft| draft.as_ref()?.profile)
                        .map(|index| index.to_string().into()),
                )
            })
            .disabled(state.busy)
            .on_change(move |value: &Option<SharedString>| {
                state.edit(|draft| {
                    draft.profile = value.as_ref().and_then(|value| value.parse().ok())
                })
            }),
        ))
        .child(labeled(
            "已登记的本地仓库",
            view()
                .app_class(Some("flex-row items-center gap-2".into()))
                .child(
                    view().app_class(Some("flex-1 min-w-0".into())).child(
                        picker("本地仓库", move || {
                            state.create.with(|draft| {
                                let draft = draft.as_ref()?;
                                let id = draft.add_repo.as_ref()?;
                                draft
                                    .catalog
                                    .iter()
                                    .find(|repo| &repo.id == id)
                                    .map(|repo| repo.display_name.clone().into())
                            })
                        })
                        .items(move || {
                            state.create.with(|draft| {
                                draft
                                    .as_ref()
                                    .map(|draft| {
                                        draft
                                            .catalog
                                            .iter()
                                            .filter(|repo| {
                                                !draft
                                                    .repos
                                                    .iter()
                                                    .any(|entry| entry.choice.repo_id == repo.id)
                                            })
                                            .map(|repo| {
                                                SelectItem::new(
                                                    repo.id.clone(),
                                                    format!(
                                                        "{} · {}",
                                                        repo.display_name, repo.path
                                                    ),
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default()
                            })
                        })
                        .value(move || {
                            SelectValue::Controlled(
                                state
                                    .create
                                    .with(|draft| draft.as_ref()?.add_repo.clone())
                                    .map(Into::into),
                            )
                        })
                        .disabled(state.busy)
                        .on_change(move |value: &Option<SharedString>| {
                            state.edit(|draft| {
                                draft.add_repo = value.as_ref().map(ToString::to_string)
                            })
                        }),
                    ),
                )
                .child(
                    action("添加仓库", false)
                        .disabled(move || {
                            state.busy.get()
                                || state.create.with(|draft| {
                                    draft.as_ref().is_none_or(|draft| draft.add_repo.is_none())
                                })
                        })
                        .on_press({
                            let api = api.clone();
                            move || add_repo(api.clone(), state, false)
                        }),
                ),
        ))
        .child(labeled(
            "登记本地 Git 仓库",
            view()
                .app_class(Some("flex-row items-center gap-2".into()))
                .child(
                    input()
                        .app_class(Some(FIELD.into()))
                        .flex_1()
                        .min_w(px(0.0))
                        .placeholder("本机仓库目录的绝对路径")
                        .label("本机仓库路径")
                        .value(move || {
                            state.create.with(|draft| {
                                draft
                                    .as_ref()
                                    .map(|draft| draft.path.clone())
                                    .unwrap_or_default()
                            })
                        })
                        .readonly(state.busy)
                        .on_change(move |value: &str| {
                            state.edit(|draft| draft.path = value.into())
                        }),
                )
                .child(
                    action("登记并添加", false)
                        .disabled(move || {
                            state.busy.get()
                                || state.create.with(|draft| {
                                    draft
                                        .as_ref()
                                        .is_none_or(|draft| draft.path.trim().is_empty())
                                })
                        })
                        .on_press({
                            let api = api.clone();
                            move || add_repo(api.clone(), state, true)
                        }),
                ),
        ))
        .child(each(
            move || {
                state.create.with(|draft| {
                    draft
                        .as_ref()
                        .map(|draft| {
                            draft
                                .repos
                                .iter()
                                .map(|repo| repo.choice.repo_id.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            },
            |id| id.clone(),
            move |id, _| repo_choice(state, id.get()),
        ))
        .child(
            view()
                .app_class(Some("flex-row flex-wrap gap-2".into()))
                .child(
                    action("取消创建", false)
                        .disabled(state.busy)
                        .on_press(move || {
                            state.create.set(None);
                            state.notice.set(None);
                        }),
                )
                .child(
                    action(
                        move || {
                            if state.busy.get() {
                                "处理中…"
                            } else {
                                "创建并启动 Agent"
                            }
                        },
                        true,
                    )
                    .disabled(move || {
                        state.busy.get()
                            || state.create.with(|draft| {
                                draft.as_ref().is_none_or(|draft| {
                                    draft.prompt.trim().is_empty()
                                        || draft.profile.is_none()
                                        || draft.repos.is_empty()
                                        || draft
                                            .repos
                                            .iter()
                                            .any(|repo| repo.choice.target_branch.is_empty())
                                })
                            })
                    })
                    .on_press(move || start_workspace(api.clone(), state)),
                ),
        )
}

fn repo_choice(state: State, id: String) -> impl IntoElement {
    let branch_id = id.clone();
    let item_id = id.clone();
    let target_id = id.clone();
    let trigger_id = id.clone();
    let edit_id = id.clone();
    view()
        .app_class(Some("border border-border p-2 rounded-md gap-2".into()))
        .child(
            view()
                .app_class(Some("flex-row items-center gap-2".into()))
                .child(
                    text(move || {
                        state.create.with(|draft| {
                            draft
                                .as_ref()
                                .and_then(|draft| {
                                    draft.catalog.iter().find(|repo| repo.id == branch_id)
                                })
                                .map(|repo| repo.display_name.clone())
                                .unwrap_or_default()
                        })
                    })
                    .app_class(Some("flex-1 font-semibold text-sm".into())),
                )
                .child(
                    action("移除仓库", false)
                        .disabled(state.busy)
                        .on_press(move || {
                            state.edit(|draft| draft.repos.retain(|repo| repo.choice.repo_id != id))
                        }),
                ),
        )
        .child(labeled(
            "目标分支",
            picker("目标分支", move || {
                state.create.with(|draft| {
                    let repo = draft
                        .as_ref()?
                        .repos
                        .iter()
                        .find(|repo| repo.choice.repo_id == trigger_id)?;
                    (!repo.choice.target_branch.is_empty())
                        .then(|| repo.choice.target_branch.clone().into())
                })
            })
            .items(move || {
                state.create.with(|draft| {
                    draft
                        .as_ref()
                        .and_then(|draft| {
                            draft
                                .repos
                                .iter()
                                .find(|repo| repo.choice.repo_id == item_id)
                        })
                        .map(|repo| branch_items(&repo.branches, ""))
                        .unwrap_or_default()
                })
            })
            .value(move || {
                SelectValue::Controlled(state.create.with(|draft| {
                    draft
                        .as_ref()
                        .and_then(|draft| {
                            draft
                                .repos
                                .iter()
                                .find(|repo| repo.choice.repo_id == target_id)
                        })
                        .map(|repo| &repo.choice.target_branch)
                        .filter(|branch| !branch.is_empty())
                        .map(|branch| SharedString::from(branch.clone()))
                }))
            })
            .disabled(state.busy)
            .on_change(move |value: &Option<SharedString>| {
                state.edit(|draft| {
                    if let Some(repo) = draft
                        .repos
                        .iter_mut()
                        .find(|repo| repo.choice.repo_id == edit_id)
                    {
                        repo.choice.target_branch =
                            value.as_ref().map(ToString::to_string).unwrap_or_default();
                    }
                })
            }),
        ))
}

fn detail(api: Rc<Api>, state: State) -> impl IntoElement {
    let archived = move || state.selected_row().is_none_or(|row| row.archived);
    let blocked = move || state.busy.get() || state.git_form.with(Option::is_some);
    view()
        .app_class(Some("border-t border-border pt-3 gap-3".into()))
        .child(
            text(move || {
                state
                    .selected_row()
                    .map(|row| row.label().to_owned())
                    .unwrap_or_default()
            })
            .app_class(Some("font-semibold".into())),
        )
        .child(
            text(move || {
                state
                    .selected_row()
                    .map(|row| row.branch)
                    .unwrap_or_default()
            })
            .app_class(Some("text-xs font-mono text-muted-foreground".into()))
            .selectable(true),
        )
        .child(
            view()
                .app_class(Some("flex-row flex-wrap items-center gap-2".into()))
                .child(
                    action(
                        move || {
                            if archived() {
                                "恢复工作区"
                            } else {
                                "归档工作区"
                            }
                        },
                        false,
                    )
                    .disabled(blocked)
                    .on_press({
                        let api = api.clone();
                        move || {
                            workspace_action(
                                api.clone(),
                                state,
                                if archived() {
                                    WorkspaceAction::Restore
                                } else {
                                    WorkspaceAction::Archive
                                },
                            )
                        }
                    }),
                )
                .child(
                    action("在编辑器打开", false)
                        .disabled(move || blocked() || archived())
                        .on_press({
                            let api = api.clone();
                            move || {
                                workspace_action(api.clone(), state, WorkspaceAction::OpenEditor)
                            }
                        }),
                )
                .child(
                    action(
                        move || {
                            if state.processes.with(|status| {
                                status
                                    .as_ref()
                                    .is_some_and(|status| !status.running_dev.is_empty())
                            }) {
                                "停止开发服务器"
                            } else {
                                "启动开发服务器"
                            }
                        },
                        false,
                    )
                    .disabled(move || {
                        blocked()
                            || archived()
                            || state.processes.with(|status| {
                                status.as_ref().is_none_or(|status| {
                                    status.running_dev.is_empty() && !has_dev_script(state)
                                })
                            })
                    })
                    .on_press({
                        let api = api.clone();
                        move || {
                            workspace_action(
                                api.clone(),
                                state,
                                if state.processes.with(|status| {
                                    status
                                        .as_ref()
                                        .is_some_and(|status| !status.running_dev.is_empty())
                                }) {
                                    WorkspaceAction::StopDev
                                } else {
                                    WorkspaceAction::StartDev
                                },
                            )
                        }
                    }),
                ),
        )
        .child(when(move || !archived(), {
            let api = api.clone();
            move || {
                view()
                    .app_class(Some("gap-2".into()))
                    .child(
                        text(move || {
                            state.process_notice.get().unwrap_or_else(|| {
                                state.processes.with(|status| match status {
                                    None => "正在连接执行进程…".into(),
                                    Some(status) if !status.running_dev.is_empty() => {
                                        format!("运行中的开发服务器 · {}", status.running_dev.len())
                                    }
                                    Some(_) if !has_dev_script(state) => {
                                        "仓库未配置开发服务器脚本。".into()
                                    }
                                    Some(_) => "开发服务器已停止。".into(),
                                })
                            })
                        })
                        .app_class(Some("text-xs text-muted-foreground".into())),
                    )
                    .child(git_panel(api.clone(), state))
            }
        }))
        .child(when(
            move || state.returned_url.with(Option::is_some),
            move || {
                view()
                    .app_class(Some("gap-1".into()))
                    .child(
                        text(move || state.returned_url.get().unwrap_or_default())
                            .app_class(Some("text-xs font-mono break-all".into()))
                            .selectable(true),
                    )
                    .child(action("打开链接", false).on_press(move || {
                        let Some(url) = state.returned_url.get() else {
                            return;
                        };
                        let opened = open_url(url);
                        spawn(async move {
                            if !opened.await {
                                state.notice.set(Some((
                                    true,
                                    "系统未能打开此链接。可以复制链接手动打开。".into(),
                                )));
                            }
                        });
                    }))
            },
        ))
}

fn has_dev_script(state: State) -> bool {
    state.repos.with(|repos| {
        repos.iter().any(|repo| {
            repo.dev_server_script
                .as_ref()
                .is_some_and(|script| !script.is_empty())
        })
    })
}

fn git_panel(api: Rc<Api>, state: State) -> impl IntoElement {
    view()
        .app_class(Some("border-t border-border pt-3 gap-2".into()))
        .child(
            view()
                .app_class(Some("flex-row items-center gap-2".into()))
                .child(text("Git · 差异与提交").app_class(Some("flex-1 font-semibold".into())))
                .child(
                    action("刷新 Git 状态", false)
                        .disabled(state.busy)
                        .on_press(move || state.refresh_git()),
                ),
        )
        .child(when(
            move || state.status_notice.with(Option::is_some),
            move || {
                text(move || state.status_notice.get().unwrap_or_default())
                    .app_class(Some("text-sm text-muted-foreground".into()))
                    .selectable(true)
            },
        ))
        .child(each(
            move || {
                state.git.with(|rows| {
                    rows.iter()
                        .map(|row| row.repo_id.clone())
                        .collect::<Vec<_>>()
                })
            },
            |id| id.clone(),
            {
                let api = api.clone();
                move |id, _| git_repo(api.clone(), state, id.get())
            },
        ))
        .child(when(
            move || state.git_form.with(Option::is_some),
            move || git_form(api.clone(), state),
        ))
}

fn git_repo(api: Rc<Api>, state: State, id: String) -> impl IntoElement {
    let current = computed({
        let id = id.clone();
        move || state.selected_status(&id).map(Rc::new)
    });
    let blocked = move || {
        state.busy.get()
            || state.git_form.with(Option::is_some)
            || state
                .processes
                .with(|status| status.as_ref().is_none_or(|status| status.agent_running))
    };
    let in_conflict = move || current.with(|git| git.as_ref().is_some_and(|git| git.conflicted()));
    let operation = {
        let api = api.clone();
        let id = id.clone();
        move |label, kind| {
            let api = api.clone();
            let id = id.clone();
            action(label, false)
                .disabled(move || blocked() || in_conflict())
                .on_press(move || open_git_form(api.clone(), state, id.clone(), kind))
        }
    };
    let abort = {
        let api = api.clone();
        let id = id.clone();
        move || git_action(api.clone(), state, id.clone(), GitAction::Abort)
    };
    let resume = {
        let api = api.clone();
        let id = id.clone();
        move || git_action(api.clone(), state, id.clone(), GitAction::Continue)
    };
    view()
        .app_class(Some("gap-2 border border-border rounded-card p-3".into()))
        .child(
            text(move || {
                current.with(|git| {
                    git.as_ref()
                        .map(|git| git.repo_name.clone())
                        .unwrap_or_default()
                })
            })
            .app_class(Some("text-sm font-semibold".into())),
        )
        .child(
            text(move || {
                current.with(|git| {
                    git.as_ref()
                        .map(|git| {
                            format!(
                                "→ {} · 领先 {} · 落后 {}",
                                git.target_branch_name,
                                count(git.commits_ahead),
                                count(git.commits_behind)
                            )
                        })
                        .unwrap_or_default()
                })
            })
            .app_class(Some("text-xs font-mono".into())),
        )
        .child(
            text(move || {
                current.with(|git| {
                    git.as_ref()
                        .map(|git| {
                            format!(
                                "HEAD {} · 未提交 {} · 未跟踪 {}",
                                git.head_oid
                                    .as_deref()
                                    .and_then(|oid| oid.get(..8))
                                    .unwrap_or("—"),
                                count(git.uncommitted_count),
                                count(git.untracked_count)
                            )
                        })
                        .unwrap_or_default()
                })
            })
            .app_class(Some("text-xs font-mono text-muted-foreground".into())),
        )
        .child(when(
            move || {
                state
                    .processes
                    .with(|status| status.as_ref().is_some_and(|status| status.agent_running))
            },
            || {
                text("Agent 正在运行。执行结束后可以修改 Git 状态。")
                    .app_class(Some("text-xs text-muted-foreground".into()))
            },
        ))
        .child(when(in_conflict, move || {
            view()
                .app_class(Some("gap-2".into()))
                .child(
                    text(move || {
                        current.with(|git| {
                            git.as_ref()
                                .map(|git| {
                                    let operation = git.conflict_op.as_deref().unwrap_or(
                                        if git.is_rebase_in_progress {
                                            "Rebase"
                                        } else {
                                            "Git"
                                        },
                                    );
                                    format!(
                                        "{operation} · {} 个冲突文件",
                                        git.conflicted_files.len()
                                    )
                                })
                                .unwrap_or_default()
                        })
                    })
                    .app_class(Some("text-sm text-status-warn".into())),
                )
                .child(
                    text(move || {
                        current.with(|git| {
                            git.as_ref()
                                .map(|git| git.conflicted_files.join("\n"))
                                .unwrap_or_default()
                        })
                    })
                    .app_class(Some("text-xs font-mono".into()))
                    .selectable(true),
                )
                .child(
                    view()
                        .app_class(Some("flex-row flex-wrap gap-2".into()))
                        .child(
                            action("中止冲突操作", false)
                                .disabled(blocked)
                                .on_press(abort.clone()),
                        )
                        .child(
                            action("继续 Rebase", true)
                                .disabled(move || {
                                    blocked()
                                        || current.with(|git| {
                                            git.as_ref()
                                                .is_none_or(|git| !git.is_rebase_in_progress)
                                        })
                                })
                                .on_press(resume.clone()),
                        ),
                )
        }))
        .child(when(
            move || !in_conflict(),
            move || {
                let api = api.clone();
                let id = id.clone();
                let merge = action("合并", false)
                    .disabled(move || {
                        blocked()
                            || current.with(|status| {
                                status
                                    .as_ref()
                                    .is_none_or(|git| git.merge_block().is_some())
                            })
                    })
                    .on_press(move || git_action(api.clone(), state, id.clone(), GitAction::Merge));
                view()
                    .app_class(Some("gap-2".into()))
                    .child(
                        view()
                            .app_class(Some("flex-row flex-wrap gap-2".into()))
                            .child(merge)
                            .child(operation("Rebase", GitFormKind::Rebase))
                            .child(operation("更改目标分支", GitFormKind::Target))
                            .child(operation("创建 PR", GitFormKind::PullRequest)),
                    )
                    .child(
                        text(move || {
                            current.with(|git| {
                                git.as_ref()
                                    .and_then(|git| git.merge_block())
                                    .unwrap_or_default()
                            })
                        })
                        .app_class(Some("text-xs text-muted-foreground".into())),
                    )
            },
        ))
        .child(each(
            move || {
                current.with(|git| {
                    git.as_ref()
                        .map(|git| {
                            git.merges
                                .iter()
                                .map(|merge| match merge {
                                    Merge::Direct { merge_commit } => {
                                        format!("已直接合并 · {merge_commit}")
                                    }
                                    Merge::Pr { pr_info } => format!(
                                        "PR #{} · {} · {}",
                                        pr_info.number, pr_info.status, pr_info.url
                                    ),
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            },
            |line| line.clone(),
            |line, _| {
                text(move || line.get())
                    .app_class(Some("text-xs font-mono text-muted-foreground".into()))
                    .selectable(true)
            },
        ))
}

fn git_form(api: Rc<Api>, state: State) -> impl IntoElement {
    let is_pr = move || {
        state.git_form.with(|form| {
            form.as_ref()
                .is_some_and(|form| form.kind == GitFormKind::PullRequest)
        })
    };
    view()
        .app_class(Some(
            "gap-3 border border-input rounded-card bg-popover p-3".into(),
        ))
        .child(
            text(move || {
                state
                    .git_form
                    .with(|form| match form.as_ref().map(|form| form.kind.clone()) {
                        Some(GitFormKind::Rebase) => "确认 Rebase",
                        Some(GitFormKind::Target) => "更改目标分支",
                        Some(GitFormKind::PullRequest) => "创建 Pull Request",
                        None => "",
                    })
            })
            .app_class(Some("font-semibold".into())),
        )
        .child(
            text(move || {
                state.git_form.with(|form| {
                    form.as_ref()
                        .map(|form| format!("{} → {}", form.source, form.target))
                        .unwrap_or_default()
                })
            })
            .app_class(Some("text-xs font-mono".into())),
        )
        .child(labeled(
            "目标分支",
            picker("目标分支", move || {
                state
                    .git_form
                    .with(|form| form.as_ref().map(|form| form.target.clone().into()))
            })
            .items(move || {
                state.git_form.with(|form| {
                    form.as_ref()
                        .map(|form| branch_items(&form.branches, &form.source))
                        .unwrap_or_default()
                })
            })
            .value(move || {
                SelectValue::Controlled(
                    state
                        .git_form
                        .with(|form| form.as_ref().map(|form| form.target.clone().into())),
                )
            })
            .disabled(state.busy)
            .on_change(move |value: &Option<SharedString>| {
                if let Some(value) = value {
                    state.edit_git(|form| form.target = value.to_string());
                }
            }),
        ))
        .child(when(is_pr, move || {
            view()
                .app_class(Some("gap-2".into()))
                .child(
                    text("确认后将推送工作区分支并在远端创建 PR。")
                        .app_class(Some("text-sm text-status-warn".into())),
                )
                .child(labeled(
                    "PR 标题",
                    input()
                        .app_class(Some(FIELD.into()))
                        .label("PR 标题")
                        .readonly(state.busy)
                        .value(move || {
                            state.git_form.with(|form| {
                                form.as_ref()
                                    .map(|form| form.title.clone())
                                    .unwrap_or_default()
                            })
                        })
                        .on_change(move |value: &str| {
                            state.edit_git(|form| form.title = value.into())
                        }),
                ))
                .child(labeled(
                    "PR 描述",
                    input()
                        .app_class(Some(FIELD.into()))
                        .min_h(px(72.0))
                        .multiline(true)
                        .label("PR 描述")
                        .readonly(state.busy)
                        .value(move || {
                            state.git_form.with(|form| {
                                form.as_ref()
                                    .map(|form| form.body.clone())
                                    .unwrap_or_default()
                            })
                        })
                        .on_change(move |value: &str| {
                            state.edit_git(|form| form.body = value.into())
                        }),
                ))
                .child(
                    action("草稿 PR", false)
                        .selected(move || {
                            state
                                .git_form
                                .with(|form| form.as_ref().is_some_and(|form| form.draft))
                        })
                        .disabled(state.busy)
                        .on_press(move || state.edit_git(|form| form.draft = !form.draft)),
                )
        }))
        .child(
            view()
                .app_class(Some("flex-row gap-2 flex-wrap".into()))
                .child(
                    action("取消 Git 操作", false)
                        .disabled(state.busy)
                        .on_press(move || state.git_form.set(None)),
                )
                .child(
                    action(
                        move || {
                            state.git_form.with(|form| {
                                match form.as_ref().map(|form| form.kind.clone()) {
                                    Some(GitFormKind::Rebase) => "执行 Rebase",
                                    Some(GitFormKind::Target) => "保存目标分支",
                                    Some(GitFormKind::PullRequest) => "推送并创建 PR",
                                    None => "确认",
                                }
                            })
                        },
                        true,
                    )
                    .disabled(move || {
                        state.busy.get()
                            || state.processes.with(|status| {
                                status.as_ref().is_none_or(|status| status.agent_running)
                            })
                            || state.git_form.with(|form| {
                                form.as_ref().is_none_or(|form| {
                                    form.target.is_empty()
                                        || (form.kind == GitFormKind::PullRequest
                                            && form.title.trim().is_empty())
                                })
                            })
                    })
                    .on_press(move || submit_git_form(api.clone(), state)),
                ),
        )
}

fn count(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".into())
}

fn picker(
    label: &'static str,
    value: impl Fn() -> Option<SharedString> + 'static,
) -> nsh_base::SelectElement {
    select()
        .accessibility_label(label)
        .searchable(true)
        .offset(px(4.0))
        .popup_class("bg-popover border border-input rounded-md shadow-md")
        .list_class("p-1 max-h-64")
        .item_class(
            "min-h-9 px-2 justify-center rounded-md text-sm focus:bg-muted selected:bg-accent",
        )
        .trigger(
            view()
                .app_class(Some(SELECT.into()))
                .flex_row()
                .items_center()
                .child(
                    text(move || value().unwrap_or_else(|| format!("选择{label}").into()))
                        .flex_1()
                        .min_w(px(0.0))
                        .number_of_lines(1),
                )
                .child(text("▾").app_class(Some("text-muted-foreground".into()))),
        )
}

fn branch_items(branches: &[Branch], exclude: &str) -> Vec<SelectItem> {
    branches
        .iter()
        .filter(|branch| branch.name != exclude)
        .map(|branch| {
            SelectItem::new(
                branch.name.clone(),
                if branch.is_remote {
                    format!("{} · 远端", branch.name)
                } else {
                    branch.name.clone()
                },
            )
        })
        .collect()
}

fn labeled(label: &'static str, content: impl IntoElement) -> impl IntoElement {
    view()
        .app_class(Some("min-w-0 gap-1".into()))
        .child(text(label).app_class(Some("text-xs font-semibold text-muted-foreground".into())))
        .child(content)
}

fn action(label: impl IntoReactive<SharedString>, primary: bool) -> nsh_base::ButtonElement {
    let surface = if primary {
        "bg-primary text-primary-foreground"
    } else {
        "bg-card text-card-foreground border border-input selected:bg-accent"
    };
    button().app_class(Some(format!("{surface} rounded-md min-h-9 px-3 justify-center items-center focus-visible:ring-2 focus-visible:ring-ring hover:opacity-80 disabled:opacity-50").into()))
        .child(text(label).app_class(Some("text-sm font-semibold".into())))
}
