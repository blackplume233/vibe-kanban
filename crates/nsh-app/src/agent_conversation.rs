use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    time::Duration,
};

use futures_util::{FutureExt, StreamExt, future::join, stream::FuturesUnordered};
use nsh_core::prelude::*;
use serde_json::{Value, json};

use crate::api::Api;

#[path = "agent_conversation_view.rs"]
mod view;
#[path = "agent_conversation_wire.rs"]
mod wire;

use wire::{Approval, Logs, Process, ProcessStatus, Profiles, QueueStatus, Scheduled, Session};

#[derive(Clone, Default, PartialEq)]
struct Transcript {
    processes: Vec<Process>,
    logs: HashMap<String, Logs>,
    finished_logs: HashSet<String>,
    log_errors: BTreeMap<String, String>,
    ready: bool,
    error: Option<String>,
}

#[derive(Clone, Default, PartialEq)]
struct SessionQueue {
    draft: QueueStatus,
    scheduled: Vec<Scheduled>,
    ready: bool,
    error: Option<String>,
}

#[derive(Clone, Default, PartialEq)]
struct SessionEditor {
    draft: String,
    executor: Option<String>,
    busy: bool,
    queue_revision: u64,
    notice: Option<(bool, String)>,
}

#[derive(Clone, Default, PartialEq)]
struct Conversation {
    transcript: Transcript,
    queue: SessionQueue,
    editor: SessionEditor,
}

#[derive(Clone, Default, PartialEq)]
struct AgentCatalog {
    executors: BTreeSet<String>,
    default: Value,
}

#[derive(Clone, Copy)]
struct State {
    owner: Owner,
    sessions: Signal<Vec<Session>>,
    selected: Signal<Option<SharedString>>,
    conversations: Signal<HashMap<SharedString, Conversation>>,
    approvals: Signal<Vec<Approval>>,
    approval_error: Signal<Option<String>>,
    catalog: Signal<AgentCatalog>,
    workspace_error: Signal<Option<String>>,
    workspace_loading: Signal<bool>,
    creating: Signal<bool>,
    reconnect: Signal<u64>,
}

impl State {
    fn new() -> Self {
        Self {
            owner: Owner::current(),
            sessions: signal(Vec::new()),
            selected: signal(None),
            conversations: signal(HashMap::new()),
            approvals: signal(Vec::new()),
            approval_error: signal(None),
            catalog: signal(AgentCatalog::default()),
            workspace_error: signal(None),
            workspace_loading: signal(true),
            creating: signal(false),
            reconnect: signal(0),
        }
    }

    fn session(self, id: &str) -> Option<Session> {
        self.sessions
            .with(|all| all.iter().find(|session| session.id == id).cloned())
    }

    fn with<R>(self, f: impl FnOnce(Option<&Conversation>) -> R) -> R {
        self.selected.with(|id| {
            self.conversations
                .with(|all| f(id.as_ref().and_then(|id| all.get(id))))
        })
    }

    fn update(self, id: &SharedString, f: impl FnOnce(&mut Conversation)) {
        self.conversations
            .update(|all| f(all.entry(id.clone()).or_default()));
    }

    fn pending(self, transcript: &Transcript, running_only: bool) -> Vec<Approval> {
        self.approvals.with(|all| {
            all.iter()
                .filter(|approval| {
                    transcript.processes.iter().any(|process| {
                        process.id == approval.execution_process_id
                            && (!running_only || process.status == ProcessStatus::Running)
                    })
                })
                .cloned()
                .collect()
        })
    }
}

impl Conversation {
    fn last_config(&self) -> Option<&Value> {
        self.transcript
            .processes
            .iter()
            .rev()
            .find_map(Process::config)
    }

    fn bound_executor<'a>(&'a self, session: &'a Session) -> Option<&'a str> {
        self.last_config()
            .and_then(executor_name)
            .or(session.executor.as_deref())
    }

    fn executor<'a>(&'a self, session: &'a Session, catalog: &'a AgentCatalog) -> Option<&'a str> {
        self.bound_executor(session)
            .or(self.editor.executor.as_deref())
            .or_else(|| {
                executor_name(&catalog.default).filter(|agent| catalog.executors.contains(*agent))
            })
            .or_else(|| catalog.executors.first().map(String::as_str))
    }

    fn executor_config(&self, session: &Session, catalog: &AgentCatalog) -> Option<Value> {
        let agent = self.executor(session, catalog)?;
        self.last_config()
            .filter(|config| executor_name(config) == Some(agent))
            .or_else(|| {
                (executor_name(&catalog.default) == Some(agent)).then_some(&catalog.default)
            })
            .cloned()
            .or_else(|| Some(json!({"executor": agent})))
    }

    fn scheduled(&self, id: &str) -> Option<&Scheduled> {
        self.queue
            .scheduled
            .iter()
            .find(|scheduled| scheduled.execution_process_id == id)
    }

    fn has_active_run(&self) -> bool {
        self.transcript.processes.iter().any(|process| {
            process.status == ProcessStatus::Running
                && process.run_reason != "devserver"
                && self.scheduled(&process.id).is_none()
        })
    }

    fn can_send(&self) -> bool {
        self.transcript.ready
            && self.queue.ready
            && !self.editor.busy
            && self.queue.draft == QueueStatus::Empty
            && !self.editor.draft.trim().is_empty()
    }
}

fn executor_name(value: &Value) -> Option<&str> {
    value
        .get("executor")
        .or_else(|| value.get("profile"))?
        .as_str()
}

/// Agent sessions in one workspace. Mount this component keyed by workspace ID.
pub fn agent_conversation(api: Api, workspace_id: String) -> impl IntoElement {
    let state = State::new();
    effect({
        let api = api.clone();
        let workspace_id = workspace_id.clone();
        move || {
            state.reconnect.get();
            let api = api.clone();
            let workspace_id = workspace_id.clone();
            let task = spawn(async move { load_workspace(&api, &workspace_id, state).await });
            cleanup(move || task.cancel());
        }
    });
    effect({
        let api = api.clone();
        move || {
            state.reconnect.get();
            let api = api.clone();
            let task = spawn(async move {
                if let Err(error) = watch_approvals(&api, state).await {
                    state.approval_error.set(Some(error));
                }
            });
            cleanup(move || task.cancel());
        }
    });
    effect({
        let api = api.clone();
        move || {
            state.reconnect.get();
            let Some(id) = state.selected.get() else {
                return;
            };
            state.update(&id, |data| {
                data.transcript.ready = false;
                data.transcript.error = None;
                data.transcript.log_errors.clear();
                data.queue.ready = false;
            });
            let process_api = api.clone();
            let process_id = id.clone();
            let process_task = spawn(async move {
                let Err(error) = watch_processes(&process_api, state, &process_id).await else {
                    return;
                };
                state.update(&process_id, |data| {
                    data.transcript.ready = false;
                    data.transcript.error = Some(error);
                });
            });
            let queue_api = api.clone();
            let queue_task = spawn(async move { watch_queue(&queue_api, state, &id).await });
            cleanup(move || {
                process_task.cancel();
                queue_task.cancel();
            });
        }
    });
    view::panel(api, workspace_id, state)
}

async fn load_workspace(api: &Api, workspace_id: &str, state: State) {
    state.workspace_loading.set(true);
    state.workspace_error.set(None);
    let path = format!("/api/sessions?workspace_id={workspace_id}");
    let (sessions, profiles) = join(
        wire::get::<Vec<Session>>(api, &path),
        wire::get::<Profiles>(api, "/api/info"),
    )
    .await;
    match sessions {
        Ok(rows) => {
            if state.selected.with(|id| {
                id.as_ref()
                    .is_none_or(|id| !rows.iter().any(|s| s.id == id.as_ref()))
            }) {
                state
                    .selected
                    .set(rows.first().map(|row| row.id.clone().into()));
            }
            state.sessions.set(rows);
        }
        Err(error) => state.workspace_error.set(Some(error)),
    }
    match profiles {
        Ok(profiles) => state.catalog.set(AgentCatalog {
            executors: wire::configured_agents(&profiles),
            default: profiles.config.executor_profile,
        }),
        Err(error) => state.workspace_error.set(Some(error)),
    }
    state.workspace_loading.set(false);
}

fn create_session(api: Api, workspace_id: String, state: State) {
    if state.workspace_loading.get() || state.creating.get() {
        return;
    }
    state.creating.set(true);
    state.workspace_error.set(None);
    spawn(async move {
        match wire::post::<Session>(&api, "/api/sessions", json!({"workspace_id": workspace_id}))
            .await
        {
            Ok(session) => {
                let id = session.id.clone().into();
                state.sessions.update(|rows| rows.insert(0, session));
                state.selected.set(Some(id));
            }
            Err(error) => state.workspace_error.set(Some(error)),
        }
        state.creating.set(false);
    });
}

async fn watch_approvals(api: &Api, state: State) -> Result<(), String> {
    let (mut frames, _connection) = api.stream("/api/approvals/stream/ws")?;
    let mut document = json!({"pending": {}});
    loop {
        let frame = wire::frame(&mut frames).await?;
        if let Some(patch) = frame.patch {
            wire::apply_approvals(&mut document, patch)?;
            let mut approvals: Vec<Approval> = wire::deserialize_map(&document["pending"])?;
            approvals.sort_unstable_by(|a, b| {
                a.timeout_at
                    .cmp(&b.timeout_at)
                    .then(a.approval_id.cmp(&b.approval_id))
            });
            state.approvals.set(approvals);
        }
        if frame.ready {
            state.approval_error.set(None);
        }
    }
}

async fn watch_processes(api: &Api, state: State, id: &SharedString) -> Result<(), String> {
    let (mut frames, _connection) = api.stream(&format!(
        "/api/execution-processes/stream/session/ws?session_id={id}"
    ))?;
    let mut document = wire::initial_processes();
    let mut readers = FuturesUnordered::new();
    let mut observed = HashSet::new();
    loop {
        let frame = if readers.is_empty() {
            wire::frame(&mut frames).await?
        } else {
            futures_util::select_biased! {
                item = wire::frame(&mut frames).fuse() => item?,
                _ = readers.next().fuse() => continue,
            }
        };
        if let Some(mut patch) = frame.patch {
            patch.0.retain(|op| {
                !matches!(op, json_patch::PatchOperation::Remove(remove)
                if document.pointer(remove.path.as_str()).is_none())
            });
            json_patch::patch(&mut document, &patch).map_err(|error| error.to_string())?;
            let mut processes: Vec<Process> =
                wire::deserialize_map(&document["execution_processes"])?;
            processes.retain(|process| !process.dropped && process.run_reason != "devserver");
            processes
                .sort_unstable_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
            state.update(id, |data| {
                for process in &processes {
                    if !data.transcript.finished_logs.contains(&process.id)
                        && observed.insert(process.id.clone())
                    {
                        readers.push(read_logs(api, state, id, process.id.clone()));
                    }
                }
                data.transcript.processes = processes;
            });
        }
        if frame.ready {
            state.update(id, |data| data.transcript.ready = true);
        }
    }
}

async fn read_logs(api: &Api, state: State, session: &SharedString, process: String) {
    loop {
        let eligible = state.conversations.with(|all| {
            let data = all.get(session)?;
            let row = data
                .transcript
                .processes
                .iter()
                .find(|row| row.id == process)?;
            Some(
                row.status != ProcessStatus::Running
                    || (data.queue.ready && data.scheduled(&process).is_none()),
            )
        });
        match eligible {
            None => return,
            Some(false) => timer(Duration::from_millis(400)).await,
            Some(true) => break,
        }
    }
    let mut result = log_stream(api, state, session, &process).await;
    // The DB reports running before the scheduler creates the live log store.
    // An early subscription can therefore close on the historical snapshot.
    while state.conversations.with(|all| {
        all.get(session).is_some_and(|data| {
            data.transcript
                .processes
                .iter()
                .any(|row| row.id == process && row.status == ProcessStatus::Running)
        })
    }) {
        timer(Duration::from_millis(500)).await;
        result = log_stream(api, state, session, &process).await;
    }
    match result {
        Ok(()) => state.update(session, |data| {
            data.transcript.finished_logs.insert(process);
        }),
        Err(error) => state.update(session, |data| {
            data.transcript.log_errors.insert(process, error);
        }),
    }
}

async fn log_stream(
    api: &Api,
    state: State,
    session: &SharedString,
    process: &str,
) -> Result<(), String> {
    let (mut frames, _connection) = api.stream(&format!(
        "/api/execution-processes/{process}/normalized-logs/ws"
    ))?;
    loop {
        let frame = wire::frame(&mut frames).await?;
        if let Some(patch) = frame.patch {
            state.update(session, |data| {
                if let Some(entries) = data.transcript.logs.get_mut(process) {
                    wire::apply_logs(entries, patch);
                } else {
                    let mut entries = Logs::new();
                    wire::apply_logs(&mut entries, patch);
                    data.transcript.logs.insert(process.to_owned(), entries);
                }
            });
        }
        if frame.finished {
            return Ok(());
        }
    }
}

async fn watch_queue(api: &Api, state: State, id: &SharedString) {
    let path = format!("/api/sessions/{id}/queue");
    loop {
        let revision = state
            .conversations
            .with(|all| all.get(id).map_or(0, |data| data.editor.queue_revision));
        let (draft, scheduled) = join(
            wire::get::<QueueStatus>(api, &path),
            wire::get::<Vec<Scheduled>>(api, "/api/execution-queue"),
        )
        .await;
        let next = match (draft, scheduled) {
            (Ok(draft), Ok(mut scheduled)) => {
                scheduled.retain(|entry| entry.session_id == id.as_ref());
                Ok(SessionQueue {
                    draft,
                    scheduled,
                    ready: true,
                    error: None,
                })
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        let changed = state.conversations.with(|all| {
            all.get(id).is_some_and(|data| {
                data.editor.queue_revision == revision
                    && !data.editor.busy
                    && match &next {
                        Ok(next) => data.queue != *next,
                        Err(error) => data.queue.ready || data.queue.error.as_ref() != Some(error),
                    }
            })
        });
        if changed {
            state.update(id, |data| match next {
                Ok(queue) => data.queue = queue,
                Err(error) => {
                    data.queue.ready = false;
                    data.queue.error = Some(error);
                }
            });
        }
        timer(Duration::from_secs(1)).await;
    }
}

enum Action {
    FollowUp {
        prompt: String,
        executor: Value,
        queue: bool,
    },
    CancelDraft,
    Stop(String),
    Respond {
        approval: Approval,
        approve: bool,
    },
}

fn submit(api: Api, state: State) {
    let Some(id) = state.selected.get() else {
        return;
    };
    let Some(session) = state.session(id.as_ref()) else {
        return;
    };
    let action = state.with(|data| {
        let data = data.filter(|data| data.can_send())?;
        let executor = state
            .catalog
            .with(|catalog| data.executor_config(&session, catalog))?;
        Some(Action::FollowUp {
            prompt: data.editor.draft.clone(),
            executor,
            queue: data.has_active_run(),
        })
    });
    if let Some(action) = action {
        run_action(api, state, id, action);
    }
}

fn run_action(api: Api, state: State, id: SharedString, action: Action) {
    if state
        .conversations
        .with(|all| all.get(&id).is_none_or(|data| data.editor.busy))
    {
        return;
    }
    state.update(&id, |data| {
        data.editor.busy = true;
        data.editor.notice = None;
        data.editor.queue_revision += 1;
    });
    // A resolved approval removes its button before the response reaches the UI.
    state.owner.run(|| {
        spawn(async move {
            let result = action.execute(&api, id.as_ref()).await;
            state.update(&id, |data| {
                data.editor.busy = false;
                data.editor.queue_revision += 1;
                data.editor.notice = Some(match result {
                    Ok(result) => {
                        if let Action::FollowUp { prompt, .. } = &action
                            && data.editor.draft == *prompt
                        {
                            data.editor.draft.clear();
                        }
                        if let Some(draft) = result.draft {
                            data.queue.draft = draft;
                        }
                        if let Some(process) = result.process {
                            // The WS may have already delivered this execution, including its latest status.
                            if !data
                                .transcript
                                .processes
                                .iter()
                                .any(|item| item.id == process.id)
                            {
                                data.transcript.processes.push(process);
                            }
                            // Wait for the scheduler snapshot before treating another send as active.
                            data.queue.ready = false;
                        }
                        (false, result.message.into())
                    }
                    Err(error) => (true, error),
                });
            });
        })
    });
}

struct ActionResult {
    message: &'static str,
    draft: Option<QueueStatus>,
    process: Option<Process>,
}

impl Action {
    async fn execute(&self, api: &Api, id: &str) -> Result<ActionResult, String> {
        let (message, draft, process) = match self {
            Self::FollowUp {
                prompt,
                executor,
                queue: true,
            } => (
                "后续消息已排队",
                Some(
                    wire::post(
                        api,
                        &format!("/api/sessions/{id}/queue"),
                        json!({"message": prompt, "executor_config": executor}),
                    )
                    .await?,
                ),
                None,
            ),
            Self::FollowUp {
                prompt,
                executor,
                queue: false,
            } => (
                "后续消息已提交",
                None,
                Some(
                    wire::post(
                        api,
                        &format!("/api/sessions/{id}/follow-up"),
                        json!({"prompt": prompt, "executor_config": executor}),
                    )
                    .await?,
                ),
            ),
            Self::CancelDraft => (
                "已取消排队消息",
                Some(
                    api.request("DELETE", &format!("/api/sessions/{id}/queue"), None)
                        .await?,
                ),
                None,
            ),
            Self::Stop(process) => {
                api.request::<()>(
                    "POST",
                    &format!("/api/execution-processes/{process}/stop"),
                    None,
                )
                .await?;
                ("已停止操作", None, None)
            }
            Self::Respond { approval, approve } => {
                let status = if *approve { "approved" } else { "denied" };
                let result: Value = wire::post(
                    api,
                    &format!("/api/approvals/{}/respond", approval.approval_id),
                    json!({
                        "execution_process_id": approval.execution_process_id,
                        "status": {"status": status}
                    }),
                )
                .await?;
                if result["status"].as_str() != Some(status) {
                    return Err("审批已失效或已由其他窗口处理".into());
                }
                (if *approve { "已批准" } else { "已拒绝" }, None, None)
            }
        };
        Ok(ActionResult {
            message,
            draft,
            process,
        })
    }
}
