use nsh_core::{element::View, prelude::*};

use super::{Read, actions, api, button, card, code, field, heading, note, pane, stack};
use crate::api::Api;

#[derive(Clone, Copy)]
pub(super) struct QueuePane {
    entries: Read<Vec<api::QueuedExecution>>,
    quota: Read<api::Quota>,
    quota_draft: Signal<Option<String>>,
    priority_entry: Signal<Option<SharedString>>,
    priority_draft: Signal<String>,
}

impl QueuePane {
    pub fn new() -> Self {
        Self {
            entries: Read::new(),
            quota: Read::new(),
            quota_draft: signal(None),
            priority_entry: signal(None),
            priority_draft: signal(String::new()),
        }
    }

    pub fn refresh(self, api: &Api) {
        let queue_api = api.clone();
        let quota_api = api.clone();
        self.entries.owner.run(|| {
            self.entries
                .load(async move { api::queue(&queue_api).await });
            self.quota.load(async move { api::quota(&quota_api).await });
        });
    }

    pub fn view(
        self,
        api: &Api,
        session: Signal<Option<SharedString>>,
        writing: Signal<bool>,
    ) -> View {
        let refresh_api = api.clone();
        let quota_api = api.clone();
        let priority_api = api.clone();
        let cannot_refresh = move || writing.get() || self.entries.busy() || self.quota.busy();
        let quota_locked = move || writing.get() || !self.quota.writable();
        let queue_locked = move || writing.get() || !self.entries.writable();
        let selected_priority = move || {
            let target = self.priority_entry.get();
            self.entries.data.with(|entries| {
                entries
                    .as_ref()?
                    .iter()
                    .find(|row| Some(row.execution_process_id.as_str()) == target.as_deref())
                    .map(|row| row.priority)
            })
        };
        pane()
            .child(
                actions()
                    .child(heading("全局执行队列", 18.0).flex_1())
                    .child(button(
                        "刷新队列",
                        cannot_refresh,
                        || false,
                        move || self.refresh(&refresh_api),
                    )),
            )
            .child(
                card()
                    .child(heading("并发配额", 14.0))
                    .child(note("为本机调度器设置全局并发上限，最少 1 个执行槽位。"))
                    .child(
                        actions()
                            .child(
                                field(
                                    "全局并发上限",
                                    move || {
                                        self.quota_draft
                                            .get()
                                            .or_else(|| {
                                                self.quota.data.with(|quota| {
                                                    quota.map(|value| {
                                                        value.max_concurrent.to_string()
                                                    })
                                                })
                                            })
                                            .unwrap_or_default()
                                    },
                                    quota_locked,
                                    move |value| self.quota_draft.set(Some(value.to_owned())),
                                )
                                .w(px(148.0)),
                            )
                            .child(button(
                                "保存配额",
                                move || quota_locked() || self.quota_draft.with(Option::is_none),
                                || false,
                                move || {
                                    self.save_quota(&quota_api, writing);
                                },
                            )),
                    )
                    .child(self.quota.message()),
            )
            .child(
                actions().child(
                    heading(
                        move || {
                            self.entries.data.with(|entries| {
                                entries.as_ref().map_or_else(
                                    || "等待执行".into(),
                                    |entries| format!("等待执行 · {} 项", entries.len()),
                                )
                            })
                        },
                        14.0,
                    )
                    .flex_1(),
                ),
            )
            .child(note(
                "较高优先级先派发。同优先级按入队顺序。此列表只显示等待项。",
            ))
            .child(self.entries.message())
            .child(dynamic(move || {
                let target = self.priority_entry.get()?;
                let current = selected_priority();
                let api = priority_api.clone();
                Some(
                    card()
                        .child(heading("调整优先级", 14.0))
                        .child(code(format!("执行 {target}")))
                        .child(if let Some(current) = current {
                            note(format!("当前优先级 · {current}"))
                        } else {
                            note("此执行已不在等待队列中。")
                        })
                        .child(
                            actions()
                                .child(
                                    field(
                                        "新优先级",
                                        self.priority_draft,
                                        move || queue_locked() || current.is_none(),
                                        move |value| self.priority_draft.set(value.to_owned()),
                                    )
                                    .w(px(148.0)),
                                )
                                .child(button(
                                    "保存优先级",
                                    move || queue_locked() || current.is_none(),
                                    || false,
                                    move || {
                                        self.save_priority(&api, writing, target.clone());
                                    },
                                ))
                                .child(button(
                                    "取消",
                                    writing,
                                    || false,
                                    move || self.priority_entry.set(None),
                                )),
                        ),
                )
            }))
            .child(dynamic(move || {
                let locked = queue_locked();
                self.entries.data.with(|data| {
                    let Some(rows) = data else { return view() };
                    if rows.is_empty() {
                        return view().child(note("没有等待执行的任务。"));
                    }
                    stack(rows.iter().enumerate().map(|(index, row)| {
                        let id: SharedString = row.execution_process_id.clone().into();
                        let selection = id.clone();
                        let priority = row.priority;
                        let same_session = session
                            .with(|session| session.as_deref() == Some(row.session_id.as_str()));
                        card()
                            .child(
                                actions()
                                    .child(
                                        heading(
                                            format!("#{} · 优先级 {}", index + 1, priority),
                                            14.0,
                                        )
                                        .flex_1(),
                                    )
                                    .child(button(
                                        "编辑优先级",
                                        locked,
                                        || false,
                                        move || {
                                            self.priority_draft.set(priority.to_string());
                                            self.priority_entry.set(Some(selection.clone()));
                                        },
                                    )),
                            )
                            .child(code(format!("执行 {id}")))
                            .child(code(format!(
                                "会话 {}{}",
                                row.session_id,
                                if same_session { " · 当前会话" } else { "" }
                            )))
                            .child(code(format!("工作区 {}", row.workspace_id)))
                            .child(note(format!("入队 {}", row.queued_at)))
                    }))
                })
            }))
    }

    fn save_quota(self, api: &Api, writing: Signal<bool>) {
        let value = self
            .quota_draft
            .with(|draft| draft.as_deref().map(api::parse_quota));
        let Some(value) = value else { return };
        let value = match value {
            Ok(value) => value,
            Err(error) => return self.quota.invalid(error),
        };
        let write_api = api.clone();
        let read_api = api.clone();
        self.quota.save(
            writing,
            async move { api::save_quota(&write_api, value).await },
            async move { api::quota(&read_api).await },
            move || self.quota_draft.set(None),
        );
    }

    fn save_priority(self, api: &Api, writing: Signal<bool>, id: SharedString) {
        let priority = match self.priority_draft.with(|draft| api::parse_priority(draft)) {
            Ok(priority) => priority,
            Err(error) => return self.entries.invalid(error),
        };
        let write_api = api.clone();
        let read_api = api.clone();
        self.entries.save(
            writing,
            async move { api::save_priority(&write_api, &id, priority).await },
            async move { api::queue(&read_api).await },
            move || self.priority_entry.set(None),
        );
    }
}
