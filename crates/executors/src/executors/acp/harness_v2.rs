use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use acp::{
    Agent, Client, ConnectTo, ConnectionTo, Lines, UntypedMessage,
    schema::{
        ProtocolVersion,
        v1::{
            CancelNotification, ContentBlock, InitializeRequest, InitializeResponse,
            LoadSessionRequest, McpServer, NewSessionRequest, PromptRequest, PromptResponse,
            RequestPermissionRequest, SessionNotification, SessionUpdate, StopReason, TextContent,
        },
    },
};
use async_trait::async_trait;
use command_group::AsyncGroupChild;
use futures::AsyncBufReadExt as _;
use serde::Deserialize;
use serde_json::Value;
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    process::{Child, Command},
    sync::{mpsc, mpsc::UnboundedSender},
};
use tokio_util::{compat::TokioAsyncReadCompatExt, sync::CancellationToken};
use workspace_utils::command_ext::GroupSpawnNoWindowExt;

use crate::{
    approvals::ExecutorApprovalService,
    command::{CmdOverrides, CommandParts},
    env::ExecutionEnv,
    executor_discovery::ExecutorConfigCacheKey,
    executors::{
        ExecutorError, ExecutorExitResult, SpawnedChild,
        acp::{
            AcpEvent, AcpUsageUpdate, SessionManager,
            permissions::{ask_user, auto_approve},
            session_config::{
                ConfigSink, ConfigValue, DesiredConfig, SessionOptions, Setter, apply_config,
                describe_changes, set_value, summary,
            },
            trusted_node::{self, RemoteSession},
        },
    },
    logs::ExecutionTokenUsage,
    profile::ExecutorConfig,
};

/// Reusable harness for ACP-based conns (Gemini, Qwen, etc.)
#[derive(Default)]
pub struct AcpAgentHarnessV2 {
    config: DesiredConfig,
    legacy_namespace: Option<String>,
    client_transcript_namespace: Option<String>,
    discovery_key: Option<ExecutorConfigCacheKey>,
    mcp_config_path: Option<PathBuf>,
}

/// Time limit for an agent to initialize / load a session / create a new session.
/// Not used for prompts, turns can legitimately run long
pub(crate) const SETUP_TIMEOUT: Duration = Duration::from_secs(120);

/// Time limit for each config change (model, reasoning, mode, ...) after the session starts.
pub(crate) const CONFIG_CHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// Per the ACP spec the agent wraps up and answers the prompt with stopReason=cancelled.
/// Stay connected so it can persist that, but stay under the container's 5s grace period.
const CANCEL_GRACE: Duration = Duration::from_secs(4);

/// State shared by the session loop and the ACP handlers. Cheap to clone (Arcs + a channel sender).
#[derive(Clone)]
struct SessionShared {
    log_tx: UnboundedSender<String>,
    cancel: CancellationToken,
    /// True while session/load replays history we already have in our logs.
    replaying: Arc<AtomicBool>,
    /// Reasons from "deny with feedback", sent to the agent after the turn ends.
    denial_feedback: Arc<Mutex<Vec<String>>>,
    /// What the agent advertises for the session, kept current so changes the agent makes
    /// itself (Copilot leaving plan mode, ...) can be shown.
    options: Arc<Mutex<SessionOptions>>,
    /// The agent's option-discovery key. Once a session opens, a failed discovery for it is out
    /// of date (the user signed in) and is forgotten.
    discovery_key: Option<ExecutorConfigCacheKey>,
    /// Set once our own config is applied. Before that, config notifications are echoes of our
    /// own changes and are not shown.
    tracking_config: Arc<AtomicBool>,
    remote: Option<Arc<RemoteSession>>,
}

impl SessionShared {
    fn send(&self, mut event: AcpEvent) {
        if let Some(remote) = &self.remote {
            match &mut event {
                AcpEvent::ToolCall(call) => {
                    remote.rebase_tool_paths(&mut call.locations, &mut call.content);
                }
                AcpEvent::ToolUpdate(update) => remote.rebase_tool_paths(
                    update.fields.locations.as_deref_mut().unwrap_or_default(),
                    update.fields.content.as_deref_mut().unwrap_or_default(),
                ),
                _ => {}
            }
        }
        let _ = self.log_tx.send(event.to_string()); // Display = JSON; the writer adds '\n'
    }

    /// Records a config or mode notification and shows what the agent changed.
    fn on_config_notification(&self, update: impl FnOnce(&mut SessionOptions)) {
        if !self.tracking_config.load(Ordering::SeqCst) {
            return;
        }
        let lines = {
            let mut options = self.options.lock().unwrap_or_else(|e| e.into_inner());
            let before = options.clone();
            update(&mut options);
            describe_changes(&before, &options)
        };
        for line in lines {
            self.send(AcpEvent::SessionConfig(line));
        }
    }
}

impl AcpAgentHarnessV2 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_legacy_sessions(mut self, legacy_namespace: impl Into<String>) -> Self {
        self.legacy_namespace = Some(legacy_namespace.into());
        self
    }

    /// Keep a local transcript and fork it for each follow-up, opening a fresh agent session.
    pub(in crate::executors) fn with_client_transcript(
        mut self,
        namespace: impl Into<String>,
    ) -> Self {
        self.client_transcript_namespace = Some(namespace.into());
        self
    }

    pub fn with_discovery_key(mut self, key: ExecutorConfigCacheKey) -> Self {
        self.discovery_key = Some(key);
        self
    }

    pub fn with_config(mut self, config: DesiredConfig) -> Self {
        self.config = config;
        self
    }

    pub(in crate::executors) fn with_mcp_config(mut self, path: Option<PathBuf>) -> Self {
        self.mcp_config_path = path;
        self
    }

    pub fn config(&self) -> &DesiredConfig {
        &self.config
    }

    pub fn apply_overrides(&mut self, executor_config: &ExecutorConfig) {
        if let Some(model_id) = &executor_config.model_id {
            self.config.model = Some(model_id.clone());
        }
        if let Some(reasoning_id) = &executor_config.reasoning_id {
            self.config.reasoning = Some(reasoning_id.clone());
        }
        if let Some(agent_id) = &executor_config.agent_id {
            self.config.mode = Some(agent_id.clone());
        }
        if let Some(permission) = &executor_config.permission_policy {
            self.config.permission = Some(permission.clone());
        }
    }

    pub async fn spawn_with_command(
        &self,
        current_dir: &Path,
        prompt: String,
        command_parts: CommandParts,
        env: &ExecutionEnv,
        cmd_overrides: &CmdOverrides,
        approvals: Option<std::sync::Arc<dyn ExecutorApprovalService>>,
    ) -> Result<SpawnedChild, ExecutorError> {
        self.spawn_turn(
            current_dir,
            prompt,
            None,
            command_parts,
            env,
            cmd_overrides,
            approvals,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_follow_up_with_command(
        &self,
        current_dir: &Path,
        prompt: String,
        session_id: &str,
        command_parts: CommandParts,
        env: &ExecutionEnv,
        cmd_overrides: &CmdOverrides,
        approvals: Option<std::sync::Arc<dyn ExecutorApprovalService>>,
    ) -> Result<SpawnedChild, ExecutorError> {
        self.spawn_turn(
            current_dir,
            prompt,
            Some(session_id),
            command_parts,
            env,
            cmd_overrides,
            approvals,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_turn(
        &self,
        current_dir: &Path,
        prompt: String,
        session_id: Option<&str>,
        command_parts: CommandParts,
        env: &ExecutionEnv,
        cmd_overrides: &CmdOverrides,
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
    ) -> Result<SpawnedChild, ExecutorError> {
        let client_transcript = self
            .client_transcript_namespace
            .as_deref()
            .map(SessionManager::new)
            .transpose()
            .map_err(ExecutorError::Io)?
            .map(Arc::new);
        // Copilot conversations started by the pre-V2 harness store our own id, which the agent
        // can't session/load. Start a new session and send the stored history in the prompt.
        let legacy = session_id
            .filter(|_| client_transcript.is_none())
            .and_then(|id| {
                let ns = self.legacy_namespace.as_deref()?;
                SessionManager::new(ns)
                    .inspect_err(|e| {
                        tracing::warn!("Failed to open legacy session store {ns}: {e}")
                    })
                    .ok()
                    .filter(|sm| sm.has_session(id))
                    .map(|sm| (sm, id))
            });
        let (existing_session, prompt_to_send) = match legacy {
            Some((sm, session_id)) => {
                tracing::debug!(
                    "Found legacy session with id {session_id}, generating resume prompt"
                );
                let resume_prompt = sm
                    .generate_resume_prompt(session_id, &prompt)
                    .unwrap_or_else(|e| {
                        tracing::warn!("Failed to load legacy session {session_id} from disk: {e}");
                        prompt
                    });

                (None, resume_prompt)
            }
            None => (session_id.map(str::to_owned), prompt),
        };

        let mcp_servers =
            super::mcp::read_servers(self.mcp_config_path.as_deref(), current_dir).await?;
        let env_vars = env.clone().with_profile(cmd_overrides).vars;
        let mut child = spawn_acp_process(current_dir, command_parts, &env_vars).await?;

        let line_ending = if cfg!(windows) && client_transcript.is_some() {
            "\r\n"
        } else {
            "\n"
        };
        let transport = take_transport_with_line_ending(child.inner(), line_ending)?;
        let writer = crate::stdout_dup::create_stdout_pipe_writer(&mut child)?;
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<ExecutorExitResult>();
        let cancel = CancellationToken::new();

        Self::bootstrap_acp_connection(
            transport,
            writer,
            current_dir.to_path_buf(),
            existing_session,
            prompt_to_send,
            exit_tx,
            self.config.clone(),
            self.discovery_key.clone(),
            mcp_servers,
            client_transcript,
            approvals,
            cancel.clone(),
            None,
            None,
        );

        Ok(SpawnedChild {
            child,
            exit_signal: Some(exit_rx),
            cancel: Some(cancel),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::executors) async fn spawn_trusted_node(
        &self,
        node: &str,
        registry_id: &str,
        current_dir: &Path,
        prompt: String,
        session_id: Option<&str>,
        env: &ExecutionEnv,
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
    ) -> Result<SpawnedChild, ExecutorError> {
        if approvals.is_none() && !self.config.auto_approve() {
            return Err(ExecutorError::Io(std::io::Error::other(
                "Trusted ACP node requires the local approval service for supervised execution",
            )));
        }
        let (mut ssh, scope, cwd, existing_session) = trusted_node::connect(
            node,
            registry_id,
            current_dir,
            &env.repo_context,
            session_id,
        )
        .await?;
        let transport = take_transport_with_line_ending(&mut ssh, "\n")?;
        // A transport exit can precede the final ACP result. Let the harness decide
        // completion while the existing container tracks this local output process.
        let (mut spawned, writer) = crate::stdout_dup::spawn_local_output_process()?;
        spawned.child.inner().stderr = ssh.stderr.take();
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        let cancel = CancellationToken::new();
        Self::bootstrap_acp_connection(
            transport,
            writer,
            cwd,
            existing_session,
            prompt,
            exit_tx,
            self.config.clone(),
            None,
            Vec::new(),
            None,
            approvals,
            cancel.clone(),
            Some(Arc::new(scope)),
            Some(ssh),
        );
        spawned.exit_signal = Some(exit_rx);
        spawned.cancel = Some(cancel);
        Ok(spawned)
    }

    #[allow(clippy::too_many_arguments)]
    fn bootstrap_acp_connection(
        transport: impl ConnectTo<Client>,
        writer: impl AsyncWrite + Send + 'static,
        cwd: PathBuf,
        existing_session: Option<String>,
        prompt: String,
        exit_signal: tokio::sync::oneshot::Sender<ExecutorExitResult>,
        config: DesiredConfig,
        discovery_key: Option<ExecutorConfigCacheKey>,
        mcp_servers: Vec<McpServer>,
        client_transcript: Option<Arc<SessionManager>>,
        approvals: Option<Arc<dyn ExecutorApprovalService>>,
        cancel: CancellationToken,
        remote: Option<Arc<RemoteSession>>,
        ssh: Option<Child>,
    ) {
        let record_client_transcript = client_transcript.is_some();
        let (log_tx, log_rx) = mpsc::unbounded_channel::<String>();
        let log_writer = tokio::spawn(write_logs(writer, log_rx, client_transcript.clone()));

        let shared = SessionShared {
            log_tx: log_tx.clone(),
            cancel,
            replaying: Arc::new(AtomicBool::new(false)),
            denial_feedback: Arc::default(),
            options: Arc::default(),
            tracking_config: Arc::new(AtomicBool::new(false)),
            discovery_key,
            remote,
        };
        let notification_shared = shared.clone();
        let permission_shared = shared.clone();
        let auto_approve_requests = config.auto_approve();

        tokio::spawn(async move {
            let result = acp::Client
                .builder()
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        tracing::trace!(
                            "RAW ACP: {}",
                            serde_json::to_string(&notification).unwrap_or_default()
                        );
                        if notification_shared.replaying.load(Ordering::SeqCst) {
                            return Ok(()); // history replay from session/load: already in our logs
                        }
                        match &notification.update {
                            SessionUpdate::ConfigOptionUpdate(update) => {
                                let value = serde_json::to_value(update).unwrap_or_default();
                                notification_shared
                                    .on_config_notification(|o| o.update_config(&value));
                            }
                            // Shown through SessionConfig with the mode's name, not its raw id
                            SessionUpdate::CurrentModeUpdate(update) => {
                                let mode_id = update.current_mode_id.0.to_string();
                                notification_shared
                                    .on_config_notification(|o| o.set_current_mode(&mode_id));
                            }
                            _ => {
                                if let Ok(event) = AcpEvent::try_from(notification) {
                                    notification_shared.send(event);
                                }
                            }
                        }
                        Ok(())
                    },
                    acp::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, connection| {
                        tracing::trace!("Received permission request: {:?}", request);
                        let approvals = match approvals.clone() {
                            Some(approvals) if !auto_approve_requests => approvals,
                            _ => {
                                if record_client_transcript {
                                    permission_shared
                                        .send(AcpEvent::RequestPermission(request.clone()));
                                }
                                let _ = responder.respond(auto_approve(&request));
                                return Ok(());
                            }
                        };

                        // the closure runs once per request → clone again for each spawned task
                        let shared = permission_shared.clone();
                        connection.spawn(async move {
                            let _ = responder.respond(
                                ask_user(
                                    &request,
                                    approvals,
                                    shared.cancel,
                                    &shared.log_tx,
                                    shared.denial_feedback,
                                )
                                .await,
                            );
                            Ok(())
                        })
                    },
                    acp::on_receive_request!(),
                )
                .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
                    run_session(
                        connection,
                        existing_session,
                        cwd,
                        prompt,
                        config,
                        shared,
                        mcp_servers,
                        client_transcript.as_deref(),
                    )
                    .await
                })
                .await;
            // Closing the connection releases the remote subsystem. Killing the local SSH
            // transport on drop also covers a node that didn't acknowledge cancellation.
            drop(ssh);

            // Notify container of completion
            let exit = match result {
                Ok(()) => ExecutorExitResult::Success,
                Err(err) => {
                    let _ = log_tx.send(exit_event(&err).to_string());
                    ExecutorExitResult::Failure
                }
            };
            // The container closes the execution log stream after the exit signal.
            drop(log_tx);
            let _ = log_writer.await;
            let _ = exit_signal.send(exit);
        });
    }
}

async fn write_logs(
    writer: impl AsyncWrite,
    mut log_rx: mpsc::UnboundedReceiver<String>,
    transcript: Option<Arc<SessionManager>>,
) {
    tokio::pin!(writer);
    let mut display_id: Option<String> = None;
    let mut before_start: Vec<String> = Vec::new();
    let mut stdout_open = true;
    while let Some(mut line) = log_rx.recv().await {
        if let Some(store) = &transcript {
            if let Some(id) = &display_id {
                append_transcript(store, id, &line);
            } else if let Ok(AcpEvent::SessionStart(id)) = line.parse() {
                for pending in before_start.drain(..) {
                    append_transcript(store, &id, &pending);
                }
                display_id = Some(id);
            }
        }
        if stdout_open {
            let end = line.len();
            line.push('\n');
            stdout_open = writer.write_all(line.as_bytes()).await.is_ok();
            if !stdout_open && transcript.is_none() {
                break;
            }
            line.truncate(end);
        }
        if transcript.is_some() && display_id.is_none() {
            before_start.push(line);
        }
    }
}

fn append_transcript(store: &SessionManager, id: &str, line: &str) {
    if let Err(err) = store.append_raw_line(id, line) {
        tracing::warn!("Failed to append ACP session {id} to disk: {err}");
    }
}

/// Spawns an ACP agent in its own process group, killed when dropped. Shared by runs and
/// option discovery, so a discovery that times out doesn't leave the agent running.
pub(crate) async fn spawn_acp_process(
    current_dir: &Path,
    command_parts: CommandParts,
    env_vars: &HashMap<String, String>,
) -> Result<AsyncGroupChild, ExecutorError> {
    let (program_path, args) = command_parts.into_resolved().await?;
    let mut command = Command::new(program_path);
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(current_dir)
        .env("NPM_CONFIG_LOGLEVEL", "error")
        .env("NODE_NO_WARNINGS", "1")
        .args(&args)
        .envs(env_vars);

    Ok(command.group_spawn_no_window()?)
}

/// Takes the child's stdin/stdout as the ACP transport.
pub(crate) fn take_transport(
    child: &mut AsyncGroupChild,
) -> Result<impl ConnectTo<Client> + use<>, ExecutorError> {
    take_transport_with_line_ending(child.inner(), "\n")
}

fn take_transport_with_line_ending(
    child: &mut Child,
    line_ending: &'static str,
) -> Result<impl ConnectTo<Client> + use<>, ExecutorError> {
    let orig_stdout = child.stdout.take().ok_or_else(|| {
        ExecutorError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Child process has no stdout",
        ))
    })?;
    let orig_stdin = child.stdin.take().ok_or_else(|| {
        ExecutorError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Child process has no stdin",
        ))
    })?;
    let incoming = futures::io::BufReader::new(orig_stdout.compat()).lines();
    let outgoing =
        futures::sink::unfold(orig_stdin, move |mut stdin, mut line: String| async move {
            line.push_str(line_ending);
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
            Ok::<_, std::io::Error>(stdin)
        });
    Ok(Lines::new(outgoing, incoming))
}

pub(crate) async fn initialize(
    connection: &ConnectionTo<Agent>,
) -> Result<InitializeResponse, acp::Error> {
    initialize_agent(connection, false).await
}

async fn initialize_agent(
    connection: &ConnectionTo<Agent>,
    tolerate_legacy_response: bool,
) -> Result<InitializeResponse, acp::Error> {
    tracing::debug!("🤝 Initializing agent...");
    let init_response = with_timeout("initialize", async {
        let response = connection
            .send_request(InitializeRequest::new(ProtocolVersion::V1))
            .block_task()
            .await;
        match response {
            Err(err)
                if tolerate_legacy_response
                    && err.code != acp::ErrorCode::AuthRequired
                    && !acp::is_incoming_transport_closed(&err) =>
            {
                // Old clients continued to session/new after an unusable initialize reply.
                // Keep that behavior without assuming any optional agent capabilities.
                tracing::debug!("Legacy ACP initialize failed: {err}");
                Ok(InitializeResponse::new(ProtocolVersion::V1))
            }
            result => result,
        }
    })
    .await?;
    tracing::debug!("✓ Agent initialized: {:?}", init_response.agent_info);
    Ok(init_response)
}

/// Sends `session/new`, or `session/load` when `existing` is set, untyped so that both
/// `configOptions` and the older `models` / `modes` objects can be read.
/// Returns the session id and what the agent advertises for it.
pub(crate) async fn open_session(
    connection: &ConnectionTo<Agent>,
    existing: Option<String>,
    cwd: PathBuf,
) -> Result<(String, SessionOptions), acp::Error> {
    open_session_with_mcp(connection, existing, cwd, Vec::new(), None).await
}

async fn open_session_with_mcp(
    connection: &ConnectionTo<Agent>,
    existing: Option<String>,
    cwd: PathBuf,
    mcp_servers: Vec<McpServer>,
    history: Option<String>,
) -> Result<(String, SessionOptions), acp::Error> {
    match existing {
        Some(session_id) => {
            tracing::debug!("📝 Loading session {session_id}...");
            let request = UntypedMessage::new(
                "session/load",
                LoadSessionRequest::new(session_id.clone(), cwd).mcp_servers(mcp_servers),
            )?;
            let response = with_timeout(
                "load the session",
                connection.send_request(request).block_task(),
            )
            .await?;
            tracing::debug!("✓ Session loaded {:?}", response);
            Ok((session_id, SessionOptions::from_response(&response)))
        }
        None => {
            tracing::debug!("📝 Creating new session...");
            let mut request = NewSessionRequest::new(cwd).mcp_servers(mcp_servers);
            if let Some(history) = history {
                request = request.meta(serde_json::Map::from_iter([(
                    "history_jsonl".to_owned(),
                    Value::String(history),
                )]));
            }
            let request = UntypedMessage::new("session/new", request)?;
            let response = with_timeout(
                "create a new session",
                connection.send_request(request).block_task(),
            )
            .await?;
            let session_id = response
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or_else(|| acp::util::internal_error("session/new returned no sessionId"))?
                .to_string();
            tracing::debug!("✓ Session created");
            Ok((session_id, SessionOptions::from_response(&response)))
        }
    }
}

/// Sends config changes over an ACP connection, each with its own time limit.
pub(crate) struct ConnectionSink<'a> {
    pub connection: &'a ConnectionTo<Agent>,
    pub session_id: String,
}

impl ConnectionSink<'_> {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let request = UntypedMessage::new(method, params).map_err(|e| e.to_string())?;
        match tokio::time::timeout(
            CONFIG_CHANGE_TIMEOUT,
            self.connection.send_request(request).block_task(),
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => Err(e.message),
            Err(_) => Err("the agent didn't answer in time".to_string()),
        }
    }
}

#[async_trait]
impl ConfigSink for ConnectionSink<'_> {
    async fn set_config_option(&mut self, id: &str, value: &ConfigValue) -> Result<Value, String> {
        let mut params = value.to_params();
        params["sessionId"] = Value::from(self.session_id.clone());
        params["configId"] = Value::from(id);
        self.request("session/set_config_option", params).await
    }

    async fn set_mode(&mut self, mode_id: &str) -> Result<(), String> {
        let params = serde_json::json!({ "sessionId": self.session_id, "modeId": mode_id });
        self.request("session/set_mode", params).await.map(|_| ())
    }

    async fn set_model(&mut self, model_id: &str) -> Result<(), String> {
        let params = serde_json::json!({ "sessionId": self.session_id, "modelId": model_id });
        self.request("session/set_model", params).await.map(|_| ())
    }
}

fn prompt_response(response: &Value) -> Result<(StopReason, Option<AcpEvent>), acp::Error> {
    let stop_reason = PromptResponse::deserialize(response)?.stop_reason;
    let usage_event = response
        .get("usage")
        .and_then(prompt_usage)
        .map(|usage| AcpEvent::Usage(AcpUsageUpdate::Prompt(usage)));
    Ok((stop_reason, usage_event))
}

fn prompt_usage(usage: &Value) -> Option<ExecutionTokenUsage> {
    let count = |name| usage.get(name).and_then(Value::as_u64);
    // The installed schema calls this a prompt response but defines these fields
    // across all turns of the session. Keep the snapshot, with no inferred model or cost.
    count("totalTokens")?;
    Some(ExecutionTokenUsage {
        input_tokens: count("inputTokens")?,
        output_tokens: count("outputTokens")?,
        cache_read_tokens: count("cachedReadTokens").unwrap_or_default(),
        cache_write_tokens: count("cachedWriteTokens").unwrap_or_default(),
        cumulative: true,
        ..Default::default()
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    connection: ConnectionTo<Agent>,
    existing_session: Option<String>,
    cwd: PathBuf,
    prompt: String,
    config: DesiredConfig,
    shared: SessionShared,
    mut mcp_servers: Vec<McpServer>,
    client_transcript: Option<&SessionManager>,
) -> Result<(), acp::Error> {
    let init_response = tokio::select! {
        biased;
        _ = shared.cancel.cancelled() => return Ok(()),
        response = initialize_agent(&connection, client_transcript.is_some()) => response?,
    };

    let capabilities = &init_response.agent_capabilities.mcp_capabilities;
    let configured = mcp_servers.len();
    mcp_servers.retain(|server| match server {
        McpServer::Stdio(_) => true,
        McpServer::Http(_) => capabilities.http,
        McpServer::Sse(_) => capabilities.sse,
        _ => false,
    });
    let skipped = configured - mcp_servers.len();

    let mut existing_session = existing_session;
    let mut display_id = None;
    let mut history = None;
    let mut resume_prompt = None;
    if let Some(store) = client_transcript
        && let Some(parent_id) = existing_session.take()
    {
        let fork_id = uuid::Uuid::new_v4().to_string();
        store
            .fork_session(&parent_id, &fork_id)
            .map_err(acp::util::internal_error)?;
        let raw = store
            .read_session_raw(&fork_id)
            .map_err(acp::util::internal_error)?;
        resume_prompt = Some(SessionManager::resume_prompt(&raw, &prompt));
        display_id = Some(fork_id);
        history = Some(raw);
    }

    if existing_session.is_some() {
        if !init_response.agent_capabilities.load_session {
            return Err(acp::util::internal_error(
                "agent does not support session/load",
            ));
        }
        shared.replaying.store(true, Ordering::SeqCst);
    }
    let opened = tokio::select! {
        biased;
        _ = shared.cancel.cancelled() => return Ok(()),
        response = open_session_with_mcp(&connection, existing_session, cwd, mcp_servers, history) => response,
    };
    shared.replaying.store(false, Ordering::SeqCst);
    let (session_id, mut options) = opened?;
    if let Some(key) = &shared.discovery_key {
        super::discovery::forget_failure(key);
    }

    shared.send(AcpEvent::SessionStart(display_id.unwrap_or_else(
        || match &shared.remote {
            Some(remote) => remote.display_id(&session_id),
            None => session_id.clone(),
        },
    )));
    // Record the user's actual input once. The resume wrapper is only for the agent.
    let prompt = if client_transcript.is_some() {
        if let Some(resume_prompt) = resume_prompt {
            shared.send(AcpEvent::User(prompt));
            resume_prompt
        } else {
            shared.send(AcpEvent::User(prompt.clone()));
            prompt
        }
    } else {
        prompt
    };
    if skipped > 0 {
        shared.send(AcpEvent::SessionConfig(format!(
            "Skipped {skipped} MCP server(s) because the agent does not support their transport."
        )));
    }

    tokio::select! {
        biased;
        _ = shared.cancel.cancelled() => {
            connection.send_notification(CancelNotification::new(session_id))?;
            return Ok(());
        }
        () = apply_session_config(
            &connection,
            &session_id,
            &mut options,
            config,
            &shared,
            client_transcript.is_some(),
        ) => {}
    }

    // Denial feedback is delivered after the turn ends. Some agents (Copilot) end their turn on
    // rejection; others may keep working first, so for them the feedback can arrive late.
    let mut next_prompt = Some(prompt);
    while let Some(text) = next_prompt.take() {
        if shared.cancel.is_cancelled() {
            break;
        }
        // Read the complete result because the typed schema gates `usage` behind
        // an unstable feature. The stable stop reason still uses its schema type.
        let request = UntypedMessage::new(
            "session/prompt",
            PromptRequest::new(
                session_id.clone(),
                vec![ContentBlock::Text(TextContent::new(text))],
            ),
        )?;
        let mut turn = std::pin::pin!(connection.send_request(request).block_task());

        // Send the prompt and await completion to obtain stop_reason
        let response = tokio::select! {
            res = &mut turn => res,
            _ = shared.cancel.cancelled() => {
                connection.send_notification(CancelNotification::new(session_id.clone()))?;

                match tokio::time::timeout(CANCEL_GRACE, &mut turn).await {
                    Ok(res) => res,
                    Err(_) if shared.remote.is_some() => return Err(acp::util::internal_error(
                        "Trusted ACP node did not acknowledge cancellation before disconnect",
                    )),
                    Err(_) => return Ok(()), // The container will kill the local agent.
                }
            }
        };
        match response {
            Ok(resp) => {
                let (stop_reason, usage) = prompt_response(&resp)?;
                if let Some(event) = usage {
                    shared.send(event);
                }
                let stop_reason = serde_json::to_string(&stop_reason).unwrap_or_default();

                tracing::debug!("✅ Agent completed!");
                tracing::debug!("Stop reason: {:?}", stop_reason);
                shared.send(AcpEvent::Done(stop_reason));
            }
            Err(e) => {
                tracing::debug!("error {} {e} {:?}", e.code, e.data);
                if shared.remote.is_none() && acp::is_incoming_transport_closed(&e) {
                    tracing::debug!("ACP server killed");
                } else {
                    return Err(e);
                }
            }
        }

        if shared.cancel.is_cancelled() {
            // never send feedback after Stop
            break;
        }
        let feedback = shared
            .denial_feedback
            .lock()
            .unwrap()
            .drain(..)
            .collect::<Vec<_>>()
            .join("\n");
        if !feedback.is_empty() {
            next_prompt = Some(feedback);
        }
    }

    Ok(())
}

/// Applies the selected config, then shows what is in effect and anything that was skipped.
/// Never fails the run.
async fn apply_session_config(
    connection: &ConnectionTo<Agent>,
    session_id: &str,
    options: &mut SessionOptions,
    mut config: DesiredConfig,
    shared: &SessionShared,
    allow_unadvertised: bool,
) {
    let mut sink = ConnectionSink {
        connection,
        session_id: session_id.to_string(),
    };
    // Older Gemini/Qwen agents accept the v1 setters without advertising their options.
    if allow_unadvertised
        && let Some(model) = config.model.as_deref()
        && options
            .model_slot()
            .is_none_or(|slot| slot.setter == Setter::SetModel && slot.choices.is_empty())
    {
        if let Err(err) = set_value(options, &Setter::SetModel, model, &mut sink).await {
            tracing::warn!("Couldn't switch the ACP model to {model}: {err}");
        }
        config.model = None;
    }
    let legacy_mode = if allow_unadvertised
        && config.mode.is_some()
        && options
            .mode_slot()
            .is_none_or(|slot| slot.setter == Setter::SetMode && slot.choices.is_empty())
    {
        config.mode.take()
    } else {
        None
    };
    let outcome = apply_config(options, &config, &mut sink).await;
    if let Some(mode) = legacy_mode
        && let Err(err) = set_value(options, &Setter::SetMode, &mode, &mut sink).await
    {
        tracing::warn!("Couldn't switch to ACP mode {mode}: {err}");
    }
    for warning in &outcome.warnings {
        tracing::warn!("ACP session config: {warning}");
    }

    *shared.options.lock().unwrap_or_else(|e| e.into_inner()) = options.clone();
    shared.tracking_config.store(true, Ordering::SeqCst);

    if let Some(line) = summary(options, outcome.permission.as_ref()) {
        shared.send(AcpEvent::SessionConfig(line));
    }
    for warning in outcome.warnings {
        shared.send(AcpEvent::SessionConfig(warning));
    }
}

/// The event logged when a run ends with `err`. -32000 on `session/new` (signed out) or
/// `session/prompt` (token expired mid-session) becomes `AuthRequired`, not a plain error.
fn exit_event(err: &acp::Error) -> AcpEvent {
    if err.code == acp::ErrorCode::AuthRequired {
        let message = if err.message.is_empty() {
            err.code.to_string()
        } else {
            err.message.clone()
        };
        AcpEvent::AuthRequired(message)
    } else {
        AcpEvent::Error(err.to_string())
    }
}

pub(crate) async fn with_timeout<T>(
    step: &str,
    request: impl Future<Output = Result<T, acp::Error>>,
) -> Result<T, acp::Error> {
    tokio::time::timeout(SETUP_TIMEOUT, request)
        .await
        .map_err(|_| acp::util::internal_error(format!("Timed out waiting for agent to {step}")))?
}

#[cfg(test)]
mod mcp_requests {
    use acp::{Channel, TransportFrame};
    use futures::StreamExt;
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, BufReader};

    use super::*;
    use crate::executors::acp::mcp;

    async fn receive(peer: &mut Channel) -> Value {
        let TransportFrame::Single(message) = peer.rx.next().await.unwrap() else {
            panic!("expected a single ACP request");
        };
        serde_json::to_value(message).unwrap()
    }

    fn reply(peer: &Channel, request: &Value, result: Value) {
        let response = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
        peer.tx
            .unbounded_send(TransportFrame::Single(
                serde_json::from_value(response).unwrap(),
            ))
            .unwrap();
    }

    #[tokio::test]
    async fn trusted_node_cancel_records_acknowledgment_and_usage() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (transport, mut peer) = Channel::duplex();
            let (log_tx, mut log_rx) = mpsc::unbounded_channel();
            let cancel = CancellationToken::new();
            let workspace = "/shared/workspace";
            let scope = Arc::new(RemoteSession::new(b"test node", workspace.into()));
            let expected_id = scope.display_id("node-session");
            let shared = SessionShared {
                log_tx,
                cancel: cancel.clone(),
                replaying: Arc::default(),
                denial_feedback: Arc::default(),
                options: Arc::default(),
                discovery_key: None,
                tracking_config: Arc::default(),
                remote: Some(scope),
            };
            let client = tokio::spawn(acp::Client.builder().connect_with(
                transport,
                async move |cx| {
                    run_session(
                        cx,
                        None,
                        PathBuf::from(workspace),
                        "test".into(),
                        DesiredConfig::default(),
                        shared,
                        Vec::new(),
                        None,
                    )
                    .await
                },
            ));
            let initialize = receive(&mut peer).await;
            assert_eq!(initialize["method"], "initialize");
            reply(&peer, &initialize, json!({"protocolVersion": 1}));
            let session = receive(&mut peer).await;
            assert_eq!(session["method"], "session/new");
            assert_eq!(session["params"]["cwd"], workspace);
            assert_eq!(session["params"]["mcpServers"], json!([]));
            reply(&peer, &session, json!({"sessionId": "node-session"}));
            let prompt = receive(&mut peer).await;
            assert_eq!(prompt["method"], "session/prompt");
            cancel.cancel();
            let stop = receive(&mut peer).await;
            assert_eq!(stop["method"], "session/cancel");
            assert_eq!(stop["params"]["sessionId"], "node-session");
            reply(
                &peer,
                &prompt,
                json!({
                    "stopReason": "cancelled",
                    "usage": {"totalTokens": 50, "inputTokens": 39, "outputTokens": 11},
                }),
            );
            client.await.unwrap().unwrap();
            let mut events = Vec::new();
            while let Ok(line) = log_rx.try_recv() {
                events.push(line.parse::<AcpEvent>().unwrap());
            }
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, AcpEvent::SessionStart(id) if id == &expected_id))
            );
            assert!(
                events.iter().any(
                    |event| matches!(event, AcpEvent::Done(reason) if reason == "\"cancelled\"")
                )
            );
            assert!(events.iter().any(|event| matches!(
                event,
                AcpEvent::Usage(AcpUsageUpdate::Prompt(ExecutionTokenUsage {
                    input_tokens: 39,
                    output_tokens: 11,
                    cumulative: true,
                    ..
                }))
            )));
        })
        .await
        .expect("trusted ACP cancellation timed out");
    }

    async fn capture_session_request(
        cwd: PathBuf,
        existing: Option<&str>,
        mcp_capabilities: Value,
        mcp_servers: Vec<McpServer>,
        discover_tools: bool,
    ) -> (Value, Vec<AcpEvent>) {
        tokio::time::timeout(Duration::from_secs(15), async move {
            let (transport, mut peer) = Channel::duplex();
            let (log_tx, mut log_rx) = mpsc::unbounded_channel();
            let shared = SessionShared {
                log_tx,
                cancel: CancellationToken::new(),
                replaying: Arc::default(),
                denial_feedback: Arc::default(),
                options: Arc::default(),
                discovery_key: None,
                tracking_config: Arc::default(),
                remote: None,
            };
            let existing_session = existing.map(str::to_owned);
            let client = tokio::spawn(acp::Client.builder().connect_with(
                transport,
                async move |cx| {
                    run_session(
                        cx,
                        existing_session,
                        cwd,
                        "test".into(),
                        DesiredConfig::default(),
                        shared,
                        mcp_servers,
                        None,
                    )
                    .await
                },
            ));

            let initialize = receive(&mut peer).await;
            assert_eq!(initialize["method"], "initialize");
            reply(
                &peer,
                &initialize,
                json!({
                    "protocolVersion": 1,
                    "agentCapabilities": {
                        "loadSession": true,
                        "mcpCapabilities": mcp_capabilities,
                    },
                }),
            );

            let session = receive(&mut peer).await;
            if discover_tools {
                let descriptor = &session["params"]["mcpServers"][0];
                assert_eq!(descriptor["name"], "vibe-kanban");
                let tools = discover_stdio_tools(descriptor).await;
                assert!(tools.iter().any(|tool| tool["name"] == "list_repos"));
                eprintln!(
                    "{} -> MCP tools/list contains list_repos",
                    session["method"]
                );
            }
            let session_id = existing.unwrap_or("new-id");
            let result = if existing.is_some() {
                json!({})
            } else {
                json!({"sessionId": session_id})
            };
            reply(&peer, &session, result);
            let prompt = receive(&mut peer).await;
            assert_eq!(prompt["method"], "session/prompt");
            assert_eq!(prompt["params"]["sessionId"], session_id);
            reply(
                &peer,
                &prompt,
                json!({
                    "stopReason": "end_turn",
                    "usage": { "totalTokens": 50, "inputTokens": 39, "outputTokens": 11 }
                }),
            );
            client.await.unwrap().unwrap();

            let mut events = Vec::new();
            while let Ok(line) = log_rx.try_recv() {
                events.push(serde_json::from_str(&line).unwrap());
            }
            assert!(events.iter().any(|event| matches!(
                event,
                AcpEvent::Usage(AcpUsageUpdate::Prompt(ExecutionTokenUsage {
                    input_tokens: 39,
                    output_tokens: 11,
                    cumulative: true,
                    ..
                }))
            )));
            (session, events)
        })
        .await
        .expect("ACP session request timed out")
    }

    async fn discover_stdio_tools(descriptor: &Value) -> Vec<Value> {
        let mut child = Command::new(descriptor["command"].as_str().unwrap())
            .args(
                descriptor["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| arg.as_str().unwrap()),
            )
            .envs(descriptor["env"].as_array().unwrap().iter().map(|entry| {
                (
                    entry["name"].as_str().unwrap(),
                    entry["value"].as_str().unwrap(),
                )
            }))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
        let write = async |stdin: &mut tokio::process::ChildStdin, message: Value| {
            stdin
                .write_all(format!("{message}\n").as_bytes())
                .await
                .unwrap();
        };
        write(
            &mut stdin,
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-03-26", "capabilities": {},
                    "clientInfo": {"name": "acp-test-agent", "version": "1"} }
            }),
        )
        .await;
        let init: Value =
            serde_json::from_str(&stdout.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "vibe-kanban-mcp");
        write(
            &mut stdin,
            json!({"jsonrpc":"2.0", "method":"notifications/initialized"}),
        )
        .await;
        write(
            &mut stdin,
            json!({"jsonrpc":"2.0", "id":2, "method":"tools/list"}),
        )
        .await;
        let list: Value =
            serde_json::from_str(&stdout.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(list["id"], 2);
        drop(stdin);
        assert!(child.wait().await.unwrap().success());
        list["result"]["tools"].as_array().unwrap().to_owned()
    }

    #[tokio::test]
    #[ignore = "requires VK_TEST_MCP_BIN pointing to the built vibe-kanban-mcp binary"]
    async fn new_and_loaded_sessions_discover_local_mcp_tools() {
        let dir = tempfile::tempdir().unwrap();
        let path = mcp::config_path(dir.path(), "test/local-mcp");
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let command =
            PathBuf::from(std::env::var_os("VK_TEST_MCP_BIN").expect("set VK_TEST_MCP_BIN"));
        let config = json!({"mcpServers": {"vibe-kanban": {
            "command": command, "args": ["--mode", "global"],
            "env": {"VIBE_BACKEND_URL": "http://127.0.0.1:1"}
        }}});
        tokio::fs::write(&path, config.to_string()).await.unwrap();
        for (existing, method) in [(None, "session/new"), (Some("new-id"), "session/load")] {
            let servers = mcp::read_servers(Some(&path), dir.path()).await.unwrap();
            let (request, _) =
                capture_session_request(dir.path().to_owned(), existing, json!({}), servers, true)
                    .await;
            assert_eq!(request["method"], method);
            assert_eq!(
                request["params"]["mcpServers"][0]["command"],
                json!(command)
            );
        }
    }

    #[tokio::test]
    async fn new_and_loaded_sessions_send_configured_mcp_servers() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        let command = cwd.join("test-runner");
        let path = mcp::config_path(cwd, "test/registry-agent");
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let config = json!({
            "mcpServers": {
                "cli": {"command": command, "args": ["--stdio"], "env": {"SETTING": "example"}},
                "http": {"type": "http", "url": "https://example.test/mcp", "headers": {"X-Test": "example"}},
                "sse": {"type": "sse", "url": "https://example.test/events"},
            },
        });
        tokio::fs::write(&path, config.to_string()).await.unwrap();
        let servers = mcp::read_servers(Some(&path), cwd).await.unwrap();
        let stdio = json!({
            "name": "cli", "command": command, "args": ["--stdio"],
            "env": [{"name": "SETTING", "value": "example"}],
        });
        let http = json!({
            "name": "http", "type": "http", "url": "https://example.test/mcp",
            "headers": [{"name": "X-Test", "value": "example"}],
        });
        let sse = json!({
            "name": "sse", "type": "sse", "url": "https://example.test/events", "headers": [],
        });
        let (new, _) = capture_session_request(
            cwd.to_owned(),
            None,
            json!({"http": true, "sse": true}),
            servers.clone(),
            false,
        )
        .await;
        assert_eq!(new["method"], "session/new");
        assert_eq!(new["params"]["cwd"], json!(cwd));
        assert_eq!(new["params"]["mcpServers"], json!([stdio, http, sse]));

        let (load, events) = capture_session_request(
            cwd.to_owned(),
            Some("existing-id"),
            json!({"sse": true}),
            servers,
            false,
        )
        .await;
        assert_eq!(load["method"], "session/load");
        assert_eq!(load["params"]["sessionId"], "existing-id");
        assert_eq!(load["params"]["cwd"], json!(cwd));
        assert_eq!(load["params"]["mcpServers"], json!([stdio, sse]));
        assert!(events.iter().any(|event| matches!(
            event,
            AcpEvent::SessionConfig(message) if message.contains("Skipped 1 MCP")
        )));
    }

    #[tokio::test]
    async fn sessions_without_mcp_settings_send_an_empty_list() {
        let cwd = tempfile::tempdir().unwrap();
        let servers = mcp::read_servers(None, cwd.path()).await.unwrap();
        let (request, events) =
            capture_session_request(cwd.path().to_path_buf(), None, json!({}), servers, false)
                .await;

        assert_eq!(request["method"], "session/new");
        assert_eq!(request["params"]["mcpServers"], json!([]));
        assert!(!events.iter().any(|event| matches!(
            event,
            AcpEvent::SessionConfig(message) if message.contains("MCP")
        )));
    }

    #[tokio::test]
    async fn client_transcript_follow_up_forks_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SessionManager::from_directory(dir.path().to_owned()).unwrap());
        let history = "{\"user\":\"Original task\"}\n{\"assistant\":\"Prior result\"}\n";
        std::fs::write(dir.path().join("parent.jsonl"), history).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            let (transport, mut peer) = Channel::duplex();
            let (log_tx, log_rx) = mpsc::unbounded_channel();
            let shared = SessionShared {
                log_tx,
                cancel: CancellationToken::new(),
                replaying: Arc::default(),
                denial_feedback: Arc::default(),
                options: Arc::default(),
                discovery_key: None,
                tracking_config: Arc::default(),
                remote: None,
            };
            let cwd = dir.path().to_owned();
            let client_store = store.clone();
            let client = tokio::spawn(acp::Client.builder().connect_with(
                transport,
                async move |cx| {
                    let config = DesiredConfig {
                        model: Some("old-model".into()),
                        mode: Some("old-mode".into()),
                        ..Default::default()
                    };
                    run_session(
                        cx,
                        Some("parent".into()),
                        cwd,
                        "Next task".into(),
                        config,
                        shared,
                        Vec::new(),
                        Some(&client_store),
                    )
                    .await
                },
            ));

            let initialize = receive(&mut peer).await;
            assert_eq!(initialize["method"], "initialize");
            assert_eq!(initialize["params"]["protocolVersion"], 1);
            reply(&peer, &initialize, json!({}));

            let session = receive(&mut peer).await;
            assert_eq!(session["method"], "session/new");
            assert_eq!(session["params"]["_meta"]["history_jsonl"], history);
            reply(&peer, &session, json!({"sessionId": "agent-id"}));

            for (method, key, value) in [
                ("session/set_model", "modelId", "old-model"),
                ("session/set_mode", "modeId", "old-mode"),
            ] {
                let setter = receive(&mut peer).await;
                assert_eq!(setter["method"], method);
                assert_eq!(setter["params"]["sessionId"], "agent-id");
                assert_eq!(setter["params"][key], value);
                reply(&peer, &setter, json!({}));
            }

            let prompt = receive(&mut peer).await;
            assert_eq!(prompt["method"], "session/prompt");
            assert_eq!(prompt["params"]["sessionId"], "agent-id");
            let text = prompt["params"]["prompt"][0]["text"].as_str().unwrap();
            assert!(text.contains(history));
            assert!(text.contains("=== CURRENT REQUEST ===\nNext task"));
            reply(&peer, &prompt, json!({"stopReason": "end_turn"}));
            client.await.unwrap().unwrap();

            let mut stdout = Vec::new();
            write_logs(&mut stdout, log_rx, Some(store.clone())).await;
            let stdout = String::from_utf8(stdout).unwrap();
            let AcpEvent::SessionStart(fork_id) = stdout.lines().next().unwrap().parse().unwrap()
            else {
                panic!("expected a forked display id");
            };
            assert!(uuid::Uuid::parse_str(&fork_id).is_ok());
            assert_eq!(store.read_session_raw("parent").unwrap(), history);
            assert_eq!(
                store.read_session_raw(&fork_id).unwrap(),
                format!("{history}{{\"user\":\"Next task\"}}\n")
            );
            assert!(!store.has_session("agent-id"));
        })
        .await
        .expect("ACP transcript follow-up timed out");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code must survive the request path; it's only turned into text by `exit_event`.
    #[tokio::test]
    async fn auth_error_keeps_its_code_through_with_timeout() {
        let err = with_timeout("create a new session", async {
            Err::<(), _>(acp::Error::new(-32000, "Authentication required"))
        })
        .await
        .unwrap_err();
        assert_eq!(err.code, acp::ErrorCode::AuthRequired);
        assert_eq!(err.message, "Authentication required");
    }

    #[test]
    fn auth_required_becomes_its_own_event() {
        let event = exit_event(&acp::Error::new(-32000, "Not signed in"));
        assert!(matches!(event, AcpEvent::AuthRequired(m) if m == "Not signed in"));

        // No message from the agent: fall back to the code's name
        let event = exit_event(&acp::Error::new(-32000, ""));
        let AcpEvent::AuthRequired(message) = event else {
            panic!("expected AuthRequired, got {event:?}");
        };
        assert_eq!(message, "Authentication required");
    }

    #[test]
    fn other_errors_stay_plain() {
        let event = exit_event(&acp::util::internal_error("boom"));
        assert!(matches!(event, AcpEvent::Error(_)));
    }

    #[test]
    fn prompt_result_usage_survives_stdout_serialization() {
        let response: Value = serde_json::from_str(
            r#"{
            "stopReason":"cancelled",
            "usage":{
                "totalTokens":4700000070,"inputTokens":4700000000,"outputTokens":40,
                "cachedReadTokens":20,"cachedWriteTokens":10,"thoughtTokens":5
            }
        }"#,
        )
        .unwrap();
        let (reason, event) = prompt_response(&response).unwrap();
        assert_eq!(reason, StopReason::Cancelled);
        let serialized = event.unwrap().to_string();
        let AcpEvent::Usage(AcpUsageUpdate::Prompt(billing)) = serialized.parse().unwrap() else {
            panic!("expected prompt billing in the serialized event");
        };
        assert_eq!(
            billing,
            ExecutionTokenUsage {
                input_tokens: 4_700_000_000,
                output_tokens: 40,
                cache_read_tokens: 20,
                cache_write_tokens: 10,
                cumulative: true,
                ..Default::default()
            }
        );

        let response = serde_json::json!({"stopReason": "end_turn", "usage": {"inputTokens": 1}});
        let (reason, event) = prompt_response(&response).unwrap();
        assert_eq!(reason, StopReason::EndTurn);
        assert!(event.is_none());
    }
}
