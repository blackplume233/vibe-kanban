#[path = "agent_settings_data.rs"]
mod data;

use std::{collections::BTreeMap, rc::Rc};

use data::{Preset, ProfileChange, Profiles};
use nsh_base::dialog;
use nsh_core::{
    element::{Owner, input},
    prelude::*,
};
use serde_json::Value;

use crate::api::Api;

#[derive(Clone, PartialEq)]
enum Screen {
    Registry,
    Profile(Preset),
    Mcp(String),
}

impl Screen {
    fn executor(&self) -> Option<&str> {
        match self {
            Self::Registry => None,
            Self::Profile(preset) => Some(&preset.executor),
            Self::Mcp(executor) => Some(executor),
        }
    }
}

#[derive(Clone, PartialEq)]
struct McpEditor {
    path: String,
    servers_path: Vec<String>,
    providers: BTreeMap<String, Value>,
}

#[derive(Clone, PartialEq)]
struct Editor {
    baseline: String,
    mcp: Option<McpEditor>,
}

enum Operation {
    Load(Screen),
    Adapter {
        id: String,
        executor: String,
        install: bool,
    },
    Preset(Preset, ProfileChange),
    SaveMcp(String),
}

struct Confirmation {
    message: String,
    action: &'static str,
    callback: Box<dyn Fn()>,
}

#[derive(Clone)]
struct Panel {
    owner: Owner,
    api: Api,
    notify_profiles: Rc<dyn Fn()>,
    screen: Signal<Screen>,
    selection: Signal<Option<Preset>>,
    profiles: Signal<Option<Profiles>>,
    agents: Signal<Vec<data::Agent>>,
    editor: Signal<Option<Editor>>,
    draft: Signal<String>,
    busy: Signal<bool>,
    status: Signal<Option<Result<&'static str, String>>>,
    confirm: Signal<Option<Rc<Confirmation>>>,
    create: Signal<Option<bool>>,
    name: Signal<String>,
    filter: Signal<String>,
}

/// Mount once in the settings view. The callback invalidates external profile pickers.
pub(crate) fn agent_settings(
    api: Api,
    on_profiles_changed: impl Fn() + 'static,
) -> impl IntoElement {
    let panel = Panel {
        owner: Owner::current(),
        api,
        notify_profiles: Rc::new(on_profiles_changed),
        screen: signal(Screen::Registry),
        selection: signal(None),
        profiles: signal(None),
        agents: signal(Vec::new()),
        editor: signal(None),
        draft: signal(String::new()),
        busy: signal(false),
        status: signal(None),
        confirm: signal_with(None, SignalEquals::Never),
        create: signal(None),
        name: signal(String::new()),
        filter: signal(String::new()),
    };
    panel.run(Operation::Load(Screen::Registry));
    let navigation = panel.clone();
    let content = panel.clone();
    let confirmation = panel.clone();
    view()
        .test_id("agent-settings")
        .flex_1()
        .min_h_0()
        .bg(|| color("background"))
        .font_family("Source Sans 3, Microsoft YaHei UI, Noto Sans CJK SC, sans-serif")
        .child(
            view()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(|| color("border"))
                .child(label("Agent 设置", "foreground").text_size(px(24.0))),
        )
        .child(
            view()
                .flex_1()
                .min_h_0()
                .flex_row()
                .child(navigation.navigation())
                .child(
                    view()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(dynamic(move || content.content())),
                ),
        )
        .child(
            view()
                .px_4()
                .py_2()
                .min_h(px(36.0))
                .border_t_1()
                .border_color(|| color("border"))
                .child(dynamic(move || {
                    if panel.busy.get() {
                        return label("正在与本地服务通信…", "muted-foreground");
                    }
                    panel.status.with(|status| match status {
                        Some(Ok(message)) => label(*message, "status-ok"),
                        Some(Err(message)) => label(message.clone(), "status-error"),
                        None => label("本地 Agent 配置", "muted-foreground"),
                    })
                })),
        )
        .child(dynamic(move || confirmation.confirmation()))
}

impl Panel {
    fn dirty(&self) -> bool {
        self.editor.with(|editor| {
            editor
                .as_ref()
                .is_some_and(|editor| self.draft.with(|draft| *draft != editor.baseline))
        })
    }

    fn error(&self, error: String) {
        self.status.set(Some(Err(error)));
    }

    fn ask(&self, message: String, action: &'static str, callback: impl Fn() + 'static) {
        if !self.busy.get() {
            self.confirm.set(Some(Rc::new(Confirmation {
                message,
                action,
                callback: Box::new(callback),
            })));
        }
    }

    fn navigate(&self, screen: Screen) {
        if self.busy.get() {
            return;
        }
        if self.dirty() {
            let panel = self.clone();
            self.ask(
                "当前修改尚未保存。要放弃修改并继续吗？".into(),
                "放弃并继续",
                move || {
                    panel.run(Operation::Load(screen.clone()));
                },
            );
        } else {
            self.run(Operation::Load(screen));
        }
    }

    fn run(&self, operation: Operation) {
        if self.busy.get() {
            return;
        }
        self.busy.set(true);
        self.status.set(None);
        let panel = self.clone();
        self.owner.run(move || {
            spawn(async move {
                match panel.execute(operation).await {
                    Ok(message) => panel.status.set(message.map(Ok)),
                    Err(error) => panel.error(error),
                }
                panel.busy.set(false);
            });
        });
    }

    async fn execute(&self, operation: Operation) -> Result<Option<&'static str>, String> {
        match operation {
            Operation::Load(screen) => {
                self.load(screen).await?;
                Ok(None)
            }
            Operation::Adapter {
                id,
                executor,
                install,
            } => {
                data::set_installed(&self.api, &id, install).await?;
                (self.notify_profiles)();
                self.agents.update(|agents| {
                    if let Some(agent) = agents.iter_mut().find(|agent| agent.id == id) {
                        agent.installed = install;
                    }
                });
                self.load(if install {
                    Screen::Profile(Preset {
                        executor,
                        variant: "DEFAULT".into(),
                    })
                } else {
                    Screen::Registry
                })
                .await
                .map_err(|error| format!("适配器状态已保存，但刷新失败。{error}"))?;
                Ok(Some(if install {
                    "适配器已安装，可选择预设。"
                } else {
                    "适配器已卸载。"
                }))
            }
            Operation::Preset(target, change) => {
                let deleted = matches!(change, ProfileChange::Delete { .. });
                data::save_profile(&self.api, &target, change).await?;
                (self.notify_profiles)();
                let next = if deleted {
                    Preset {
                        executor: target.executor,
                        variant: "DEFAULT".into(),
                    }
                } else {
                    target
                };
                self.load(Screen::Profile(next))
                    .await
                    .map_err(|error| format!("预设已保存，但刷新失败。{error}"))?;
                Ok(Some(if deleted {
                    "预设已删除。"
                } else {
                    "预设已保存。"
                }))
            }
            Operation::SaveMcp(executor) => {
                let mut edited = self.draft.with(|draft| data::parse_mcp(draft))?;
                let path = self
                    .editor
                    .with(|editor| {
                        editor
                            .as_ref()
                            .and_then(|editor| editor.mcp.as_ref())
                            .map(|mcp| mcp.servers_path.clone())
                    })
                    .ok_or("请重新载入 MCP 配置。")?;
                data::save_mcp(&self.api, &executor, data::mcp_servers(&mut edited, &path)?)
                    .await?;
                let baseline = self.draft.get();
                self.editor.update(|editor| {
                    if let Some(editor) = editor {
                        editor.baseline = baseline;
                    }
                });
                Ok(Some("MCP 服务配置已保存。"))
            }
        }
    }

    async fn load(&self, mut screen: Screen) -> Result<(), String> {
        self.editor.set(None);
        self.draft.set(String::new());
        self.create.set(None);
        self.screen.set(screen.clone());
        let profiles = data::profiles(&self.api).await?;
        let editor = match &mut screen {
            Screen::Registry => Ok(None),
            Screen::Profile(target) => {
                if profiles.variant(target).is_none() {
                    target.variant = "DEFAULT".into();
                }
                self.selection.set(Some(target.clone()));
                profiles
                    .variant(target)
                    .ok_or_else(|| "执行器已移除或缺少预设。".into())
                    .and_then(data::preset_body)
                    .map(|(_, body)| {
                        Some(Editor {
                            baseline: data::pretty(body),
                            mcp: None,
                        })
                    })
            }
            Screen::Mcp(_) => Ok(None),
        };
        self.screen.set(screen.clone());
        self.profiles.set(Some(profiles));
        let editor = match screen {
            Screen::Registry => {
                let mut agents = data::agents(&self.api).await?;
                agents.sort_unstable_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
                self.agents.set(agents);
                None
            }
            Screen::Mcp(executor) => {
                let response = data::mcp(&self.api, &executor).await?;
                let (baseline, servers_path, providers) = response.mcp_config.editor()?;
                Some(Editor {
                    baseline,
                    mcp: Some(McpEditor {
                        path: response.config_path,
                        servers_path,
                        providers,
                    }),
                })
            }
            Screen::Profile(_) => editor?,
        };
        if let Some(editor) = &editor {
            self.draft.set(editor.baseline.clone());
        }
        self.editor.set(editor);
        Ok(())
    }

    fn show_profile(&self, executor: String) {
        let selected = self.selection.with(|selected| {
            selected
                .as_ref()
                .filter(|selected| selected.executor == executor)
                .cloned()
        });
        self.navigate(Screen::Profile(selected.unwrap_or(Preset {
            executor,
            variant: "DEFAULT".into(),
        })));
    }

    fn change_adapter(&self, id: String, executor: String, name: &str, install: bool) {
        let panel = self.clone();
        let message = if install {
            format!("安装 {name} 并将它添加到本地 Agent 预设？")
        } else {
            format!("卸载 {name}？此适配器的本地预设将被删除。")
        };
        self.ask(
            message,
            if install {
                "确认安装"
            } else {
                "确认卸载"
            },
            move || {
                panel.run(Operation::Adapter {
                    id: id.clone(),
                    executor: executor.clone(),
                    install,
                });
            },
        );
    }

    fn save_preset(&self, target: &Preset) {
        let change = self.profiles.with(|profiles| -> Result<_, String> {
            let before = profiles
                .as_ref()
                .and_then(|profiles| profiles.variant(target))
                .ok_or("请重新载入预设。")?;
            let after = self
                .draft
                .with(|draft| data::edited_preset(before, draft))?;
            Ok(ProfileChange::Save {
                before: Some(before.clone()),
                after,
            })
        });
        match change {
            Ok(change) => self.run(Operation::Preset(target.clone(), change)),
            Err(error) => self.error(error),
        }
    }

    fn delete_preset(&self, target: Preset) {
        let panel = self.clone();
        self.ask(
            format!("删除预设 {} / {}？", target.executor, target.variant),
            "确认删除",
            move || {
                let before = panel.profiles.with(|profiles| {
                    profiles
                        .as_ref()
                        .and_then(|profiles| profiles.variant(&target))
                        .cloned()
                });
                if let Some(before) = before {
                    panel.run(Operation::Preset(
                        target.clone(),
                        ProfileChange::Delete { before },
                    ));
                }
            },
        );
    }

    fn create_preset(&self, selected: &Preset, clone: bool) {
        let create = || -> Result<_, String> {
            let variant = self.name.with(|name| data::preset_name(name))?;
            let target = Preset {
                executor: selected.executor.clone(),
                variant,
            };
            let after = self.profiles.with(|profiles| {
                let profiles = profiles.as_ref().ok_or("请重新载入预设。")?;
                if profiles.variant(&target).is_some() {
                    return Err("该名称的预设已存在。".into());
                }
                let base = profiles.variant(selected).ok_or("请重新载入预设。")?;
                if clone {
                    Ok(base.clone())
                } else {
                    data::blank_preset(base)
                }
            })?;
            Ok((
                target,
                ProfileChange::Save {
                    before: None,
                    after,
                },
            ))
        };
        match create() {
            Ok((target, change)) => self.run(Operation::Preset(target, change)),
            Err(error) => self.error(error),
        }
    }

    fn save_mcp(&self, executor: String) {
        let result = self.editor.with(|editor| -> Result<_, String> {
            let mcp = editor
                .as_ref()
                .and_then(|editor| editor.mcp.as_ref())
                .ok_or("请重新载入 MCP 配置。")?;
            let mut draft = self.draft.with(|draft| data::parse_mcp(draft))?;
            data::mcp_servers(&mut draft, &mcp.servers_path)?;
            Ok(())
        });
        match result {
            Ok(()) => {
                let panel = self.clone();
                self.ask(
                    format!("将 MCP 服务配置写入 {executor} 的本地文件？"),
                    "确认写入",
                    move || {
                        panel.run(Operation::SaveMcp(executor.clone()));
                    },
                );
            }
            Err(error) => self.error(error),
        }
    }

    fn add_mcp_provider(&self, provider: &str) {
        let result = self.editor.with(|editor| -> Result<_, String> {
            let mcp = editor
                .as_ref()
                .and_then(|editor| editor.mcp.as_ref())
                .ok_or("请重新载入 MCP 配置。")?;
            let value = mcp.providers.get(provider).ok_or("未找到 MCP 模板。")?;
            let mut draft = self.draft.with(|draft| data::parse_mcp(draft))?;
            let servers = data::mcp_servers(&mut draft, &mcp.servers_path)?;
            if servers.contains_key(provider) {
                return Err("已包含同名 MCP 服务。".into());
            }
            servers.insert(provider.to_owned(), value.clone());
            Ok(data::pretty(&draft))
        });
        match result {
            Ok(draft) => self.draft.set(draft),
            Err(error) => self.error(error),
        }
    }

    fn navigation(&self) -> View {
        let registry = self.clone();
        let items = self.clone();
        let screen = self.screen;
        view()
            .w(px(208.0))
            .flex_shrink_0()
            .min_h_0()
            .p_4()
            .gap_3()
            .bg(|| color("sidebar"))
            .border_r_1()
            .border_color(|| color("border"))
            .child(action(
                "ACP 适配器",
                self.busy,
                move || screen.with(|s| *s == Screen::Registry),
                move || registry.navigate(Screen::Registry),
            ))
            .child(label("本地执行器", "muted-foreground").text_size(px(12.0)))
            .child(scroll().flex_1().min_h_0().child(dynamic(move || {
                items.profiles.with(|profiles| {
                    profiles
                        .as_ref()
                        .map(|profiles| {
                            profiles.executors().fold(view().gap_2(), |list, executor| {
                                let panel = items.clone();
                                let selected = executor.to_owned();
                                let key = selected.clone();
                                list.child(action(
                                    executor.to_owned(),
                                    items.busy,
                                    move || screen.with(|screen| screen.executor() == Some(&key)),
                                    move || panel.show_profile(selected.clone()),
                                ))
                            })
                        })
                        .unwrap_or_else(view)
                })
            })))
    }

    fn content(&self) -> View {
        let screen = self.screen.get();
        let refresh = self.clone();
        let target = screen.clone();
        let title = match &screen {
            Screen::Registry => "ACP 适配器",
            Screen::Profile(target) => &target.executor,
            Screen::Mcp(executor) => executor,
        };
        let mut area = view().flex_1().min_h_0().p_4().gap_3().child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    label(title.to_owned(), "foreground")
                        .text_size(px(18.0))
                        .flex_1(),
                )
                .child(action(
                    "重新载入",
                    self.busy,
                    || false,
                    move || refresh.navigate(target.clone()),
                )),
        );
        if let Some(executor) = screen.executor() {
            let profile = self.clone();
            let mcp = self.clone();
            let profile_key = executor.to_owned();
            let mcp_key = profile_key.clone();
            let is_mcp = matches!(screen, Screen::Mcp(_));
            area = area.child(
                view()
                    .flex_row()
                    .gap_2()
                    .child(action(
                        "Agent 预设",
                        self.busy,
                        move || !is_mcp,
                        move || profile.show_profile(profile_key.clone()),
                    ))
                    .child(action(
                        "MCP 配置",
                        self.busy,
                        move || is_mcp,
                        move || mcp.navigate(Screen::Mcp(mcp_key.clone())),
                    )),
            );
        }
        match screen {
            Screen::Registry => area.child(self.registry()),
            Screen::Profile(target) => area.child(self.profile(&target)),
            Screen::Mcp(executor) => area.child(self.mcp_content(executor)),
        }
    }

    fn registry(&self) -> View {
        let panel = self.clone();
        let filter = self.filter;
        view()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                field()
                    .test_id("acp-search")
                    .value(filter)
                    .placeholder("搜索适配器")
                    .on_change(move |value: &str| filter.set(value.to_owned())),
            )
            .child(scroll().flex_1().min_h_0().child(dynamic(move || {
                let filter = filter.with(|filter| filter.trim().to_lowercase());
                panel.agents.with(|agents| {
                    let mut rows = view().gap_2();
                    let mut visible = false;
                    for agent in agents.iter().filter(|agent| {
                        filter.is_empty()
                            || agent.name.to_lowercase().contains(&filter)
                            || agent.id.to_lowercase().contains(&filter)
                    }) {
                        visible = true;
                        let busy = panel.busy;
                        let selected = panel.clone();
                        let id = agent.id.clone();
                        let executor = agent.executor.clone();
                        let name = agent.name.clone();
                        let install = !agent.installed;
                        rows = rows.child(
                            card()
                                .flex_row()
                                .gap_3()
                                .items_center()
                                .child(
                                    view()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .child(
                                            label(agent.name.clone(), "foreground")
                                                .text_size(px(16.0)),
                                        )
                                        .child(
                                            label(
                                                format!(
                                                    "{}  ·  {}  ·  {}",
                                                    agent.id,
                                                    agent.version,
                                                    if agent.installed {
                                                        "已安装"
                                                    } else {
                                                        "可安装"
                                                    }
                                                ),
                                                "muted-foreground",
                                            )
                                            .text_size(px(12.0)),
                                        )
                                        .child(label(
                                            agent.description.clone(),
                                            "muted-foreground",
                                        )),
                                )
                                .child(action(
                                    if install { "安装" } else { "卸载" },
                                    busy,
                                    || false,
                                    move || {
                                        selected.change_adapter(
                                            id.clone(),
                                            executor.clone(),
                                            &name,
                                            install,
                                        )
                                    },
                                )),
                        );
                    }
                    if !visible && !panel.busy.get() {
                        rows = rows.child(label("没有可显示的适配器。", "muted-foreground"));
                    }
                    rows
                })
            })))
    }

    fn profile(&self, target: &Preset) -> View {
        let mut row = view().flex_row().flex_wrap().gap_2();
        if let Some(profiles) = self.profiles.with(|profiles| {
            profiles.as_ref().map(|p| {
                p.variants(&target.executor)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
        }) {
            for variant in profiles {
                let selection = Preset {
                    executor: target.executor.clone(),
                    variant,
                };
                let selected = selection == *target;
                let panel = self.clone();
                row = row.child(action(
                    selection.variant.clone(),
                    self.busy,
                    move || selected,
                    move || panel.navigate(Screen::Profile(selection.clone())),
                ));
            }
        }
        let mut area = view().flex_1().min_h_0().gap_3().child(row);
        if !self.editor.with(Option::is_some) {
            return area;
        }
        if let Some(path) = self.profiles.with(|p| p.as_ref().map(|p| p.path.clone())) {
            area = area.child(
                label(path, "muted-foreground")
                    .text_size(px(12.0))
                    .truncate(),
            );
        }
        let save = self.clone();
        let selected = target.clone();
        let new_panel = self.clone();
        let new_target = target.clone();
        let mut tools = view()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .child(self.save_button(move || save.save_preset(&selected)))
            .child(self.discard_button())
            .child(self.create_button(false))
            .child(self.create_button(true));
        if target.variant != "DEFAULT" {
            let delete = self.clone();
            let selected = target.clone();
            tools = tools.child(self.clean_action("删除预设", move || {
                delete.delete_preset(selected.clone())
            }));
        }
        area.child(self.json_editor("Agent 预设 JSON"))
            .child(tools)
            .child(dynamic(move || new_panel.create_form(&new_target)))
    }

    fn create_button(&self, clone: bool) -> View {
        let panel = self.clone();
        self.clean_action(
            if clone {
                "复制预设"
            } else {
                "新建空预设"
            },
            move || {
                panel.name.set(String::new());
                panel.create.set(Some(clone));
            },
        )
    }

    fn create_form(&self, target: &Preset) -> Option<View> {
        let clone = self.create.get()?;
        let panel = self.clone();
        let selection = target.clone();
        let name = self.name;
        let create = self.create;
        Some(
            card()
                .gap_2()
                .child(label(
                    if clone {
                        "复制当前已保存预设"
                    } else {
                        "新建空预设"
                    },
                    "foreground",
                ))
                .child(
                    field()
                        .test_id("agent-preset-name")
                        .label("预设名称")
                        .value(name)
                        .max_length(40)
                        .readonly(self.busy)
                        .auto_focus(true)
                        .placeholder("预设名称，例如 REVIEW")
                        .on_change(move |value: &str| name.set(value.to_owned())),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child(self.clean_action("创建预设", move || {
                            panel.create_preset(&selection, clone)
                        }))
                        .child(action(
                            "取消",
                            self.busy,
                            || false,
                            move || create.set(None),
                        )),
                ),
        )
    }

    fn mcp_content(&self, executor: String) -> View {
        let mut area = view().flex_1().min_h_0().gap_3();
        let Some((path, servers_path, providers)) = self.editor.with(|editor| {
            let mcp = editor.as_ref()?.mcp.as_ref()?;
            Some((
                mcp.path.clone(),
                mcp.servers_path.join("."),
                mcp.providers.keys().cloned().collect::<Vec<_>>(),
            ))
        }) else {
            return area;
        };
        let save = self.clone();
        area = area
            .child(
                label(path, "muted-foreground")
                    .text_size(px(12.0))
                    .truncate(),
            )
            .child(label(
                format!("只写入 {servers_path}，保留文件中的其他配置。"),
                "muted-foreground",
            ))
            .child(self.json_editor("MCP 配置 JSON"))
            .child(
                view()
                    .flex_row()
                    .gap_2()
                    .child(self.save_button(move || save.save_mcp(executor.clone())))
                    .child(self.discard_button()),
            );
        if !providers.is_empty() {
            let mut row = view()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .items_center()
                .child(label("添加模板", "muted-foreground"));
            for provider in providers {
                let panel = self.clone();
                row = row.child(action(
                    provider.clone(),
                    self.busy,
                    || false,
                    move || panel.add_mcp_provider(&provider),
                ));
            }
            area = area.child(row);
        }
        area
    }

    fn json_editor(&self, name: &'static str) -> impl IntoElement {
        let panel = self.clone();
        let draft = self.draft;
        let status = self.status;
        field()
            .flex_1()
            .min_h(px(140.0))
            .multiline(true)
            .label(name)
            .test_id("agent-settings-json")
            .font_family("IBM Plex Mono, Microsoft YaHei UI, monospace")
            .readonly(move || panel.busy.get() || panel.confirm.with(Option::is_some))
            .value(draft)
            .on_change(move |value: &str| {
                draft.set(value.to_owned());
                status.set(None);
            })
    }

    fn save_button(&self, save: impl Fn() + 'static) -> View {
        let panel = self.clone();
        button(
            "保存",
            move || panel.busy.get() || !panel.dirty(),
            || true,
            save,
        )
    }

    fn discard_button(&self) -> View {
        let panel = self.clone();
        let restore = self.clone();
        button(
            "撤销修改",
            move || panel.busy.get() || !panel.dirty(),
            || false,
            move || {
                if let Some(value) = restore
                    .editor
                    .with(|editor| editor.as_ref().map(|editor| editor.baseline.clone()))
                {
                    restore.draft.set(value);
                    restore.status.set(None);
                }
            },
        )
    }

    fn clean_action(&self, title: &'static str, pressed: impl Fn() + 'static) -> View {
        let panel = self.clone();
        button(
            title,
            move || panel.busy.get() || panel.dirty(),
            || false,
            pressed,
        )
    }

    fn confirmation(&self) -> Option<nsh_core::element::Modal> {
        let pending = self.confirm.get()?;
        let confirm = self.confirm;
        Some(
            dialog()
                .flex_1()
                .animation(if cfg!(windows) {
                    ModalAnimation::Fade
                } else {
                    ModalAnimation::None
                })
                .on_cancel(move || confirm.set(None))
                .child(
                    view()
                        .absolute()
                        .inset_0()
                        .bg(|| color("background").opacity(0.72)),
                )
                .child(
                    view().m(auto()).p_4().child(
                        card()
                            .w(px(420.0))
                            .gap_3()
                            .bg(|| color("popover"))
                            .child(label("确认操作", "foreground").text_size(px(18.0)))
                            .child(label(pending.message.clone(), "foreground"))
                            .child(
                                view()
                                    .flex_row()
                                    .justify_end()
                                    .gap_2()
                                    .child(action(
                                        "取消",
                                        self.busy,
                                        || false,
                                        move || confirm.set(None),
                                    ))
                                    .child(action(
                                        pending.action,
                                        self.busy,
                                        || true,
                                        move || {
                                            confirm.set(None);
                                            (pending.callback)();
                                        },
                                    )),
                            ),
                    ),
                ),
        )
    }
}

fn color(role: &str) -> AdaptiveColor {
    nsh_base::theme()
        .color(role)
        .expect("generated design color role")
}

fn label(value: impl IntoReactive<SharedString>, role: &'static str) -> nsh_core::element::Text {
    text(value)
        .text_size(px(14.0))
        .text_color(move || color(role))
}

fn card() -> View {
    view()
        .p_3()
        .rounded(px(6.0))
        .bg(|| color("card"))
        .border_1()
        .border_color(|| color("border"))
}

fn field() -> nsh_core::element::Input {
    input()
        .min_h(px(36.0))
        .p_2()
        .rounded(px(4.0))
        .border_1()
        .border_color(|| color("input"))
        .bg(|| color("card"))
        .text_color(|| color("foreground"))
        .text_size(px(14.0))
        .auto_correct(false)
        .focus(|style| {
            style
                .outline(px(2.0))
                .outline_offset(px(2.0))
                .outline_color(|| color("ring"))
        })
}

fn action(
    title: impl Into<SharedString>,
    busy: Signal<bool>,
    selected: impl Fn() -> bool + Clone + 'static,
    pressed: impl Fn() + 'static,
) -> View {
    button(title, move || busy.get(), selected, pressed)
}

fn button(
    title: impl Into<SharedString>,
    disabled: impl Fn() -> bool + Clone + 'static,
    selected: impl Fn() -> bool + Clone + 'static,
    pressed: impl Fn() + 'static,
) -> View {
    let title = title.into();
    let pressed = Rc::new(pressed);
    // Rebuild the button with a concrete disabled state. Win32 can retain
    // stale reactive accessibility attributes for buttons rebuilt under a
    // loading view.
    view().child(dynamic(move || {
        let inactive = disabled();
        let selected = selected();
        let title = title.clone();
        let pressed = pressed.clone();
        let disabled = disabled.clone();
        view()
            .min_h(px(36.0))
            .justify_center()
            .px_3()
            .rounded(px(4.0))
            .border_1()
            .border_color(|| color("input"))
            .role(AccessibilityRole::Button)
            .label(title.clone())
            .disabled(inactive)
            .bg(move || color(if selected { "accent" } else { "card" }))
            .hover(|style| style.bg(|| color("surface-hover")))
            .focus(|style| {
                style
                    .outline(px(2.0))
                    .outline_offset(px(2.0))
                    .outline_color(|| color("ring"))
            })
            .on_press(move || {
                if !disabled() {
                    pressed();
                }
            })
            .child(
                text(title)
                    .text_size(px(14.0))
                    .text_color(move || {
                        color(if inactive {
                            "muted-foreground"
                        } else {
                            "foreground"
                        })
                    })
                    .truncate(),
            )
    }))
}
