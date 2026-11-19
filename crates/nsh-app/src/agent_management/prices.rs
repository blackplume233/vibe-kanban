use nsh_core::{element::View, prelude::*};

use super::{Read, actions, api, button, card, field, heading, note, pane, stack, token_metrics};
use crate::api::Api;

const FIELDS: [&str; 6] = [
    "执行器",
    "模型 ID",
    "输入费率",
    "输出费率",
    "缓存读取费率",
    "缓存写入费率",
];

#[derive(Clone, PartialEq)]
enum Edit {
    New,
    Existing { executor: String, model: String },
}

#[derive(Clone, Copy)]
pub(super) struct PricePane {
    rows: Read<Vec<api::Price>>,
    edit: Signal<Option<Edit>>,
    draft: [Signal<String>; 6],
    confirm_delete: Signal<bool>,
}

impl PricePane {
    pub fn new() -> Self {
        Self {
            rows: Read::new(),
            edit: signal(None),
            draft: std::array::from_fn(|_| signal(String::new())),
            confirm_delete: signal(false),
        }
    }

    pub fn refresh(self, api: &Api) {
        let api = api.clone();
        self.rows
            .owner
            .run(|| self.rows.load(async move { api::prices(&api).await }));
    }

    pub fn view(self, api: &Api, writing: Signal<bool>) -> View {
        let refresh_api = api.clone();
        let edit_api = api.clone();
        let locked = move || writing.get() || !self.rows.writable();
        pane()
            .child(
                actions()
                    .child(heading("模型价目表", 18.0).flex_1())
                    .child(button(
                        "刷新价格",
                        move || writing.get() || self.rows.busy(),
                        || false,
                        move || {
                            self.refresh(&refresh_api);
                        },
                    ))
                    .child(button(
                        "新建价格",
                        locked,
                        || false,
                        move || self.open(None),
                    )),
            )
            .child(note(
                "费率单位为 USD / 百万 token。模型 ID 填 * 可作为该执行器的默认费率。",
            ))
            .child(note("新费率只用于之后入库的样本，历史费用保留原值。"))
            .child(self.rows.message())
            .child(dynamic(move || {
                let edit = self.edit.get()?;
                Some(self.editor(&edit_api, writing, edit))
            }))
            .child(dynamic(move || {
                let locked = locked();
                self.rows.data.with(|data| {
                    let Some(rows) = data else { return view() };
                    if rows.is_empty() {
                        return view().child(note("尚未配置模型价格。"));
                    }
                    stack(rows.iter().map(|row| {
                        let item = row.clone();
                        card()
                            .child(
                                actions()
                                    .child(
                                        heading(format!("{} / {}", row.executor, row.model), 14.0)
                                            .flex_1(),
                                    )
                                    .child(button(
                                        "编辑价格",
                                        locked,
                                        || false,
                                        move || self.open(Some(&item)),
                                    )),
                            )
                            .child(token_metrics(row.rates()))
                    }))
                })
            }))
    }

    fn editor(self, api: &Api, writing: Signal<bool>, edit: Edit) -> View {
        let locked = move || writing.get() || !self.rows.writable();
        let save_api = api.clone();
        let delete_api = api.clone();
        let existing = matches!(edit, Edit::Existing { .. });
        let fields = FIELDS
            .into_iter()
            .zip(self.draft)
            .fold(actions(), |row, (label, draft)| {
                row.child(
                    view()
                        .gap_1()
                        .min_w(px(136.0))
                        .flex_1()
                        .child(note(label))
                        .child(field(label, draft, locked, move |text| {
                            draft.set(text.to_owned())
                        })),
                )
            });
        card()
            .child(heading(
                if existing {
                    "编辑费率"
                } else {
                    "新建费率"
                },
                14.0,
            ))
            .child(fields)
            .child(
                actions()
                    .child(button(
                        "保存价格",
                        locked,
                        || false,
                        move || self.save(&save_api, writing, false),
                    ))
                    .child(existing.then(|| {
                        button(
                            "删除价格",
                            locked,
                            || false,
                            move || self.confirm_delete.set(true),
                        )
                    }))
                    .child(button("取消", writing, || false, move || self.close())),
            )
            .child(dynamic(move || {
                self.confirm_delete.get().then(|| {
                    let api = delete_api.clone();
                    actions()
                        .child(note("从价目表移除此条费率？"))
                        .child(button(
                            "确认删除",
                            locked,
                            || false,
                            move || self.save(&api, writing, true),
                        ))
                        .child(button(
                            "保留费率",
                            writing,
                            || false,
                            move || self.confirm_delete.set(false),
                        ))
                })
            }))
    }

    fn open(self, row: Option<&api::Price>) {
        let fields = row.map_or_else(
            || ["", "", "", "", "", ""].map(str::to_owned),
            api::Price::fields,
        );
        for (draft, value) in self.draft.into_iter().zip(fields) {
            draft.set(value);
        }
        self.confirm_delete.set(false);
        self.edit
            .set(Some(row.map_or(Edit::New, |row| Edit::Existing {
                executor: row.executor.clone(),
                model: row.model.clone(),
            })));
    }

    fn close(self) {
        self.confirm_delete.set(false);
        self.edit.set(None);
    }

    fn save(self, api: &Api, writing: Signal<bool>, delete: bool) {
        let Some(edit) = self.edit.get() else { return };
        let Some(baseline) = self.rows.data.get() else {
            return;
        };
        let price = if delete {
            None
        } else {
            match api::Price::from_fields(&self.draft.map(|draft| draft.get())) {
                Ok(price) => Some(price),
                Err(error) => return self.rows.invalid(error),
            }
        };
        let write_api = api.clone();
        let read_api = api.clone();
        self.rows.save(
            writing,
            async move {
                // PUT replaces the whole table; refuse to overwrite a table changed since it loaded.
                let mut latest = api::prices(&write_api).await?;
                if latest != baseline {
                    return Err("价目表已变更，请刷新后核对再保存。".into());
                }
                if let Edit::Existing { executor, model } = edit {
                    let Some(index) = latest
                        .iter()
                        .position(|row| row.executor == executor && row.model == model)
                    else {
                        return Err("此费率已不存在，请刷新价目表。".into());
                    };
                    if let Some(price) = price {
                        latest[index] = price;
                    } else {
                        latest.remove(index);
                    }
                } else if let Some(price) = price {
                    latest.push(price);
                }
                api::distinct_prices(&latest)?;
                api::save_prices(&write_api, &latest).await
            },
            async move { api::prices(&read_api).await },
            move || self.close(),
        );
    }
}
