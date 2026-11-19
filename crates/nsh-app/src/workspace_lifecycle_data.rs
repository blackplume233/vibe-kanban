use std::collections::{BTreeMap, BTreeSet};

use futures_util::{StreamExt, stream::FuturesUnordered};
use nsh_core::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::api::Api;

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct Workspace {
    pub id: String,
    pub branch: String,
    pub name: Option<String>,
    pub archived: bool,
    pub local_issue_id: Option<String>,
}

impl Workspace {
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.branch)
    }
}

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct Repo {
    pub id: String,
    pub display_name: String,
    pub path: String,
    pub default_target_branch: Option<String>,
    pub dev_server_script: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct Branch {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
}

#[derive(Clone, Default, Serialize, PartialEq)]
pub(super) struct RepoChoice {
    pub repo_id: String,
    pub target_branch: String,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
pub(super) struct ProfileChoice {
    pub executor: String,
    pub variant: Option<String>,
}

type Profile = BTreeMap<String, serde::de::IgnoredAny>;

#[derive(Deserialize)]
struct Config {
    executor_profile: ProfileChoice,
}

#[derive(Deserialize)]
pub(super) struct SystemInfo {
    config: Config,
    executors: BTreeMap<String, Profile>,
}

impl SystemInfo {
    pub fn profiles(self) -> (Vec<ProfileChoice>, Option<usize>) {
        let choices: Vec<_> = self
            .executors
            .into_iter()
            .flat_map(|(executor, profile)| {
                profile.into_keys().map(move |variant| ProfileChoice {
                    executor: executor.clone(),
                    variant: Some(variant),
                })
            })
            .collect();
        let default = self.config.executor_profile;
        let index = choices.iter().position(|choice| {
            choice.executor == default.executor
                && choice.variant.as_deref()
                    == Some(default.variant.as_deref().unwrap_or("DEFAULT"))
        });
        (choices, index)
    }
}

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct GitStatus {
    pub repo_id: String,
    pub repo_name: String,
    pub target_branch_name: String,
    pub commits_ahead: Option<usize>,
    pub commits_behind: Option<usize>,
    pub uncommitted_count: Option<usize>,
    pub untracked_count: Option<usize>,
    pub head_oid: Option<String>,
    pub is_rebase_in_progress: bool,
    pub conflict_op: Option<String>,
    pub conflicted_files: Vec<String>,
    pub is_target_remote: bool,
    pub merges: Vec<Merge>,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Merge {
    Direct { merge_commit: String },
    Pr { pr_info: PullRequest },
}

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct PullRequest {
    pub number: u64,
    pub status: String,
    pub url: String,
}

impl GitStatus {
    pub fn conflicted(&self) -> bool {
        self.is_rebase_in_progress || !self.conflicted_files.is_empty()
    }

    pub fn has_open_pr(&self) -> bool {
        self.merges
            .iter()
            .any(|item| matches!(item, Merge::Pr { pr_info } if pr_info.status == "open"))
    }

    pub fn merge_block(&self) -> Option<&'static str> {
        if self.is_target_remote {
            Some("目标是远端分支。可创建 PR。")
        } else if self.has_open_pr() {
            Some("已有打开的 PR，关闭或合并 PR 后才能直接合并。")
        } else if self.conflicted() {
            Some("请先处理当前 Git 冲突。")
        } else if self.commits_behind.is_some_and(|count| count > 0) {
            Some("当前分支落后，请先 Rebase。")
        } else if self.commits_ahead == Some(0) {
            Some("没有待合并的提交。")
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Started {
    pub workspace: Workspace,
}

#[derive(Deserialize)]
pub(super) struct EditorResponse {
    pub url: Option<String>,
}

#[derive(Clone, Deserialize)]
pub(super) struct Process {
    pub id: String,
    run_reason: String,
    status: String,
}

impl Process {
    pub fn running_dev(&self) -> bool {
        self.run_reason == "devserver" && self.status == "running"
    }
}

#[derive(Deserialize)]
struct Session {
    id: String,
}

#[derive(Deserialize)]
struct Frame {
    #[serde(rename = "JsonPatch")]
    patch: Option<json_patch::Patch>,
    #[serde(rename = "Ready", default)]
    ready: bool,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Processes {
    pub running_dev: BTreeSet<String>,
    pub agent_running: bool,
}

pub(super) async fn watch_processes(
    api: Api,
    workspace: String,
    status: Signal<Option<Processes>>,
) -> Result<(), String> {
    let sessions: Vec<Session> =
        get(&api, &format!("/api/sessions?workspace_id={workspace}")).await?;
    if sessions.is_empty() {
        status.set(Some(Processes::default()));
        return Ok(());
    }
    let mut streams = FuturesUnordered::new();
    let mut snapshots = Vec::with_capacity(sessions.len());
    for (index, session) in sessions.into_iter().enumerate() {
        let (frames, connection) = api.stream(&format!(
            "/api/execution-processes/stream/session/ws?session_id={}",
            session.id,
        ))?;
        snapshots.push((json!({"execution_processes": {}}), false));
        streams.push(
            frames
                .map(move |frame| {
                    let _keep_open = &connection;
                    (index, frame)
                })
                .into_future(),
        );
    }
    while let Some((frame, stream)) = streams.next().await {
        let (session, frame) = frame.ok_or("执行进程流已断开，请刷新。")?;
        let frame: Frame = serde_json::from_str(&frame?).map_err(|error| error.to_string())?;
        let (snapshot, ready) = &mut snapshots[session];
        if let Some(patch) = frame.patch {
            json_patch::patch(snapshot, &patch).map_err(|error| error.to_string())?;
        }
        *ready |= frame.ready;
        if snapshots.iter().all(|(_, ready)| *ready) {
            let mut processes = Processes::default();
            for (snapshot, _) in &snapshots {
                if let Some(entries) = snapshot["execution_processes"].as_object() {
                    for (id, process) in entries {
                        if process["status"] == "running" {
                            match process["run_reason"].as_str() {
                                Some("devserver") => {
                                    processes.running_dev.insert(id.clone());
                                }
                                Some("codingagent" | "setupscript" | "cleanupscript") => {
                                    processes.agent_running = true
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            status.set(Some(processes));
        }
        streams.push(stream.into_future());
    }
    Ok(())
}

pub(super) async fn get<T: DeserializeOwned>(api: &Api, path: &str) -> Result<T, String> {
    api.request("GET", path, None).await
}

pub(super) async fn send<T: DeserializeOwned>(
    api: &Api,
    method: &str,
    path: &str,
    body: Value,
) -> Result<T, String> {
    api.request(method, path, Some(json_bytes(&body))).await
}

pub(super) fn json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("a JSON Value is serializable")
}

pub(super) fn action_error(error: crate::api::ApiFailure<Value>) -> String {
    let Some(data) = error.data else {
        return error.message;
    };
    match data["type"].as_str() {
        Some("merge_conflicts") => data["message"]
            .as_str()
            .unwrap_or("存在文件冲突，请先解决。")
            .into(),
        Some("rebase_in_progress") => "正在 Rebase。请解决冲突后继续，或中止 Rebase。".into(),
        Some("cli_not_installed") => format!(
            "未安装 PR 服务 CLI：{}",
            data["provider"].as_str().unwrap_or("未知")
        ),
        Some("cli_not_logged_in") => format!(
            "请先登录 PR 服务：{}",
            data["provider"].as_str().unwrap_or("未知")
        ),
        Some("git_cli_not_installed") => "后端未找到 Git CLI。".into(),
        Some("git_cli_not_logged_in") => "Git 远端认证失败。请检查本机 Git 凭据。".into(),
        Some("target_branch_not_found") => format!(
            "远端没有目标分支 {}。",
            data["branch"].as_str().unwrap_or_default()
        ),
        Some("unsupported_provider") => "远端不支持通过此服务创建 PR。".into(),
        _ => error.message,
    }
}
