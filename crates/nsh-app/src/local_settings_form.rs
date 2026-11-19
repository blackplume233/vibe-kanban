use nsh_base::{SelectEntry, checkbox, select};
use nsh_core::element::{Input, IntoProp, Text, input};

use super::*;

const CONTROL: &str = "min-h-9 px-2 rounded-(--radius) border border-input text-foreground focus:outline-2 focus:outline-ring focus:outline-offset-2";
const EDITOR_TYPE: &str = "/editor/editor_type";
const SSH_HOST: &str = "/editor/remote_ssh_host";
const PR_PROMPT: &str = "/pr_auto_description_prompt";
const COMMIT_PROMPT: &str = "/commit_reminder_prompt";
const COMMIT_REMINDER: &str = "/commit_reminder_enabled";
const SOUND: &str = "/notifications/sound_enabled";

pub(super) fn label(value: impl IntoReactive<SharedString>) -> Text {
    text(value).app_class("text-body text-foreground")
}

fn hint(value: impl IntoReactive<SharedString>) -> Text {
    text(value).app_class("text-meta text-muted-foreground")
}

pub(super) fn action(
    id: &'static str,
    title: impl Into<SharedString>,
    disabled: impl IntoReactive<bool>,
    primary: bool,
    run: impl Fn() + 'static,
) -> View {
    let title = title.into();
    view().test_id(id).label(title.clone()).role(AccessibilityRole::Button).disabled(disabled)
        .app_class(if primary { "min-h-9 px-2 items-center justify-center rounded-(--radius) border border-primary bg-primary disabled:opacity-50 focus:outline-2 focus:outline-ring focus:outline-offset-2" } else { "min-h-9 px-2 items-center justify-center rounded-(--radius) border border-input hover:bg-surface-hover disabled:opacity-50 focus:outline-2 focus:outline-ring focus:outline-offset-2" })
        .child(text(title).app_class(if primary { "text-body text-primary-foreground" } else { "text-body text-foreground" }))
        .on_press(move |_: &()| run())
}

pub(super) fn tabs(
    page: Signal<Page>,
    disabled: impl Fn() -> bool + Copy + 'static,
    switch: impl Fn(Page) + Copy + 'static,
) -> View {
    view().app_class("flex-row flex-wrap gap-2").children(
        [
            (Page::Defaults, "settings-defaults", "产品默认值"),
            (Page::EditorGit, "settings-editor-git", "编辑器与 Git"),
            (Page::Repos, "settings-repos", "仓库"),
        ]
        .map(|(target, id, title)| {
            action(id, title, disabled, false, move || switch(target)).bg(move || {
                nsh_base::theme()
                    .color(if page.get() == target {
                        "accent"
                    } else {
                        "sidebar"
                    })
                    .expect("settings theme color")
            })
        }),
    )
}

pub(super) fn card(title: &'static str) -> View {
    view()
        .app_class("bg-card border border-border rounded-card p-4 gap-3 shrink-0")
        .child(label(title).app_class("text-title font-semibold"))
}

pub(super) fn field(title: &'static str, help: &'static str, control: impl IntoElement) -> View {
    let body = view()
        .app_class("gap-1")
        .child(label(title).app_class("font-semibold"));
    let body = if help.is_empty() {
        body
    } else {
        body.child(hint(help))
    };
    body.child(control)
}

pub(super) fn input_field(
    id: &'static str,
    title: &'static str,
    value: impl IntoReactive<String>,
    locked: impl IntoReactive<bool>,
    multiline: bool,
    mono: bool,
    change: impl Fn(&str) + 'static,
) -> Input {
    let control = input()
        .test_id(id)
        .label(title)
        .value(value)
        .disabled(locked)
        .multiline(multiline)
        .on_change(move |value: &str| change(value));
    let extra = match (multiline, mono) {
        (true, true) => " min-h-20 py-2 font-mono",
        (true, false) => " min-h-20 py-2",
        (false, true) => " font-mono",
        _ => "",
    };
    control.app_class(format!("{CONTROL} w-full bg-background text-body{extra}"))
}

pub(super) fn choice<Items, Disabled>(
    id: &'static str,
    title: &'static str,
    items: impl IntoProp<Vec<SelectItem>, Items>,
    selected: impl Fn() -> Option<SharedString> + Copy + 'static,
    selected_title: impl Fn() -> SharedString + 'static,
    disabled: impl IntoProp<bool, Disabled>,
    searchable: bool,
    change: impl Fn(&str) + 'static,
) -> impl IntoElement {
    select().items(items).value(selected).accessibility_label(title).disabled(disabled)
        .searchable(searchable).search_placeholder("搜索选项").search_accessibility_label("搜索选项")
        .popup_class("p-1 bg-popover border border-input rounded-card shadow-lg")
        .search_class(CONTROL).list_class("gap-1")
        .item_class("px-2 min-h-9 justify-center rounded-(--radius) hover:bg-surface-hover focus:bg-accent selected:bg-accent")
        .render_item(|item: &SelectEntry| label(item.title.clone()))
        .empty(hint("没有匹配的选项"))
        .trigger(view().test_id(id).app_class("min-h-9 px-2 rounded-(--radius) border border-input bg-background flex-row items-center justify-between gap-2 hover:bg-surface-hover disabled:opacity-50 focus:outline-2 focus:outline-ring focus:outline-offset-2")
            .child(label(selected_title)).child(hint("▾")))
        .on_change(move |next: &Option<SharedString>| { if let Some(next) = next { change(next); } })
}

fn enum_field(
    state: State,
    path: &'static str,
    id: &'static str,
    title: &'static str,
    options: &'static [(&'static str, &'static str)],
) -> View {
    let read = move || string(state.product, path);
    field(
        title,
        "",
        choice(
            id,
            title,
            options
                .iter()
                .map(|&(value, label)| SelectItem::new(value, label))
                .collect::<Vec<_>>(),
            move || Some(read().into()),
            move || {
                let value = read();
                options
                    .iter()
                    .find(|(key, _)| *key == value)
                    .map_or_else(|| value.into(), |&(_, name)| name.into())
            },
            state.busy,
            false,
            move |value| edit(state.product, path, json!(value)),
        ),
    )
}

fn toggle<Checked, Disabled>(
    title: &'static str,
    checked: impl IntoProp<bool, Checked>,
    disabled: impl IntoProp<bool, Disabled>,
    change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    checkbox().accessibility_label(title).checked(checked).disabled(disabled)
        .app_class("flex-row items-center min-h-9 gap-2 rounded-(--radius) disabled:opacity-50 focus:outline-2 focus:outline-ring focus:outline-offset-2")
        .indicator_class("size-4 border border-input rounded-(--radius) items-center justify-center checked:bg-primary")
        .indicator(text("✓").app_class("text-primary-foreground"))
        .child(label(title)).on_change(move |next: &bool| change(*next))
}

fn truth(draft: Signal<Option<Draft>>, path: &str) -> bool {
    draft.with(|draft| {
        draft
            .as_ref()
            .and_then(|draft| draft.get(path).as_bool())
            .unwrap_or(false)
    })
}

fn toggle_field(state: State, path: &'static str, title: &'static str) -> impl IntoElement {
    toggle(
        title,
        move || truth(state.product, path),
        state.busy,
        move |next| edit(state.product, path, Value::Bool(next)),
    )
}

fn has_value(draft: Signal<Option<Draft>>, path: &str) -> bool {
    draft.with(|draft| {
        draft
            .as_ref()
            .is_some_and(|draft| !draft.get(path).is_null())
    })
}

fn optional_field(
    draft: Signal<Option<Draft>>,
    locked: Signal<bool>,
    path: &'static str,
    id: &'static str,
    title: &'static str,
    help: &'static str,
    multiline: bool,
    mono: bool,
) -> View {
    field(
        title,
        help,
        input_field(
            id,
            title,
            move || string(draft, path),
            locked,
            multiline,
            mono,
            move |value| edit(draft, path, optional(value)),
        ),
    )
}

fn custom_prompt(state: State, path: &'static str, id: &'static str, title: &'static str) -> View {
    view()
        .app_class("gap-2")
        .child(toggle(
            "使用自定义提示词",
            move || has_value(state.product, path),
            state.busy,
            move |enabled| {
                edit(
                    state.product,
                    path,
                    if enabled { json!("") } else { Value::Null },
                );
            },
        ))
        .child(when(
            move || has_value(state.product, path),
            move || {
                field(
                    title,
                    "",
                    input_field(
                        id,
                        title,
                        move || string(state.product, path),
                        state.busy,
                        true,
                        false,
                        move |value| edit(state.product, path, json!(value)),
                    ),
                )
            },
        ))
}

pub(super) fn defaults(state: State) -> View {
    view()
        .app_class("gap-3")
        .child(
            card("外观与输入")
                .child(enum_field(
                    state,
                    "/theme",
                    "settings-theme",
                    "外观",
                    &[("SYSTEM", "跟随系统"), ("LIGHT", "亮色"), ("DARK", "暗色")],
                ))
                .child(enum_field(
                    state,
                    "/language",
                    "settings-language",
                    "首选语言",
                    &[
                        ("BROWSER", "跟随系统"),
                        ("EN", "English"),
                        ("FR", "Français"),
                        ("JA", "日本語"),
                        ("ES", "Español"),
                        ("KO", "한국어"),
                        ("ZH_HANS", "简体中文"),
                        ("ZH_HANT", "繁體中文"),
                    ],
                ))
                .child(enum_field(
                    state,
                    "/send_message_shortcut",
                    "settings-send-shortcut",
                    "发送消息快捷键",
                    &[("ModifierEnter", "Ctrl / ⌘ + Enter"), ("Enter", "Enter")],
                )),
        )
        .child(card("默认 Agent").child(executor_choices(state)))
        .child(
            card("Pull Request 与提交")
                .child(toggle_field(
                    state,
                    "/pr_auto_description_enabled",
                    "自动生成 Pull Request 描述",
                ))
                .child(custom_prompt(
                    state,
                    PR_PROMPT,
                    "settings-pr-prompt",
                    "Pull Request 描述提示词",
                ))
                .child(toggle_field(
                    state,
                    COMMIT_REMINDER,
                    "提醒 Agent 提交未提交的修改",
                ))
                .child(when(
                    move || truth(state.product, COMMIT_REMINDER),
                    move || {
                        custom_prompt(
                            state,
                            COMMIT_PROMPT,
                            "settings-commit-prompt",
                            "提交提醒提示词",
                        )
                    },
                )),
        )
        .child(
            card("通知")
                .child(toggle_field(state, SOUND, "提示音"))
                .child(when(
                    move || truth(state.product, SOUND),
                    move || {
                        enum_field(
                            state,
                            "/notifications/sound_file",
                            "settings-sound",
                            "提示音效",
                            &[
                                ("ABSTRACT_SOUND1", "提示音 1"),
                                ("ABSTRACT_SOUND2", "提示音 2"),
                                ("ABSTRACT_SOUND3", "提示音 3"),
                                ("ABSTRACT_SOUND4", "提示音 4"),
                                ("COW_MOOING", "牛叫"),
                                ("FAHHHHH", "Fahhhhh"),
                                ("PHONE_VIBRATION", "手机振动"),
                                ("ROOSTER", "鸡鸣"),
                            ],
                        )
                    },
                ))
                .child(toggle_field(
                    state,
                    "/notifications/push_enabled",
                    "系统通知",
                )),
        )
}

fn executor_choices(state: State) -> View {
    let value = move || string(state.product, EXECUTOR);
    let variant = move || string(state.product, VARIANT);
    view()
        .app_class("gap-3")
        .child(field(
            "默认执行器",
            "用于新建工作区。",
            choice(
                "settings-executor",
                "默认执行器",
                move || {
                    state.profiles.with(|profiles| {
                        profiles
                            .keys()
                            .map(|key| SelectItem::new(key.clone(), key.replace('_', " ")))
                            .collect::<Vec<_>>()
                    })
                },
                move || Some(value().into()),
                move || value().replace('_', " ").into(),
                move || state.busy.get() || state.profiles.with(BTreeMap::is_empty),
                true,
                move |executor| {
                    let old_variant = variant();
                    state.product.update(|draft| {
                        if let Some(draft) = draft {
                            draft.edit(EXECUTOR, json!(executor));
                            let compatible = state.profiles.with_untracked(|profiles| {
                                profiles
                                    .get(executor)
                                    .is_some_and(|profile| profile.has_variant(&old_variant))
                            });
                            if !compatible {
                                draft.edit(VARIANT, Value::Null);
                            }
                        }
                    });
                },
            ),
        ))
        .child(field(
            "默认配置变体",
            "",
            choice(
                "settings-variant",
                "默认配置变体",
                move || {
                    state.profiles.with(|profiles| {
                        let mut items = vec![SelectItem::new("", "默认")];
                        if let Some(profile) = profiles.get(&value()) {
                            items.extend(
                                profile
                                    .variants()
                                    .filter(|variant| *variant != "DEFAULT")
                                    .map(|variant| {
                                        let value = SharedString::from(variant.to_owned());
                                        SelectItem::new(value.clone(), value)
                                    }),
                            );
                        }
                        items
                    })
                },
                move || Some(variant().into()),
                move || {
                    let value = variant();
                    if value.is_empty() {
                        "默认".into()
                    } else {
                        value.into()
                    }
                },
                move || {
                    state.busy.get()
                        || state.profiles.with(|profiles| {
                            profiles
                                .get(&value())
                                .is_none_or(|profile| profile.variants().next().is_none())
                        })
                },
                false,
                move |value| edit(state.product, VARIANT, optional(value)),
            ),
        ))
}

pub(super) fn editor_git(state: State) -> View {
    let git_prefix = move || string(state.product, PREFIX);
    let editor = move || string(state.product, EDITOR_TYPE);
    view()
        .app_class("gap-3")
        .child(
            card("编辑器")
                .child(enum_field(
                    state,
                    EDITOR_TYPE,
                    "settings-editor",
                    "默认编辑器",
                    &[
                        ("VS_CODE", "VS Code"),
                        ("VS_CODE_INSIDERS", "VS Code Insiders"),
                        ("CURSOR", "Cursor"),
                        ("WINDSURF", "Windsurf"),
                        ("INTELLI_J", "IntelliJ IDEA"),
                        ("ZED", "Zed"),
                        ("XCODE", "Xcode"),
                        ("GOOGLE_ANTIGRAVITY", "Google Antigravity"),
                        ("CUSTOM", "自定义命令"),
                    ],
                ))
                .child(when(
                    move || editor() == "CUSTOM",
                    move || {
                        optional_field(
                            state.product,
                            state.busy,
                            "/editor/custom_command",
                            "settings-editor-command",
                            "启动命令",
                            "由本机后端执行。",
                            false,
                            true,
                        )
                    },
                ))
                .child(when(
                    move || {
                        matches!(
                            editor().as_str(),
                            "VS_CODE" | "CURSOR" | "WINDSURF" | "GOOGLE_ANTIGRAVITY" | "ZED"
                        )
                    },
                    move || {
                        view()
                            .app_class("gap-3")
                            .child(optional_field(
                                state.product,
                                state.busy,
                                SSH_HOST,
                                "settings-editor-ssh-host",
                                "SSH 主机",
                                "留空时使用本机编辑器。",
                                false,
                                true,
                            ))
                            .child(when(
                                move || has_value(state.product, SSH_HOST),
                                move || {
                                    optional_field(
                                        state.product,
                                        state.busy,
                                        "/editor/remote_ssh_user",
                                        "settings-editor-ssh-user",
                                        "SSH 用户",
                                        "",
                                        false,
                                        true,
                                    )
                                },
                            ))
                    },
                ))
                .child(when(
                    move || matches!(editor().as_str(), "VS_CODE" | "VS_CODE_INSIDERS" | "CURSOR"),
                    move || {
                        toggle_field(
                            state,
                            "/editor/auto_install_extension",
                            "自动安装编辑器扩展",
                        )
                    },
                )),
        )
        .child(
            card("Git 分支与工作区")
                .child(field(
                    "分支前缀",
                    "留空时不添加前缀。",
                    input_field(
                        "settings-prefix",
                        "分支前缀",
                        git_prefix,
                        state.busy,
                        false,
                        true,
                        move |value| edit(state.product, PREFIX, json!(value.trim())),
                    ),
                ))
                .child(dynamic(move || {
                    let prefix = git_prefix();
                    if let Some(error) = prefix_error(&prefix) {
                        label(error).app_class("text-status-error text-meta")
                    } else {
                        hint(if prefix.is_empty() {
                            "示例：vo-014-update-settings".to_owned()
                        } else {
                            format!("示例：{prefix}/vo-014-update-settings")
                        })
                        .app_class("font-mono")
                    }
                }))
                .child(optional_field(
                    state.product,
                    state.busy,
                    WORKSPACE_DIR,
                    "settings-workspace-dir",
                    "工作区目录",
                    "留空时使用后端的默认目录。",
                    false,
                    true,
                ))
                .child(view().app_class("items-start").child(action(
                    "settings-browse-workspace",
                    "浏览本机目录",
                    state.busy,
                    false,
                    move || state.open_folder(FolderUse::Workspace),
                ))),
        )
}

pub(super) fn repos(state: State) -> View {
    let blocked = move || state.busy.get() || dirty(state.repo);
    let repo_id = move || string(state.repo, "/id");
    let can_register = move || blocked() || state.repo_path.with(|path| path.trim().is_empty());
    view()
        .app_class("gap-3")
        .child(
            card("本机仓库")
                .child(field(
                    "已注册仓库",
                    "",
                    choice(
                        "settings-repo-choice",
                        "已注册仓库",
                        move || {
                            state.repos.with(|repos| {
                                repos
                                    .as_ref()
                                    .into_iter()
                                    .flatten()
                                    .map(|repo| {
                                        SelectItem::new(repo.id.clone(), repo.display_name.clone())
                                    })
                                    .collect::<Vec<_>>()
                            })
                        },
                        move || {
                            let id = repo_id();
                            (!id.is_empty()).then(|| id.into())
                        },
                        move || {
                            let display = string(state.repo, "/display_name");
                            if !display.is_empty() {
                                return display.into();
                            }
                            let empty = state
                                .repos
                                .with(|repos| repos.as_ref().is_none_or(Vec::is_empty));
                            if empty {
                                "尚无已注册仓库".into()
                            } else {
                                "选择仓库".into()
                            }
                        },
                        move || {
                            blocked()
                                || state
                                    .repos
                                    .with(|repos| repos.as_ref().is_none_or(Vec::is_empty))
                        },
                        true,
                        move |id| state.select_repo(id.to_owned()),
                    ),
                ))
                .child(when(
                    move || dirty(state.repo),
                    || hint("请先保存或放弃修改，再切换或添加仓库。"),
                ))
                .child(field(
                    "Git 仓库路径",
                    "填写后端所在机器上的路径。",
                    input_field(
                        "settings-repo-register-path",
                        "Git 仓库路径",
                        state.repo_path,
                        blocked,
                        false,
                        true,
                        move |value| state.repo_path.set(value.to_owned()),
                    ),
                ))
                .child(
                    view()
                        .app_class("flex-row flex-wrap gap-2")
                        .child(action(
                            "settings-repo-browse",
                            "浏览本机目录",
                            blocked,
                            false,
                            move || state.open_folder(FolderUse::Repository),
                        ))
                        .child(action(
                            "settings-repo-register",
                            "添加仓库",
                            can_register,
                            true,
                            move || state.register(),
                        )),
                ),
        )
        .child(when(
            move || state.repo.with(Option::is_some),
            move || repo_fields(state),
        ))
}

fn repo_fields(state: State) -> View {
    let opt = move |path, id, title, help, multiline| {
        optional_field(
            state.repo, state.busy, path, id, title, help, multiline, true,
        )
    };
    let branch_value = move || string(state.repo, REPO_BRANCH);
    view()
        .app_class("gap-3")
        .child(
            card("仓库默认值")
                .child(opt(
                    "/display_name",
                    "settings-repo-name",
                    "显示名称",
                    "",
                    false,
                ))
                .child(field(
                    "仓库路径",
                    "",
                    label(move || string(state.repo, "/path")).app_class("font-mono"),
                ))
                .child(opt(
                    "/default_working_dir",
                    "settings-repo-working-dir",
                    "默认工作目录",
                    "相对于工作树根目录。",
                    false,
                ))
                .child(field(
                    "默认目标分支",
                    "未选择时使用仓库当前分支。",
                    choice(
                        "settings-repo-branch",
                        "默认目标分支",
                        move || {
                            state
                                .branches
                                .with(|branches| branches.as_ref().cloned().unwrap_or_default())
                        },
                        move || Some(branch_value().into()),
                        move || {
                            let value = branch_value();
                            if value.is_empty() {
                                "使用当前分支".into()
                            } else {
                                value.into()
                            }
                        },
                        move || state.busy.get() || state.branches.with(Result::is_err),
                        true,
                        move |value| edit(state.repo, REPO_BRANCH, optional(value)),
                    ),
                ))
                .child(when(
                    move || state.branches.with(Result::is_err),
                    move || {
                        view()
                            .app_class("gap-2 items-start")
                            .child(
                                label(move || {
                                    state.branches.with(|result| {
                                        result.as_ref().err().map_or_else(String::new, |error| {
                                            format!("读取分支失败：{error}")
                                        })
                                    })
                                })
                                .app_class("text-status-error"),
                            )
                            .child(action(
                                "settings-repo-branch-retry",
                                "重新读取分支",
                                move || state.busy.get() || dirty(state.repo),
                                false,
                                move || state.select_repo(string(state.repo, "/id")),
                            ))
                    },
                ))
                .child(view().app_class("items-start").child(action(
                    "settings-repo-remove",
                    "移除仓库",
                    state.busy,
                    false,
                    move || state.confirm.set(Some(Confirm::RemoveRepo)),
                ))),
        )
        .child(
            card("工作区脚本")
                .child(opt(
                    "/dev_server_script",
                    "settings-repo-dev",
                    "开发服务器脚本",
                    "启动用于预览的开发服务器。",
                    true,
                ))
                .child(opt(
                    "/setup_script",
                    "settings-repo-setup",
                    "初始化脚本",
                    "创建工作区时执行。",
                    true,
                ))
                .child(toggle(
                    "初始化脚本与 Agent 并行执行",
                    move || truth(state.repo, "/parallel_setup_script"),
                    move || {
                        state.busy.get() || string(state.repo, "/setup_script").trim().is_empty()
                    },
                    move |next| edit(state.repo, "/parallel_setup_script", Value::Bool(next)),
                ))
                .child(opt(
                    "/cleanup_script",
                    "settings-repo-cleanup",
                    "清理脚本",
                    "执行清理操作时运行。",
                    true,
                ))
                .child(opt(
                    "/archive_script",
                    "settings-repo-archive",
                    "归档脚本",
                    "归档工作区前执行。",
                    true,
                ))
                .child(opt(
                    "/copy_files",
                    "settings-repo-copy",
                    "复制文件",
                    "填写逗号分隔的相对路径。",
                    true,
                )),
        )
}
