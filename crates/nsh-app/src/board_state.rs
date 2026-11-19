//! State transitions shared by the issue list and the board.

use std::{cmp::Ordering, rc::Rc};

use crate::{
    issue_actions::{
        Action, ActionContext, BoardIssue, EntityId, FocusAnchor, Invocation, IssueId, IssueRef,
        ProjectId, RelativePosition, StatusId, StatusRef, Unavailable, WorkspaceId,
    },
    issues_api::{Issue, IssuePatch, IssueUpdate, Status},
};

#[derive(Debug, PartialEq, Eq)]
pub enum BoardError {
    Unavailable(Unavailable),
    WrongProject,
    MissingIssue,
    MissingStatus,
}

impl BoardError {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unavailable(reason) => reason.label(),
            Self::WrongProject => "议题或状态不属于当前项目",
            Self::MissingIssue => "议题已不存在，请刷新看板",
            Self::MissingStatus => "目标状态已不存在，请刷新看板",
        }
    }
}

impl From<Unavailable> for BoardError {
    fn from(value: Unavailable) -> Self {
        Self::Unavailable(value)
    }
}

/// A saved focus anchor owns its IDs so it survives an HTTP round trip.
#[derive(Debug, PartialEq, Eq)]
pub enum SavedFocus {
    CommandButton,
    Issue {
        project_id: String,
        issue_id: String,
    },
    Project(String),
    Workspace(String),
}

impl SavedFocus {
    pub fn anchor(&self) -> FocusAnchor<'_> {
        let entity = match self {
            Self::CommandButton => return FocusAnchor::CommandButton,
            Self::Issue {
                project_id,
                issue_id,
            } => EntityId::Issue(IssueRef {
                project_id: ProjectId(project_id),
                issue_id: IssueId(issue_id),
            }),
            Self::Project(id) => EntityId::Project(ProjectId(id)),
            Self::Workspace(id) => EntityId::Workspace(WorkspaceId(id)),
        };
        FocusAnchor::Entity(entity)
    }
}

impl From<FocusAnchor<'_>> for SavedFocus {
    fn from(value: FocusAnchor<'_>) -> Self {
        match value {
            FocusAnchor::CommandButton => Self::CommandButton,
            FocusAnchor::Entity(EntityId::Issue(issue)) => Self::Issue {
                project_id: issue.project_id.0.into(),
                issue_id: issue.issue_id.0.into(),
            },
            FocusAnchor::Entity(EntityId::Project(id)) => Self::Project(id.0.into()),
            FocusAnchor::Entity(EntityId::Workspace(id)) => Self::Workspace(id.0.into()),
        }
    }
}

#[derive(Debug)]
pub enum BoardEffect<'a> {
    Navigate(Invocation<'a>),
    Submit(Submission),
}

#[derive(Debug)]
enum Request {
    Create { status_id: String, title: String },
    Patch(Vec<IssueUpdate>),
}

#[derive(Debug)]
pub struct Submission {
    ticket: Rc<()>,
    project_id: String,
    request: Request,
}

#[derive(Debug)]
pub struct BoardResponse {
    ticket: Rc<()>,
    result: Result<Vec<Issue>, String>,
}

#[derive(Debug)]
pub struct FinishedSubmission {
    pub result: Result<(), String>,
    pub return_focus: Option<SavedFocus>,
}

impl Submission {
    /// Finish a cancelled request through the same rollback path as a network error.
    pub fn cancel(self) -> BoardResponse {
        BoardResponse {
            ticket: self.ticket,
            result: Err("操作已取消".into()),
        }
    }

    /// Takes only the owned request across `await`; the live board remains available to UI code.
    #[cfg(any(windows, target_arch = "wasm32"))]
    pub async fn execute(self, api: &crate::Api) -> BoardResponse {
        use crate::issues_api::CreateIssue;

        let project = ProjectId(&self.project_id);
        let result = match &self.request {
            Request::Create { status_id, title } => api
                .create_issue(
                    project,
                    &CreateIssue {
                        title,
                        status_id,
                        ..CreateIssue::default()
                    },
                )
                .await
                .map(|issue| vec![issue]),
            Request::Patch(updates) => match updates.as_slice() {
                [one] => api
                    .update_issue(
                        IssueRef {
                            project_id: project,
                            issue_id: IssueId(&one.issue_id),
                        },
                        &one.changes,
                    )
                    .await
                    .map(|issue| vec![issue]),
                _ => api.update_issues(project, updates).await,
            },
        };
        BoardResponse {
            ticket: self.ticket,
            result,
        }
    }
}

#[derive(Debug)]
struct Pending {
    ticket: Rc<()>,
    request_kind: PendingKind,
    return_focus: Option<SavedFocus>,
}

#[derive(Debug)]
enum PendingKind {
    Create { status_id: String },
    Patch(Vec<IssueUpdate>),
}

/// One complete, unfiltered M2 project. Views should filter these rows instead
/// of feeding filtered server results back into a board used for drag ordering.
#[derive(Debug)]
pub struct BoardState {
    project_id: String,
    statuses: Vec<Status>,
    issues: Vec<Issue>,
    selection: Option<String>,
    scroll_anchor: Option<String>,
    pending: Option<Pending>,
    revision: u64,
}

impl BoardState {
    pub fn new(project: ProjectId<'_>, statuses: Vec<Status>, mut issues: Vec<Issue>) -> Self {
        issues.sort_unstable_by(issue_order);
        Self {
            project_id: project.0.into(),
            statuses,
            issues,
            selection: None,
            scroll_anchor: None,
            pending: None,
            revision: 0,
        }
    }

    #[cfg(any(windows, target_arch = "wasm32"))]
    pub async fn load(api: &crate::Api, project: ProjectId<'_>) -> Result<Self, String> {
        let filter = crate::issues_api::IssueFilter::default();
        let (statuses, issues) =
            futures_util::try_join!(api.statuses(project), api.issues(project, &filter))?;
        Ok(Self::new(project, statuses, issues))
    }

    pub fn project(&self) -> ProjectId<'_> {
        ProjectId(&self.project_id)
    }

    pub fn statuses(&self) -> &[Status] {
        &self.statuses
    }

    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }

    pub fn issue(&self, id: IssueId<'_>) -> Option<&Issue> {
        self.issues.iter().find(|issue| issue.id == id.0)
    }

    pub fn selection(&self) -> Option<IssueId<'_>> {
        self.selection.as_deref().map(IssueId)
    }

    pub fn scroll_anchor(&self) -> Option<IssueId<'_>> {
        self.scroll_anchor.as_deref().map(IssueId)
    }

    pub fn select(&mut self, issue: Option<IssueId<'_>>) {
        self.selection = issue
            .filter(|id| self.issue(*id).is_some())
            .map(|id| id.0.into());
    }

    pub fn set_scroll_anchor(&mut self, issue: Option<IssueId<'_>>) {
        self.scroll_anchor = issue
            .filter(|id| self.issue(*id).is_some())
            .map(|id| id.0.into());
    }

    pub fn is_saving(&self) -> bool {
        self.pending.is_some()
    }

    /// Capture this before starting a load and compare it when applying the response.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn refresh(&mut self, revision: u64, snapshot: Self) -> bool {
        if self.revision != revision || self.is_saving() || snapshot.project_id != self.project_id {
            return false;
        }
        self.statuses = snapshot.statuses;
        self.issues = snapshot.issues;
        if self.selection().is_some_and(|id| self.issue(id).is_none()) {
            self.selection = None;
        }
        if self
            .scroll_anchor()
            .is_some_and(|id| self.issue(id).is_none())
        {
            self.scroll_anchor = self
                .selection
                .clone()
                .or_else(|| self.issues.first().map(|issue| issue.id.clone()));
        }
        self.revision += 1;
        true
    }

    /// Re-read actual source/anchor status IDs, then apply the core action availability rules.
    pub fn dispatch<'a>(
        &mut self,
        invocation: Invocation<'a>,
        mut context: ActionContext,
    ) -> Result<BoardEffect<'a>, BoardError> {
        context.mutation_pending |= self.is_saving();
        if let Action::MoveIssue {
            source,
            destination,
            position,
        } = invocation.action
        {
            invocation.action.id().availability(context)?;
            self.require_status(destination)?;
            let source = self.board_issue(source.issue)?;
            let position = match position {
                Some(RelativePosition::Before(anchor)) => {
                    Some(RelativePosition::Before(self.board_issue(anchor.issue)?))
                }
                Some(RelativePosition::After(anchor)) => {
                    Some(RelativePosition::After(self.board_issue(anchor.issue)?))
                }
                None => None,
            };
            let live = Invocation {
                action: Action::MoveIssue {
                    source,
                    destination,
                    position,
                },
                source: invocation.source,
                return_focus: invocation.return_focus,
            };
            let changes = live.dispatch(context, |_| {
                self.move_updates(source.issue.issue_id, destination.status_id, position)
            })?;
            let (request, kind) = self.prepare_patch(changes);
            return Ok(BoardEffect::Submit(self.submit(
                request,
                kind,
                invocation.return_focus,
            )));
        }
        invocation.dispatch(context, |event| self.apply(event))?
    }

    pub fn finish(&mut self, response: BoardResponse) -> Option<FinishedSubmission> {
        if !Rc::ptr_eq(&self.pending.as_ref()?.ticket, &response.ticket) {
            return None;
        }
        let pending = self.pending.take().unwrap();
        let result = match response.result {
            Ok(rows) if self.valid_response(&pending.request_kind, &rows) => {
                for row in rows {
                    self.put_issue(row);
                }
                Ok(())
            }
            error => {
                if let PendingKind::Patch(undo) = pending.request_kind {
                    for update in undo {
                        let row = self
                            .issues
                            .iter_mut()
                            .find(|row| row.id == update.issue_id)
                            .unwrap();
                        swap_patch(update.changes, row);
                    }
                }
                Err(error
                    .err()
                    .unwrap_or_else(|| "服务返回的议题与此次操作不匹配".into()))
            }
        };
        self.issues.sort_unstable_by(issue_order);
        self.revision += 1;
        Some(FinishedSubmission {
            result,
            return_focus: pending.return_focus,
        })
    }

    fn valid_response(&self, kind: &PendingKind, rows: &[Issue]) -> bool {
        if rows.iter().any(|issue| issue.project_id != self.project_id) {
            return false;
        }
        match kind {
            PendingKind::Create { status_id } => {
                matches!(rows, [issue] if issue.status_id == *status_id && self.issue(IssueId(&issue.id)).is_none())
            }
            // M2's batch returns records in request order.
            PendingKind::Patch(undo) => {
                rows.len() == undo.len()
                    && rows
                        .iter()
                        .zip(undo)
                        .all(|(row, update)| row.id == update.issue_id)
            }
        }
    }

    fn apply<'a>(&mut self, event: Invocation<'a>) -> Result<BoardEffect<'a>, BoardError> {
        let (request, request_kind) = match event.action {
            Action::CreateIssue { status, title } => {
                self.require_status(status)?;
                (
                    Request::Create {
                        status_id: status.status_id.0.into(),
                        title: title.trim().into(),
                    },
                    PendingKind::Create {
                        status_id: status.status_id.0.into(),
                    },
                )
            }
            Action::EditIssue { issue, change } => {
                self.board_issue(issue)?;
                self.prepare_patch(vec![IssueUpdate {
                    issue_id: issue.issue_id.0.into(),
                    changes: change.into(),
                }])
            }
            Action::OpenEntity(EntityId::Issue(issue)) if issue.project_id == self.project() => {
                self.board_issue(issue)?;
                self.select(Some(issue.issue_id));
                return Ok(BoardEffect::Navigate(event));
            }
            _ => return Ok(BoardEffect::Navigate(event)),
        };
        Ok(BoardEffect::Submit(self.submit(
            request,
            request_kind,
            event.return_focus,
        )))
    }

    fn submit(
        &mut self,
        request: Request,
        request_kind: PendingKind,
        return_focus: Option<FocusAnchor<'_>>,
    ) -> Submission {
        let ticket = Rc::new(());
        self.pending = Some(Pending {
            ticket: Rc::clone(&ticket),
            request_kind,
            return_focus: return_focus.map(Into::into),
        });
        self.revision += 1;
        Submission {
            ticket,
            project_id: self.project_id.clone(),
            request,
        }
    }

    fn put_issue(&mut self, row: Issue) {
        if let Some(current) = self.issues.iter_mut().find(|issue| issue.id == row.id) {
            *current = row;
        } else {
            self.issues.push(row);
        }
    }

    fn require_status(&self, status: StatusRef<'_>) -> Result<(), BoardError> {
        if status.project_id != self.project() {
            return Err(BoardError::WrongProject);
        }
        self.statuses
            .iter()
            .any(|row| row.id == status.status_id.0)
            .then_some(())
            .ok_or(BoardError::MissingStatus)
    }

    fn board_issue(&self, issue: IssueRef<'_>) -> Result<BoardIssue<'_>, BoardError> {
        if issue.project_id != self.project() {
            return Err(BoardError::WrongProject);
        }
        let row = self.issue(issue.issue_id).ok_or(BoardError::MissingIssue)?;
        Ok(BoardIssue {
            issue: row.reference(),
            status_id: StatusId(&row.status_id),
        })
    }

    fn prepare_patch(&mut self, changes: Vec<IssueUpdate>) -> (Request, PendingKind) {
        let undo = changes
            .iter()
            .map(|update| {
                let row = self
                    .issues
                    .iter_mut()
                    .find(|row| row.id == update.issue_id)
                    .unwrap();
                IssueUpdate {
                    issue_id: update.issue_id.clone(),
                    changes: swap_patch(update.changes.clone(), row),
                }
            })
            .collect();
        self.issues.sort_unstable_by(issue_order);
        (Request::Patch(changes), PendingKind::Patch(undo))
    }

    fn move_updates(
        &self,
        source: IssueId<'_>,
        destination: StatusId<'_>,
        position: Option<RelativePosition<'_>>,
    ) -> Vec<IssueUpdate> {
        let column = || {
            self.issues
                .iter()
                .filter(|row| row.status_id == destination.0 && row.id != source.0)
        };
        let mut rows = column();
        let (mut slot, mut lower, mut upper) = (0, None, None);
        let anchor = position.map(|position| match position {
            RelativePosition::Before(row) => (row.issue.issue_id, false),
            RelativePosition::After(row) => (row.issue.issue_id, true),
        });
        while let Some(row) = rows.next() {
            if let Some((id, after)) = anchor
                && row.id == id.0
            {
                if after {
                    slot += 1;
                    lower = Some(row.sort_order);
                    upper = rows.next().map(|row| row.sort_order);
                } else {
                    upper = Some(row.sort_order);
                }
                break;
            }
            slot += 1;
            lower = Some(row.sort_order);
        }
        let move_to = |sort_order| IssueUpdate {
            issue_id: source.0.into(),
            changes: IssuePatch {
                status_id: Some(destination.0.into()),
                sort_order: Some(sort_order),
                ..IssuePatch::default()
            },
        };
        if let Some(order) = between(lower, upper) {
            return vec![move_to(order)];
        }

        // Equal or adjacent floats have no representable midpoint. Re-space
        // this column and commit the moved card with its neighbors atomically.
        let mut updates = vec![move_to(slot as f64)];
        for (index, row) in column().enumerate() {
            let order = (index + usize::from(index >= slot)) as f64;
            if row.sort_order != order {
                updates.push(IssueUpdate {
                    issue_id: row.id.clone(),
                    changes: IssuePatch {
                        sort_order: Some(order),
                        ..IssuePatch::default()
                    },
                });
            }
        }
        updates
    }
}

fn issue_order(a: &Issue, b: &Issue) -> Ordering {
    // SQLite treats -0 and 0 as equal and breaks ties by issue number.
    a.sort_order
        .partial_cmp(&b.sort_order)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.issue_number.cmp(&b.issue_number))
}

fn between(lower: Option<f64>, upper: Option<f64>) -> Option<f64> {
    let order = match (lower, upper) {
        (Some(lower), Some(upper)) => lower.midpoint(upper),
        (Some(lower), None) => lower + 1.0,
        (None, Some(upper)) => upper - 1.0,
        (None, None) => 0.0,
    };
    (order.is_finite()
        && lower.is_none_or(|left| left < order)
        && upper.is_none_or(|right| order < right))
    .then_some(order)
}

fn swap_patch(mut patch: IssuePatch, issue: &mut Issue) -> IssuePatch {
    macro_rules! swap_fields {
        ($($field:ident),+ $(,)?) => {$({
            if let Some(value) = &mut patch.$field {
                std::mem::swap(value, &mut issue.$field);
            }
        })+};
    }
    swap_fields!(
        title,
        status_id,
        description,
        priority,
        start_date,
        target_date,
        sort_order,
        parent_issue_id,
        cycle_id,
        triage,
        tag_ids
    );
    patch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        issue_actions::{ActionScope, ActionSource, InputState, IssueEdit},
        issues_api::StatusCategory,
    };

    const PROJECT: ProjectId<'static> = ProjectId("project");
    const READY: ActionContext = ActionContext {
        scope: ActionScope::Issue,
        input: InputState::Ready,
        connected: true,
        mutation_pending: false,
    };

    fn issue_ref(id: &str) -> IssueRef<'_> {
        IssueRef {
            project_id: PROJECT,
            issue_id: IssueId(id),
        }
    }

    fn status_ref(id: &str) -> StatusRef<'_> {
        StatusRef {
            project_id: PROJECT,
            status_id: StatusId(id),
        }
    }

    fn row(id: &str, status: &str, number: i64, order: f64) -> Issue {
        Issue {
            id: id.into(),
            project_id: PROJECT.0.into(),
            issue_number: number,
            simple_id: format!("VO-{number}"),
            status_id: status.into(),
            title: id.into(),
            description: Some("previous".into()),
            priority: None,
            start_date: None,
            target_date: None,
            sort_order: order,
            parent_issue_id: None,
            cycle_id: None,
            triage: false,
            tag_ids: Vec::new(),
            extension_metadata: serde_json::json!({}),
            external_provider: None,
            external_id: None,
            external_url: None,
        }
    }

    fn board(issues: Vec<Issue>) -> BoardState {
        let statuses = ["backlog", "started"]
            .into_iter()
            .enumerate()
            .map(|(order, id)| Status {
                id: id.into(),
                name: id.into(),
                project_id: PROJECT.0.into(),
                color: "#000000".into(),
                category: StatusCategory::Started,
                sort_order: order as i32,
            })
            .collect();
        BoardState::new(PROJECT, statuses, issues)
    }

    fn event(action: Action<'static>) -> Invocation<'static> {
        Invocation {
            action,
            source: ActionSource::DragDrop,
            return_focus: Some(focus()),
        }
    }

    fn focus() -> FocusAnchor<'static> {
        FocusAnchor::Entity(EntityId::Issue(issue_ref("moving")))
    }

    fn move_issue(target: &'static str, before: bool) -> Action<'static> {
        Action::MoveIssue {
            source: BoardIssue {
                issue: issue_ref("moving"),
                status_id: StatusId("backlog"),
            },
            destination: status_ref(target),
            position: before.then_some(RelativePosition::Before(BoardIssue {
                issue: issue_ref("anchor"),
                status_id: StatusId("backlog"),
            })),
        }
    }

    fn submission(effect: BoardEffect<'_>) -> Submission {
        let BoardEffect::Submit(submission) = effect else {
            panic!("expected submission")
        };
        submission
    }

    #[test]
    fn move_rechecks_latest_anchor_and_preserves_ids_on_response() {
        let mut state = board(vec![
            row("moving", "backlog", 1, 1.0),
            row("anchor", "started", 2, 30.0),
            row("hidden", "started", 3, 20.0),
        ]);
        state.select(Some(IssueId("moving")));
        state.set_scroll_anchor(Some(IssueId("hidden")));
        let request = submission(
            state
                .dispatch(event(move_issue("started", true)), READY)
                .unwrap(),
        );
        let Request::Patch(updates) = &request.request else {
            panic!("expected patch")
        };
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].changes.sort_order, Some(25.0));
        assert_eq!(state.issue(IssueId("moving")).unwrap().status_id, "started");
        assert_eq!(state.selection(), Some(IssueId("moving")));
        assert_eq!(state.scroll_anchor(), Some(IssueId("hidden")));
        let reply = row("moving", "started", 1, 25.25);
        let finished = state
            .finish(BoardResponse {
                ticket: request.ticket,
                result: Ok(vec![reply.clone()]),
            })
            .unwrap();
        assert_eq!(finished.result, Ok(()));
        assert_eq!(finished.return_focus.unwrap().anchor(), focus());
        assert_eq!(state.issue(IssueId("moving")), Some(&reply));
    }

    #[test]
    fn stale_drop_uses_live_source_and_rejects_moved_anchor() {
        let mut state = board(vec![
            row("moving", "started", 1, 1.0),
            row("anchor", "backlog", 2, 2.0),
        ]);
        for (before, reason) in [
            (false, Unavailable::AlreadyInStatus),
            (true, Unavailable::InvalidMoveAnchor),
        ] {
            assert_eq!(
                state
                    .dispatch(event(move_issue("started", before)), READY)
                    .unwrap_err(),
                BoardError::Unavailable(reason)
            );
        }
        assert!(!state.is_saving());
        let request = submission(
            state
                .dispatch(event(move_issue("backlog", false)), READY)
                .unwrap(),
        );
        assert_eq!(state.issue(IssueId("moving")).unwrap().sort_order, 3.0);
        state.finish(request.cancel()).unwrap();
    }

    #[test]
    fn dense_column_batch_rolls_back_and_rejects_stale_refresh() {
        let rows = vec![
            row("moving", "backlog", 1, 1.0),
            row("first", "started", 2, 4.0),
            row("anchor", "started", 3, 4.0),
        ];
        let mut state = board(rows.clone());
        let revision = state.revision();
        let request = submission(
            state
                .dispatch(event(move_issue("started", true)), READY)
                .unwrap(),
        );
        let Request::Patch(updates) = &request.request else {
            panic!("expected patch")
        };
        assert_eq!(updates.len(), 3);
        assert_eq!(
            state
                .issues()
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "moving", "anchor"]
        );
        assert!(!state.refresh(revision, board(rows.clone())));
        assert_eq!(
            state
                .dispatch(
                    event(Action::EditIssue {
                        issue: issue_ref("moving"),
                        change: IssueEdit::Title("blocked")
                    }),
                    READY
                )
                .unwrap_err(),
            BoardError::Unavailable(Unavailable::SaveInProgress)
        );
        let finished = state
            .finish(BoardResponse {
                ticket: request.ticket,
                result: Err("network failed".into()),
            })
            .unwrap();
        assert_eq!(finished.result, Err("network failed".into()));
        assert_eq!(state.issues(), rows);
        assert!(!state.is_saving());
        assert!(!state.refresh(revision, board(rows)));
    }

    #[test]
    fn create_uses_server_id_and_ignores_another_boards_response() {
        let mut state = board(vec![]);
        let mut other = board(vec![]);
        let create = || {
            event(Action::CreateIssue {
                status: status_ref("backlog"),
                title: "  新议题  ",
            })
        };
        let request = submission(state.dispatch(create(), READY).unwrap());
        let other_request = submission(other.dispatch(create(), READY).unwrap());
        assert_eq!(request.project_id, "project");
        assert!(
            matches!(&request.request, Request::Create { status_id, title } if status_id == "backlog" && title == "新议题")
        );
        assert!(state.finish(other_request.cancel()).is_none());
        assert!(state.issues().is_empty());
        let created = row("server-id", "backlog", 42, 0.0);
        let response = BoardResponse {
            ticket: request.ticket,
            result: Ok(vec![created]),
        };
        assert_eq!(state.finish(response).unwrap().result, Ok(()));
        assert_eq!(state.issues()[0].id, "server-id");
    }

    #[test]
    fn inline_clear_rolls_back_on_mismatched_response() {
        let mut state = board(vec![row("moving", "backlog", 1, 0.0)]);
        let request = submission(
            state
                .dispatch(
                    event(Action::EditIssue {
                        issue: issue_ref("moving"),
                        change: IssueEdit::Description(None),
                    }),
                    READY,
                )
                .unwrap(),
        );
        assert_eq!(state.issues()[0].description, None);
        let result = state
            .finish(BoardResponse {
                ticket: request.ticket,
                result: Ok(vec![row("other", "backlog", 2, 0.0)]),
            })
            .unwrap();
        assert!(result.result.is_err());
        assert_eq!(state.issues()[0].description.as_deref(), Some("previous"));
    }
}
