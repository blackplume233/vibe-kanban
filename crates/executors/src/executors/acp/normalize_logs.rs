use std::{
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};

use acp::schema::v1;
use futures::StreamExt;
use regex::Regex;
use serde::Deserialize;
use workspace_utils::{approvals::ApprovalStatus, msg_store::MsgStore};

use super::{AcpEvent, AcpUsageUpdate};
use crate::{
    approvals::ToolCallMetadata,
    logs::{
        ActionType, FileChange, NormalizedEntry, NormalizedEntryError, NormalizedEntryType,
        TodoItem, TokenUsageInfo, ToolResult, ToolResultValueType, ToolStatus as LogToolStatus,
        plain_text_processor::PlainTextLogProcessor,
        stderr_processor::normalize_stderr_logs,
        utils::{
            ConversationPatch, EntryIndexProvider, patch::upsert_normalized_entry,
            shell_command_parsing::CommandCategory,
        },
    },
};

pub fn normalize_logs(
    msg_store: Arc<MsgStore>,
    worktree_path: &Path,
) -> Vec<tokio::task::JoinHandle<()>> {
    normalize_logs_with_suppressed_stderr_patterns(msg_store, worktree_path, &[])
}

fn push_error(
    msg_store: &MsgStore,
    entry_index: &EntryIndexProvider,
    error_type: NormalizedEntryError,
    content: String,
) {
    let entry = NormalizedEntry {
        timestamp: None,
        entry_type: NormalizedEntryType::ErrorMessage { error_type },
        content,
        metadata: None,
    };
    msg_store.push_patch(ConversationPatch::add_normalized_entry(
        entry_index.next(),
        entry,
    ));
}

pub fn normalize_logs_with_suppressed_stderr_patterns(
    msg_store: Arc<MsgStore>,
    worktree_path: &Path,
    suppressed_stderr_patterns: &[&str],
) -> Vec<tokio::task::JoinHandle<()>> {
    // stderr normalization
    let entry_index = EntryIndexProvider::start_from(&msg_store);
    let h1 = if suppressed_stderr_patterns.is_empty() {
        normalize_stderr_logs(msg_store.clone(), entry_index.clone())
    } else {
        normalize_acp_stderr_logs(
            msg_store.clone(),
            entry_index.clone(),
            suppressed_stderr_patterns
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
        )
    };

    // stdout normalization (main loop)
    let worktree_path = worktree_path.to_path_buf();
    // Type aliases to simplify complex state types and appease clippy
    let h2 = tokio::spawn(async move {
        type ToolStates = HashMap<Arc<str>, PartialToolCallData>;

        let mut stored_session_id = false;
        let mut streaming: StreamingState = StreamingState::default();
        let mut tool_states: ToolStates = HashMap::new();
        let mut usage_index = None;
        let mut usage = TokenUsageInfo {
            total_tokens: 0,
            model_context_window: 0,
            billing: None,
        };

        let mut stdout_lines = msg_store.stdout_lines_stream();
        while let Some(Ok(line)) = stdout_lines.next().await {
            if let Some(parsed) = AcpEventParser::parse_line(&line) {
                tracing::trace!("Parsed ACP line: {:?}", parsed);
                match parsed {
                    AcpEvent::SessionStart(id) => {
                        if !stored_session_id {
                            msg_store.push_session_id(id);
                            stored_session_id = true;
                        }
                    }
                    AcpEvent::Error(msg) => {
                        push_error(&msg_store, &entry_index, NormalizedEntryError::Other, msg);
                    }
                    // Setup-required is what offers the Sign in button
                    AcpEvent::AuthRequired(msg) => {
                        push_error(
                            &msg_store,
                            &entry_index,
                            NormalizedEntryError::SetupRequired,
                            msg,
                        );
                    }
                    AcpEvent::Done(_) => {
                        streaming.assistant_text = None;
                        streaming.thinking_text = None;
                    }
                    AcpEvent::Usage(update) => {
                        match update {
                            AcpUsageUpdate::Context {
                                used,
                                size,
                                cost_usd,
                            } => {
                                usage.total_tokens = u32::try_from(used).unwrap_or(u32::MAX);
                                usage.model_context_window =
                                    u32::try_from(size).unwrap_or(u32::MAX);
                                if let Some(cost) = cost_usd {
                                    let billing = usage.billing.get_or_insert_default();
                                    billing.cost_usd = Some(cost);
                                    billing.cumulative = true;
                                }
                            }
                            AcpUsageUpdate::Prompt(mut billing) => {
                                billing.cost_usd = usage
                                    .billing
                                    .as_ref()
                                    .and_then(|previous| previous.cost_usd);
                                usage.billing = Some(billing);
                            }
                        }
                        let content = if usage.model_context_window == 0 {
                            "Token usage".to_string()
                        } else {
                            format!(
                                "Tokens used: {} / Context window: {}",
                                usage.total_tokens, usage.model_context_window
                            )
                        };
                        let is_new = usage_index.is_none();
                        let index = *usage_index.get_or_insert_with(|| entry_index.next());
                        upsert_normalized_entry(
                            &msg_store,
                            index,
                            NormalizedEntry {
                                timestamp: None,
                                entry_type: NormalizedEntryType::TokenUsageInfo(usage.clone()),
                                content,
                                metadata: None,
                            },
                            is_new,
                        );
                    }
                    AcpEvent::Message(content) => {
                        streaming.thinking_text = None;
                        if let v1::ContentBlock::Text(text) = content {
                            let is_new = streaming.assistant_text.is_none();
                            if is_new {
                                if text.text == "\n" {
                                    continue;
                                }
                                let idx = entry_index.next();
                                streaming.assistant_text = Some(StreamingText {
                                    index: idx,
                                    content: String::new(),
                                });
                            }
                            if let Some(ref mut s) = streaming.assistant_text {
                                s.content.push_str(&text.text);
                                let entry = NormalizedEntry {
                                    timestamp: None,
                                    entry_type: NormalizedEntryType::AssistantMessage,
                                    content: s.content.clone(),
                                    metadata: None,
                                };
                                let patch = if is_new {
                                    ConversationPatch::add_normalized_entry(s.index, entry)
                                } else {
                                    ConversationPatch::replace(s.index, entry)
                                };
                                msg_store.push_patch(patch);
                            }
                        }
                    }
                    AcpEvent::Thought(content) => {
                        streaming.assistant_text = None;
                        if let v1::ContentBlock::Text(text) = content {
                            let is_new = streaming.thinking_text.is_none();
                            if is_new {
                                let idx = entry_index.next();
                                streaming.thinking_text = Some(StreamingText {
                                    index: idx,
                                    content: String::new(),
                                });
                            }
                            if let Some(ref mut s) = streaming.thinking_text {
                                s.content.push_str(&text.text);
                                let entry = NormalizedEntry {
                                    timestamp: None,
                                    entry_type: NormalizedEntryType::Thinking,
                                    content: s.content.clone(),
                                    metadata: None,
                                };
                                let patch = if is_new {
                                    ConversationPatch::add_normalized_entry(s.index, entry)
                                } else {
                                    ConversationPatch::replace(s.index, entry)
                                };
                                msg_store.push_patch(patch);
                            }
                        }
                    }
                    AcpEvent::Plan(plan) => {
                        streaming.assistant_text = None;
                        streaming.thinking_text = None;
                        let todos: Vec<TodoItem> = plan
                            .entries
                            .into_iter()
                            .map(|e| TodoItem {
                                content: e.content,
                                status: match e.status {
                                    v1::PlanEntryStatus::Pending => "pending",
                                    v1::PlanEntryStatus::InProgress => "in_progress",
                                    v1::PlanEntryStatus::Completed => "completed",
                                    _ => "unknown",
                                }
                                .to_string(),
                                priority: match e.priority {
                                    v1::PlanEntryPriority::High => Some("high"),
                                    v1::PlanEntryPriority::Medium => Some("medium"),
                                    v1::PlanEntryPriority::Low => Some("low"),
                                    _ => None,
                                }
                                .map(str::to_string),
                            })
                            .collect();

                        let idx = entry_index.next();
                        let entry = NormalizedEntry {
                            timestamp: None,
                            entry_type: NormalizedEntryType::ToolUse {
                                tool_name: "plan".to_string(),
                                action_type: ActionType::TodoManagement {
                                    todos,
                                    operation: "update".to_string(),
                                },
                                status: LogToolStatus::Success,
                            },
                            content: "Plan updated".to_string(),
                            metadata: None,
                        };
                        msg_store.push_patch(ConversationPatch::add_normalized_entry(idx, entry));
                    }
                    AcpEvent::AvailableCommands(cmds) => {
                        let mut body = String::from("Available commands:\n");
                        for c in &cmds {
                            body.push_str(&format!("- {}\n", c.name));
                        }
                        let idx = entry_index.next();
                        let entry = NormalizedEntry {
                            timestamp: None,
                            entry_type: NormalizedEntryType::SystemMessage,
                            content: body,
                            metadata: None,
                        };
                        msg_store.push_patch(ConversationPatch::add_normalized_entry(idx, entry));
                    }
                    AcpEvent::SessionConfig(content) => {
                        let idx = entry_index.next();
                        let entry = NormalizedEntry {
                            timestamp: None,
                            entry_type: NormalizedEntryType::SystemMessage,
                            content,
                            metadata: None,
                        };
                        msg_store.push_patch(ConversationPatch::add_normalized_entry(idx, entry));
                    }
                    AcpEvent::CurrentMode(mode_id) => {
                        let idx = entry_index.next();
                        let entry = NormalizedEntry {
                            timestamp: None,
                            entry_type: NormalizedEntryType::SystemMessage,
                            content: format!("Current mode: {}", mode_id.0),
                            metadata: None,
                        };
                        msg_store.push_patch(ConversationPatch::add_normalized_entry(idx, entry));
                    }
                    AcpEvent::RequestPermission(perm) => {
                        if perm.tool_call.fields.title.is_some() {
                            handle_tool_update(
                                perm.tool_call,
                                &worktree_path,
                                &mut streaming,
                                &mut tool_states,
                                &entry_index,
                                &msg_store,
                            );
                        }
                    }
                    AcpEvent::ToolCall(tc) => handle_tool_update(
                        tc.into(),
                        &worktree_path,
                        &mut streaming,
                        &mut tool_states,
                        &entry_index,
                        &msg_store,
                    ),
                    AcpEvent::ToolUpdate(update) => {
                        tracing::trace!("Got tool call update: {:?}", update);
                        handle_tool_update(
                            update,
                            &worktree_path,
                            &mut streaming,
                            &mut tool_states,
                            &entry_index,
                            &msg_store,
                        );
                    }
                    AcpEvent::ApprovalRequested {
                        tool_call_id,
                        approval_id,
                    } => {
                        if let Some(tool_data) = tool_states.get(tool_call_id.as_str()) {
                            let action = map_to_action_type(tool_data);
                            let entry = NormalizedEntry {
                                timestamp: None,
                                entry_type: NormalizedEntryType::ToolUse {
                                    tool_name: tool_data.title.clone(),
                                    action_type: action,
                                    status: LogToolStatus::PendingApproval { approval_id },
                                },
                                content: get_tool_content(tool_data),
                                metadata: None,
                            };
                            msg_store
                                .push_patch(ConversationPatch::replace(tool_data.index, entry));
                        }
                    }
                    AcpEvent::ApprovalResponse(resp) => {
                        tracing::trace!("Received approval response: {:?}", resp);

                        if let Some(tool_data) = tool_states.get(resp.tool_call_id.as_str()) {
                            let new_status = LogToolStatus::from_approval_status(&resp.status);
                            if let Some(status) = new_status {
                                let action = map_to_action_type(tool_data);
                                let entry = NormalizedEntry {
                                    timestamp: None,
                                    entry_type: NormalizedEntryType::ToolUse {
                                        tool_name: tool_data.title.clone(),
                                        action_type: action,
                                        status,
                                    },
                                    content: get_tool_content(tool_data),
                                    metadata: serde_json::to_value(ToolCallMetadata {
                                        tool_call_id: tool_data.id.0.to_string(),
                                    })
                                    .ok(),
                                };
                                msg_store
                                    .push_patch(ConversationPatch::replace(tool_data.index, entry));
                            }
                        }

                        if let ApprovalStatus::Denied { reason } = resp.status {
                            let tool_name = tool_states
                                .get(resp.tool_call_id.as_str())
                                .map(|t| {
                                    extract_tool_name_from_id(t.id.0.as_ref())
                                        .unwrap_or_else(|| t.title.clone())
                                })
                                .unwrap_or_default();
                            let idx = entry_index.next();
                            let entry = NormalizedEntry {
                                timestamp: None,
                                entry_type: NormalizedEntryType::UserFeedback {
                                    denied_tool: tool_name,
                                },
                                content: reason
                                    .clone()
                                    .unwrap_or_else(|| {
                                        "User denied this tool use request".to_string()
                                    })
                                    .trim()
                                    .to_string(),
                                metadata: None,
                            };
                            msg_store
                                .push_patch(ConversationPatch::add_normalized_entry(idx, entry));
                        }
                    }
                    AcpEvent::User(_) | AcpEvent::Other(_) => (),
                }
            }
        }

        fn handle_tool_update(
            update: v1::ToolCallUpdate,
            worktree_path: &Path,
            streaming: &mut StreamingState,
            tool_states: &mut ToolStates,
            entry_index: &EntryIndexProvider,
            msg_store: &Arc<MsgStore>,
        ) {
            streaming.assistant_text = None;
            streaming.thinking_text = None;
            let (tool_data, is_new) = match tool_states.entry(update.tool_call_id.0.clone()) {
                Entry::Occupied(entry) => (entry.into_mut(), false),
                Entry::Vacant(entry) => (
                    entry.insert(PartialToolCallData::new(
                        update.tool_call_id,
                        entry_index.next(),
                    )),
                    true,
                ),
            };
            tool_data.extend(update.fields, worktree_path);
            let action = map_to_action_type(tool_data);
            let entry = NormalizedEntry {
                timestamp: None,
                entry_type: NormalizedEntryType::ToolUse {
                    tool_name: tool_data.title.clone(),
                    action_type: action,
                    status: convert_tool_status(tool_data.status),
                },
                content: get_tool_content(tool_data),
                metadata: serde_json::to_value(ToolCallMetadata {
                    tool_call_id: tool_data.id.0.to_string(),
                })
                .ok(),
            };
            let patch = if is_new {
                ConversationPatch::add_normalized_entry(tool_data.index, entry)
            } else {
                ConversationPatch::replace(tool_data.index, entry)
            };
            msg_store.push_patch(patch);
        }

        fn map_to_action_type(tc: &PartialToolCallData) -> ActionType {
            match tc.kind {
                v1::ToolKind::Read => {
                    // Special-case: read_many_files style titles parsed via helper
                    if tc.id.0.starts_with("read_many_files") {
                        let result = collect_text_content(&tc.content).map(|text| ToolResult {
                            r#type: ToolResultValueType::Markdown,
                            value: serde_json::Value::String(text),
                        });
                        return ActionType::Tool {
                            tool_name: "read_many_files".to_string(),
                            arguments: Some(serde_json::Value::String(tc.title.clone())),
                            result,
                        };
                    }
                    ActionType::FileRead {
                        path: tc
                            .path
                            .clone()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    }
                }
                v1::ToolKind::Edit => {
                    let changes = extract_file_changes(tc);
                    ActionType::FileEdit {
                        path: tc
                            .path
                            .clone()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        changes,
                    }
                }
                v1::ToolKind::Execute => {
                    let command = AcpEventParser::parse_execute_command(tc);
                    // Prefer structured raw_output, else fallback to aggregated text content
                    let completed = matches!(tc.status, v1::ToolCallStatus::Completed);
                    tracing::trace!(
                        "Mapping execute tool call, completed: {}, command: {}",
                        completed,
                        command
                    );
                    let tc_exit_status = match tc.status {
                        v1::ToolCallStatus::Completed => {
                            Some(crate::logs::CommandExitStatus::Success { success: true })
                        }
                        v1::ToolCallStatus::Failed => {
                            Some(crate::logs::CommandExitStatus::Success { success: false })
                        }
                        _ => None,
                    };

                    let result = if let Some(text) = collect_text_content(&tc.content) {
                        Some(crate::logs::CommandRunResult {
                            exit_status: tc_exit_status,
                            output: Some(text),
                        })
                    } else {
                        Some(crate::logs::CommandRunResult {
                            exit_status: tc_exit_status,
                            output: None,
                        })
                    };
                    ActionType::CommandRun {
                        command: command.clone(),
                        result,
                        category: CommandCategory::from_command(&command),
                    }
                }
                v1::ToolKind::Delete => ActionType::FileEdit {
                    path: tc
                        .path
                        .clone()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    changes: vec![FileChange::Delete],
                },
                v1::ToolKind::Search => {
                    let query = tc
                        .raw_input
                        .as_ref()
                        .and_then(|v| serde_json::from_value::<SearchArgs>(v.clone()).ok())
                        .map(|a| a.query)
                        .unwrap_or_else(|| tc.title.clone());
                    ActionType::Search { query }
                }
                v1::ToolKind::Fetch => {
                    let mut url = tc
                        .raw_input
                        .as_ref()
                        .and_then(|v| serde_json::from_value::<FetchArgs>(v.clone()).ok())
                        .map(|a| a.url)
                        .unwrap_or_default();
                    if url.is_empty() {
                        // Fallback: try to extract first URL from the title
                        if let Some(extracted) = extract_url_from_text(&tc.title) {
                            url = extracted;
                        }
                    }
                    ActionType::WebFetch { url }
                }
                v1::ToolKind::Think => {
                    let tool_name = extract_tool_name_from_id(tc.id.0.as_ref())
                        .unwrap_or_else(|| tc.title.clone());
                    // For think/save_memory, surface both title and aggregated text content as arguments
                    let text = collect_text_content(&tc.content);
                    let arguments = Some(match &text {
                        Some(t) => serde_json::json!({ "title": tc.title, "content": t }),
                        None => serde_json::json!({ "title": tc.title }),
                    });
                    let result = if let Some(output) = &tc.raw_output {
                        Some(ToolResult {
                            r#type: ToolResultValueType::Json,
                            value: output.clone(),
                        })
                    } else {
                        collect_text_content(&tc.content).map(|text| ToolResult {
                            r#type: ToolResultValueType::Markdown,
                            value: serde_json::Value::String(text),
                        })
                    };
                    ActionType::Tool {
                        tool_name,
                        arguments,
                        result,
                    }
                }
                v1::ToolKind::SwitchMode => ActionType::Other {
                    description: "switch_mode".to_string(),
                },
                _ => {
                    // Derive a friendlier tool name from the id if it looks like name-<digits>
                    let tool_name = extract_tool_name_from_id(tc.id.0.as_ref())
                        .unwrap_or_else(|| tc.title.clone());

                    // Some tools embed JSON args into the title instead of raw_input
                    let arguments = if let Some(raw) = &tc.raw_input {
                        Some(raw.clone())
                    } else if tc.title.trim_start().starts_with('{') {
                        // Title contains JSON arguments for the tool
                        serde_json::from_str::<serde_json::Value>(&tc.title).ok()
                    } else {
                        None
                    };
                    // Extract result: prefer raw_output (structured), else text content as Markdown
                    let result = if let Some(output) = &tc.raw_output {
                        Some(ToolResult {
                            r#type: ToolResultValueType::Json,
                            value: output.clone(),
                        })
                    } else {
                        collect_text_content(&tc.content).map(|text| ToolResult {
                            r#type: ToolResultValueType::Markdown,
                            value: serde_json::Value::String(text),
                        })
                    };
                    ActionType::Tool {
                        tool_name,
                        arguments,
                        result,
                    }
                }
            }
        }

        fn extract_file_changes(tc: &PartialToolCallData) -> Vec<FileChange> {
            let mut changes = Vec::new();
            for c in &tc.content {
                if let v1::ToolCallContent::Diff(diff) = c {
                    let path = diff.path.to_string_lossy().to_string();
                    let rel = if !path.is_empty() {
                        path
                    } else {
                        tc.path
                            .clone()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    };
                    let old_text = diff.old_text.as_deref().unwrap_or("");
                    if old_text.is_empty() {
                        changes.push(FileChange::Write {
                            content: diff.new_text.clone(),
                        });
                    } else {
                        let unified = workspace_utils::diff::create_unified_diff(
                            &rel,
                            old_text,
                            &diff.new_text,
                        );
                        changes.push(FileChange::Edit {
                            unified_diff: unified,
                            has_line_numbers: false,
                        });
                    }
                }
            }
            if changes.is_empty()
                && let Some(raw) = &tc.raw_input
                && let Ok(edit_input) = serde_json::from_value::<EditInput>(raw.clone())
            {
                if let Some(diff) = edit_input.diff {
                    changes.push(FileChange::Edit {
                        unified_diff: workspace_utils::diff::normalize_unified_diff(
                            &edit_input.file_path,
                            &diff,
                        ),
                        has_line_numbers: true,
                    });
                } else if let Some(old) = edit_input.old_string
                    && let Some(new) = edit_input.new_string
                {
                    changes.push(FileChange::Edit {
                        unified_diff: workspace_utils::diff::create_unified_diff(
                            &edit_input.file_path,
                            &old,
                            &new,
                        ),
                        has_line_numbers: false,
                    });
                }
            }
            changes
        }

        fn get_tool_content(tc: &PartialToolCallData) -> String {
            match tc.kind {
                v1::ToolKind::Execute => AcpEventParser::parse_execute_command(tc),
                v1::ToolKind::Think => "Saving memory".to_string(),
                v1::ToolKind::Other => {
                    let tool_name = extract_tool_name_from_id(tc.id.0.as_ref())
                        .unwrap_or_else(|| "tool".to_string());
                    if tc.title.is_empty() {
                        tool_name
                    } else {
                        format!("{}: {}", tool_name, tc.title)
                    }
                }
                v1::ToolKind::Read => {
                    if tc.id.0.starts_with("read_many_files") {
                        "Read files".to_string()
                    } else {
                        tc.path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| tc.title.clone())
                    }
                }
                _ => tc.title.clone(),
            }
        }

        fn extract_tool_name_from_id(id: &str) -> Option<String> {
            if let Some(idx) = id.rfind('-') {
                let (head, tail) = id.split_at(idx);
                if tail
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit())
                {
                    return Some(head.to_string());
                }
            }
            None
        }

        fn extract_url_from_text(text: &str) -> Option<String> {
            // Simple URL extractor
            static URL_RE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r#"https?://[^\s"')]+"#).expect("valid regex"));
            URL_RE.find(text).map(|m| m.as_str().to_string())
        }

        fn collect_text_content(content: &[v1::ToolCallContent]) -> Option<String> {
            let mut out = String::new();
            for c in content {
                if let v1::ToolCallContent::Content(inner) = c
                    && let v1::ContentBlock::Text(t) = &inner.content
                {
                    out.push_str(&t.text);
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }

        fn convert_tool_status(status: v1::ToolCallStatus) -> LogToolStatus {
            match status {
                v1::ToolCallStatus::Pending | v1::ToolCallStatus::InProgress => {
                    LogToolStatus::Created
                }
                v1::ToolCallStatus::Completed => LogToolStatus::Success,
                v1::ToolCallStatus::Failed => LogToolStatus::Failed,
                _ => {
                    tracing::debug!("Unknown tool call status: {:?}", status);
                    LogToolStatus::Created
                }
            }
        }
    });

    vec![h1, h2]
}

fn normalize_acp_stderr_logs(
    msg_store: Arc<MsgStore>,
    entry_index_provider: EntryIndexProvider,
    suppressed_patterns: Vec<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stderr = msg_store.stderr_chunked_stream();

        let mut processor = PlainTextLogProcessor::builder()
            .normalized_entry_producer(Box::new(|content: String| NormalizedEntry {
                timestamp: None,
                entry_type: NormalizedEntryType::ErrorMessage {
                    error_type: NormalizedEntryError::Other,
                },
                content: strip_ansi_escapes::strip_str(&content),
                metadata: None,
            }))
            .time_gap(Duration::from_secs(2))
            .index_provider(entry_index_provider)
            .transform_lines(Box::new(move |lines: &mut Vec<String>| {
                lines.retain(|line| {
                    !suppressed_patterns
                        .iter()
                        .any(|pattern| line.contains(pattern))
                });
            }))
            .build();

        while let Some(Ok(chunk)) = stderr.next().await {
            for patch in processor.process(chunk) {
                msg_store.push_patch(patch);
            }
        }
    })
}

struct PartialToolCallData {
    index: usize,
    id: v1::ToolCallId,
    kind: v1::ToolKind,
    title: String,
    status: v1::ToolCallStatus,
    path: Option<PathBuf>,
    content: Vec<v1::ToolCallContent>,
    raw_input: Option<serde_json::Value>,
    raw_output: Option<serde_json::Value>,
}

impl PartialToolCallData {
    fn new(id: v1::ToolCallId, index: usize) -> Self {
        Self {
            id,
            index,
            kind: Default::default(),
            title: String::new(),
            status: Default::default(),
            path: None,
            content: Vec::new(),
            raw_input: None,
            raw_output: None,
        }
    }

    fn extend(&mut self, fields: v1::ToolCallUpdateFields, worktree_path: &Path) {
        // Keep the previously shown values when an update omits fields or sends defaults.
        if let Some(kind) = fields.kind.filter(|kind| *kind != v1::ToolKind::default()) {
            self.kind = kind;
        }
        if let Some(title) = fields.title.filter(|title| !title.is_empty()) {
            self.title = title;
        }
        if let Some(status) = fields
            .status
            .filter(|status| *status != v1::ToolCallStatus::default())
        {
            self.status = status;
        }
        if let Some(location) = fields
            .locations
            .as_deref()
            .and_then(|locations| locations.first())
        {
            self.path = Some(PathBuf::from(workspace_utils::path::make_path_relative(
                &location.path.to_string_lossy(),
                &worktree_path.to_string_lossy(),
            )));
        }
        if let Some(content) = fields.content.filter(|content| !content.is_empty()) {
            self.content = content;
        }
        if fields.raw_input.is_some() {
            self.raw_input = fields.raw_input;
        }
        if fields.raw_output.is_some() {
            self.raw_output = fields.raw_output;
        }
    }
}

struct AcpEventParser;

impl AcpEventParser {
    /// Parse a line that may contain an ACP event
    fn parse_line(line: &str) -> Option<AcpEvent> {
        let trimmed = line.trim();

        if let Ok(acp_event) = serde_json::from_str::<AcpEvent>(trimmed) {
            return Some(acp_event);
        }

        tracing::debug!("Failed to parse ACP raw log {trimmed}");

        None
    }

    /// Parse command from tool title (for execute tools)
    fn parse_execute_command(tc: &PartialToolCallData) -> String {
        if let Some(command) = tc.raw_input.as_ref().and_then(|value| {
            value
                .as_object()
                .and_then(|o| o.get("command").and_then(|v| v.as_str()))
        }) {
            return command.to_string();
        }
        let title = &tc.title;
        if let Some(command) = title.split(" [current working directory ").next() {
            command.trim().to_string()
        } else if let Some(command) = title.split(" (").next() {
            command.trim().to_string()
        } else {
            title.trim().to_string()
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SearchArgs {
    query: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FetchArgs {
    url: String,
}

#[derive(Debug, Clone, Default)]
struct StreamingState {
    assistant_text: Option<StreamingText>,
    thinking_text: Option<StreamingText>,
}

#[derive(Debug, Clone)]
struct StreamingText {
    index: usize,
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditInput {
    file_path: String,
    #[serde(default)]
    diff: Option<String>,
    #[serde(default)]
    old_string: Option<String>,
    #[serde(default)]
    new_string: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use workspace_utils::log_msg::LogMsg;

    use super::*;

    async fn patches_for(events: impl IntoIterator<Item = AcpEvent>) -> Vec<Value> {
        let msg_store = Arc::new(MsgStore::new());
        for event in events {
            msg_store.push_stdout(format!("{event}\n"));
        }
        msg_store.push_finished();
        for handle in normalize_logs(msg_store.clone(), Path::new("/tmp")) {
            handle.await.unwrap();
        }
        msg_store
            .get_history()
            .into_iter()
            .filter_map(|m| match m {
                LogMsg::JsonPatch(patch) => Some(serde_json::to_value(patch).unwrap()),
                _ => None,
            })
            .flat_map(|patch| patch.as_array().cloned().unwrap_or_default())
            .collect()
    }

    async fn errors_for(event: AcpEvent) -> Vec<Value> {
        patches_for([event])
            .await
            .into_iter()
            .map(|mut op| op["value"]["content"].take())
            .filter(|entry| entry["entry_type"]["type"] == "error_message")
            .collect()
    }

    #[tokio::test]
    async fn v1_tool_updates_reuse_the_entry_and_preserve_tool_details() {
        let id = "execute-1";
        let call = v1::ToolCall::new(id, "Print a greeting")
            .kind(v1::ToolKind::Execute)
            .raw_input(json!({"command": "echo greeting"}));
        let permission = v1::RequestPermissionRequest::new(
            v1::SessionId::new("s1"),
            v1::ToolCallUpdate::new(
                id,
                v1::ToolCallUpdateFields::new().title("Print a greeting"),
            ),
            vec![v1::PermissionOption::new(
                "once",
                "Allow once",
                v1::PermissionOptionKind::AllowOnce,
            )],
        );
        let update = v1::ToolCallUpdate::new(
            id,
            v1::ToolCallUpdateFields::new()
                .status(v1::ToolCallStatus::Completed)
                .content(vec![v1::ToolCallContent::Content(v1::Content::new(
                    v1::ContentBlock::Text(v1::TextContent::new("greeting")),
                ))]),
        );
        let patches = patches_for([
            AcpEvent::ToolCall(call),
            AcpEvent::RequestPermission(permission),
            AcpEvent::ApprovalRequested {
                tool_call_id: id.into(),
                approval_id: "approval-1".into(),
            },
            AcpEvent::ToolUpdate(update),
        ])
        .await;
        let [created, permission, pending, completed] = patches.as_slice() else {
            panic!("expected four tool patches, got {patches:?}");
        };
        assert_eq!(created["op"], "add");
        assert_eq!(
            created["value"]["content"]["entry_type"]["status"]["status"],
            "created"
        );
        for patch in [permission, pending, completed] {
            assert_eq!(patch["op"], "replace");
            assert_eq!(patch["path"], created["path"]);
        }
        assert_eq!(
            pending["value"]["content"]["entry_type"]["status"],
            json!({"status": "pending_approval", "approval_id": "approval-1"})
        );
        let entry = &completed["value"]["content"];
        let tool = &entry["entry_type"];
        assert_eq!(entry["metadata"]["tool_call_id"], id);
        assert_eq!(entry["content"], "echo greeting");
        assert_eq!(tool["tool_name"], "Print a greeting");
        assert_eq!(tool["status"]["status"], "success");
        assert_eq!(tool["action_type"]["action"], "command_run");
        assert_eq!(tool["action_type"]["result"]["output"], "greeting\n");
    }

    #[tokio::test]
    async fn context_and_billing_share_one_latest_usage_entry() {
        let context = |used, cost: Option<v1::Cost>| {
            AcpEvent::try_from(v1::SessionNotification::new(
                v1::SessionId::new("s1"),
                v1::SessionUpdate::UsageUpdate(v1::UsageUpdate::new(used, 200_000).cost(cost)),
            ))
            .unwrap()
        };
        let prompt = |input_tokens| {
            AcpEvent::Usage(AcpUsageUpdate::Prompt(crate::logs::ExecutionTokenUsage {
                input_tokens,
                cumulative: true,
                ..Default::default()
            }))
        };
        let patches = patches_for([
            context(1000, None),
            context(1500, Some(v1::Cost::new(0.125, "USD"))),
            prompt(400),
            context(800, Some(v1::Cost::new(5.0, "EUR"))),
            prompt(450),
        ])
        .await;
        assert_eq!(patches.len(), 5);
        assert_eq!(patches[0]["op"], "add");
        for patch in &patches[1..] {
            assert_eq!(patch["op"], "replace");
            assert_eq!(patch["path"], patches[0]["path"]);
        }
        let context_only = &patches[0]["value"]["content"]["entry_type"];
        assert_eq!(context_only["total_tokens"], 1000);
        assert!(context_only.get("billing").is_none());
        let cost_only = &patches[1]["value"]["content"]["entry_type"]["billing"];
        assert_eq!(cost_only["input_tokens"], 0);
        assert_eq!(cost_only["cost_usd"], 0.125);
        let latest = &patches[4]["value"]["content"]["entry_type"];
        assert_eq!(latest["total_tokens"], 800);
        assert_eq!(latest["model_context_window"], 200_000);
        assert_eq!(latest["billing"]["input_tokens"], 450);
        assert_eq!(latest["billing"]["cost_usd"], 0.125);
        assert_eq!(latest["billing"]["cumulative"], true);
    }

    #[tokio::test]
    async fn auth_required_is_a_setup_required_error() {
        let errors = errors_for(AcpEvent::AuthRequired("Not signed in".into())).await;
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0]["entry_type"]["error_type"]["type"],
            "setup_required"
        );
        assert_eq!(errors[0]["content"], "Not signed in");
    }

    #[tokio::test]
    async fn plain_error_stays_other() {
        let errors = errors_for(AcpEvent::Error("boom".into())).await;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["entry_type"]["error_type"]["type"], "other");
    }
}
