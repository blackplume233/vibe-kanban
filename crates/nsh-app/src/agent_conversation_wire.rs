use std::collections::{BTreeMap, BTreeSet};

use futures_util::StreamExt;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::api::{Api, PatchFrame};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct Session {
    pub id: String,
    pub name: Option<String>,
    pub executor: Option<String>,
}

impl Session {
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            let short_id = self.id.split('-').next().unwrap_or(&self.id);
            format!("会话 {short_id}")
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Profiles {
    pub executors: BTreeMap<String, serde::de::IgnoredAny>,
    pub config: ProfileConfig,
}

#[derive(Deserialize)]
pub(super) struct ProfileConfig {
    pub executor_profile: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct Process {
    pub id: String,
    pub status: ProcessStatus,
    pub run_reason: String,
    pub dropped: bool,
    pub created_at: String,
    pub executor_action: Value,
}

impl Process {
    pub fn config(&self) -> Option<&Value> {
        self.executor_action.pointer("/typ/executor_config")
    }

    pub fn prompt(&self) -> Option<&str> {
        self.executor_action.pointer("/typ/prompt")?.as_str()
    }

    pub fn label(&self) -> &str {
        match self.run_reason.as_str() {
            "codingagent" => "Agent",
            "setupscript" => "初始化脚本",
            "cleanupscript" => "清理脚本",
            "archivescript" => "归档脚本",
            "devserver" => "开发服务",
            other => other,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ProcessStatus {
    Running,
    Completed,
    Failed,
    Killed,
}

impl ProcessStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "正在执行",
            Self::Completed => "已完成",
            Self::Failed => "执行失败",
            Self::Killed => "已停止",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct Approval {
    pub approval_id: String,
    pub tool_name: String,
    pub execution_process_id: String,
    pub is_question: bool,
    pub timeout_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct Scheduled {
    pub execution_process_id: String,
    pub session_id: String,
    pub priority: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum QueueStatus {
    #[default]
    Empty,
    Queued {
        message: QueuedMessage,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct QueuedMessage {
    pub data: QueuedDraft,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct QueuedDraft {
    pub message: String,
    pub executor_config: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LogEntry {
    pub label: String,
    pub content: String,
    pub is_user: bool,
}

impl LogEntry {
    fn from_wire(value: Value) -> Option<Self> {
        let kind = value["type"].as_str()?;
        let (label, content, is_user) = match kind {
            "STDOUT" => ("输出", value["content"].as_str()?, false),
            "STDERR" => ("标准错误", value["content"].as_str()?, false),
            "NORMALIZED_ENTRY" => {
                let entry = &value["content"];
                let meta = &entry["entry_type"];
                let kind = meta["type"].as_str()?;
                let label = match kind {
                    "user_message" => "你",
                    "assistant_message" => "Agent",
                    "tool_use" => meta["tool_name"].as_str().unwrap_or("工具"),
                    "thinking" => "推理",
                    "system_message" => "系统",
                    "error_message" => "错误",
                    "user_feedback" => "审批结果",
                    "user_answered_questions" => "已回答",
                    "token_usage_info" | "next_action" | "loading" => return None,
                    _ => kind,
                };
                (label, entry["content"].as_str()?, kind == "user_message")
            }
            _ => return None,
        };
        let content = content.trim_end_matches(['\r', '\n']);
        (!content.is_empty()).then(|| Self {
            label: label.to_owned(),
            content: content.to_owned(),
            is_user,
        })
    }
}

pub(super) type Logs = BTreeMap<usize, LogEntry>;

pub(super) fn apply_logs(entries: &mut Logs, patch: json_patch::Patch) {
    use json_patch::PatchOperation::{Add, Remove, Replace};

    for operation in patch.0 {
        let (path, entry) = match operation {
            Add(op) => (op.path, LogEntry::from_wire(op.value)),
            Replace(op) => (op.path, LogEntry::from_wire(op.value)),
            Remove(op) => (op.path, None),
            _ => continue,
        };
        let Some(index) = path
            .as_str()
            .strip_prefix("/entries/")
            .and_then(|index| index.parse::<usize>().ok())
        else {
            continue;
        };
        if let Some(entry) = entry {
            entries.insert(index, entry);
        } else {
            entries.remove(&index);
        }
    }
}

// The approval stream uses Replace for both inserts and edits to /pending/{id}.
// An approval can resolve while the subscription is catching up with its snapshot.
pub(super) fn apply_approvals(
    document: &mut Value,
    patch: json_patch::Patch,
) -> Result<(), String> {
    use json_patch::{AddOperation, PatchOperation};

    for operation in patch.0 {
        let operation = match operation {
            PatchOperation::Replace(op)
                if op.path.as_str().starts_with("/pending/")
                    && document.pointer(op.path.as_str()).is_none() =>
            {
                PatchOperation::Add(AddOperation {
                    path: op.path,
                    value: op.value,
                })
            }
            PatchOperation::Remove(op)
                if op.path.as_str().starts_with("/pending/")
                    && document.pointer(op.path.as_str()).is_none() =>
            {
                continue;
            }
            operation => operation,
        };
        json_patch::patch(document, &[operation]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn deserialize_map<T: DeserializeOwned>(value: &Value) -> Result<Vec<T>, String> {
    value
        .as_object()
        .ok_or_else(|| "服务器返回了无效的记录集合".to_owned())?
        .values()
        .map(|entry| T::deserialize(entry).map_err(|error| error.to_string()))
        .collect()
}

pub(super) async fn frame(frames: &mut crate::socket::Frames) -> Result<PatchFrame, String> {
    let message = frames.next().await.ok_or("实时连接已关闭")??;
    serde_json::from_str(&message).map_err(|error| error.to_string())
}

pub(super) async fn get<T: DeserializeOwned>(api: &Api, path: &str) -> Result<T, String> {
    api.request("GET", path, None).await
}

pub(super) async fn post<T: DeserializeOwned>(
    api: &Api,
    path: &str,
    data: Value,
) -> Result<T, String> {
    let body = serde_json::to_vec(&data).map_err(|error| error.to_string())?;
    api.request("POST", path, Some(body)).await
}

pub(super) fn initial_processes() -> Value {
    json!({"execution_processes": {}})
}

pub(super) fn configured_agents(profiles: &Profiles) -> BTreeSet<String> {
    profiles.executors.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_replace_adds_pending_item_and_remove_resolves_it() {
        let mut state = json!({"pending": {}});
        let patch = serde_json::from_value(json!([
            {"op": "replace", "path": "/pending/a", "value": {"approval_id": "a"}},
            {"op": "remove", "path": "/pending/missing"}
        ]))
        .unwrap();
        apply_approvals(&mut state, patch).unwrap();
        assert_eq!(state["pending"]["a"]["approval_id"], "a");
        let patch = serde_json::from_value(json!([
            {"op": "remove", "path": "/pending/a"}
        ]))
        .unwrap();
        apply_approvals(&mut state, patch).unwrap();
        assert_eq!(state["pending"], json!({}));
    }

    #[test]
    fn streamed_message_updates_at_same_index() {
        let mut logs = Logs::new();
        let entry = |content| {
            json!({"type": "NORMALIZED_ENTRY", "content": {
                "entry_type": {"type": "assistant_message"}, "content": content
            }})
        };
        let patch = serde_json::from_value(json!([
            {"op": "add", "path": "/entries/9", "value": entry("one")},
            {"op": "replace", "path": "/entries/9", "value": entry("one two")}
        ]))
        .unwrap();
        apply_logs(&mut logs, patch);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[&9].content, "one two");
    }
}
