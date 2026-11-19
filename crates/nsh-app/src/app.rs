use nsh_base::{DiffLayout, diff_view};
use nsh_core::prelude::*;

use crate::{
    api::{Api, Workspace},
    board_ui::Tailwind,
};

const INK: u32 = 0x162b2b;
const MUTED: u32 = 0x526563;
const ACCENT: u32 = 0x17695a;
const LINE: u32 = 0xd8e2de;
const PAPER: u32 = 0xf7faf7;

#[component]
pub fn App() -> impl IntoElement {
    let api = Api::local();
    let workspaces = signal(Vec::<Workspace>::new());
    let selected = signal(None::<SharedString>);
    let selected_issue = signal(None::<SharedString>);
    let workspace_screen = signal(false);
    let overview = signal(SharedString::from("Connecting to local server"));
    let loader = api.clone();
    let loader_task = spawn(async move {
        match loader.workspaces().await {
            Ok(rows) => {
                overview.set(format!("{} workspaces · Local server", rows.len()).into());
                workspaces.set(rows);
            }
            Err(error) => overview.set(error.into()),
        }
    });
    cleanup(move || loader_task.cancel());
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
                .child(crate::board_ui::control("议题", move || {
                    workspace_screen.set(false)
                }))
                .child(crate::board_ui::control("工作区", move || {
                    workspace_screen.set(true)
                })),
        )
        .child(crate::board_ui::board_view(
            api.clone(),
            workspaces,
            selected,
            selected_issue,
            workspace_screen,
        ))
        .child(when(workspace_screen, move || {
            workspace_pane(api.clone(), workspaces, selected, overview)
        }))
}

fn workspace_pane(
    api: Api,
    workspaces: Signal<Vec<Workspace>>,
    selected: Signal<Option<SharedString>>,
    overview: Signal<SharedString>,
) -> impl IntoElement {
    let files = signal(Vec::new());
    let file_index = signal(0usize);
    let diff_status = signal(SharedString::from("Select a workspace"));
    let lines = signal(Vec::<SharedString>::new());
    let log_status = signal(SharedString::from("Select a workspace"));

    effect(move || {
        let Some(id) = selected.get() else { return };
        files.set(Vec::new());
        file_index.set(0);
        lines.set(Vec::new());
        diff_status.set("Connecting to worktree".into());
        log_status.set("Loading latest session".into());
        let diff_api = api.clone();
        let diff_id = id.clone();
        let diff_task = spawn(async move {
            if let Err(error) = diff_api.diffs(diff_id.as_str(), files, diff_status).await {
                diff_status.set(error.into());
            }
        });
        let log_api = api.clone();
        let log_task = spawn(async move {
            if let Err(error) = log_api.logs(id.as_str(), lines, log_status).await {
                log_status.set(error.into());
            }
        });
        cleanup(move || {
            diff_task.cancel();
            log_task.cancel();
        });
    });

    let active_file = computed(move || {
        files.with(|entries| {
            entries
                .get(file_index.get())
                .or_else(|| entries.first())
                .map(|(_, file)| file.clone())
        })
    });
    let log_rows = virtual_list(
        move || lines.with(Vec::len),
        move |row| {
            view().h(px(21.0)).px_3().justify_center().child(
                text(move || lines.with(|log| log.get(row.index()).cloned().unwrap_or_default()))
                    .font_family("Consolas")
                    .text_size(px(13.0))
                    .text_color(rgb(0xc7e6d5))
                    .number_of_lines(1),
            )
        },
    )
    .estimated_item_size(px(22.0))
    .follow_end(px(100.0))
    .flex_1();

    view()
        .flex_1()
        .bg(rgb(PAPER))
        .child(
            view()
                .h(px(68.0))
                .px_5()
                .flex_row()
                .items_center()
                .gap_4()
                .bg(rgb(0x102d2a))
                .child(
                    text("vibe-own")
                        .text_size(px(27.0))
                        .text_color(rgb(0xf0fff5)),
                )
                .child(text(overview).text_size(px(13.0)).text_color(rgb(0xa9ccbe))),
        )
        .child(
            view()
                .flex_1()
                .flex_row()
                .child(
                    view()
                        .w(px(244.0))
                        .p_4()
                        .gap_2()
                        .border_r_1()
                        .border_r_color(rgb(LINE))
                        .child(label("WORKSPACES"))
                        .child(
                            virtual_list(
                                move || workspaces.with(Vec::len),
                                move |row| {
                                    view()
                                        .h(px(38.0))
                                        .px_2()
                                        .justify_center()
                                        .rounded(px(4.0))
                                        .bg(move || {
                                            let id = selected.get();
                                            rgb(
                                                if workspaces.with(|rows| {
                                                    rows.get(row.index()).is_some_and(|item| {
                                                        id.as_ref().is_some_and(|id| {
                                                            id.as_str() == item.id
                                                        })
                                                    })
                                                }) {
                                                    0xd1ebdf
                                                } else {
                                                    0xecf3ed
                                                },
                                            )
                                        })
                                        .on_press(move || {
                                            let id = workspaces.with(|rows| {
                                                rows.get(row.index())
                                                    .map(|row| SharedString::from(row.id.clone()))
                                            });
                                            selected.set(id);
                                        })
                                        .child(
                                            text(move || {
                                                workspaces.with(|rows| {
                                                    rows.get(row.index())
                                                        .map(|item| {
                                                            SharedString::from(
                                                                item.label().to_owned(),
                                                            )
                                                        })
                                                        .unwrap_or_default()
                                                })
                                            })
                                            .text_color(rgb(INK))
                                            .number_of_lines(1),
                                        )
                                },
                            )
                            .estimated_item_size(px(38.0))
                            .row_gap(px(4.0))
                            .h(px(124.0)),
                        )
                        .child(label("SCROLL BENCHMARK · 5,000 ROWS"))
                        .child(benchmark()),
                )
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .p_4()
                        .gap_3()
                        .child(
                            text(move || {
                                workspaces.with(|rows| {
                                    selected
                                        .get()
                                        .and_then(|id| {
                                            rows.iter().find(|row| row.id == id.as_str())
                                        })
                                        .map(|row| SharedString::from(row.label().to_owned()))
                                        .unwrap_or_else(|| "Select a workspace".into())
                                })
                            })
                            .text_size(px(24.0))
                            .text_color(rgb(INK)),
                        )
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(label("SESSION LOG"))
                                .child(text(log_status).text_size(px(12.0)).text_color(rgb(MUTED)))
                                .child(
                                    text(move || format!("{} entries", lines.with(Vec::len)))
                                        .text_size(px(12.0))
                                        .text_color(rgb(ACCENT)),
                                ),
                        )
                        .child(
                            view()
                                .h(px(152.0))
                                .py_2()
                                .rounded(px(5.0))
                                .bg(rgb(0x102d2a))
                                .child(log_rows),
                        )
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(label("WORKTREE DIFF"))
                                .child(
                                    text(diff_status).text_size(px(12.0)).text_color(rgb(MUTED)),
                                ),
                        )
                        .child(
                            view()
                                .flex_row()
                                .gap_3()
                                .items_center()
                                .child(
                                    view()
                                        .px_2()
                                        .py_1()
                                        .rounded(px(3.0))
                                        .bg(rgb(0xe4f1e9))
                                        .on_press(move || {
                                            files.with(|entries| {
                                                if !entries.is_empty() {
                                                    file_index.update(|index| {
                                                        *index = (*index + 1) % entries.len()
                                                    });
                                                }
                                            })
                                        })
                                        .child(
                                            text(move || {
                                                files.with(|entries| {
                                                    if entries.is_empty() {
                                                        "No changed files".to_owned()
                                                    } else if entries.len() == 1 {
                                                        "1 changed file".to_owned()
                                                    } else {
                                                        format!(
                                                            "Next file · {} changed files",
                                                            entries.len()
                                                        )
                                                    }
                                                })
                                            })
                                            .text_color(rgb(ACCENT))
                                            .text_size(px(13.0)),
                                        ),
                                )
                                .child(
                                    text(move || {
                                        active_file.with(|file| {
                                            file.as_ref()
                                                .map(|file| {
                                                    if file.content_omitted {
                                                        format!(
                                                            "{} (contents omitted by server)",
                                                            file.path()
                                                        )
                                                    } else {
                                                        file.path().to_owned()
                                                    }
                                                })
                                                .unwrap_or_default()
                                        })
                                    })
                                    .font_family("Consolas")
                                    .text_color(rgb(INK))
                                    .text_size(px(13.0)),
                                ),
                        )
                        .child(
                            view()
                                .flex_1()
                                .min_h(px(0.0))
                                .border_1()
                                .border_color(rgb(LINE))
                                .bg(white())
                                .child(
                                    diff_view()
                                        .layout(DiffLayout::Unified)
                                        .language(move || {
                                            active_file.with(|file| {
                                                file.as_ref()
                                                    .map(|file| {
                                                        SharedString::from(file.path().to_owned())
                                                    })
                                                    .unwrap_or_default()
                                            })
                                        })
                                        .old_value(move || {
                                            active_file.with(|file| {
                                                file.as_ref()
                                                    .and_then(|file| file.old_content.as_ref())
                                                    .map(|text| SharedString::from(text.clone()))
                                            })
                                        })
                                        .new_value(move || {
                                            active_file.with(|file| {
                                                file.as_ref()
                                                    .and_then(|file| file.new_content.as_ref())
                                                    .map(|text| SharedString::from(text.clone()))
                                            })
                                        })
                                        .flex_1(),
                                ),
                        ),
                ),
        )
}

fn label(title: &'static str) -> impl IntoElement {
    text(title).text_size(px(12.0)).text_color(rgb(ACCENT))
}

fn benchmark() -> impl IntoElement {
    virtual_list(5000, |row| {
        view()
            .h(px(27.0))
            .px_2()
            .justify_center()
            .border_b_1()
            .border_b_color(rgb(LINE))
            .child(
                text(move || format!("{:04}  Scroll sample", row.index() + 1))
                    .font_family("Consolas")
                    .text_size(px(12.0))
                    .text_color(rgb(MUTED)),
            )
    })
    .estimated_item_size(px(27.0))
    .on_scroll(|event: &nsh_core::element::ScrollEvent| {
        log::info!("benchmark offset={:.0}", event.offset.y.0);
    })
    .flex_1()
}
