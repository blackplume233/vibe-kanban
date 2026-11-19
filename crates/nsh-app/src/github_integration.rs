//! Workspace pull requests and explicit GitHub Issue synchronization.

mod wire;

use nsh_base::{SelectItem, combobox};
#[cfg(windows)]
use nsh_core::{element::KeyOptions, keymap::When};
use nsh_core::{element::input, prelude::*};

use self::wire::{FixCi, PrStatus, PullRequest, SyncIssue, SyncOutcome};
use crate::{
    Api,
    issues_api::{Issue, Status, StatusCategory},
};

const FIELD_STYLE: &str = "min-h-9 rounded border border-input bg-card px-2 text-sm text-foreground focus:outline-2 focus:outline-offset-2 focus:outline-ring";
const ACTION_STYLE: &str = "min-h-9 rounded border border-input bg-card px-3 hover:bg-surface-hover active:bg-accent focus:outline-2 focus:outline-offset-2 focus:outline-ring";

#[derive(Clone, PartialEq)]
struct Feedback {
    message: SharedString,
    role: &'static str,
}

impl Feedback {
    fn new(message: impl Into<SharedString>, role: &'static str) -> Self {
        Self {
            message: message.into(),
            role,
        }
    }
}

fn section(id: &str, title: &'static str) -> View {
    view()
        .test_id(id)
        .app_class(Some(
            "min-w-0 shrink-0 gap-3 rounded-md border border-border bg-card p-4".into(),
        ))
        .child(text(title).app_class(Some("text-lg font-semibold text-foreground".into())))
}

fn color(role: &str) -> AdaptiveColor {
    let theme = nsh_base::theme();
    theme
        .color(role)
        .unwrap_or_else(|| theme.color("foreground").expect("foreground theme role"))
}

fn note(message: impl IntoReactive<SharedString>) -> Text {
    text(message).app_class(Some("text-xs text-muted-foreground".into()))
}

fn feedback(value: Signal<Option<Feedback>>) -> impl IntoElement {
    dynamic(move || {
        value.get().map(|feedback| {
            text(feedback.message)
                .app_class(Some("text-sm".into()))
                .text_color(move || color(feedback.role))
        })
    })
}

fn action(
    id: &str,
    label: &'static str,
    enabled: impl Fn() -> bool + Clone + 'static,
    press: impl Fn() + 'static,
) -> View {
    let disabled = enabled.clone();
    view()
        .test_id(id)
        .app_class(Some(ACTION_STYLE.into()))
        .justify_center()
        .role(AccessibilityRole::Button)
        .label(label)
        .disabled(move || !disabled())
        .opacity(move || if enabled() { 1.0_f32 } else { 0.58_f32 })
        .on_press(press)
        .child(text(label).app_class(Some("text-sm font-semibold text-foreground".into())))
}

fn choice(
    label: &'static str,
    items: Signal<Vec<SelectItem>>,
    selected: Signal<Option<SharedString>>,
    disabled: impl Fn() -> bool + 'static,
) -> impl IntoElement {
    view()
        .app_class(Some("min-w-0 flex-1 gap-1".into()))
        .child(note(label))
        .child(
            combobox()
                .items(items)
                .value(move || Some(selected.get().into_iter().collect::<Vec<_>>()))
                .on_change(move |values: &Vec<SharedString>| selected.set(values.first().cloned()))
                .disabled(disabled)
                .accessibility_label(label)
                .placeholder("选择…")
                .input_class(FIELD_STYLE)
                .popup_class("rounded-card border border-border bg-popover p-1")
                .item_class("min-h-9 px-2 text-body text-foreground focus:bg-accent"),
        )
}

/// Mount under the selected workspace, keyed by its ID.
pub(crate) fn workspace_pull_requests(
    api: Api,
    workspace_id: SharedString,
    shortcuts_active: impl Fn() -> bool + Clone + 'static,
) -> View {
    let repos = signal(Vec::new());
    let selected = signal(None::<SharedString>);
    let details = signal(None::<PullRequest>);
    let loading_repos = signal(false);
    let loading_pr = signal(false);
    let repairing = signal(false);
    let refresh_count = signal(0_u32);
    let repo_epoch = signal(0_u32);
    let pr_feedback = signal(None);
    let repair_feedback = signal(None);
    let draft = signal(None::<SharedString>);
    let draft_feedback = signal(None);

    effect({
        let api = api.clone();
        let workspace = workspace_id.clone();
        move || {
            refresh_count.get();
            loading_repos.set(true);
            pr_feedback.set(Some(Feedback::new("正在读取工作区仓库…", "status-info")));
            let api = api.clone();
            let workspace = workspace.clone();
            spawn(async move {
                let result = api.github_workspace_repos(&workspace).await;
                loading_repos.set(false);
                match result {
                    Ok(rows) => {
                        let current = selected.get_untracked();
                        let next = current
                            .filter(|id| rows.iter().any(|repo| repo.id == id.as_str()))
                            .or_else(|| rows.first().map(|repo| repo.id.clone().into()));
                        if next.is_none() {
                            details.set(None);
                            pr_feedback.set(Some(Feedback::new(
                                "工作区尚未关联仓库。",
                                "muted-foreground",
                            )));
                        }
                        repos.set(
                            rows.into_iter()
                                .map(|repo| SelectItem::new(repo.id, repo.display_name))
                                .collect(),
                        );
                        selected.set(next);
                        repo_epoch.update(|count| *count = count.wrapping_add(1));
                    }
                    Err(error) => pr_feedback.set(Some(Feedback::new(
                        format!("读取仓库失败 · {error}"),
                        "status-error",
                    ))),
                }
            });
        }
    });
    effect({
        let api = api.clone();
        let workspace = workspace_id.clone();
        move || {
            let repo = selected.get();
            repo_epoch.get();
            let Some(repo) = repo else { return };
            details.set(None);
            repair_feedback.set(None);
            loading_pr.set(true);
            pr_feedback.set(Some(Feedback::new(
                "正在读取 PR 和 CI 检查…",
                "status-info",
            )));
            let api = api.clone();
            let workspace = workspace.clone();
            spawn(async move {
                let result = api.github_pull_request(&workspace, &repo).await;
                loading_pr.set(false);
                match result {
                    Ok(pr) => {
                        pr_feedback.set(None);
                        details.set(Some(pr));
                    }
                    Err(error) => pr_feedback.set(Some(Feedback::new(
                        format!("读取 PR 失败 · {error}"),
                        "status-error",
                    ))),
                }
            });
        }
    });
    effect({
        let api = api.clone();
        let workspace = workspace_id.clone();
        move || {
            refresh_count.get();
            draft_feedback.set(Some(Feedback::new(
                "正在读取 Agent 跟进草稿…",
                "status-info",
            )));
            let api = api.clone();
            let workspace = workspace.clone();
            spawn(async move {
                match api.github_follow_up_draft(&workspace).await {
                    Ok(message) => {
                        let message = message.filter(|message| !message.trim().is_empty());
                        draft_feedback.set(
                            message
                                .is_none()
                                .then(|| Feedback::new("暂无跟进草稿。", "muted-foreground")),
                        );
                        draft.set(message.map(Into::into));
                    }
                    Err(error) => draft_feedback.set(Some(Feedback::new(
                        format!("读取跟进草稿失败 · {error}"),
                        "status-error",
                    ))),
                }
            });
        }
    });

    let can_refresh = move || !loading_repos.get() && !loading_pr.get() && !repairing.get();
    let refresh = move || {
        if !loading_repos.get_untracked()
            && !loading_pr.get_untracked()
            && !repairing.get_untracked()
        {
            refresh_count.update(|count| *count = count.wrapping_add(1));
        }
    };
    let can_fix = move || {
        can_refresh()
            && details.with(|pr| {
                pr.as_ref().is_some_and(|pr| {
                    pr.status == PrStatus::Open
                        && pr.checks.as_ref().is_some_and(|checks| checks.failed > 0)
                })
            })
    };
    let repair = move || {
        if !can_fix() {
            return;
        }
        let Some(repo) = selected.get_untracked() else {
            return;
        };
        repairing.set(true);
        repair_feedback.set(Some(Feedback::new("正在提交 CI 修复任务…", "status-info")));
        let api = api.clone();
        let workspace = workspace_id.clone();
        spawn(async move {
            let result = api.github_fix_ci(&workspace, &repo).await;
            repairing.set(false);
            let feedback = match result {
                Ok(FixCi::Scheduled) => Feedback::new("已启动 CI 修复任务。", "status-ok"),
                Ok(FixCi::Queued) => Feedback::new("CI 修复已加入 Agent 队列。", "status-ok"),
                Err(error) => Feedback::new(format!("提交 CI 修复失败 · {error}"), "status-error"),
            };
            repair_feedback.set(Some(feedback));
        });
    };
    // Win32's NSH host only tabs between text inputs. Use workspace-scoped
    // access keys for these two buttons while keeping native Tab on the web.
    #[cfg(windows)]
    {
        let options = KeyOptions {
            when: When::NotEditing,
            pass_through: true,
            ..Default::default()
        };
        let active = shortcuts_active.clone();
        on_key_with("mod+shift+j", options, move |_| {
            if active() {
                refresh();
            }
        });
        let repair = repair.clone();
        on_key_with("mod+shift+f", options, move |_| {
            if shortcuts_active() {
                repair();
            }
        });
    }
    #[cfg(not(windows))]
    let _ = shortcuts_active;

    let panel = section("github-workspace-panel", "Pull Request · CI")
        .child(choice("工作区仓库", repos, selected, move || {
            loading_repos.get() || repairing.get() || repos.with(Vec::is_empty)
        }))
        .child(feedback(pr_feedback))
        .child(dynamic(move || {
            details.with(|pr| pr.as_ref().map(render_pr))
        }))
        .child(
            view()
                .flex_row()
                .app_class(Some("gap-2".into()))
                .child(action("github-refresh-pr", "刷新 PR", can_refresh, refresh))
                .child(action("github-fix-ci", "修复 CI", can_fix, repair)),
        )
        .child(feedback(repair_feedback));
    #[cfg(windows)]
    let panel = panel.child(note("Ctrl+Shift+J 刷新 · Ctrl+Shift+F 修复 CI"));
    panel.child(
        view()
            .app_class(Some("gap-2 border-t border-border pt-3".into()))
            .child(
                text("Agent 跟进草稿")
                    .app_class(Some("text-sm font-semibold text-foreground".into())),
            )
            .child(feedback(draft_feedback))
            .child(when(
                move || draft.with(Option::is_some),
                move || {
                    view()
                        .app_class(Some("gap-1".into()))
                        .child(
                            text(move || draft.get().unwrap_or_default())
                                .app_class(Some("text-sm text-foreground".into()))
                                .number_of_lines(5),
                        )
                        .child(note("在 Agent 会话中查看完整草稿并发送。"))
                },
            )),
    )
}

fn render_pr(pr: &PullRequest) -> View {
    let state = match pr.status {
        PrStatus::Open => "开放",
        PrStatus::Merged => "已合并",
        PrStatus::Closed => "已关闭",
        PrStatus::Unknown => "状态未知",
    };
    let mut content = view()
        .app_class(Some("gap-1".into()))
        .child(
            text(format!("#{} · {}", pr.number, pr.title))
                .app_class(Some("text-sm font-semibold text-foreground".into())),
        )
        .child(note(format!("{state} · {}", pr.url)));
    let Some(checks) = &pr.checks else {
        return content.child(note("暂无 CI 检查数据。"));
    };
    content = content.child(
        text(format!(
            "通过 {} · 失败 {} · 等待 {} · 共 {}",
            checks.passed, checks.failed, checks.pending, checks.total
        ))
        .app_class(Some("text-sm".into()))
        .text_color({
            let role = if checks.failed > 0 {
                "status-error"
            } else {
                "foreground"
            };
            move || color(role)
        }),
    );
    for check in &checks.failures {
        content = content.child(note(format!("失败 · {}", check.name)));
    }
    content
}

/// Mount under the selected project, keyed by its ID. `on_synced` receives
/// the persisted local issue on every write-capable outcome except conflict.
pub(crate) fn project_issue_sync(
    api: Api,
    project_id: SharedString,
    statuses: &[Status],
    on_synced: impl Fn(Issue) + 'static,
) -> View {
    let closed = |status: &Status| {
        matches!(
            status.category,
            StatusCategory::Completed | StatusCategory::Canceled
        )
    };
    let items = |is_closed| {
        statuses
            .iter()
            .filter(|status| closed(status) == is_closed)
            .map(|status| SelectItem::new(status.id.clone(), status.name.clone()))
            .collect::<Vec<_>>()
    };
    let open_items = items(false);
    let closed_items = items(true);
    let open_default = statuses
        .iter()
        .find(|status| status.category == StatusCategory::Unstarted)
        .or_else(|| statuses.iter().find(|status| !closed(status)));
    let closed_default = statuses
        .iter()
        .find(|status| status.category == StatusCategory::Completed)
        .or_else(|| statuses.iter().find(|status| closed(status)));
    let open = signal(open_default.map(|status| SharedString::from(status.id.clone())));
    let closed = signal(closed_default.map(|status| SharedString::from(status.id.clone())));
    let repository = signal(String::new());
    let number = signal(String::new());
    let busy = signal(false);
    let result = signal(None::<Feedback>);
    let ready = move || {
        !busy.get()
            && !repository.with(|value| value.trim().is_empty())
            && number.with(|value| issue_number(value).is_some())
            && open.with(Option::is_some)
            && closed.with(Option::is_some)
    };
    let on_synced = std::rc::Rc::new(on_synced);
    let sync = move |write_back| {
        if busy.get_untracked() {
            return;
        }
        let repository = repository.get_untracked();
        let Some(number) = issue_number(&number.get_untracked()) else {
            result.set(Some(Feedback::new(
                "请输入正整数议题编号。",
                "status-error",
            )));
            return;
        };
        if repository.trim().is_empty() {
            result.set(Some(Feedback::new(
                "请输入 GitHub 仓库，例如 owner/repo。",
                "status-error",
            )));
            return;
        }
        let (Some(open), Some(closed)) = (open.get_untracked(), closed.get_untracked()) else {
            result.set(Some(Feedback::new(
                "请映射开放和关闭状态。",
                "status-error",
            )));
            return;
        };
        busy.set(true);
        result.set(Some(Feedback::new(
            if write_back {
                "正在双向同步…"
            } else {
                "正在从 GitHub 同步…"
            },
            "status-info",
        )));
        let api = api.clone();
        let project = project_id.clone();
        let on_synced = on_synced.clone();
        spawn(async move {
            let response = api
                .github_sync_issue(
                    &project,
                    SyncIssue {
                        repository: repository.trim(),
                        number,
                        open_status_id: &open,
                        closed_status_id: &closed,
                        write_back,
                    },
                )
                .await;
            busy.set(false);
            match response {
                Ok(response) => {
                    let (label, role) = match response.outcome {
                        SyncOutcome::Imported => ("已导入", "status-ok"),
                        SyncOutcome::Synced => ("已同步", "status-ok"),
                        SyncOutcome::Unchanged => ("已是最新", "status-ok"),
                        SyncOutcome::PendingWrite => ("有本地变更待写回 GitHub", "status-warn"),
                        SyncOutcome::Conflict => ("同步冲突，请核对两端修改", "status-error"),
                    };
                    let mut message = format!(
                        "{label} · {} · {repository}#{number}",
                        response.issue.simple_id
                    );
                    for field in &response.fields {
                        message.push_str(" · ");
                        message.push_str(field.label());
                    }
                    result.set(Some(Feedback::new(message, role)));
                    if response.outcome != SyncOutcome::Conflict {
                        on_synced(response.issue);
                    }
                }
                Err(error) => result.set(Some(Feedback::new(
                    format!("同步失败 · {error}"),
                    "status-error",
                ))),
            }
        });
    };
    let pull = sync.clone();
    let submit = sync.clone();
    let number_field = input()
        .test_id("github-issue-number")
        .app_class(Some(FIELD_STYLE.into()))
        .label("GitHub 议题编号")
        .placeholder("例如 42")
        .value(move || number.get())
        .on_change(move |value: &str| number.set(value.into()))
        .on_submit(move |_: &str| submit(false))
        .readonly(move || busy.get());
    #[cfg(windows)]
    let number_field = {
        let focused = signal(false);
        let write = sync.clone();
        effect(move || {
            if focused.get() {
                let write = write.clone();
                on_key_with(
                    "mod+enter",
                    KeyOptions {
                        when: When::Always,
                        ..Default::default()
                    },
                    move |_| write(true),
                );
            }
        });
        number_field
            .on_focus(move || focused.set(true))
            .on_blur(move || focused.set(false))
    };
    let available = !open_items.is_empty() && !closed_items.is_empty();
    let panel = section("github-project-panel", "GitHub Issue 同步")
        .child(
            view()
                .flex_row()
                .app_class(Some("gap-3".into()))
                .child(
                    view()
                        .app_class(Some("min-w-0 flex-1 gap-1".into()))
                        .child(note("GitHub 仓库"))
                        .child(
                            input()
                                .test_id("github-repository")
                                .app_class(Some(FIELD_STYLE.into()))
                                .label("GitHub 仓库")
                                .placeholder("owner/repo")
                                .value(move || repository.get())
                                .on_change(move |value: &str| repository.set(value.into()))
                                .readonly(move || busy.get()),
                        ),
                )
                .child(
                    view()
                        .app_class(Some("min-w-0 flex-1 gap-1".into()))
                        .child(note("议题编号"))
                        .child(number_field),
                ),
        )
        .child(
            view()
                .flex_row()
                .app_class(Some("gap-3".into()))
                .child(choice(
                    "开放 → 本地状态",
                    signal(open_items),
                    open,
                    move || busy.get() || !available,
                ))
                .child(choice(
                    "关闭 → 本地状态",
                    signal(closed_items),
                    closed,
                    move || busy.get() || !available,
                )),
        )
        .child(when(
            move || !available,
            || note("项目需要至少一个未完成状态和一个已完成或已取消状态。"),
        ))
        .child(
            view()
                .flex_row()
                .app_class(Some("gap-2".into()))
                .child(action(
                    "github-import-issue",
                    "从 GitHub 同步",
                    ready,
                    move || pull(false),
                ))
                .child(action(
                    "github-write-issue",
                    "双向同步",
                    ready,
                    move || sync(true),
                )),
        )
        .child(note(
            "双向同步会将本地的标题、描述和开关状态变更写回 GitHub。",
        ))
        .child(feedback(result));
    #[cfg(windows)]
    let panel = panel.child(note("议题编号内按 Enter 拉取 · Ctrl+Enter 双向同步"));
    panel
}

fn issue_number(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|number| *number > 0)
}
