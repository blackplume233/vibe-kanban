use std::{cell::RefCell, collections::HashMap, rc::Rc};

use nsh_base::{CommandEntry, CommandItem, command, dialog};
use nsh_core::{
    Bounds, Point, Points,
    element::{
        AnyElement, Composition, KeyOptions, Owner, PanEvent, SignalEquals, input, scroll,
        signal_with,
    },
    keymap::When,
    prelude::*,
};

use crate::{
    Api,
    api::Workspace,
    board_state::{BoardEffect, BoardState},
    command_search::{MAX_RESULTS, SearchEntity, SearchEntry, search},
    issue_actions::{
        Action, ActionContext, ActionId, ActionScope, ActionSource, BoardIssue, EntityId,
        FocusAnchor, InputState, Invocation, IssueEdit, IssueId, IssueRef, ProjectId,
        RelativePosition, StatusId, StatusRef, WorkspaceId,
    },
    issues_api::{
        Cycle, Issue, IssueFilter, Project, SaveCycle, SaveStatus, SaveView, SavedView,
        StatusCategory,
    },
};

mod launch;

const ROW_HEIGHT: f32 = 44.0;
type Id = SharedString;

#[derive(Clone, Copy)]
struct BoardSlot(Signal<Rc<RefCell<Option<BoardState>>>>);

impl BoardSlot {
    fn new() -> Self {
        Self(signal_with(
            Rc::new(RefCell::new(None)),
            SignalEquals::Never,
        ))
    }

    fn with<R>(self, read: impl FnOnce(&Option<BoardState>) -> R) -> R {
        read(&self.0.get().borrow())
    }

    fn with_untracked<R>(self, read: impl FnOnce(&Option<BoardState>) -> R) -> R {
        read(&self.0.get_untracked().borrow())
    }

    fn set(self, board: Option<BoardState>) {
        self.update(|current| *current = board);
    }

    fn update(self, write: impl FnOnce(&mut Option<BoardState>)) {
        let board = self.0.get_untracked();
        write(&mut board.borrow_mut());
        self.0.set(board);
    }
}

pub(super) trait Tailwind: Element {
    fn class(self, classes: &'static str) -> Self {
        self.app_class(Some(Id::from(classes)))
    }
}

impl<T: Element> Tailwind for T {}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    List,
    Board,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modal {
    Commands,
    Projects,
    IssueMenu,
    Move,
    NewIssue,
    NewProject,
    NewStatus,
    NewCycle,
    SaveView,
    Launch,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    Triage,
    Cycle,
    Saved,
}

struct Ui {
    api: Api,
    owner: Owner,
    projects: Signal<Vec<Project>>,
    project: Signal<Option<Id>>,
    board: BoardSlot,
    cycles: Signal<Vec<Cycle>>,
    views: Signal<Vec<SavedView>>,
    inventory: Signal<Vec<Issue>>,
    workspaces: Signal<Vec<Workspace>>,
    workspace_id: Signal<Option<String>>,
    workspace_screen: Signal<bool>,
    shortcuts_active: Box<dyn Fn() -> bool>,
    status: Signal<Id>,
    connected: Signal<bool>,
    reload: Signal<u32>,
    mode: Signal<Mode>,
    filter: Signal<Filter>,
    filter_id: Signal<Option<Id>>,
    cursor: Signal<Option<Id>>,
    detail: Signal<Option<Id>>,
    modal: Signal<Option<Modal>>,
    launch: Signal<Option<Rc<launch::LaunchState>>>,
    query: Signal<Id>,
    draft: Signal<Id>,
    secondary: Signal<Id>,
    tertiary: Signal<Id>,
    status_choice: Signal<Option<Id>>,
    editing: Signal<Option<bool>>,
    composing: Signal<bool>,
    dragging: Signal<Option<Id>>,
    drop_zones: RefCell<HashMap<Id, ElementRef>>,
    card_zones: RefCell<HashMap<Id, ElementRef>>,
    list_ref: ElementRef,
    command_ref: ElementRef,
}

pub(crate) fn control(label: impl Into<Id>, press: impl Fn() + 'static) -> View {
    let label = label.into();
    view()
        .class("rounded-md border border-border bg-card px-2 hover:bg-accent focus:bg-accent")
        .h(px(32.0))
        .justify_center()
        .role(AccessibilityRole::Button)
        .label(label.clone())
        .on_press(press)
        .child(text(label).class("text-sm text-foreground"))
}

pub(crate) fn board_view(
    api: Api,
    workspaces: Signal<Vec<Workspace>>,
    workspace_id: Signal<Option<String>>,
    selected_issue: Signal<Option<Id>>,
    workspace_screen: Signal<bool>,
    command_open: Signal<bool>,
    shortcuts_active: impl Fn() -> bool + 'static,
) -> impl IntoElement {
    let ui = Rc::new(Ui {
        api,
        owner: Owner::current(),
        projects: signal(Vec::new()),
        project: signal(None),
        board: BoardSlot::new(),
        cycles: signal(Vec::new()),
        views: signal(Vec::new()),
        inventory: signal(Vec::new()),
        workspaces,
        workspace_id,
        workspace_screen,
        shortcuts_active: Box::new(shortcuts_active),
        status: signal("正在连接本地服务".into()),
        connected: signal(false),
        reload: signal(0),
        mode: signal(Mode::List),
        filter: signal(Filter::All),
        filter_id: signal(None),
        cursor: signal(None),
        detail: selected_issue,
        modal: signal(None),
        query: signal("".into()),
        launch: signal(None),
        draft: signal("".into()),
        secondary: signal("".into()),
        tertiary: signal("".into()),
        status_choice: signal(None),
        editing: signal(None),
        composing: signal(false),
        dragging: signal(None),
        drop_zones: RefCell::new(HashMap::new()),
        card_zones: RefCell::new(HashMap::new()),
        list_ref: element_ref(),
        command_ref: element_ref(),
    });
    ui.clone().load_projects();
    effect({
        let ui = ui.clone();
        move || ui.clone().load_project()
    });
    effect({
        let ui = ui.clone();
        move || command_open.set(ui.modal.with(Option::is_some))
    });
    register_keys(ui.clone());
    view()
        .flex_grow(move || {
            if workspace_screen.get() {
                0.0_f32
            } else {
                1.0_f32
            }
        })
        .min_h(px(0.0))
        .child(when(move || !workspace_screen.get(), {
            let ui = ui.clone();
            move || surface(ui.clone())
        }))
        .child(dynamic(move || {
            ui.modal.get().map(|kind| overlay(ui.clone(), kind))
        }))
}

impl Ui {
    fn load_projects(self: Rc<Self>) {
        spawn(async move {
            match self.api.projects().await {
                Ok(projects) => {
                    self.connected.set(true);
                    self.status.set("本地已连接".into());
                    if self.project.get_untracked().is_none() {
                        self.project
                            .set(projects.first().map(|project| project.id.clone().into()));
                    }
                    self.projects.set(projects);
                }
                Err(error) => self.fail(error),
            }
        });
    }

    fn load_project(self: Rc<Self>) {
        let _ = self.reload.get();
        let Some(project) = self.project.get() else {
            return;
        };
        let generation = self.reload.get_untracked();
        let revision = self
            .board
            .with_untracked(|board| board.as_ref().map(BoardState::revision));
        self.status.set("正在加载项目".into());
        spawn(async move {
            let id = ProjectId(project.as_str());
            let result = futures_util::try_join!(
                BoardState::load(&self.api, id),
                self.api.cycles(id),
                self.api.saved_views(id)
            );
            if self.project.get_untracked().as_ref() != Some(&project)
                || self.reload.get_untracked() != generation
            {
                return;
            }
            match result {
                Ok((board, cycles, views)) => {
                    self.board
                        .update(|current| match (revision, current.as_mut()) {
                            (Some(revision), Some(current)) => {
                                current.refresh(revision, board);
                            }
                            (None, None) => *current = Some(board),
                            _ => {}
                        });
                    self.cycles.set(cycles);
                    self.views.set(views);
                    self.connected.set(true);
                    self.status.set("本地已连接".into());
                    self.clone().load_search_inventory(project);
                }
                Err(error) => self.fail(error),
            }
        });
    }

    fn load_search_inventory(self: Rc<Self>, active: Id) {
        let project_ids = self.projects.with_untracked(|projects| {
            projects
                .iter()
                .filter(|project| project.id != active.as_str())
                .map(|project| project.id.clone())
                .collect::<Vec<_>>()
        });
        spawn(async move {
            let mut inventory = Vec::new();
            for id in project_ids {
                if self.project.get_untracked().as_ref() != Some(&active) {
                    return;
                }
                if let Ok(issues) = self
                    .api
                    .issues(ProjectId(&id), &IssueFilter::default())
                    .await
                {
                    inventory.extend(issues);
                }
            }
            if self.project.get_untracked().as_ref() == Some(&active) {
                self.inventory.set(inventory);
            }
        });
    }

    fn fail(&self, error: impl Into<Id>) {
        self.status.set(error.into());
    }

    fn choose_project(&self, project: Id, issue: Option<Id>) {
        if self.project.get_untracked().as_ref() != Some(&project) {
            self.board.set(None);
            self.cycles.set(Vec::new());
            self.views.set(Vec::new());
            self.filter.set(Filter::All);
            self.project.set(Some(project));
        }
        self.cursor.set(issue.clone());
        self.detail.set(issue);
        self.editing.set(None);
        self.workspace_screen.set(false);
    }

    fn context(&self, issue: bool, input: InputState) -> ActionContext {
        ActionContext {
            input,
            scope: if issue {
                ActionScope::Issue
            } else if self.project.get().is_some() {
                ActionScope::Project
            } else {
                ActionScope::Global
            },
            connected: self.connected.get(),
            mutation_pending: self
                .board
                .with(|board| board.as_ref().is_some_and(BoardState::is_saving)),
        }
    }

    fn invoke(self: &Rc<Self>, action: Action<'_>, source: ActionSource) {
        let project = self.project.get_untracked();
        let cursor = self.cursor.get_untracked();
        let focus = match (&project, &cursor) {
            (Some(project), Some(issue)) => FocusAnchor::Entity(EntityId::Issue(IssueRef {
                project_id: ProjectId(project.as_str()),
                issue_id: IssueId(issue.as_str()),
            })),
            _ => FocusAnchor::CommandButton,
        };
        let input = if self.composing.get_untracked() {
            InputState::Composing
        } else if self.editing.get_untracked().is_some() {
            InputState::Editing
        } else {
            InputState::Ready
        };
        let context = self.context(
            matches!(
                action,
                Action::MoveIssue { .. } | Action::EditIssue { .. } | Action::StartWorkspace(_)
            ) || cursor.is_some(),
            input,
        );
        let event = Invocation {
            action,
            source,
            return_focus: Some(focus),
        };
        let effect = if matches!(
            event.action,
            Action::OpenCommand | Action::OpenEntity(_) | Action::StartWorkspace(_)
        ) {
            event
                .dispatch(context, BoardEffect::Navigate)
                .map_err(|reason| reason.label())
        } else {
            let mut outcome = Err("项目尚未加载");
            self.board.update(|board| {
                if let Some(board) = board {
                    outcome = board
                        .dispatch(event, context)
                        .map_err(|error| error.label());
                }
            });
            outcome
        };
        match effect {
            Ok(BoardEffect::Navigate(event)) => match event.action {
                Action::OpenCommand => self.open(Modal::Commands),
                Action::OpenEntity(EntityId::Issue(issue)) => self.choose_project(
                    issue.project_id.0.to_owned().into(),
                    Some(issue.issue_id.0.to_owned().into()),
                ),
                Action::OpenEntity(EntityId::Project(project)) => {
                    self.choose_project(project.0.to_owned().into(), None)
                }
                Action::OpenEntity(EntityId::Workspace(workspace)) => {
                    self.workspace_id.set(Some(workspace.0.to_owned()));
                    self.workspace_screen.set(true);
                }
                Action::StartWorkspace(issue) => self.launch_workspace(issue),
                _ => unreachable!(),
            },
            Ok(BoardEffect::Submit(submission)) => {
                self.status.set("正在保存".into());
                let ui = self.clone();
                self.owner.run(|| {
                    spawn(async move {
                        let response = submission.execute(&ui.api).await;
                        let mut finished = None;
                        ui.board.update(|board| {
                            finished = board.as_mut().and_then(|board| board.finish(response));
                        });
                        if let Some(finished) = finished {
                            match finished.result {
                                Ok(()) => {
                                    ui.editing.set(None);
                                    ui.modal.set(None);
                                    ui.status.set("已保存".into());
                                    if let Some(anchor) = finished.return_focus {
                                        ui.restore(anchor.anchor());
                                    }
                                }
                                Err(error) => ui.fail(error),
                            }
                        }
                    })
                });
            }
            Err(message) => self.fail(message),
        }
    }

    fn restore(&self, anchor: FocusAnchor<'_>) {
        if let FocusAnchor::Entity(EntityId::Issue(issue)) = anchor {
            self.cursor.set(Some(issue.issue_id.0.to_owned().into()));
            self.list_ref.focus();
        } else {
            self.command_ref.focus();
        }
    }

    fn open(&self, modal: Modal) {
        self.query.set("".into());
        self.draft.set("".into());
        self.secondary.set("".into());
        self.tertiary.set("".into());
        self.status_choice.set(self.board.with_untracked(|board| {
            board
                .as_ref()?
                .statuses()
                .first()
                .map(|status| status.id.clone().into())
        }));
        self.modal.set(Some(modal));
    }

    fn close(&self) {
        self.modal.set(None);
        self.composing.set(false);
        if self.cursor.get_untracked().is_some() {
            self.list_ref.focus();
        } else {
            self.command_ref.focus();
        }
    }

    fn issue_ref<'a>(&self, project: &'a Id, id: &'a Id) -> IssueRef<'a> {
        IssueRef {
            project_id: ProjectId(project.as_str()),
            issue_id: IssueId(id.as_str()),
        }
    }

    fn open_issue(self: &Rc<Self>, id: &Id, source: ActionSource) {
        if let Some(project) = self.project.get_untracked() {
            self.invoke(
                Action::OpenEntity(EntityId::Issue(self.issue_ref(&project, id))),
                source,
            );
        }
    }

    fn edit(self: &Rc<Self>, id: &Id, edit: IssueEdit<'_>, source: ActionSource) {
        if let Some(project) = self.project.get_untracked() {
            self.invoke(
                Action::EditIssue {
                    issue: self.issue_ref(&project, id),
                    change: edit,
                },
                source,
            );
        }
    }

    fn edit_field(&self, id: &Id, description: bool) {
        let value = self.board.with_untracked(|board| {
            let issue = board.as_ref()?.issue(IssueId(id.as_str()))?;
            Some(if description {
                issue.description.clone().unwrap_or_default()
            } else {
                issue.title.clone()
            })
        });
        if let Some(value) = value {
            self.draft.set(value.into());
            self.editing.set(Some(description));
        }
    }

    fn move_issue(
        self: &Rc<Self>,
        id: &Id,
        status: &Id,
        relative: Option<(&Id, bool)>,
        source: ActionSource,
    ) {
        let Some(project) = self.project.get_untracked() else {
            return;
        };
        let Some(source_status) = self.board.with_untracked(|board| {
            board
                .as_ref()?
                .issue(IssueId(id.as_str()))
                .map(|issue| issue.status_id.clone())
        }) else {
            return;
        };
        let source_issue = BoardIssue {
            issue: self.issue_ref(&project, id),
            status_id: StatusId(&source_status),
        };
        let destination = StatusRef {
            project_id: ProjectId(project.as_str()),
            status_id: StatusId(status.as_str()),
        };
        let position = relative.map(|(id, after)| {
            let anchor = BoardIssue {
                issue: self.issue_ref(&project, id),
                status_id: destination.status_id,
            };
            if after {
                RelativePosition::After(anchor)
            } else {
                RelativePosition::Before(anchor)
            }
        });
        self.invoke(
            Action::MoveIssue {
                source: source_issue,
                destination,
                position,
            },
            source,
        );
    }
}

fn register_keys(ui: Rc<Ui>) {
    let options = KeyOptions {
        when: When::NotEditing,
        pass_through: true,
        ..KeyOptions::default()
    };
    on_key_with(
        "mod+k",
        KeyOptions {
            pass_through: false,
            ..options
        },
        {
            let ui = ui.clone();
            move |_| {
                if (ui.shortcuts_active)() && ui.modal.get_untracked().is_none() {
                    ui.invoke(Action::OpenCommand, ActionSource::Keyboard);
                }
            }
        },
    );
    for (key, offset) in [("down", 1isize), ("up", -1)] {
        let ui = ui.clone();
        on_key_with(
            key,
            KeyOptions {
                repeat: true,
                ..options
            },
            move |_| {
                if ui.modal.get_untracked().is_some() || ui.workspace_screen.get_untracked() {
                    return;
                }
                let current = ui.cursor.get_untracked();
                let next = ui.board.with_untracked(|board| {
                    let board = board.as_ref()?;
                    let filter = effective_filter(&ui);
                    let ids = board
                        .issues()
                        .iter()
                        .filter(|issue| matches_filter(issue, board, &filter));
                    let count = ids.clone().count();
                    if count == 0 {
                        return None;
                    }
                    let at = current.as_ref().and_then(|current| {
                        ids.clone().position(|issue| issue.id == current.as_str())
                    });
                    let at = at.map_or(0, |at| at.saturating_add_signed(offset).min(count - 1));
                    Some((at, Id::from(ids.clone().nth(at)?.id.clone())))
                });
                if let Some((index, id)) = next {
                    ui.cursor.set(Some(id));
                    if ui.mode.get_untracked() == Mode::List {
                        ui.list_ref
                            .scroll_to_index(index as u32, percentage(0.5), px(0.0), false);
                    }
                }
            },
        );
    }
    for (key, action) in [
        ("enter", ActionId::OpenIssue),
        ("shift+f10", ActionId::MoveIssue),
        ("e", ActionId::EditIssue),
        ("c", ActionId::CreateIssue),
    ] {
        let ui = ui.clone();
        on_key_with(key, options, move |_| {
            if ui.modal.get_untracked().is_some() || ui.workspace_screen.get_untracked() {
                return;
            }
            if action == ActionId::CreateIssue {
                ui.open(Modal::NewIssue);
                return;
            }
            let Some(id) = ui.cursor.get_untracked() else {
                return;
            };
            match action {
                ActionId::OpenIssue => ui.open_issue(&id, ActionSource::Keyboard),
                ActionId::MoveIssue => ui.open(Modal::IssueMenu),
                ActionId::EditIssue => {
                    ui.open_issue(&id, ActionSource::Keyboard);
                    ui.edit_field(&id, false);
                }
                _ => unreachable!(),
            }
        });
    }
    on_key_with("escape", options, move |_| {
        if ui.composing.get_untracked() || ui.modal.get_untracked().is_some() {
            return;
        }
        if ui.editing.get_untracked().is_some() {
            ui.editing.set(None);
        } else if ui.dragging.get_untracked().is_some() {
            ui.dragging.set(None);
        } else if ui.detail.get_untracked().is_some() {
            ui.detail.set(None);
            ui.list_ref.focus();
        }
    });
}

fn effective_filter(ui: &Ui) -> IssueFilter {
    let id = ui.filter_id.get();
    match ui.filter.get() {
        Filter::All => IssueFilter::default(),
        Filter::Triage => IssueFilter {
            triage: Some(true),
            ..IssueFilter::default()
        },
        Filter::Cycle => IssueFilter {
            cycle_id: id.map(|id| id.to_string()),
            ..IssueFilter::default()
        },
        Filter::Saved => ui.views.with(|views| {
            id.as_ref()
                .and_then(|id| views.iter().find(|view| view.id == id.as_str()))
                .and_then(|view| serde_json::from_value(view.filter.clone()).ok())
                .unwrap_or_default()
        }),
    }
}

fn matches_filter(issue: &Issue, board: &BoardState, filter: &IssueFilter) -> bool {
    filter.triage.is_none_or(|value| issue.triage == value)
        && filter
            .status_id
            .as_ref()
            .is_none_or(|value| issue.status_id == *value)
        && filter.status_category.is_none_or(|value| {
            board
                .statuses()
                .iter()
                .any(|status| status.id == issue.status_id && status.category == value)
        })
        && filter
            .priority
            .is_none_or(|value| issue.priority == Some(value))
        && filter
            .tag_id
            .as_ref()
            .is_none_or(|value| issue.tag_ids.contains(value))
        && filter
            .cycle_id
            .as_ref()
            .is_none_or(|value| issue.cycle_id.as_ref() == Some(value))
        && filter
            .parent_issue_id
            .as_ref()
            .is_none_or(|value| issue.parent_issue_id.as_ref() == Some(value))
        && filter
            .root_only
            .is_none_or(|value| !value || issue.parent_issue_id.is_none())
        && filter
            .external_provider
            .as_ref()
            .is_none_or(|value| issue.external_provider.as_ref() == Some(value))
        && filter
            .external_id
            .as_ref()
            .is_none_or(|value| issue.external_id.as_ref() == Some(value))
        && filter
            .start_on_or_after
            .as_ref()
            .is_none_or(|value| issue.start_date.as_ref().is_some_and(|date| date >= value))
        && filter
            .target_on_or_before
            .as_ref()
            .is_none_or(|value| issue.target_date.as_ref().is_some_and(|date| date <= value))
        && filter.search.as_ref().is_none_or(|value| {
            let value = value.trim().to_ascii_lowercase();
            [
                Some(issue.title.as_str()),
                Some(issue.simple_id.as_str()),
                issue.description.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.to_ascii_lowercase().contains(&value))
        })
}

fn surface(ui: Rc<Ui>) -> impl IntoElement {
    let board_mode = ui.mode;
    let designer_width = signal(1200.0f32);
    let filter = {
        let ui = ui.clone();
        computed(move || ui.filter.get())
    };
    view()
        .flex_1()
        .min_h(px(0.0))
        .on_layout(move |bounds: &Bounds<Points>| designer_width.set(bounds.size.width.0))
        .child(
            view()
                .px_4()
                .py_2()
                .flex_row()
                .items_center()
                .gap_2()
                .class("border-b border-border")
                .child(control("项目", {
                    let ui = ui.clone();
                    move || ui.open(Modal::Projects)
                }))
                .child(
                    text({
                        let ui = ui.clone();
                        move || {
                            let active = ui.project.get();
                            ui.projects.with(|projects| {
                                projects
                                    .iter()
                                    .find(|project| {
                                        active.as_ref().is_some_and(|id| project.id == id.as_str())
                                    })
                                    .map_or_else(
                                        || "选择项目".into(),
                                        |project| Id::from(project.name.clone()),
                                    )
                            })
                        }
                    })
                    .class("font-semibold text-foreground"),
                )
                .child(control("列表", move || board_mode.set(Mode::List)))
                .child(control("看板", move || board_mode.set(Mode::Board)))
                .child(control("新建议题", {
                    let ui = ui.clone();
                    move || ui.open(Modal::NewIssue)
                }))
                .child(view().flex_1())
                .child(
                    control(
                        if nsh_core::element::apple_keys() {
                            "搜索命令  ⌘K"
                        } else {
                            "搜索命令  Ctrl+K"
                        },
                        {
                            let ui = ui.clone();
                            move || ui.invoke(Action::OpenCommand, ActionSource::Pointer)
                        },
                    )
                    .element_ref(&ui.command_ref),
                ),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .flex_row()
                .child(when(move || designer_width.get() >= 960.0, {
                    let ui = ui.clone();
                    move || rail(ui.clone())
                }))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .min_h(px(0.0))
                        .child(when(move || designer_width.get() < 960.0, {
                            let ui = ui.clone();
                            move || filters(ui.clone(), true)
                        }))
                        .child(
                            view()
                                .flex_1()
                                .min_h(px(0.0))
                                .app_class({
                                    let ui = ui.clone();
                                    move || {
                                        (designer_width.get() < 1200.0 && ui.detail.get().is_some())
                                            .then(|| Id::from("hidden"))
                                    }
                                })
                                .child(when(move || board_mode.get() == Mode::List, {
                                    let ui = ui.clone();
                                    move || issue_list(ui.clone(), None)
                                }))
                                .child(when(move || board_mode.get() == Mode::Board, {
                                    let ui = ui.clone();
                                    move || columns(ui.clone())
                                })),
                        )
                        .child(when(
                            {
                                let ui = ui.clone();
                                move || designer_width.get() < 1200.0 && ui.detail.get().is_some()
                            },
                            {
                                let ui = ui.clone();
                                move || detail(ui.clone(), false)
                            },
                        )),
                )
                .child(when(
                    {
                        let ui = ui.clone();
                        move || designer_width.get() >= 1200.0 && ui.detail.get().is_some()
                    },
                    {
                        let ui = ui.clone();
                        move || detail(ui.clone(), true)
                    },
                )),
        )
        .child(
            view()
                .h(px(34.0))
                .px_4()
                .gap_3()
                .flex_row()
                .items_center()
                .class("border-t border-border bg-sidebar")
                .child(text(ui.status).class("text-xs text-foreground"))
                .child(
                    text(move || match filter.get() {
                        Filter::All => "全部议题",
                        Filter::Triage => "待分诊",
                        Filter::Cycle => "周期",
                        Filter::Saved => "保存视图",
                    })
                    .class("text-xs text-muted-foreground"),
                )
                .child(control("刷新", move || {
                    if ui
                        .board
                        .with_untracked(|board| board.as_ref().is_some_and(BoardState::is_saving))
                    {
                        return;
                    }
                    if ui.project.get_untracked().is_some() {
                        ui.reload.update(|n| *n += 1);
                    } else {
                        ui.clone().load_projects();
                    }
                })),
        )
}

fn rail(ui: Rc<Ui>) -> impl IntoElement {
    let syncing = signal(false);
    view()
        .w(px(208.0))
        .flex_shrink_0()
        .p_3()
        .gap_3()
        .class("border-r border-border bg-sidebar")
        .child(text("项目索引").class("text-xs font-semibold text-muted-foreground"))
        .child(dynamic({
            let ui = ui.clone();
            move || {
                ui.projects.with(|projects| {
                    projects
                        .iter()
                        .map(|project| {
                            let id: Id = project.id.clone().into();
                            let label: Id = project.name.clone().into();
                            control(label, {
                                let ui = ui.clone();
                                move || ui.choose_project(id.clone(), None)
                            })
                        })
                        .collect::<Vec<_>>()
                })
            }
        }))
        .child(control("新建项目", {
            let ui = ui.clone();
            move || ui.open(Modal::NewProject)
        }))
        .child(control("GitHub Issues", move || {
            syncing.update(|open| *open = !*open)
        }))
        .child(when(syncing, {
            let ui = ui.clone();
            move || {
                scroll().flex_1().min_h(px(0.0)).child(dynamic({
                    let ui = ui.clone();
                    move || {
                        let project_id = ui.project.get()?;
                        ui.board.with(|board| {
                            let statuses = board.as_ref()?.statuses();
                            let reload = ui.reload;
                            Some(
                                crate::github_integration::project_issue_sync(
                                    ui.api.clone(),
                                    project_id.clone(),
                                    statuses,
                                    move |_| {
                                        reload
                                            .update(|revision| *revision = revision.wrapping_add(1))
                                    },
                                )
                                .key(project_id),
                            )
                        })
                    }
                }))
            }
        }))
        .child(when(
            move || !syncing.get(),
            move || filters(ui.clone(), false),
        ))
}

fn filters(ui: Rc<Ui>, horizontal: bool) -> impl IntoElement {
    let strip = view().gap_2();
    let strip = if horizontal {
        strip.flex_row().items_center().px_3().py_2()
    } else {
        strip
    };
    strip
        .child(control("全部议题", {
            let ui = ui.clone();
            move || ui.filter.set(Filter::All)
        }))
        .child(control("待分诊", {
            let ui = ui.clone();
            move || ui.filter.set(Filter::Triage)
        }))
        .child(dynamic({
            let ui = ui.clone();
            move || {
                ui.cycles.with(|cycles| {
                    cycles
                        .iter()
                        .map(|cycle| {
                            let id: Id = cycle.id.clone().into();
                            control(cycle.name.clone(), {
                                let ui = ui.clone();
                                move || {
                                    ui.filter_id.set(Some(id.clone()));
                                    ui.filter.set(Filter::Cycle);
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
            }
        }))
        .child(dynamic({
            let ui = ui.clone();
            move || {
                ui.views.with(|views| {
                    views
                        .iter()
                        .map(|saved| {
                            let id: Id = saved.id.clone().into();
                            control(saved.name.clone(), {
                                let ui = ui.clone();
                                move || {
                                    ui.filter_id.set(Some(id.clone()));
                                    ui.filter.set(Filter::Saved);
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
            }
        }))
        .child(control("新建周期", {
            let ui = ui.clone();
            move || ui.open(Modal::NewCycle)
        }))
        .child(control("保存当前视图", move || {
            ui.open(Modal::SaveView)
        }))
}

fn columns(ui: Rc<Ui>) -> impl IntoElement {
    let statuses = {
        let ui = ui.clone();
        computed(move || {
            ui.board.with(|board| {
                board
                    .as_ref()
                    .map(|board| {
                        board
                            .statuses()
                            .iter()
                            .map(|status| {
                                (Id::from(status.id.clone()), Id::from(status.name.clone()))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
        })
    };
    scroll().horizontal().flex_1().min_h(px(0.0)).child(
        view()
            .flex_row()
            .gap_3()
            .px_3()
            .py_2()
            .flex_1()
            .child(control("新增状态", {
                let ui = ui.clone();
                move || ui.open(Modal::NewStatus)
            }))
            .child(dynamic(move || {
                statuses.with(|statuses| {
                    statuses
                        .iter()
                        .map(|(id, name)| {
                            column(ui.clone(), id.clone(), name.clone()).key(id.clone())
                        })
                        .collect::<Vec<_>>()
                })
            })),
    )
}

fn column(ui: Rc<Ui>, status: Id, label: Id) -> View {
    let bounds_ref = element_ref();
    ui.drop_zones
        .borrow_mut()
        .insert(status.clone(), bounds_ref);
    cleanup({
        let ui = ui.clone();
        let status = status.clone();
        move || {
            ui.drop_zones.borrow_mut().remove(&status);
        }
    });
    view()
        .w(px(250.0))
        .min_h(px(0.0))
        .flex_shrink_0()
        .element_ref(&bounds_ref)
        .class("rounded-lg border border-border bg-card")
        .child(
            view()
                .h(px(40.0))
                .justify_center()
                .px_3()
                .class("border-b border-border")
                .child(text(label).class("font-semibold text-foreground")),
        )
        .child(issue_list(ui, Some(status)))
}

fn issue_list(ui: Rc<Ui>, column: Option<Id>) -> impl IntoElement {
    let is_list = column.is_none();
    let indices = {
        let ui = ui.clone();
        let column = column.clone();
        computed(move || {
            let filter = effective_filter(&ui);
            ui.board.with(|board| {
                board
                    .as_ref()
                    .map(|board| {
                        board
                            .issues()
                            .iter()
                            .enumerate()
                            .filter_map(|(index, issue)| {
                                (column
                                    .as_ref()
                                    .is_none_or(|id| issue.status_id == id.as_str())
                                    && matches_filter(issue, board, &filter))
                                .then_some(index)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
        })
    };
    let list = virtual_list(move || indices.with(Vec::len), {
        let ui = ui.clone();
        move |row| {
            let issue = {
                let ui = ui.clone();
                move || {
                    indices
                        .with(|indices| indices.get(row.index()).copied())
                        .and_then(|index| {
                            ui.board.with(|board| {
                                let issue = board.as_ref()?.issues().get(index)?;
                                Some((
                                    Id::from(issue.id.clone()),
                                    Id::from(issue.simple_id.clone()),
                                    Id::from(issue.title.clone()),
                                    issue.triage,
                                ))
                            })
                        })
                }
            };
            let issue = computed(issue);
            let grip = element_ref();
            let card_ref = element_ref();
            if !is_list {
                effect({
                    let ui = ui.clone();
                    move || {
                        if let Some((id, ..)) = issue.get() {
                            ui.card_zones.borrow_mut().insert(id.clone(), card_ref);
                            cleanup({
                                let ui = ui.clone();
                                move || {
                                    ui.card_zones.borrow_mut().remove(&id);
                                }
                            });
                        }
                    }
                });
            }
            let row_style = {
                let ui = ui.clone();
                move || {
                    let id = issue.get().map(|item| item.0);
                    Some(SharedString::from(
                        if id.is_some() && id == ui.dragging.get() {
                            "border-b border-border bg-muted"
                        } else if id.is_some() && id == ui.cursor.get() {
                            "border-b border-border bg-accent"
                        } else {
                            "border-b border-border bg-card hover:bg-accent focus:bg-accent"
                        },
                    ))
                }
            };
            view()
                .h(px(ROW_HEIGHT))
                .px_2()
                .flex_row()
                .items_center()
                .gap_2()
                .element_ref(&card_ref)
                .app_class(row_style)
                .role(AccessibilityRole::Button)
                .label(move || {
                    issue.get().map_or_else(
                        || Id::from("议题"),
                        |item| format!("{} {}", item.1, item.2).into(),
                    )
                })
                .on_press({
                    let ui = ui.clone();
                    move || {
                        if let Some((id, ..)) = issue.get_untracked() {
                            ui.open_issue(&id, ActionSource::Pointer);
                        }
                    }
                })
                .on_context_menu({
                    let ui = ui.clone();
                    move |_: &nsh_core::ContextMenuEvent| {
                        if let Some((id, ..)) = issue.get_untracked() {
                            ui.cursor.set(Some(id));
                            ui.open(Modal::IssueMenu);
                        }
                    }
                })
                .child(when(move || !is_list, {
                    let ui = ui.clone();
                    move || {
                        view()
                            .w(px(24.0))
                            .h(px(32.0))
                            .justify_center()
                            .items_center()
                            .element_ref(&grip)
                            .label("拖动议题")
                            .child(text("⠿").class("text-muted-foreground"))
                            .on_pan_start({
                                let ui = ui.clone();
                                move |_: &PanEvent| {
                                    ui.dragging.set(issue.get_untracked().map(|item| item.0));
                                }
                            })
                            .on_pan_end({
                                let ui = ui.clone();
                                move |event: &PanEvent| {
                                    let id = ui.dragging.get_untracked();
                                    ui.dragging.set(None);
                                    if !event.cancelled
                                        && let Some(id) = id
                                    {
                                        ui.clone().drop_card(id, grip, event.translation);
                                    }
                                }
                            })
                    }
                }))
                .child(
                    text(move || issue.get().map(|item| item.1).unwrap_or_default())
                        .class("font-mono text-xs text-muted-foreground"),
                )
                .child(
                    text(move || issue.get().map(|item| item.2).unwrap_or_default())
                        .flex_1()
                        .min_w(px(0.0))
                        .number_of_lines(1)
                        .class("text-sm text-foreground"),
                )
                .child(
                    text(move || {
                        if issue.get().is_some_and(|item| item.3) {
                            "待分诊"
                        } else {
                            ""
                        }
                    })
                    .class("text-xs text-muted-foreground"),
                )
                .child(control("⋯", {
                    let ui = ui.clone();
                    move || {
                        if let Some((id, ..)) = issue.get_untracked() {
                            ui.cursor.set(Some(id));
                            ui.open(Modal::IssueMenu);
                        }
                    }
                }))
        }
    })
    .keys({
        let ui = ui.clone();
        move || {
            indices.with(|indices| {
                ui.board.with(|board| {
                    board
                        .as_ref()
                        .map(|board| {
                            indices
                                .iter()
                                .map(|&index| board.issues()[index].id.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            })
        }
    })
    .estimated_item_size(px(ROW_HEIGHT))
    .flex_1()
    .min_h(px(0.0))
    .empty(
        view()
            .p_4()
            .gap_2()
            .child(text("此视图没有议题").class("text-sm text-muted-foreground"))
            .child(control("新建议题", {
                let ui = ui.clone();
                move || {
                    ui.open(Modal::NewIssue);
                    if let Some(id) = &column {
                        ui.status_choice.set(Some(id.clone()));
                    }
                }
            })),
    )
    .on_scroll(move |event: &nsh_core::element::ScrollEvent| {
        if is_list {
            log::debug!("m4 issue offset={:.0}", event.offset.y.0);
        }
    });
    if is_list {
        let list = list.element_ref(&ui.list_ref).focusable(true);
        if log::log_enabled!(log::Level::Debug) {
            list.on_viewable_items_change(|event: &nsh_core::element::ViewableItems| {
                if let Some((first, last)) = event.viewable {
                    log::debug!("m4 issue viewport={first}..={last}");
                }
            })
        } else {
            list
        }
    } else {
        list
    }
}

impl Ui {
    fn drop_card(self: Rc<Self>, id: Id, grip: ElementRef, translation: Point<Points>) {
        let candidates = self
            .drop_zones
            .borrow()
            .iter()
            .map(|(id, reference)| (id.clone(), reference.measure()))
            .collect::<Vec<_>>();
        let cards = self
            .card_zones
            .borrow()
            .iter()
            .filter(|(other, _)| **other != id)
            .map(|(id, reference)| (id.clone(), reference.measure()))
            .collect::<Vec<_>>();
        let start = grip.measure();
        spawn(async move {
            let Some(start) = start.await else { return };
            let x = start.page.x.0 + start.frame.size.width.0 / 2.0 + translation.x.0;
            let y = start.page.y.0 + start.frame.size.height.0 / 2.0 + translation.y.0;
            for (status, measured) in candidates {
                if let Some(bounds) = measured.await
                    && x >= bounds.page.x.0
                    && x < bounds.page.x.0 + bounds.frame.size.width.0
                    && y >= bounds.page.y.0
                    && y < bounds.page.y.0 + bounds.frame.size.height.0
                {
                    for (anchor, measured) in cards {
                        if let Some(card) = measured.await
                            && x >= card.page.x.0
                            && x < card.page.x.0 + card.frame.size.width.0
                            && y >= card.page.y.0
                            && y < card.page.y.0 + card.frame.size.height.0
                        {
                            let after = y >= card.page.y.0 + card.frame.size.height.0 / 2.0;
                            self.move_issue(
                                &id,
                                &status,
                                Some((&anchor, after)),
                                ActionSource::DragDrop,
                            );
                            return;
                        }
                    }
                    self.move_issue(&id, &status, None, ActionSource::DragDrop);
                    break;
                }
            }
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PaletteTarget {
    Action(ActionId),
    Project(Id),
    Issue { project: Id, issue: Id },
    Workspace(Id),
    Status(Id),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    target: PaletteTarget,
    key: Id,
    label: Id,
    group: &'static str,
    disabled: Option<&'static str>,
    shortcut: Option<&'static str>,
}

impl Choice {
    fn action(action: ActionId, context: ActionContext) -> Self {
        let spec = action.spec();
        Self {
            target: PaletteTarget::Action(action),
            key: action.as_str().into(),
            label: spec.label.into(),
            group: "动作",
            disabled: action
                .availability(context)
                .err()
                .map(|reason| reason.label()),
            shortcut: spec.shortcut,
        }
    }

    fn item(&self) -> CommandItem {
        let label = if let Some(reason) = self.disabled {
            Id::from(format!("{} · {reason}", self.label))
        } else {
            self.label.clone()
        };
        let item = CommandItem::new(self.key.clone(), label)
            .group(self.group)
            .disabled(self.disabled.is_some());
        if let Some(shortcut) = self.shortcut {
            item.shortcut(shortcut)
        } else {
            item
        }
    }
}

impl Ui {
    fn choices(&self, kind: Modal) -> Vec<Choice> {
        let context = self.context(self.cursor.get().is_some(), InputState::Ready);
        match kind {
            Modal::Commands => {
                let query = self.query.get();
                self.projects.with(|projects| {
                    self.board.with(|board| {
                        self.inventory.with(|inventory| {
                            self.workspaces.with(|workspaces| {
                                let project_rows =
                                    projects.iter().map(|project| SearchEntity::Project {
                                        id: ProjectId(&project.id),
                                        name: &project.name,
                                    });
                                fn issue_rows(issue: &Issue) -> SearchEntity<'_> {
                                    SearchEntity::Issue {
                                        id: issue.reference(),
                                        short_id: &issue.simple_id,
                                        title: &issue.title,
                                    }
                                }
                                let current = board.as_ref().map_or(&[][..], BoardState::issues);
                                let workspace_rows =
                                    workspaces.iter().map(|workspace| SearchEntity::Workspace {
                                        id: WorkspaceId(&workspace.id),
                                        name: workspace.name.as_deref(),
                                        branch: &workspace.branch,
                                    });
                                let all = project_rows
                                    .chain(
                                        current
                                            .iter()
                                            .chain(inventory.iter().filter(|issue| {
                                                board.as_ref().is_none_or(|board| {
                                                    board.project().0 != issue.project_id
                                                })
                                            }))
                                            .map(issue_rows),
                                    )
                                    .chain(workspace_rows);
                                search(query.as_str(), all, context, MAX_RESULTS)
                                    .into_iter()
                                    .map(|hit| match hit.entry {
                                        SearchEntry::Action(action) => {
                                            Choice::action(action, context)
                                        }
                                        SearchEntry::Entity(entity) => {
                                            let (target, key, label, group) = match entity {
                                                SearchEntity::Issue {
                                                    id,
                                                    short_id,
                                                    title,
                                                } => (
                                                    PaletteTarget::Issue {
                                                        project: id.project_id.0.to_owned().into(),
                                                        issue: id.issue_id.0.to_owned().into(),
                                                    },
                                                    format!(
                                                        "issue:{}:{}",
                                                        id.project_id.0, id.issue_id.0
                                                    )
                                                    .into(),
                                                    format!("{short_id}  {title}").into(),
                                                    "议题",
                                                ),
                                                SearchEntity::Project { id, name } => (
                                                    PaletteTarget::Project(id.0.to_owned().into()),
                                                    format!("project:{}", id.0).into(),
                                                    name.to_owned().into(),
                                                    "项目",
                                                ),
                                                SearchEntity::Workspace { id, name, branch } => (
                                                    PaletteTarget::Workspace(
                                                        id.0.to_owned().into(),
                                                    ),
                                                    format!("workspace:{}", id.0).into(),
                                                    name.unwrap_or(branch).to_owned().into(),
                                                    "工作区",
                                                ),
                                            };
                                            Choice {
                                                target,
                                                key,
                                                label,
                                                group,
                                                disabled: hit
                                                    .availability
                                                    .err()
                                                    .map(|reason| reason.label()),
                                                shortcut: hit.entry.action_id().spec().shortcut,
                                            }
                                        }
                                    })
                                    .collect()
                            })
                        })
                    })
                })
            }
            Modal::Projects => self.projects.with(|projects| {
                projects
                    .iter()
                    .map(|project| Choice {
                        target: PaletteTarget::Project(project.id.clone().into()),
                        key: project.id.clone().into(),
                        label: project.name.clone().into(),
                        group: "项目",
                        disabled: None,
                        shortcut: None,
                    })
                    .collect()
            }),
            Modal::IssueMenu => [
                ActionId::OpenIssue,
                ActionId::EditIssue,
                ActionId::MoveIssue,
                ActionId::StartWorkspace,
            ]
            .into_iter()
            .map(|action| Choice::action(action, context))
            .collect(),
            Modal::Move => self.board.with(|board| {
                board
                    .as_ref()
                    .map(|board| {
                        let selected = self.cursor.get();
                        let current = selected
                            .as_ref()
                            .and_then(|id| board.issue(IssueId(id.as_str())))
                            .map(|issue| issue.status_id.as_str());
                        board
                            .statuses()
                            .iter()
                            .map(|status| Choice {
                                target: PaletteTarget::Status(status.id.clone().into()),
                                key: status.id.clone().into(),
                                label: status.name.clone().into(),
                                group: "状态",
                                disabled: (Some(status.id.as_str()) == current)
                                    .then_some("当前状态"),
                                shortcut: None,
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }),
            _ => Vec::new(),
        }
    }

    fn run_choice(self: &Rc<Self>, choice: &Choice) {
        if choice.disabled.is_some() {
            return;
        }
        self.modal.set(None);
        let source = ActionSource::Keyboard;
        match &choice.target {
            PaletteTarget::Project(project) => self.invoke(
                Action::OpenEntity(EntityId::Project(ProjectId(project.as_str()))),
                source,
            ),
            PaletteTarget::Issue { project, issue } => self.invoke(
                Action::OpenEntity(EntityId::Issue(self.issue_ref(project, issue))),
                source,
            ),
            PaletteTarget::Workspace(workspace) => self.invoke(
                Action::OpenEntity(EntityId::Workspace(WorkspaceId(workspace.as_str()))),
                source,
            ),
            PaletteTarget::Status(status) => {
                if let Some(issue) = self.cursor.get_untracked() {
                    self.move_issue(&issue, status, None, source);
                }
            }
            PaletteTarget::Action(action) => match action {
                ActionId::CreateIssue => self.open(Modal::NewIssue),
                ActionId::OpenIssue
                | ActionId::EditIssue
                | ActionId::StartWorkspace
                | ActionId::MoveIssue => {
                    let Some(issue) = self.cursor.get_untracked() else {
                        return;
                    };
                    match action {
                        ActionId::MoveIssue => self.open(Modal::Move),
                        ActionId::OpenIssue => self.open_issue(&issue, source),
                        ActionId::EditIssue => {
                            self.open_issue(&issue, source);
                            self.edit_field(&issue, false);
                        }
                        ActionId::StartWorkspace => {
                            if let Some(project) = self.project.get_untracked() {
                                self.invoke(
                                    Action::StartWorkspace(self.issue_ref(&project, &issue)),
                                    source,
                                );
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                _ => {}
            },
        }
    }
}

fn overlay(ui: Rc<Ui>, kind: Modal) -> AnyElement {
    if kind == Modal::Launch {
        return launch::launch_dialog(ui);
    }
    let title = match kind {
        Modal::Commands => "搜索命令",
        Modal::Projects => "切换项目",
        Modal::IssueMenu => "议题操作",
        Modal::Move => "移到状态",
        Modal::NewIssue => "新建议题",
        Modal::NewProject => "新建项目",
        Modal::NewStatus => "新增状态",
        Modal::NewCycle => "新建周期",
        Modal::SaveView => "保存当前视图",
        Modal::Launch => unreachable!(),
    };
    let body = match kind {
        Modal::Commands | Modal::Projects | Modal::Move | Modal::IssueMenu => {
            picker(ui.clone(), kind).into_element()
        }
        _ => form(ui.clone(), kind).into_element(),
    };
    dialog()
        .on_cancel({
            let ui = ui.clone();
            move || {
                if !ui.composing.get_untracked() {
                    ui.close();
                }
            }
        })
        .child(
            view()
                .flex_1()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000063))
                .child(
                    view()
                        .w(px(540.0))
                        .class("max-w-full rounded-lg border border-border bg-popover")
                        .p_4()
                        .gap_2()
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(
                                    text(title)
                                        .flex_1()
                                        .class("text-lg font-semibold text-foreground"),
                                )
                                .child(control("关闭", {
                                    let ui = ui.clone();
                                    move || ui.close()
                                })),
                        )
                        .child(body),
                ),
        )
        .into_element()
}

fn picker(ui: Rc<Ui>, kind: Modal) -> impl IntoElement {
    let choices = {
        let ui = ui.clone();
        computed(move || ui.choices(kind))
    };
    command()
        .items(move || choices.with(|items| items.iter().map(Choice::item).collect::<Vec<_>>()))
        .query({
            let query = ui.query;
            move || Some(query.get())
        })
        .on_query_change({
            let query = ui.query;
            move |value: &Id| query.set(value.clone())
        })
        .searchable(matches!(kind, Modal::Commands | Modal::Projects))
        .filterable(kind == Modal::Projects)
        .placeholder("输入议题短 ID、项目、工作区或命令")
        .accessibility_label("命令结果")
        .search_accessibility_label("搜索命令")
        .auto_focus(true)
        .h(px(if matches!(kind, Modal::Commands | Modal::Projects) {
            356.0
        } else {
            232.0
        }))
        .search_bar_class("border-b border-border px-2")
        .search_class("h-10 text-sm text-foreground")
        .list_class("p-1")
        .group_class("px-2 py-1 text-xs text-muted-foreground")
        .item_class("h-9 px-2 rounded-md focus:bg-accent hover:bg-accent disabled:opacity-50")
        .render_item(|entry: &CommandEntry| {
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    text(entry.label.clone())
                        .flex_1()
                        .class("text-sm text-foreground"),
                )
                .child(
                    text(entry.shortcut_label.clone().unwrap_or_default())
                        .class("font-mono text-xs text-muted-foreground"),
                )
        })
        .empty(text("没有匹配项").class("px-3 py-2 text-sm text-muted-foreground"))
        .on_cancel({
            let ui = ui.clone();
            move || ui.close()
        })
        .on_select(move |id: &Id| {
            if let Some(choice) = choices
                .with_untracked(|items| items.iter().find(|choice| &choice.key == id).cloned())
            {
                ui.run_choice(&choice);
            }
        })
}

fn field(
    ui: &Ui,
    label: &'static str,
    value: Signal<Id>,
    autofocus: bool,
) -> nsh_core::element::Input {
    let composing = ui.composing;
    input().class("h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground focus:border-ring")
        .value(move || value.get().as_str().to_owned()).placeholder(label).label(label).auto_focus(autofocus)
        .on_change(move |text: &str| value.set(text.to_owned().into()))
        .on_composition_change(move |event: &Composition| composing.set(event.composing))
}

fn form(ui: Rc<Ui>, kind: Modal) -> impl IntoElement {
    let primary = match kind {
        Modal::NewIssue => "议题标题",
        Modal::NewProject => "项目名称",
        Modal::NewStatus => "状态名称",
        Modal::NewCycle => "周期名称",
        _ => "视图名称",
    };
    view().gap_2()
        .child(field(&ui, primary, ui.draft, true).on_submit({ let ui = ui.clone(); move |_: &str| { if kind != Modal::NewCycle { ui.submit_form(kind); } } }))
        .child(when(move || kind == Modal::NewCycle, { let ui = ui.clone(); move || view().gap_2()
            .child(field(&ui, "开始日期 YYYY-MM-DD", ui.secondary, false))
            .child(field(&ui, "结束日期 YYYY-MM-DD", ui.tertiary, false).on_submit({ let ui = ui.clone(); move |_: &str| ui.submit_form(kind) })) }))
        .child(when(move || kind == Modal::NewIssue, { let ui = ui.clone(); move || {
            view().flex_row().flex_wrap().gap_2().child(dynamic({ let ui = ui.clone(); move || ui.board.with(|board| board.as_ref().map(|board| board.statuses().iter().map(|status| {
                let id = Id::from(status.id.clone());
                let label = status.name.clone();
                control(label, { let ui = ui.clone(); let id = id.clone(); move || ui.status_choice.set(Some(id.clone())) })
                    .app_class({ let ui = ui.clone(); move || Some(Id::from(if ui.status_choice.get().as_ref() == Some(&id) { "rounded-md border border-border bg-accent px-2" } else { "rounded-md border border-border bg-card px-2" })) })
            }).collect::<Vec<_>>()).unwrap_or_default()) }))
        } }))
        .child(text(ui.status).class("text-xs text-muted-foreground"))
        .child(control("保存", move || ui.submit_form(kind)))
}

impl Ui {
    fn submit_form(self: &Rc<Self>, kind: Modal) {
        if self.composing.get_untracked() {
            return;
        }
        let title = self.draft.get_untracked();
        if title.as_str().trim().is_empty() {
            self.fail("名称不能为空");
            return;
        }
        let project = self.project.get_untracked();
        if kind == Modal::NewIssue {
            let (Some(project), Some(status)) = (project, self.status_choice.get_untracked())
            else {
                self.fail("请选择项目与状态");
                return;
            };
            self.invoke(
                Action::CreateIssue {
                    title: title.as_str(),
                    status: StatusRef {
                        project_id: ProjectId(project.as_str()),
                        status_id: StatusId(status.as_str()),
                    },
                },
                ActionSource::Keyboard,
            );
            return;
        }
        if kind != Modal::NewProject && project.is_none() {
            self.fail("请先选择项目");
            return;
        }
        let ui = self.clone();
        let begin = ui.secondary.get_untracked();
        let end = ui.tertiary.get_untracked();
        let view_filter =
            serde_json::to_value(effective_filter(&ui)).expect("serializable issue filter");
        spawn(async move {
            let project = project.as_ref().map(|project| ProjectId(project.as_str()));
            let name = title.as_str();
            let result = match kind {
                Modal::NewProject => ui.api.create_project(name).await.map(|project| {
                    ui.choose_project(project.id.clone().into(), None);
                    ui.projects.update(|projects| projects.push(project));
                }),
                Modal::NewStatus => ui
                    .api
                    .create_status(
                        project.unwrap(),
                        &SaveStatus {
                            name,
                            color: "#00695F",
                            category: StatusCategory::Unstarted,
                        },
                    )
                    .await
                    .map(|_| ui.reload.update(|n| *n += 1)),
                Modal::NewCycle => ui
                    .api
                    .create_cycle(
                        project.unwrap(),
                        &SaveCycle {
                            name,
                            start_date: begin.as_str(),
                            end_date: end.as_str(),
                        },
                    )
                    .await
                    .map(|cycle| ui.cycles.update(|cycles| cycles.push(cycle))),
                Modal::SaveView => ui
                    .api
                    .create_saved_view(
                        project.unwrap(),
                        &SaveView {
                            name,
                            filter: &view_filter,
                        },
                    )
                    .await
                    .map(|view| ui.views.update(|views| views.push(view))),
                _ => unreachable!(),
            };
            match result {
                Ok(()) => {
                    ui.status.set("已保存".into());
                    ui.close();
                }
                Err(error) => ui.fail(error),
            }
        });
    }
}

fn detail(ui: Rc<Ui>, wide: bool) -> impl IntoElement {
    let settings = if wide {
        view()
            .w(px(360.0))
            .flex_shrink_0()
            .class("border-l border-border bg-card")
    } else {
        view().flex_1()
    };
    settings.min_h(px(0.0)).child(dynamic(move || {
        let id = ui.detail.get()?;
        let issue = ui
            .board
            .with(|board| board.as_ref()?.issue(IssueId(id.as_str())).cloned())?;
        Some(detail_body(ui.clone(), id, issue))
    }))
}

fn detail_body(ui: Rc<Ui>, id: Id, issue: Issue) -> impl IntoElement {
    let editor = ui.editing;
    let title = issue.title;
    let description = issue.description.unwrap_or_default();
    let triage = issue.triage;
    let cycle = issue.cycle_id;
    let issue_project: Id = issue.project_id.into();
    let status_name = ui
        .board
        .with_untracked(|board| {
            board
                .as_ref()?
                .statuses()
                .iter()
                .find(|status| status.id == issue.status_id)
                .map(|status| status.name.clone())
        })
        .unwrap_or_default();
    scroll().flex_1().min_h(px(0.0)).child(
        view()
            .p_4()
            .gap_3()
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(control("返回", {
                        let ui = ui.clone();
                        move || {
                            ui.detail.set(None);
                            ui.editing.set(None);
                            ui.list_ref.focus();
                        }
                    }))
                    .child(
                        text(issue.simple_id)
                            .class("font-mono text-sm text-muted-foreground")
                            .flex_1(),
                    )
                    .child(control("更多操作", {
                        let ui = ui.clone();
                        move || ui.open(Modal::IssueMenu)
                    })),
            )
            .child(when(move || editor.get() != Some(false), {
                let ui = ui.clone();
                let id = id.clone();
                move || {
                    view()
                        .gap_2()
                        .child(text(title.clone()).class("text-lg font-semibold text-foreground"))
                        .child(control("编辑标题", {
                            let ui = ui.clone();
                            let id = id.clone();
                            move || ui.edit_field(&id, false)
                        }))
                }
            }))
            .child(when(move || editor.get() == Some(false), {
                let ui = ui.clone();
                let id = id.clone();
                move || inline_editor(ui.clone(), id.clone(), false)
            }))
            .child(when(move || editor.get() != Some(true), {
                let ui = ui.clone();
                let id = id.clone();
                move || {
                    view()
                        .gap_2()
                        .child(
                            text(if description.is_empty() {
                                "暂无描述".into()
                            } else {
                                description.clone()
                            })
                            .class("text-sm text-muted-foreground"),
                        )
                        .child(control("编辑描述", {
                            let ui = ui.clone();
                            let id = id.clone();
                            move || ui.edit_field(&id, true)
                        }))
                }
            }))
            .child(when(move || editor.get() == Some(true), {
                let ui = ui.clone();
                let id = id.clone();
                move || inline_editor(ui.clone(), id.clone(), true)
            }))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(control(format!("状态 · {status_name}"), {
                        let ui = ui.clone();
                        move || ui.open(Modal::Move)
                    }))
                    .child(control(
                        if triage {
                            "完成分诊"
                        } else {
                            "移入待分诊"
                        },
                        {
                            let ui = ui.clone();
                            let id = id.clone();
                            move || ui.edit(&id, IssueEdit::Triage(!triage), ActionSource::Pointer)
                        },
                    )),
            )
            .child(
                view()
                    .gap_2()
                    .child(text("周期").class("text-xs font-semibold text-muted-foreground"))
                    .child(dynamic({
                        let ui = ui.clone();
                        let id = id.clone();
                        move || {
                            ui.cycles.with(|cycles| {
                                cycles
                                    .iter()
                                    .map(|item| {
                                        let cycle_id = item.id.clone();
                                        let label = if cycle.as_ref() == Some(&item.id) {
                                            format!("当前 · {}", item.name)
                                        } else {
                                            item.name.clone()
                                        };
                                        control(label, {
                                            let ui = ui.clone();
                                            let id = id.clone();
                                            let selected = cycle.as_ref() == Some(&item.id);
                                            move || {
                                                ui.edit(
                                                    &id,
                                                    IssueEdit::Cycle(
                                                        (!selected).then_some(cycle_id.as_str()),
                                                    ),
                                                    ActionSource::Pointer,
                                                )
                                            }
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                        }
                    })),
            )
            .child(
                view()
                    .gap_2()
                    .child(text("子议题").class("text-xs font-semibold text-muted-foreground"))
                    .child(dynamic({
                        let ui = ui.clone();
                        let id = id.clone();
                        move || {
                            ui.board.with(|board| {
                                board
                                    .as_ref()
                                    .map(|board| {
                                        board
                                            .issues()
                                            .iter()
                                            .filter(|child| {
                                                child.parent_issue_id.as_deref()
                                                    == Some(id.as_str())
                                            })
                                            .map(|child| {
                                                let child_id: Id = child.id.clone().into();
                                                control(
                                                    format!("{} {}", child.simple_id, child.title),
                                                    {
                                                        let ui = ui.clone();
                                                        move || {
                                                            ui.open_issue(
                                                                &child_id,
                                                                ActionSource::Pointer,
                                                            )
                                                        }
                                                    },
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default()
                            })
                        }
                    })),
            )
            .child(control("启动 Agent 工作区", move || {
                ui.invoke(
                    Action::StartWorkspace(ui.issue_ref(&issue_project, &id)),
                    ActionSource::Pointer,
                )
            })),
    )
}

fn inline_editor(ui: Rc<Ui>, id: Id, description: bool) -> impl IntoElement {
    let save = {
        let ui = ui.clone();
        move |origin| {
            let value = ui.draft.get_untracked();
            let value = value.as_str();
            let edit = if description {
                IssueEdit::Description((!value.is_empty()).then_some(value))
            } else {
                IssueEdit::Title(value)
            };
            ui.edit(&id, edit, origin);
        }
    };
    let keyboard = save.clone();
    let input = field(
        &ui,
        if description {
            "议题描述"
        } else {
            "议题标题"
        },
        ui.draft,
        true,
    )
    .on_submit(move |_: &str| keyboard(ActionSource::Keyboard));
    let input = if description {
        input.h(px(104.0)).multiline(true)
    } else {
        input
    };
    view().gap_2().child(input).child(
        view()
            .flex_row()
            .gap_2()
            .child(control("保存", move || save(ActionSource::Pointer)))
            .child(control("取消编辑", move || {
                ui.editing.set(None);
                ui.composing.set(false);
                ui.list_ref.focus();
            })),
    )
}
