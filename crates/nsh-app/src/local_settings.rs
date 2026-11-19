#[path = "local_settings_form.rs"]
mod form;
#[path = "local_settings_api.rs"]
mod wire;

use std::collections::BTreeMap;

use form::{action, card, label, tabs};
use nsh_base::SelectItem;
use nsh_core::{
    element::{Appearance, set_color_scheme},
    prelude::*,
};
use serde::Deserialize;
use serde_json::{Value, json};
use wire::{
    Branch, Directory, Draft, INFO, Info, Profile, REPOS, RepoSummary, directory_url, optional,
    prefix_error, write,
};

use crate::api::Api;

const PREFIX: &str = "/git_branch_prefix";
const WORKSPACE_DIR: &str = "/workspace_dir";
const EXECUTOR: &str = "/executor_profile/executor";
const VARIANT: &str = "/executor_profile/variant";
const REPO_BRANCH: &str = "/default_target_branch";

pub(crate) fn panel(on_back: impl Fn() + 'static) -> impl IntoElement {
    local_settings().on_close(move |_: &()| on_back())
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Defaults,
    EditorGit,
    Repos,
}

#[derive(Clone, Copy, PartialEq)]
enum Confirm {
    Leave,
    RemoveRepo,
}

#[derive(Clone, Copy, PartialEq)]
enum FolderUse {
    Workspace,
    Repository,
}

#[derive(Clone, Copy)]
struct State {
    page: Signal<Page>,
    busy: Signal<bool>,
    status: Signal<Option<(bool, String)>>,
    product: Signal<Option<Draft>>,
    profiles: Signal<BTreeMap<String, Profile>>,
    appearance: Signal<Option<Appearance>>,
    repos: Signal<Option<Vec<RepoSummary>>>,
    repo: Signal<Option<Draft>>,
    branches: Signal<Result<Vec<SelectItem>, String>>,
    repo_path: Signal<String>,
    folder: Signal<Option<FolderUse>>,
    directory: Signal<Option<Directory>>,
    confirm: Signal<Option<Confirm>>,
    pending: Stored<Option<Spawned<()>>>,
}

impl State {
    fn new() -> Self {
        Self {
            page: signal(Page::Defaults),
            busy: signal(false),
            status: signal(None),
            product: signal(None),
            profiles: signal_with(BTreeMap::new(), SignalEquals::Never),
            appearance: signal(None),
            repos: signal(None),
            repo: signal(None),
            branches: signal(Ok(Vec::new())),
            repo_path: signal(String::new()),
            folder: signal(None),
            directory: signal_with(None, SignalEquals::Never),
            confirm: signal(None),
            pending: stored(None),
        }
    }

    fn request(
        self,
        operation: impl Future<Output = Result<Option<&'static str>, String>> + 'static,
    ) {
        if self.busy.get_untracked() {
            return;
        }
        self.busy.set(true);
        self.status.set(None);
        self.pending.replace(Some(spawn(async move {
            match operation.await {
                Ok(Some(message)) => self.status.set(Some((false, message.to_owned()))),
                Ok(None) => {}
                Err(error) => self.status.set(Some((true, error))),
            }
            self.busy.set(false);
        })));
    }

    fn load(self, page: Page) {
        self.request(async move {
            let api = Api::local();
            if page == Page::Repos {
                let repos = api.request("GET", REPOS, None).await?;
                self.repos.set(Some(repos));
            } else {
                let Info { config, executors } = api.request("GET", INFO, None).await?;
                self.apply_appearance(&config);
                self.product.set(Some(Draft::new(config)));
                self.profiles.set(executors);
            }
            Ok(None)
        });
    }

    fn apply_appearance(self, config: &Value) {
        self.appearance.set(Some(match config["theme"].as_str() {
            Some("LIGHT") => Appearance::Light,
            Some("DARK") => Appearance::Dark,
            _ => Appearance::System,
        }));
    }

    fn save_product(self) {
        if !dirty(self.product) || prefix_error(&string(self.product, PREFIX)).is_some() {
            return;
        }
        self.request(async move {
            #[derive(Deserialize)]
            struct LatestConfig {
                config: Value,
            }
            let api = Api::local();
            let LatestConfig { mut config } = api.request("GET", INFO, None).await?;
            self.product.with_untracked(|draft| {
                draft
                    .as_ref()
                    .expect("loaded product settings")
                    .config_body(&mut config);
            });
            let saved = write(&api, "PUT", "/api/config", &config).await?;
            self.apply_appearance(&saved);
            self.product.set(Some(Draft::new(saved)));
            Ok(Some("产品设置已保存"))
        });
    }

    async fn read_repo(self, api: &Api, id: &str) -> Result<(), String> {
        let url = format!("{REPOS}/{id}");
        let repo: Value = api.request("GET", &url, None).await?;
        self.repo.set(Some(Draft::new(repo)));
        let current = string(self.repo, REPO_BRANCH);
        let branches = api
            .request::<Vec<Branch>>("GET", &format!("{url}/branches"), None)
            .await;
        self.branches.set(branches.map(|branches| {
            let mut items = vec![SelectItem::new("", "使用当前分支")];
            if !current.is_empty() && !branches.iter().any(|branch| branch.name == current) {
                items.push(SelectItem::new(
                    current.clone(),
                    format!("{current}（已保存）"),
                ));
            }
            items.extend(branches.into_iter().map(|branch| {
                let title = if branch.is_current {
                    format!("{}（当前）", branch.name)
                } else {
                    branch.name.clone()
                };
                SelectItem::new(branch.name, title)
            }));
            items
        }));
        Ok(())
    }

    fn select_repo(self, id: String) {
        self.request(async move { self.read_repo(&Api::local(), &id).await.map(|()| None) });
    }

    fn save_repo(self) {
        if !dirty(self.repo) {
            return;
        }
        self.request(async move {
            let id = string(self.repo, "/id");
            let body = self
                .repo
                .with_untracked(|draft| draft.as_ref().expect("selected repository").repo_body());
            let saved: Value = write(&Api::local(), "PUT", &format!("{REPOS}/{id}"), &body).await?;
            self.update_repo_list(&saved);
            self.repo.set(Some(Draft::new(saved)));
            Ok(Some("仓库设置已保存"))
        });
    }

    fn update_repo_list(self, saved: &Value) {
        self.repos.update(|repos| {
            let (Some(repos), Some(id), Some(name)) =
                (repos, saved["id"].as_str(), saved["display_name"].as_str())
            else {
                return;
            };
            if let Some(repo) = repos.iter_mut().find(|repo| repo.id == id) {
                name.clone_into(&mut repo.display_name);
            } else {
                repos.push(RepoSummary {
                    id: id.to_owned(),
                    display_name: name.to_owned(),
                });
            }
        });
    }

    fn register(self) {
        let path = self.repo_path.with_untracked(|path| path.trim().to_owned());
        if path.is_empty() || dirty(self.repo) {
            return;
        }
        self.request(async move {
            let api = Api::local();
            let saved: Value = write(&api, "POST", REPOS, &json!({"path": path})).await?;
            self.update_repo_list(&saved);
            self.repo_path.set(String::new());
            self.read_repo(&api, saved["id"].as_str().ok_or("仓库响应缺少 ID")?)
                .await?;
            Ok(Some("仓库已添加"))
        });
    }

    fn remove(self) {
        self.request(async move {
            let id = string(self.repo, "/id");
            Api::local()
                .request::<()>("DELETE", &format!("{REPOS}/{id}"), None)
                .await?;
            self.repos.update(|repos| {
                if let Some(repos) = repos {
                    repos.retain(|repo| repo.id != id);
                }
            });
            self.repo.set(None);
            Ok(Some("仓库已移除"))
        });
    }

    fn browse(self, path: String) {
        self.request(async move {
            let directory = Api::local()
                .request("GET", &directory_url(&path), None)
                .await?;
            self.directory.set(Some(directory));
            Ok(None)
        });
    }

    fn open_folder(self, kind: FolderUse) {
        self.directory.set(None);
        self.folder.set(Some(kind));
        self.browse(match kind {
            FolderUse::Workspace => string(self.product, WORKSPACE_DIR),
            FolderUse::Repository => self.repo_path.get_untracked(),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Event)]
enum LocalSettingsEvent {
    Close,
}

#[component]
fn LocalSettings(cx: Scope<LocalSettingsEvent>) -> impl IntoElement {
    let state = State::new();
    effect(move || {
        if let Some(appearance) = state.appearance.get() {
            set_color_scheme(appearance);
        }
    });
    state.load(Page::Defaults);
    cleanup(move || {
        if let Some(task) = state.pending.replace(None) {
            task.cancel();
        }
    });

    let close = move || cx.emit(LocalSettingsEvent::Close);
    let blocked = move || {
        state.busy.get()
            || state.folder.with(Option::is_some)
            || state.confirm.with(Option::is_some)
    };
    view()
        .app_class("flex-1 min-h-0 min-w-0 bg-background")
        .test_id("local-settings")
        .child(
            view()
                .app_class("bg-sidebar border-b border-sidebar-border p-4 gap-3 shrink-0")
                .child(
                    view()
                        .app_class("flex-row justify-between items-center gap-2")
                        .child(label("本地设置").app_class("text-page font-semibold"))
                        .child(action(
                            "settings-close",
                            "返回",
                            blocked,
                            false,
                            move || {
                                if dirty(state.product) || dirty(state.repo) {
                                    state.confirm.set(Some(Confirm::Leave));
                                } else {
                                    close();
                                }
                            },
                        )),
                )
                .child(tabs(state.page, blocked, move |page| {
                    state.page.set(page);
                    state.status.set(None);
                    if page == Page::Repos {
                        if !state.repos.with_untracked(Option::is_some) {
                            state.load(page);
                        }
                    } else if !state.product.with_untracked(Option::is_some) {
                        state.load(page);
                    }
                })),
        )
        .child(dynamic(move || {
            if let Some(confirm) = state.confirm.get() {
                return confirm_content(state, confirm, close).into_element();
            }
            if let Some(kind) = state.folder.get() {
                return folder_content(state, kind).into_element();
            }
            let page = state.page.get();
            scroll()
                .app_class("flex-1 min-h-0")
                .child(
                    view()
                        .app_class("p-4 gap-3 w-full max-w-[760px] mx-auto")
                        .child(dynamic(move || match page {
                            Page::Defaults | Page::EditorGit
                                if !state.product.with(Option::is_some) =>
                            {
                                loading(state, page).into_element()
                            }
                            Page::Repos if !state.repos.with(Option::is_some) => {
                                loading(state, page).into_element()
                            }
                            Page::Defaults => form::defaults(state).into_element(),
                            Page::EditorGit => form::editor_git(state).into_element(),
                            Page::Repos => form::repos(state).into_element(),
                        })),
                )
                .into_element()
        }))
        .child(save_bar(state))
}

fn loading(state: State, page: Page) -> View {
    view()
        .app_class("gap-2 items-start")
        .child(label(move || {
            if state.busy.get() {
                "正在读取本地设置…"
            } else {
                "无法读取本地设置"
            }
        }))
        .child(action(
            "settings-retry",
            "重试",
            state.busy,
            false,
            move || state.load(page),
        ))
}

fn confirm_content(state: State, confirmation: Confirm, close: impl Fn() + 'static) -> View {
    let (title, description, verb) = match confirmation {
        Confirm::Leave => (
            "放弃未保存的修改？",
            "返回后，本次未保存的修改将丢失。",
            "放弃并返回",
        ),
        Confirm::RemoveRepo => (
            "移除仓库？",
            "这会移除仓库注册和设置，磁盘上的 Git 仓库会保留。",
            "移除仓库",
        ),
    };
    view().app_class("flex-1 p-4 gap-3").child(
        card(title).child(label(description)).child(
            view()
                .app_class("flex-row gap-2")
                .child(action(
                    "settings-confirm-cancel",
                    "取消",
                    state.busy,
                    false,
                    move || state.confirm.set(None),
                ))
                .child(action(
                    "settings-confirm",
                    verb,
                    state.busy,
                    true,
                    move || {
                        state.confirm.set(None);
                        match confirmation {
                            Confirm::Leave => close(),
                            Confirm::RemoveRepo => state.remove(),
                        }
                    },
                )),
        ),
    )
}

fn folder_content(state: State, kind: FolderUse) -> View {
    let no_folder = move || state.busy.get() || !state.directory.with(Option::is_some);
    view()
        .app_class("flex-1 min-h-0 p-4 gap-3")
        .child(label("选择本机目录").app_class("text-title font-semibold"))
        .child(
            view()
                .app_class("flex-row flex-wrap gap-2")
                .child(action(
                    "settings-folder-cancel",
                    "取消",
                    state.busy,
                    false,
                    move || state.folder.set(None),
                ))
                .child(action(
                    "settings-folder-home",
                    "主目录",
                    state.busy,
                    false,
                    move || state.browse(String::new()),
                ))
                .child(action(
                    "settings-folder-parent",
                    "上级目录",
                    move || {
                        state.busy.get()
                            || !state.directory.with(|directory| {
                                directory.as_ref().is_some_and(|dir| {
                                    wire::parent_dir(&dir.current_path).is_some()
                                })
                            })
                    },
                    false,
                    move || {
                        if let Some(path) = state.directory.with_untracked(|directory| {
                            directory
                                .as_ref()
                                .and_then(|dir| wire::parent_dir(&dir.current_path))
                                .map(str::to_owned)
                        }) {
                            state.browse(path);
                        }
                    },
                ))
                .child(action(
                    "settings-folder-select",
                    "使用当前目录",
                    no_folder,
                    true,
                    move || {
                        if let Some(path) = state.directory.with_untracked(|directory| {
                            directory.as_ref().map(|dir| dir.current_path.clone())
                        }) {
                            match kind {
                                FolderUse::Workspace => {
                                    edit(state.product, WORKSPACE_DIR, Value::String(path))
                                }
                                FolderUse::Repository => state.repo_path.set(path),
                            }
                            state.folder.set(None);
                        }
                    },
                )),
        )
        .child(
            label(move || {
                state.directory.with(|directory| {
                    directory
                        .as_ref()
                        .map_or_else(String::new, |dir| dir.current_path.clone())
                })
            })
            .app_class("font-mono"),
        )
        .child(scroll().app_class("flex-1 min-h-0").child(dynamic(move || {
            view()
                .app_class("gap-1 items-start")
                .children(state.directory.with(|directory| {
                    directory
                        .as_ref()
                        .into_iter()
                        .flat_map(|dir| &dir.entries)
                        .filter(|entry| entry.is_directory)
                        .map(|entry| {
                            let path = entry.path.clone();
                            action(
                                "settings-folder-entry",
                                entry.name.clone(),
                                state.busy,
                                false,
                                move || state.browse(path.clone()),
                            )
                        })
                        .collect::<Vec<_>>()
                }))
        })))
}

fn save_bar(state: State) -> View {
    let draft = move || {
        if state.page.get() == Page::Repos {
            state.repo
        } else {
            state.product
        }
    };
    let unavailable = move || state.busy.get() || !dirty(draft());
    let invalid = move || {
        state.page.get() != Page::Repos && prefix_error(&string(state.product, PREFIX)).is_some()
    };
    view()
        .app_class("border-t border-border bg-sidebar p-4 shrink-0 gap-2")
        .child(dynamic(move || {
            if let Some((error, message)) = state.status.get() {
                label(message).app_class(if error {
                    "text-status-error"
                } else {
                    "text-status-ok"
                })
            } else if state.busy.get() {
                label("正在处理…").app_class("text-muted-foreground")
            } else if dirty(draft()) {
                label("有未保存的修改").app_class("text-muted-foreground")
            } else {
                label("修改仅应用于本机").app_class("text-muted-foreground")
            }
        }))
        .child(when(
            move || state.folder.get().is_none() && state.confirm.with(|value| value.is_none()),
            move || {
                view()
                    .app_class("flex-row gap-2")
                    .child(action(
                        "settings-save",
                        "保存设置",
                        move || unavailable() || invalid(),
                        true,
                        move || {
                            if state.page.get_untracked() == Page::Repos {
                                state.save_repo();
                            } else {
                                state.save_product();
                            }
                        },
                    ))
                    .child(action(
                        "settings-discard",
                        "放弃修改",
                        unavailable,
                        false,
                        move || {
                            draft().update(|draft| {
                                if let Some(draft) = draft {
                                    draft.discard();
                                }
                            });
                            state.status.set(None);
                        },
                    ))
            },
        ))
}

fn dirty(draft: Signal<Option<Draft>>) -> bool {
    draft.with(|draft| draft.as_ref().is_some_and(Draft::dirty))
}

fn string(draft: Signal<Option<Draft>>, path: &str) -> String {
    draft.with(|draft| {
        draft
            .as_ref()
            .and_then(|draft| draft.get(path).as_str())
            .unwrap_or_default()
            .to_owned()
    })
}

fn edit(draft: Signal<Option<Draft>>, path: &'static str, value: Value) {
    draft.update(|draft| {
        if let Some(draft) = draft {
            draft.edit(path, value);
        }
    });
}
