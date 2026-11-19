use std::{collections::BTreeMap, rc::Rc};

use nsh_base::{SelectItem, dialog, select};
use nsh_core::{
    element::{AnyElement, scroll},
    prelude::*,
};
use serde::{Deserialize, Serialize, de::IgnoredAny};

use super::{Id, Modal, Tailwind, Ui, control};
use crate::{
    Api,
    api::Workspace,
    issue_actions::{IssueId, IssueRef, ProjectId},
    issues_api::Issue,
};

#[derive(PartialEq)]
pub(super) struct LaunchState {
    project: Id,
    issue: Id,
}

impl Ui {
    pub(super) fn launch_workspace(self: &Rc<Self>, issue: IssueRef<'_>) {
        self.launch.set(Some(Rc::new(LaunchState {
            project: issue.project_id.0.to_owned().into(),
            issue: issue.issue_id.0.to_owned().into(),
        })));
        self.open(Modal::Launch);
    }
}

#[derive(Deserialize, PartialEq)]
struct Repository {
    id: String,
    display_name: String,
    path: String,
    default_target_branch: Option<String>,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
    is_current: bool,
    is_remote: bool,
}

#[derive(Deserialize, Serialize, PartialEq)]
struct Executor {
    executor: String,
    variant: Option<String>,
}

#[derive(Deserialize)]
struct SystemInfo {
    config: Config,
    executors: BTreeMap<String, BTreeMap<String, IgnoredAny>>,
}

#[derive(Deserialize)]
struct Config {
    executor_profile: Executor,
}

#[derive(PartialEq)]
struct Options {
    name: Id,
    prompt: Id,
    repositories: Vec<Repository>,
    executors: Vec<(SelectItem, Executor)>,
}

impl Options {
    async fn load(api: &Api, state: &LaunchState) -> Result<(Self, Option<Id>), String> {
        let issue = IssueRef {
            project_id: ProjectId(state.project.as_str()),
            issue_id: IssueId(state.issue.as_str()),
        };
        let (issue, repositories, info) = futures_util::try_join!(
            async {
                api.issue(issue)
                    .await
                    .map_err(|err| format!("读取议题失败：{err}"))
            },
            async {
                api.request::<Vec<Repository>>("GET", "/api/repos", None)
                    .await
                    .map_err(|err| format!("读取仓库失败：{err}"))
            },
            async {
                api.request::<SystemInfo>("GET", "/api/info", None)
                    .await
                    .map_err(|err| format!("读取执行器配置失败：{err}"))
            },
        )?;
        let preferred = info.config.executor_profile;
        let preferred_variant = preferred
            .variant
            .as_deref()
            .filter(|variant| *variant != "DEFAULT");
        let mut executors = Vec::new();
        let mut initial_executor = None;
        for (executor, variants) in info.executors {
            for (variant, _) in variants {
                if variant == "recently_used_models" {
                    continue;
                }
                let variant = (variant != "DEFAULT").then_some(variant);
                let id = format!("{executor}/{}", variant.as_deref().unwrap_or_default());
                let label = match &variant {
                    Some(variant) => format!("{executor} · {variant}"),
                    None => executor.clone(),
                };
                let item = SelectItem::new(id, label);
                if executor == preferred.executor && variant.as_deref() == preferred_variant {
                    initial_executor = Some(item.value.clone());
                }
                executors.push((
                    item,
                    Executor {
                        executor: executor.clone(),
                        variant,
                    },
                ));
            }
        }
        let (name, prompt) = issue_prompt(&issue);
        Ok((
            Self {
                name,
                prompt,
                repositories,
                executors,
            },
            initial_executor,
        ))
    }
}

fn issue_prompt(issue: &Issue) -> (Id, Id) {
    let name = format!("{} · {}", issue.simple_id, issue.title);
    let mut prompt = format!("请处理本地议题 {}：{}", issue.simple_id, issue.title);
    if let Some(description) = issue
        .description
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(description);
    }
    (name.into(), prompt.into())
}

#[derive(Serialize)]
struct StartRequest<'a> {
    name: &'a str,
    repos: [RepoInput<'a>; 1],
    executor_config: &'a Executor,
    local_issue_id: &'a str,
    prompt: &'a str,
}

#[derive(Serialize)]
struct RepoInput<'a> {
    repo_id: &'a str,
    target_branch: &'a str,
}

#[derive(Deserialize)]
struct StartedWorkspace {
    workspace: Workspace,
}

#[derive(Clone, Copy)]
struct Form {
    options: Signal<Option<Options>>,
    repository: Signal<Option<Id>>,
    branches: Signal<Option<Result<Vec<SelectItem>, Id>>>,
    branch: Signal<Option<Id>>,
    executor: Signal<Option<Id>>,
    error: Signal<Option<Id>>,
    submitting: Signal<bool>,
}

impl Form {
    fn new(api: Api, state: Rc<LaunchState>) -> Self {
        let form = Self {
            options: signal(None),
            repository: signal(None),
            branches: signal(None),
            branch: signal(None),
            executor: signal(None),
            error: signal(None),
            submitting: signal(false),
        };
        spawn({
            let api = api.clone();
            async move {
                match Options::load(&api, &state).await {
                    Ok((options, executor)) => {
                        let repository = match options.repositories.as_slice() {
                            [repo] => Some(Id::from(repo.id.clone())),
                            _ => None,
                        };
                        batch(|| {
                            form.options.set(Some(options));
                            form.repository.set(repository);
                            form.executor.set(executor);
                        });
                    }
                    Err(error) => form.error.set(Some(error.into())),
                }
            }
        });
        effect(move || {
            let repository = form.repository.get();
            form.branch.set(None);
            form.branches.set(None);
            let Some(repository) = repository else { return };
            let default_branch = form.options.with_untracked(|options| {
                options
                    .as_ref()?
                    .repositories
                    .iter()
                    .find(|repo| repo.id == repository.as_str())?
                    .default_target_branch
                    .clone()
            });
            let api = api.clone();
            spawn(async move {
                let path = format!("/api/repos/{repository}/branches");
                match api.request::<Vec<Branch>>("GET", &path, None).await {
                    Ok(branches) => {
                        let initial = default_branch
                            .as_deref()
                            .and_then(|name| branches.iter().find(|branch| branch.name == name))
                            .or_else(|| branches.iter().find(|branch| branch.is_current))
                            .or_else(|| branches.first())
                            .map(|branch| Id::from(branch.name.clone()));
                        let items = branches
                            .into_iter()
                            .map(|branch| {
                                let name = Id::from(branch.name);
                                SelectItem::new(name.clone(), name).group(if branch.is_remote {
                                    "远程分支"
                                } else {
                                    "本地分支"
                                })
                            })
                            .collect();
                        batch(|| {
                            form.branches.set(Some(Ok(items)));
                            form.branch.set(initial);
                        });
                    }
                    Err(error) => form
                        .branches
                        .set(Some(Err(format!("读取分支失败：{error}").into()))),
                }
            });
        });
        form
    }

    fn ready(self) -> bool {
        !self.submitting.get()
            && self.repository.with(Option::is_some)
            && self.branch.with(Option::is_some)
            && self.executor.with(Option::is_some)
    }

    fn status(self) -> Id {
        if let Some(error) = self.error.get() {
            return error;
        }
        if self.submitting.get() {
            return "正在创建并启动工作区…".into();
        }
        self.options.with(|options| {
            let Some(options) = options else {
                return "正在读取议题和启动选项…".into();
            };
            if options.repositories.is_empty() {
                return "没有可用的仓库。请先在本地服务注册仓库。".into();
            }
            if options.executors.is_empty() {
                return "没有可用的执行器配置。".into();
            }
            if !self.repository.with(Option::is_some) {
                return "请选择仓库。".into();
            }
            self.branches.with(|branches| match branches {
                None => "正在读取仓库分支…".into(),
                Some(Err(err)) => err.clone(),
                Some(Ok(items)) if items.is_empty() => "此仓库没有可用的分支。".into(),
                Some(Ok(_)) if !self.executor.with(Option::is_some) => "请选择执行器。".into(),
                Some(Ok(_)) => Id::default(),
            })
        })
    }

    fn submit(self, ui: Rc<Ui>) {
        if self.submitting.get_untracked() {
            return;
        }
        let (Some(repo_id), Some(target_branch), Some(executor)) = (
            self.repository.get_untracked(),
            self.branch.get_untracked(),
            self.executor.get_untracked(),
        ) else {
            return;
        };
        let Some(issue) = ui.launch.get_untracked() else {
            return;
        };
        let body = self.options.with_untracked(|options| {
            let options = options.as_ref()?;
            let (_, executor_config) = options
                .executors
                .iter()
                .find(|(item, _)| item.value == executor)?;
            Some(serde_json::to_vec(&StartRequest {
                name: options.name.as_str(),
                repos: [RepoInput {
                    repo_id: repo_id.as_str(),
                    target_branch: target_branch.as_str(),
                }],
                executor_config,
                local_issue_id: issue.issue.as_str(),
                prompt: options.prompt.as_str(),
            }))
        });
        let body = match body {
            Some(Ok(body)) => body,
            Some(Err(error)) => {
                self.error
                    .set(Some(format!("无法准备启动请求：{error}").into()));
                return;
            }
            None => return,
        };
        batch(|| {
            self.error.set(None);
            self.submitting.set(true);
        });
        spawn(async move {
            match ui
                .api
                .request::<StartedWorkspace>("POST", "/api/workspaces/start", Some(body))
                .await
            {
                Ok(StartedWorkspace { workspace }) => {
                    let id = workspace.id.clone();
                    batch(|| {
                        ui.workspaces.update(|rows| {
                            if let Some(row) = rows.iter_mut().find(|row| row.id == workspace.id) {
                                *row = workspace;
                            } else {
                                rows.insert(0, workspace);
                            }
                        });
                        ui.workspace_id.set(Some(id));
                        ui.workspace_screen.set(true);
                        ui.modal.set(None);
                        ui.launch.set(None);
                    });
                }
                Err(error) => batch(|| {
                    self.submitting.set(false);
                    self.error
                        .set(Some(format!("启动工作区失败：{error}").into()));
                }),
            }
        });
    }
}

pub(super) fn launch_dialog(ui: Rc<Ui>) -> AnyElement {
    let Some(state) = ui.launch.get_untracked() else {
        return view().into_element();
    };
    let form = Form::new(ui.api.clone(), state);
    let cancel_ref = element_ref();
    let cancel = {
        let ui = ui.clone();
        move || {
            if !form.submitting.get_untracked() {
                ui.close();
                ui.launch.set(None);
            }
        }
    };
    let status = computed(move || form.status());
    let footer = view().flex_row().gap_2().justify_end()
        .child(control("取消", cancel.clone()).focusable(true).element_ref(&cancel_ref)
            .disabled(form.submitting))
        .child(view().h(px(32.0)).justify_center()
            .class("rounded-md border border-primary bg-primary px-3 hover:opacity-90 focus:opacity-80 disabled:opacity-50")
            .role(AccessibilityRole::Button).label("启动工作区")
            .focusable(move || form.ready()).disabled(move || !form.ready())
            .on_press(move || form.submit(ui.clone()))
            .child(text("启动工作区").class("text-sm font-semibold text-primary-foreground")));
    let clear_error = move || form.error.set(None);
    let body = view()
        .p_4()
        .gap_3()
        .child(text("启动工作区").class("text-lg font-semibold text-foreground"))
        .child(
            text(move || {
                form.options.with(|options| {
                    options
                        .as_ref()
                        .map(|options| options.name.clone())
                        .unwrap_or_default()
                })
            })
            .class("text-sm text-muted-foreground"),
        )
        .child(select_field(
            "仓库",
            form.repository,
            form.submitting,
            move || {
                form.options.with(|options| {
                    options
                        .as_ref()
                        .map(|options| {
                            options
                                .repositories
                                .iter()
                                .map(|repo| {
                                    SelectItem::new(
                                        repo.id.clone(),
                                        format!("{} · {}", repo.display_name, repo.path),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
            },
            clear_error,
        ))
        .child(select_field(
            "目标分支",
            form.branch,
            form.submitting,
            move || {
                form.branches.with(|branches| {
                    branches
                        .as_ref()
                        .and_then(|result| result.as_ref().ok())
                        .cloned()
                        .unwrap_or_default()
                })
            },
            clear_error,
        ))
        .child(select_field(
            "执行器",
            form.executor,
            form.submitting,
            move || {
                form.options.with(|options| {
                    options
                        .as_ref()
                        .map(|options| {
                            options
                                .executors
                                .iter()
                                .map(|(item, _)| item.clone())
                                .collect()
                        })
                        .unwrap_or_default()
                })
            },
            clear_error,
        ))
        .child(when(
            move || status.with(|message| !message.is_empty()),
            move || {
                text(status).app_class(move || {
                    let has_error = form.error.with(Option::is_some)
                        || form
                            .branches
                            .with(|branches| matches!(branches, Some(Err(_))));
                    Some(Id::from(if has_error {
                        "text-sm text-destructive"
                    } else {
                        "text-sm text-foreground"
                    }))
                })
            },
        ))
        .child(footer);
    dialog()
        .close_on_escape(move || !form.submitting.get())
        .on_cancel(cancel)
        .on_open(move || cancel_ref.focus())
        .child(
            view()
                .flex_1()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000063))
                .p_3()
                .child(
                    view()
                        .w(px(540.0))
                        .class("max-h-full max-w-full rounded-lg border border-border bg-popover")
                        .child(scroll().min_h(px(0.0)).child(body)),
                ),
        )
        .into_element()
}

fn select_field(
    label: &'static str,
    value: Signal<Option<Id>>,
    submitting: Signal<bool>,
    items: impl Fn() -> Vec<SelectItem> + 'static,
    on_change: impl Fn() + 'static,
) -> View {
    let items = computed(items);
    let placeholder = Id::from(format!("选择{label}"));
    let trigger_label = placeholder.clone();
    view().gap_1()
        .child(text(label).class("text-xs font-semibold text-muted-foreground"))
        .child(select()
            .items(items)
            .value(move || value.get())
            .disabled(move || submitting.get() || items.with(Vec::is_empty))
            .placeholder(placeholder)
            .accessibility_label(label)
            .searchable(true)
            .search_placeholder("搜索选项")
            .search_accessibility_label(format!("搜索{label}"))
            .popup_class("rounded-md border border-border bg-popover")
            .search_class("h-9 px-2 text-sm text-foreground")
            .list_class("p-1")
            .item_class("h-9 px-2 rounded-md text-sm text-foreground focus:bg-accent hover:bg-accent")
            .group_class("px-2 py-1 text-xs text-muted-foreground")
            .empty(text("没有匹配项").class("px-2 py-1 text-sm text-muted-foreground"))
            .trigger(view().h(px(36.0)).flex_row().items_center().gap_2().px_2()
                .class("rounded-md border border-border bg-card hover:bg-accent focus:bg-accent disabled:opacity-50")
                .child(text(move || {
                    let selected = value.get();
                    items.with(|items| items.iter().find(|item| Some(&item.value) == selected.as_ref())
                        .map_or_else(|| trigger_label.clone(), |item| item.title.clone()))
                }).flex_1().min_w(px(0.0)).number_of_lines(1).class("text-sm text-foreground"))
                .child(text("▾").class("text-muted-foreground")))
            .on_change(move |selected: &Option<Id>| {
                batch(|| {
                    value.set(selected.clone());
                    on_change();
                });
            }))
}
