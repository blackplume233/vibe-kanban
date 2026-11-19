use nsh_base::{DiffLayout, diff_view};
use nsh_core::prelude::*;

use crate::{
    agent_conversation::agent_conversation,
    agent_management::agent_management,
    agent_settings::agent_settings,
    api::{Api, Workspace},
    board_ui::{Tailwind, control},
    github_integration::workspace_pull_requests,
    local_settings,
    terminal::terminal_panel,
    workspace_lifecycle::{LifecycleContext, workspace_lifecycle},
};

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Usage,
    Agents,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum WorkspacePane {
    Conversation,
    Diff,
    GitHub,
}

#[component]
pub fn App() -> impl IntoElement {
    let api = Api::local();
    let workspaces = signal(Vec::<Workspace>::new());
    let selected = signal(None::<String>);
    let selected_issue = signal(None::<SharedString>);
    let issue = signal(None::<String>);
    let workspace_screen = signal(false);
    let settings = signal(None::<Panel>);
    let command_open = signal(false);
    effect(move || issue.set(selected_issue.get().map(|id| id.to_string())));
    effect({
        let api = api.clone();
        move || {
            selected.get();
            let api = api.clone();
            let task = spawn(async move {
                if let Ok(rows) = api.workspaces().await {
                    workspaces.set(rows);
                }
            });
            cleanup(move || task.cancel());
        }
    });
    let open_panel = move |panel| {
        settings.set(Some(panel));
        workspace_screen.set(true);
    };
    view()
        .flex_1()
        .class("bg-background")
        .child(
            view()
                .h(px(48.0))
                .px_4()
                .flex_row()
                .items_center()
                .gap_3()
                .class("border-b border-border bg-sidebar")
                .child(text("vibe-own").class("font-semibold text-foreground"))
                .child(control("议题", move || {
                    settings.set(None);
                    workspace_screen.set(false);
                }))
                .child(control("工作区", move || {
                    settings.set(None);
                    workspace_screen.set(true);
                }))
                .child(view().flex_1())
                .child(control("执行用量", move || open_panel(Panel::Usage)))
                .child(control("Agent 配置", move || open_panel(Panel::Agents)))
                .child(control("设置", move || open_panel(Panel::Settings))),
        )
        .child(crate::board_ui::board_view(
            api.clone(),
            workspaces,
            selected,
            selected_issue,
            workspace_screen,
            command_open,
            move || settings.with_untracked(Option::is_none),
        ))
        .child(when(
            move || workspace_screen.get() && settings.with(Option::is_none),
            {
                let api = api.clone();
                move || workspace_pane(api.clone(), selected, issue, command_open)
            },
        ))
        .child(dynamic(move || {
            settings.get().map(|panel| match panel {
                Panel::Usage => agent_management().into_element(),
                Panel::Agents => agent_settings(api.clone()).into_element(),
                Panel::Settings => local_settings::panel(move || settings.set(None)).into_element(),
            })
        }))
}

fn workspace_pane(
    api: Api,
    selected: Signal<Option<String>>,
    issue: Signal<Option<String>>,
    command_open: Signal<bool>,
) -> impl IntoElement {
    let pane = signal(WorkspacePane::Conversation);
    let terminal_open = signal(false);
    let origin = api.http_origin().to_owned();
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(
            view()
                .w(px(360.0))
                .flex_shrink_0()
                .class("border-r border-border")
                .child(workspace_lifecycle(
                    LifecycleContext::new(api.clone(), selected).for_issue(issue),
                )),
        )
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .child(when(
                    move || selected.with(Option::is_none),
                    || text("选择或新建工作区").class("p-4 text-muted-foreground"),
                ))
                .child(when(
                    move || selected.with(Option::is_some),
                    move || {
                        view()
                            .flex_row()
                            .gap_2()
                            .p_2()
                            .class("border-b border-border")
                            .child(control("会话", move || {
                                pane.set(WorkspacePane::Conversation)
                            }))
                            .child(control("日志与变更", move || {
                                pane.set(WorkspacePane::Diff)
                            }))
                            .child(control("Pull Requests", move || {
                                pane.set(WorkspacePane::GitHub)
                            }))
                    },
                ))
                .child(dynamic(move || {
                    selected.get().map(|id| {
                        let (tab, content) = match pane.get() {
                            WorkspacePane::Conversation => (
                                "conversation",
                                agent_conversation(api.clone(), id.clone()).into_element(),
                            ),
                            WorkspacePane::Diff => (
                                "diff",
                                workspace_changes(api.clone(), id.clone()).into_element(),
                            ),
                            WorkspacePane::GitHub => (
                                "github",
                                scroll()
                                    .flex_1()
                                    .child(workspace_pull_requests(
                                        api.clone(),
                                        id.clone().into(),
                                        move || !command_open.get_untracked(),
                                    ))
                                    .into_element(),
                            ),
                        };
                        view()
                            .key(format!("{id}:{tab}"))
                            .flex_1()
                            .min_h(px(0.0))
                            .child(content)
                    })
                }))
                .child(terminal_panel(
                    origin,
                    move || selected.get(),
                    terminal_open,
                    move || command_open.get(),
                )),
        )
}

fn workspace_changes(api: Api, id: String) -> impl IntoElement {
    let files = signal(Vec::new());
    let file_index = signal(0usize);
    let diff_status = signal(SharedString::from("正在连接工作树"));
    let lines = signal(Vec::<SharedString>::new());
    let log_status = signal(SharedString::from("正在加载最新会话"));
    let diff_api = api.clone();
    let diff_id = id.clone();
    let diff_task = spawn(async move {
        if let Err(error) = diff_api.diffs(&diff_id, files, diff_status).await {
            diff_status.set(error.into());
        }
    });
    let log_task = spawn(async move {
        if let Err(error) = api.logs(&id, lines, log_status).await {
            log_status.set(error.into());
        }
    });
    cleanup(move || {
        diff_task.cancel();
        log_task.cancel();
    });

    let active_file = computed(move || {
        files.with(|entries| {
            entries
                .get(file_index.get())
                .or_else(|| entries.first())
                .map(|(_, file)| file.clone())
        })
    });
    view()
        .flex_1()
        .min_h(px(0.0))
        .p_4()
        .gap_2()
        .child(text(log_status).class("text-xs text-muted-foreground"))
        .child(
            virtual_list(
                move || lines.with(Vec::len),
                move |row| {
                    text(move || {
                        lines.with(|log| log.get(row.index()).cloned().unwrap_or_default())
                    })
                    .font_family("IBM Plex Mono")
                    .class("text-xs text-foreground")
                    .number_of_lines(1)
                },
            )
            .estimated_item_size(px(22.0))
            .follow_end(px(100.0))
            .h(px(132.0)),
        )
        .child(
            view()
                .flex_row()
                .gap_2()
                .items_center()
                .child(control("下一个变更文件", move || {
                    let count = files.with(Vec::len);
                    if count != 0 {
                        file_index.update(|index| *index = (*index + 1) % count);
                    }
                }))
                .child(text(diff_status).class("text-xs text-muted-foreground"))
                .child(
                    text(move || {
                        active_file.with(|file| {
                            file.as_ref()
                                .map_or_else(String::new, |file| file.path().to_owned())
                        })
                    })
                    .font_family("IBM Plex Mono")
                    .class("text-xs text-foreground"),
                ),
        )
        .child(
            diff_view()
                .layout(DiffLayout::Unified)
                .language(move || {
                    active_file.with(|file| {
                        file.as_ref().map_or_else(SharedString::default, |file| {
                            SharedString::from(file.path().to_owned())
                        })
                    })
                })
                .old_value(move || {
                    active_file.with(|file| {
                        file.as_ref()
                            .and_then(|file| file.old_content.as_ref())
                            .map(|content| SharedString::from(content.clone()))
                    })
                })
                .new_value(move || {
                    active_file.with(|file| {
                        file.as_ref()
                            .and_then(|file| file.new_content.as_ref())
                            .map(|content| SharedString::from(content.clone()))
                    })
                })
                .flex_1(),
        )
}
