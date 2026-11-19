use std::collections::BTreeMap;

use nsh_base::{checkbox, input};
use nsh_core::prelude::*;

use super::{
    super::{
        Action, State, run_action, submit_review,
        wire::{Approval, Question, QuestionAnswer},
    },
    action_button,
};
use crate::api::Api;

const FIELD: &str = "px-3 py-2 bg-background border border-input rounded-md text-sm focus-visible:ring-2 focus-visible:ring-ring";

pub(super) fn review_button(state: State) -> impl IntoElement {
    action_button("代码评审", false)
        .disabled(move || !state.can_review())
        .on_press(move || {
            if let Some(id) = state.selected.get() {
                state.update(&id, |data| {
                    data.editor.review.get_or_insert_with(Default::default);
                });
            }
        })
}

pub(super) fn review_form(api: Api, state: State) -> impl IntoElement {
    when(
        move || state.with(|data| data.is_some_and(|data| data.editor.review.is_some())),
        move || {
            let api = api.clone();
            let include = move || {
                state.with(|data| {
                    data.and_then(|data| data.editor.review.as_ref())
                        .is_some_and(|draft| draft.use_all_workspace_commits)
                })
            };
            let busy = move || state.with(|data| data.is_some_and(|data| data.editor.busy));
            view().app_class(Some("border border-border rounded-md p-3 gap-2 bg-muted".into()))
                .child(text("评审当前会话的代码变更").app_class(Some("font-semibold text-sm".into())))
                .child(input().app_class(Some(format!("min-h-14 {FIELD}").into()))
                    .multiline(true).label("评审说明").placeholder("附加说明（可选）")
                    .disabled(busy)
                    .value(move || state.with(|data| data.and_then(|data| data.editor.review.as_ref())
                        .map(|draft| draft.instructions.clone()).unwrap_or_default()))
                    .on_change(move |value: &str| {
                        if let Some(id) = state.selected.get() {
                            state.update(&id, |data| {
                                if let Some(draft) = &mut data.editor.review { draft.instructions = value.to_owned(); }
                            });
                        }
                    }))
                .child(checkbox()
                    .accessibility_label("评审工作区的所有提交")
                    .app_class(Some("flex-row items-center gap-2 min-h-9 px-2 border border-input rounded-md focus-visible:ring-2 focus-visible:ring-ring".into()))
                    .checked(include).disabled(busy)
                    .on_change(move |checked: &bool| {
                        if let Some(id) = state.selected.get() {
                            state.update(&id, |data| {
                                if let Some(draft) = &mut data.editor.review { draft.use_all_workspace_commits = *checked; }
                            });
                        }
                    })
                    .child(text(move || if include() { "✓  评审工作区的所有提交" } else { "□  评审工作区的所有提交" })))
                .child(view().app_class(Some("flex-row gap-2".into()))
                    .child(action_button("开始评审", true)
                        .disabled(move || !state.can_review()).on_press(move || submit_review(api.clone(), state)))
                    .child(action_button("取消评审", false).disabled(busy).on_press(move || {
                        if let Some(id) = state.selected.get() { state.update(&id, |data| data.editor.review = None); }
                    })))
        },
    )
}

#[derive(Clone, Default, PartialEq)]
struct AnswerDraft {
    selected: Vec<usize>,
    custom: String,
}

impl AnswerDraft {
    fn select(&mut self, index: usize, multiple: bool) {
        self.custom.clear();
        if let Some(position) = self.selected.iter().position(|&selected| selected == index) {
            self.selected.remove(position);
        } else {
            if !multiple {
                self.selected.clear();
            }
            self.selected.push(index);
        }
    }

    fn complete(&self, question: &Question) -> bool {
        !self.custom.trim().is_empty()
            || self
                .selected
                .iter()
                .any(|&index| index < question.options.len())
    }

    fn answer(&self, question: &Question) -> QuestionAnswer {
        let custom = self.custom.trim();
        let answer = if custom.is_empty() {
            self.selected
                .iter()
                .filter_map(|&index| {
                    question
                        .options
                        .get(index)
                        .map(|option| option.label.clone())
                })
                .collect()
        } else {
            vec![custom.to_owned()]
        };
        QuestionAnswer {
            question: question.question.clone(),
            answer,
        }
    }
}

pub(super) fn question_form(
    api: Api,
    state: State,
    approval: ReadSignal<Approval>,
) -> impl IntoElement {
    let questions = computed(move || {
        approval.with(|approval| {
            state.with(|data| {
                data.and_then(|data| data.transcript.logs.get(&approval.execution_process_id))
                    .and_then(|logs| {
                        logs.values()
                            .rev()
                            .filter_map(|entry| entry.questions.as_ref())
                            .find(|entry| entry.approval_id == approval.approval_id)
                    })
                    .map(|entry| entry.questions.clone())
                    .unwrap_or_default()
            })
        })
    });
    let drafts = signal(BTreeMap::<usize, AnswerDraft>::new());
    let complete = move || {
        questions.with(|questions| {
            !questions.is_empty()
                && drafts.with(|drafts| {
                    questions.iter().enumerate().all(|(index, question)| {
                        drafts
                            .get(&index)
                            .is_some_and(|draft| draft.complete(question))
                    })
                })
        })
    };
    view()
        .app_class(Some("gap-2".into()))
        .child(when(
            move || questions.with(Vec::is_empty),
            || text("正在等待问题内容…").app_class(Some("text-xs text-muted-foreground".into())),
        ))
        .child(when(
            move || !questions.with(Vec::is_empty),
            move || {
                let api = api.clone();
                view()
                    .app_class(Some("gap-2".into()))
                    .child(scroll().app_class(Some("max-h-64".into())).child(
                        view().app_class(Some("gap-3 p-1".into())).child(each(
                            move || questions.get().into_iter().enumerate().collect::<Vec<_>>(),
                            |(index, question)| (*index, question.question.clone()),
                            move |row, _| question_row(row, state, drafts),
                        )),
                    ))
                    .child(
                        action_button("提交回答", true)
                            .disabled(move || state.approval_disabled() || !complete())
                            .on_press(move || {
                                if state.approval_disabled() || !complete() {
                                    return;
                                }
                                let Some(id) = state.selected.get() else {
                                    return;
                                };
                                let answers = questions.with(|questions| {
                                    drafts.with(|drafts| {
                                        questions
                                            .iter()
                                            .enumerate()
                                            .map(|(index, question)| {
                                                drafts[&index].answer(question)
                                            })
                                            .collect()
                                    })
                                });
                                run_action(
                                    api.clone(),
                                    state,
                                    id,
                                    Action::Answer {
                                        approval: approval.get(),
                                        answers,
                                    },
                                );
                            }),
                    )
            },
        ))
}

fn question_row(
    row: ReadSignal<(usize, Question)>,
    state: State,
    drafts: Signal<BTreeMap<usize, AnswerDraft>>,
) -> impl IntoElement {
    let index = row.with(|(index, _)| *index);
    view()
        .app_class(Some("gap-1".into()))
        .child(
            text(move || row.with(|(_, question)| question.header.clone()))
                .app_class(Some("text-xs text-muted-foreground".into())),
        )
        .child(
            text(move || row.with(|(_, question)| question.question.clone()))
                .app_class(Some("text-sm font-semibold".into())),
        )
        .child(when(
            move || row.with(|(_, question)| question.multi_select),
            || text("可选择多项").app_class(Some("text-xs text-muted-foreground".into())),
        ))
        .child(each(
            move || {
                row.with(|(_, question)| {
                    question
                        .options
                        .iter()
                        .cloned()
                        .enumerate()
                        .collect::<Vec<_>>()
                })
            },
            |(index, option)| (*index, option.label.clone()),
            move |option, _| {
                let option_index = option.with(|(index, _)| *index);
                let selected = move || {
                    drafts.with(|drafts| {
                        drafts.get(&index).is_some_and(|draft| {
                            draft.custom.trim().is_empty() && draft.selected.contains(&option_index)
                        })
                    })
                };
                button_option(option, selected)
                    .disabled(move || state.approval_disabled())
                    .on_press(move || {
                        drafts.update(|drafts| {
                            drafts.entry(index).or_default().select(
                                option_index,
                                row.with(|(_, question)| question.multi_select),
                            );
                        })
                    })
            },
        ))
        .child(
            input()
                .app_class(Some(format!("min-h-9 {FIELD}").into()))
                .label(format!("问题 {} 的自定义回答", index + 1))
                .placeholder("输入自定义回答")
                .disabled(move || state.approval_disabled())
                .value(move || {
                    drafts.with(|drafts| {
                        drafts
                            .get(&index)
                            .map(|draft| draft.custom.clone())
                            .unwrap_or_default()
                    })
                })
                .on_change(move |value: &str| {
                    drafts
                        .update(|drafts| drafts.entry(index).or_default().custom = value.to_owned())
                }),
        )
}

fn button_option(
    option: ReadSignal<(usize, super::super::wire::QuestionOption)>,
    selected: impl Fn() -> bool + 'static,
) -> nsh_base::ButtonElement {
    nsh_base::button()
        .app_class(Some("bg-card border border-input p-2 rounded-md focus-visible:ring-2 focus-visible:ring-ring selected:bg-accent".into()))
        .selected(selected)
        .child(text(move || option.with(|(_, option)| option.label.clone())))
        .child(text(move || option.with(|(_, option)| option.description.clone()))
            .app_class(Some("text-xs text-muted-foreground".into())))
}

#[cfg(test)]
mod tests {
    use super::{super::super::wire::QuestionOption, *};

    #[test]
    fn answer_supports_single_multi_and_custom_values() {
        let question = Question {
            question: "Which checks?".to_owned(),
            header: "Checks".to_owned(),
            options: ["Build", "Tests"]
                .into_iter()
                .map(|label| QuestionOption {
                    label: label.to_owned(),
                    description: String::new(),
                })
                .collect(),
            multi_select: true,
        };
        let mut draft = AnswerDraft::default();
        assert!(!draft.complete(&question));
        draft.select(0, false);
        draft.select(1, false);
        assert_eq!(draft.answer(&question).answer, ["Tests"]);
        draft.select(0, question.multi_select);
        assert_eq!(draft.answer(&question).answer, ["Tests", "Build"]);
        draft.custom = "  Manual inspection  ".to_owned();
        assert_eq!(draft.answer(&question).answer, ["Manual inspection"]);
        draft.select(1, question.multi_select);
        assert_eq!(draft.answer(&question).answer, ["Build"]);
    }
}
