//! Shared IDs, action metadata, and dispatch rules for the NSH input adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId<'a>(pub &'a str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IssueId<'a>(pub &'a str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusId<'a>(pub &'a str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceId<'a>(pub &'a str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IssueRef<'a> {
    pub project_id: ProjectId<'a>,
    pub issue_id: IssueId<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityId<'a> {
    Issue(IssueRef<'a>),
    Project(ProjectId<'a>),
    Workspace(WorkspaceId<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusRef<'a> {
    pub project_id: ProjectId<'a>,
    pub status_id: StatusId<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardIssue<'a> {
    pub issue: IssueRef<'a>,
    pub status_id: StatusId<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativePosition<'a> {
    Before(BoardIssue<'a>),
    After(BoardIssue<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Urgent,
    High,
    Medium,
    Low,
}

/// One field per inline edit. `None` clears a nullable field in the M2 PATCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueEdit<'a> {
    Title(&'a str),
    Description(Option<&'a str>),
    Priority(Option<Priority>),
    StartDate(Option<&'a str>),
    TargetDate(Option<&'a str>),
    Parent(Option<IssueRef<'a>>),
    Cycle(Option<&'a str>),
    Triage(bool),
    Tags(&'a [&'a str]),
}

/// Persist and transmit `as_str()`, which is independent of enum order and labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionId {
    OpenCommand,
    OpenIssue,
    CreateIssue,
    MoveIssue,
    EditIssue,
    OpenProject,
    OpenWorkspace,
    StartWorkspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionScope {
    Global,
    Project,
    Issue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputState {
    Ready,
    Editing,
    Composing,
}

/// The current logical scope and target state, including while a menu owns focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionContext {
    pub scope: ActionScope,
    pub input: InputState,
    pub connected: bool,
    pub mutation_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    CompositionInProgress,
    FinishEditing,
    SelectProject,
    SelectIssue,
    Disconnected,
    SaveInProgress,
    WrongProject,
    InvalidMoveAnchor,
    AlreadyInStatus,
    EmptyTitle,
    InvalidParent,
}

impl Unavailable {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CompositionInProgress => "请先完成输入法组合",
            Self::FinishEditing => "请先保存或取消当前编辑",
            Self::SelectProject => "请先选择项目",
            Self::SelectIssue => "请先选择议题",
            Self::Disconnected => "本地服务未连接",
            Self::SaveInProgress => "正在保存，请稍候",
            Self::WrongProject => "目标属于其他项目",
            Self::InvalidMoveAnchor => "排序目标必须是目标状态中的另一条议题",
            Self::AlreadyInStatus => "议题已在该状态中",
            Self::EmptyTitle => "标题不能为空",
            Self::InvalidParent => "议题不能是自己的父议题",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActionSpec {
    pub id: ActionId,
    pub key: &'static str,
    pub label: &'static str,
    pub aliases: &'static [&'static str],
    pub scope: ActionScope,
    pub shortcut: Option<&'static str>,
    pub in_palette: bool,
    writes: bool,
}

pub const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        id: ActionId::OpenCommand,
        key: "command.open",
        label: "搜索命令",
        aliases: &["命令菜单", "command palette", "search"],
        scope: ActionScope::Global,
        shortcut: Some("mod+k"),
        in_palette: false,
        writes: false,
    },
    ActionSpec {
        id: ActionId::OpenIssue,
        key: "issue.open",
        label: "打开议题",
        aliases: &["议题详情", "open issue"],
        scope: ActionScope::Global,
        shortcut: Some("enter"),
        in_palette: false,
        writes: false,
    },
    ActionSpec {
        id: ActionId::CreateIssue,
        key: "issue.create",
        label: "新建议题",
        aliases: &["新增任务", "new issue", "create issue"],
        scope: ActionScope::Project,
        shortcut: None,
        in_palette: true,
        writes: true,
    },
    ActionSpec {
        id: ActionId::MoveIssue,
        key: "issue.move",
        label: "移到状态",
        aliases: &["移动议题", "move issue", "change status"],
        scope: ActionScope::Issue,
        shortcut: None,
        in_palette: true,
        writes: true,
    },
    ActionSpec {
        id: ActionId::EditIssue,
        key: "issue.edit",
        label: "编辑议题",
        aliases: &["行内编辑", "edit issue"],
        scope: ActionScope::Issue,
        shortcut: None,
        in_palette: true,
        writes: true,
    },
    ActionSpec {
        id: ActionId::OpenProject,
        key: "project.open",
        label: "打开项目",
        aliases: &["open project"],
        scope: ActionScope::Global,
        shortcut: None,
        in_palette: false,
        writes: false,
    },
    ActionSpec {
        id: ActionId::OpenWorkspace,
        key: "workspace.open",
        label: "打开工作区",
        aliases: &["open workspace"],
        scope: ActionScope::Global,
        shortcut: None,
        in_palette: false,
        writes: false,
    },
    ActionSpec {
        id: ActionId::StartWorkspace,
        key: "workspace.start",
        label: "启动 Agent 工作区",
        aliases: &["运行议题", "start workspace", "launch agent"],
        scope: ActionScope::Issue,
        shortcut: None,
        in_palette: true,
        writes: true,
    },
];

impl ActionId {
    pub const fn spec(self) -> &'static ActionSpec {
        &ACTIONS[self as usize]
    }

    pub const fn as_str(self) -> &'static str {
        self.spec().key
    }

    pub fn from_key(key: &str) -> Option<Self> {
        ACTIONS
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| spec.id)
    }

    pub const fn availability(self, context: ActionContext) -> Result<(), Unavailable> {
        let spec = self.spec();
        match context.input {
            InputState::Composing => return Err(Unavailable::CompositionInProgress),
            InputState::Editing if !matches!(self, Self::EditIssue) => {
                return Err(Unavailable::FinishEditing);
            }
            _ => {}
        }
        match (spec.scope, context.scope) {
            (ActionScope::Project, ActionScope::Global) => return Err(Unavailable::SelectProject),
            (ActionScope::Issue, ActionScope::Global | ActionScope::Project) => {
                return Err(Unavailable::SelectIssue);
            }
            _ => {}
        }
        if spec.writes {
            if !context.connected {
                return Err(Unavailable::Disconnected);
            }
            if context.mutation_pending {
                return Err(Unavailable::SaveInProgress);
            }
        }
        Ok(())
    }
}

/// A concrete invocation. A menu may use `ActionId` before collecting arguments.
#[derive(Debug, PartialEq, Eq)]
pub enum Action<'a> {
    OpenCommand,
    OpenEntity(EntityId<'a>),
    CreateIssue {
        status: StatusRef<'a>,
        title: &'a str,
    },
    MoveIssue {
        source: BoardIssue<'a>,
        destination: StatusRef<'a>,
        position: Option<RelativePosition<'a>>,
    },
    EditIssue {
        issue: IssueRef<'a>,
        change: IssueEdit<'a>,
    },
    StartWorkspace(IssueRef<'a>),
}

impl Action<'_> {
    pub const fn id(&self) -> ActionId {
        match self {
            Self::OpenCommand => ActionId::OpenCommand,
            Self::OpenEntity(EntityId::Issue(_)) => ActionId::OpenIssue,
            Self::OpenEntity(EntityId::Project(_)) => ActionId::OpenProject,
            Self::OpenEntity(EntityId::Workspace(_)) => ActionId::OpenWorkspace,
            Self::CreateIssue { .. } => ActionId::CreateIssue,
            Self::MoveIssue { .. } => ActionId::MoveIssue,
            Self::EditIssue { .. } => ActionId::EditIssue,
            Self::StartWorkspace(_) => ActionId::StartWorkspace,
        }
    }

    /// Also used by drag previews with the current board snapshot.
    pub fn availability(&self, context: ActionContext) -> Result<(), Unavailable> {
        self.id().availability(context)?;
        match self {
            Self::MoveIssue {
                source,
                destination,
                position,
            } => {
                if source.issue.project_id != destination.project_id {
                    return Err(Unavailable::WrongProject);
                }
                match position {
                    Some(RelativePosition::Before(anchor) | RelativePosition::After(anchor))
                        if anchor.issue == source.issue
                            || anchor.issue.project_id != destination.project_id
                            || anchor.status_id != destination.status_id =>
                    {
                        return Err(Unavailable::InvalidMoveAnchor);
                    }
                    None if source.status_id == destination.status_id => {
                        return Err(Unavailable::AlreadyInStatus);
                    }
                    _ => {}
                }
            }
            Self::CreateIssue { title, .. }
            | Self::EditIssue {
                change: IssueEdit::Title(title),
                ..
            } if title.trim().is_empty() => return Err(Unavailable::EmptyTitle),
            Self::EditIssue {
                issue,
                change: IssueEdit::Parent(Some(parent)),
            } => {
                if issue.project_id != parent.project_id {
                    return Err(Unavailable::WrongProject);
                }
                if issue == parent {
                    return Err(Unavailable::InvalidParent);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSource {
    Keyboard,
    Pointer,
    DragDrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusAnchor<'a> {
    CommandButton,
    Entity(EntityId<'a>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invocation<'a> {
    pub action: Action<'a>,
    pub source: ActionSource,
    pub return_focus: Option<FocusAnchor<'a>>,
}

impl<'a> Invocation<'a> {
    /// Consumes a validated event and calls the shared handler once. The handler
    /// owns confirmation, optimistic state, and submission against the live board.
    pub fn dispatch<R>(
        self,
        context: ActionContext,
        handler: impl FnOnce(Self) -> R,
    ) -> Result<R, Unavailable> {
        self.action.availability(context)?;
        Ok(handler(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUE: IssueRef<'_> = IssueRef {
        project_id: ProjectId("project"),
        issue_id: IssueId("issue"),
    };
    const SOURCE: BoardIssue<'_> = BoardIssue {
        issue: ISSUE,
        status_id: StatusId("backlog"),
    };
    const DESTINATION: StatusRef<'_> = StatusRef {
        project_id: ISSUE.project_id,
        status_id: StatusId("started"),
    };
    const READY: ActionContext = ActionContext {
        scope: ActionScope::Issue,
        input: InputState::Ready,
        connected: true,
        mutation_pending: false,
    };

    #[test]
    fn registry_has_stable_keys() {
        let keys = [
            "command.open",
            "issue.open",
            "issue.create",
            "issue.move",
            "issue.edit",
            "project.open",
            "workspace.open",
            "workspace.start",
        ];
        assert_eq!(ACTIONS.len(), keys.len());
        for (spec, key) in ACTIONS.iter().zip(keys) {
            assert_eq!(spec.id.as_str(), key);
            assert_eq!(ActionId::from_key(key), Some(spec.id));
        }
        assert_eq!(ActionId::from_key("unknown"), None);
    }

    #[test]
    fn all_sources_dispatch_the_same_move_once() {
        let mut calls = 0;
        for source in [
            ActionSource::Keyboard,
            ActionSource::Pointer,
            ActionSource::DragDrop,
        ] {
            let focus = Some(FocusAnchor::Entity(EntityId::Issue(ISSUE)));
            let event = Invocation {
                action: Action::MoveIssue {
                    source: SOURCE,
                    destination: DESTINATION,
                    position: None,
                },
                source,
                return_focus: focus,
            };
            assert_eq!(event.action.availability(READY), Ok(()));
            assert_eq!(
                event.dispatch(READY, |event| {
                    calls += 1;
                    assert_eq!(event.source, source);
                    assert_eq!(event.return_focus, focus);
                    assert_eq!(event.action.id(), ActionId::MoveIssue);
                    event.action
                }),
                Ok(Action::MoveIssue {
                    source: SOURCE,
                    destination: DESTINATION,
                    position: None,
                }),
            );
        }
        assert_eq!(calls, 3);
    }

    #[test]
    fn invalid_moves_never_reach_the_handler() {
        let same_status = StatusRef {
            status_id: SOURCE.status_id,
            ..DESTINATION
        };
        let other_project = StatusRef {
            project_id: ProjectId("other"),
            ..DESTINATION
        };
        for (destination, position, reason) in [
            (other_project, None, Unavailable::WrongProject),
            (same_status, None, Unavailable::AlreadyInStatus),
            (
                same_status,
                Some(RelativePosition::After(SOURCE)),
                Unavailable::InvalidMoveAnchor,
            ),
            (
                DESTINATION,
                Some(RelativePosition::Before(BoardIssue {
                    issue: IssueRef {
                        issue_id: IssueId("anchor"),
                        ..ISSUE
                    },
                    ..SOURCE
                })),
                Unavailable::InvalidMoveAnchor,
            ),
        ] {
            let event = Invocation {
                action: Action::MoveIssue {
                    source: SOURCE,
                    destination,
                    position,
                },
                source: ActionSource::DragDrop,
                return_focus: None,
            };
            assert_eq!(event.action.availability(READY), Err(reason));
            assert_eq!(
                event.dispatch(READY, |_| panic!("invalid move dispatched")),
                Err(reason)
            );
        }
        let anchor = BoardIssue {
            issue: IssueRef {
                issue_id: IssueId("anchor"),
                ..ISSUE
            },
            ..SOURCE
        };
        assert_eq!(
            Action::MoveIssue {
                source: SOURCE,
                destination: same_status,
                position: Some(RelativePosition::After(anchor))
            }
            .availability(READY),
            Ok(()),
        );
    }

    #[test]
    fn action_context_gates_text_input_scope_and_writes() {
        let edit = Action::EditIssue {
            issue: ISSUE,
            change: IssueEdit::Description(None),
        };
        let editing = ActionContext {
            input: InputState::Editing,
            ..READY
        };
        let composing = ActionContext {
            input: InputState::Composing,
            ..READY
        };
        let disconnected = ActionContext {
            connected: false,
            ..READY
        };
        let pending = ActionContext {
            mutation_pending: true,
            ..READY
        };
        let global = ActionContext {
            scope: ActionScope::Global,
            ..READY
        };
        assert_eq!(edit.availability(editing), Ok(()));
        assert_eq!(
            edit.availability(composing),
            Err(Unavailable::CompositionInProgress)
        );
        assert_eq!(
            edit.availability(disconnected),
            Err(Unavailable::Disconnected)
        );
        assert_eq!(edit.availability(pending), Err(Unavailable::SaveInProgress));
        assert_eq!(edit.availability(global), Err(Unavailable::SelectIssue));
        assert_eq!(
            ActionId::CreateIssue.availability(global),
            Err(Unavailable::SelectProject)
        );
        let open = Action::OpenEntity(EntityId::Issue(ISSUE));
        assert_eq!(open.availability(editing), Err(Unavailable::FinishEditing));
        assert_eq!(open.availability(disconnected), Ok(()));
        assert_eq!(open.availability(global), Ok(()));
    }

    #[test]
    fn inline_edit_validates_title_and_parent() {
        for (change, reason) in [
            (IssueEdit::Title("　 "), Unavailable::EmptyTitle),
            (IssueEdit::Parent(Some(ISSUE)), Unavailable::InvalidParent),
            (
                IssueEdit::Parent(Some(IssueRef {
                    project_id: ProjectId("other"),
                    ..ISSUE
                })),
                Unavailable::WrongProject,
            ),
        ] {
            assert_eq!(
                Action::EditIssue {
                    issue: ISSUE,
                    change
                }
                .availability(READY),
                Err(reason)
            );
        }
    }
}
