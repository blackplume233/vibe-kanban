pub mod discovery;
pub mod harness_v2;
pub(super) mod mcp;
pub mod normalize_logs;
mod permissions;
pub mod registry;
pub mod session;
pub mod session_config;
pub(super) mod trusted_node;

use std::{fmt::Display, str::FromStr};

use acp::schema::v1;
pub use harness_v2::AcpAgentHarnessV2;
pub use normalize_logs::*;
use serde::{Deserialize, Serialize};
pub use session::SessionManager;
use workspace_utils::approvals::ApprovalStatus;

use crate::logs::ExecutionTokenUsage;

/// Parsed event types for internal processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcpEvent {
    User(String),
    SessionStart(String),
    Message(v1::ContentBlock),
    Thought(v1::ContentBlock),
    ToolCall(v1::ToolCall),
    ToolUpdate(v1::ToolCallUpdate),
    Plan(v1::Plan),
    AvailableCommands(Vec<v1::AvailableCommand>),
    CurrentMode(v1::SessionModeId),
    /// Config in effect, a skipped setting, or a change the agent made itself
    /// ("Model: X · Mode: Agent · Permissions: Auto"). Shown as a system message.
    SessionConfig(String),
    Usage(AcpUsageUpdate),
    RequestPermission(v1::RequestPermissionRequest),
    ApprovalRequested {
        tool_call_id: String,
        approval_id: String,
    },
    ApprovalResponse(ApprovalResponse),
    /// The agent rejected a request with -32000 (not signed in, or the token expired).
    /// Shown as a setup-required error, which offers the Sign in button.
    AuthRequired(String),
    Error(String),
    Done(String),
    Other(v1::SessionNotification),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcpUsageUpdate {
    Context {
        used: u64,
        size: u64,
        cost_usd: Option<f64>,
    },
    Prompt(ExecutionTokenUsage),
}

impl Display for AcpEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap_or_default())
    }
}

impl FromStr for AcpEvent {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub tool_call_id: String,
    pub status: ApprovalStatus,
}

impl TryFrom<v1::SessionNotification> for AcpEvent {
    type Error = ();

    fn try_from(notification: v1::SessionNotification) -> Result<Self, ()> {
        use v1::SessionUpdate;

        Ok(match notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => Self::Message(chunk.content),
            SessionUpdate::AgentThoughtChunk(chunk) => Self::Thought(chunk.content),
            SessionUpdate::ToolCall(call) => Self::ToolCall(call),
            SessionUpdate::ToolCallUpdate(update) => Self::ToolUpdate(update),
            SessionUpdate::Plan(plan) => Self::Plan(plan),
            SessionUpdate::AvailableCommandsUpdate(update) => {
                Self::AvailableCommands(update.available_commands)
            }
            SessionUpdate::CurrentModeUpdate(update) => Self::CurrentMode(update.current_mode_id),
            SessionUpdate::UsageUpdate(usage) => Self::Usage(AcpUsageUpdate::Context {
                used: usage.used,
                size: usage.size,
                cost_usd: usage
                    .cost
                    .filter(|cost| cost.currency == "USD")
                    .map(|cost| cost.amount),
            }),
            _ => {
                tracing::debug!("Unknown ACP Notification Update type {notification:?}");
                return Err(());
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn notif(update: serde_json::Value) -> v1::SessionNotification {
        serde_json::from_value(json!({ "sessionId": "s1", "update": update })).unwrap()
    }

    #[test]
    fn saved_event_lines_keep_their_external_tags_and_payloads() {
        let saved = r#"{"Message":{"type":"text","text":"Checking files"}}
{"ToolCall":{"toolCallId":"read-1","title":"Read README","kind":"read","locations":[{"path":"/repo/README.md","line":3}]}}
{"ToolUpdate":{"toolCallId":"read-1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Hello"}}]}}
{"RequestPermission":{"sessionId":"s1","toolCall":{"toolCallId":"read-1","title":"Read README"},"options":[{"optionId":"once","name":"Allow once","kind":"allow_once"}]}}
{"CurrentMode":"plan"}
{"Usage":{"Context":{"used":12000,"size":200000,"cost_usd":0.125}}}
{"Other":{"sessionId":"s1","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"Check the README"}}}}"#;

        for line in saved.lines() {
            let event = line
                .parse::<AcpEvent>()
                .unwrap_or_else(|err| panic!("{line}: {err}"));
            assert_eq!(
                serde_json::to_value(event).unwrap(),
                serde_json::from_str::<serde_json::Value>(line).unwrap(),
                "saved event: {line}"
            );
        }
    }

    #[test]
    fn loads_legacy_null_options() {
        let line =
            r#"{"AvailableCommands":[{"name":"context","description":"Show usage","input":null}]}"#;
        let AcpEvent::AvailableCommands(commands) = line.parse::<AcpEvent>().unwrap() else {
            panic!("expected available commands");
        };
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "context");
        assert!(commands[0].input.is_none());
    }

    #[test]
    fn thought_chunk_maps_to_thought() {
        let n = notif(json!({
            "sessionUpdate": "agent_thought_chunk",
            "content": {"type":"text","text":"I should glance at the project struct"}
        }));
        assert!(matches!(AcpEvent::try_from(n), Ok(AcpEvent::Thought(_))));
    }

    #[test]
    fn agent_message_chunk_maps_to_message() {
        let n = notif(json!({
            "sessionUpdate":"agent_message_chunk",
            "content":{"type":"text","text":"The README is the st"}
        }));
        assert!(matches!(AcpEvent::try_from(n), Ok(AcpEvent::Message(_))));
    }

    #[test]
    fn tool_call_maps_to_tool_call() {
        let n = notif(json!({
            "sessionUpdate":"tool_call",
            "toolCallId":"toolu_011adCUaiEksbXc17jHQu8wu",
            "title":"Finding files matching README*",
            "kind":"read",
            "rawInput":{"pattern":"README*"}
        }));
        assert!(matches!(AcpEvent::try_from(n), Ok(AcpEvent::ToolCall(_))));
    }

    #[test]
    fn tool_call_update_maps_to_tool_update() {
        let n = notif(json!({
            "sessionUpdate":"tool_call_update",
            "toolCallId":"toolu_011adCUaiEksbXc17jHQu8wu",
            "status":"completed",
            "content":[{"type":"content","content":{"type":"text","text":"./README.md"}}],
            "rawOutput":{"content":"./README.md","detailedContent":"./README.md"}
        }));
        assert!(matches!(AcpEvent::try_from(n), Ok(AcpEvent::ToolUpdate(_))));
    }

    #[test]
    fn available_commands_update_maps_to_available_commands() {
        let n = notif(json!({"sessionUpdate":"available_commands_update",
        "availableCommands":[
            {"name":"add-dir","description":"Allow file access to a directory and load its .github skills and agents as trusted configuration","input":{"hint":"directory"}},
            {"name":"allow-all","description":"Enable all permissions","input":{"hint":"[on|off|show]"}},
            {"name":"permissions","description":"Switch between permission modes","input":{"hint":"[default|allow-all|show]"}},
            {"name":"autopilot","description":"Toggle autopilot mode, or set an autopilot objective with an optional AI-credit limit (--max-ai-credits)","input":{"hint":"[on|off|<objective>] [--max-ai-credits <N>]"}},
            {"name":"compact","description":"Summarize conversation history to reduce context window usage. Optionally provide focus instructions.","input":{"hint":"focus instructions"}},
            {"name":"blame","description":"Trace code history with git blame and linked Copilot sessions","input":{"hint":"text | pull request | commit"}},
            {"name":"chronicle","description":"Session history tools and insights","input":{"hint":"standup|search|tips|cost-tips|improve|reindex"}},
            {"name":"context","description":"Show context window token usage and visualization"},
        ]}));
        assert!(matches!(
            AcpEvent::try_from(n),
            Ok(AcpEvent::AvailableCommands(_))
        ));
    }

    #[test]
    fn agent_message_maps_to_text() {
        let n = notif(json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "Hello"}
        }));
        let Ok(AcpEvent::Message(v1::ContentBlock::Text(text))) = AcpEvent::try_from(n) else {
            panic!("expected text Message");
        };
        assert_eq!(text.text, "Hello");
    }

    #[test]
    fn tool_call_with_diff_maps_to_tool_call() {
        let n = notif(json!({
            "sessionUpdate": "tool_call",
            "toolCallId": "call_2",
            "title": "Edit README.md",
            "kind": "edit",
            "status": "pending",
            "content": [{
                "type": "diff",
                "path": "/repo/README.md",
                "oldText": "old line",
                "newText": "new line"
            }],
            "locations": [{"path": "/repo/README.md", "line": 3}]
        }));
        assert!(matches!(AcpEvent::try_from(n), Ok(AcpEvent::ToolCall(_))));
    }

    #[test]
    fn plan_maps_to_plan() {
        let n = notif(json!({
            "sessionUpdate": "plan",
            "entries": [
                {"content": "Read README", "priority": "high", "status": "completed"},
                {"content": "Summarize it", "priority": "medium", "status": "in_progress"}
            ]
        }));
        let Ok(AcpEvent::Plan(plan)) = AcpEvent::try_from(n) else {
            panic!("expected Plan");
        };
        assert_eq!(plan.entries.len(), 2);
    }

    #[test]
    fn current_mode_update_maps_to_current_mode() {
        let n = notif(json!({
            "sessionUpdate": "current_mode_update",
            "currentModeId": "plan"
        }));
        let Ok(AcpEvent::CurrentMode(mode)) = AcpEvent::try_from(n) else {
            panic!("expected CurrentMode");
        };
        assert_eq!(&*mode.0, "plan");
    }

    #[test]
    fn user_message_chunk_is_ignored() {
        // Agents replay the user's own messages on session/load; they must not
        // show up as assistant output.
        let n = notif(json!({
            "sessionUpdate": "user_message_chunk",
            "content": {"type": "text", "text": "Summarize the README"}
        }));
        assert!(AcpEvent::try_from(n).is_err());
    }

    #[test]
    fn usage_update_maps_context_and_usd_session_cost() {
        for (currency, expected_cost_usd) in [("USD", Some(0.125)), ("EUR", None)] {
            let n = notif(json!({
                "sessionUpdate": "usage_update",
                "used": 12000,
                "size": 200000,
                "cost": {"amount": 0.125, "currency": currency}
            }));
            let Ok(AcpEvent::Usage(AcpUsageUpdate::Context {
                used,
                size,
                cost_usd,
            })) = AcpEvent::try_from(n)
            else {
                panic!("expected a context usage event");
            };
            assert_eq!((used, size, cost_usd), (12000, 200000, expected_cost_usd));
        }
    }
}
