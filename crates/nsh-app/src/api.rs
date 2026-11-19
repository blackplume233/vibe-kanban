use std::{collections::HashSet, rc::Rc};

use futures_util::{FutureExt, StreamExt, stream::FuturesUnordered};
use nsh_core::{platform::HttpRequest, prelude::*};
use serde::{
    Deserialize, Deserializer,
    de::{DeserializeOwned, IgnoredAny},
};
use serde_json::json;

use crate::socket;

#[derive(Clone)]
pub struct Api {
    http: String,
    ws: String,
}

#[derive(Clone, Deserialize, PartialEq)]
pub(crate) struct Workspace {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) branch: String,
}

impl Workspace {
    pub(crate) fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.branch)
    }
}

#[derive(Deserialize)]
struct Session {
    id: String,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>, E: Deserialize<'de>"))]
struct Envelope<T, E> {
    success: bool,
    #[serde(default, deserialize_with = "present_data")]
    data: Option<T>,
    message: Option<String>,
    error_data: Option<E>,
}

fn present_data<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug)]
pub(crate) struct ApiFailure<E> {
    pub(crate) message: String,
    pub(crate) data: Option<E>,
}

fn decode_response<T: DeserializeOwned, E: DeserializeOwned>(
    path: &str,
    status: u16,
    body: &[u8],
) -> Result<T, ApiFailure<E>> {
    let ok = (200..300).contains(&status);
    match serde_json::from_slice::<Envelope<T, E>>(body) {
        Ok(envelope) if ok && envelope.success => envelope.data.ok_or_else(|| ApiFailure {
            message: format!("{path}: response has no data"),
            data: None,
        }),
        Ok(envelope) => Err(ApiFailure {
            message: envelope.message.unwrap_or_else(|| {
                if ok {
                    format!("{path}: request failed")
                } else {
                    format!("{path}: HTTP {status}")
                }
            }),
            data: envelope.error_data,
        }),
        Err(error) => Err(ApiFailure {
            message: serde_json::from_slice::<Envelope<IgnoredAny, IgnoredAny>>(body)
                .ok()
                .and_then(|envelope| envelope.message)
                .unwrap_or_else(|| {
                    if ok {
                        format!("{path}: {error}")
                    } else {
                        format!("{path}: HTTP {status}")
                    }
                }),
            data: None,
        }),
    }
}

#[derive(Deserialize)]
pub(crate) struct PatchFrame {
    #[serde(rename = "JsonPatch")]
    pub(crate) patch: Option<json_patch::Patch>,
    #[serde(default, rename = "Ready")]
    pub(crate) ready: bool,
    #[serde(default)]
    pub(crate) finished: bool,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Diff {
    old_path: Option<String>,
    new_path: Option<String>,
    pub(crate) old_content: Option<String>,
    pub(crate) new_content: Option<String>,
    pub(crate) content_omitted: bool,
}

impl Diff {
    pub(crate) fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }
}

type Files = Vec<(String, Rc<Diff>)>;

impl Api {
    #[cfg(windows)]
    pub fn local() -> Self {
        let port = std::env::var("BACKEND_PORT")
            .ok()
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or(59301);
        Self {
            http: format!("http://127.0.0.1:{port}"),
            ws: format!("ws://127.0.0.1:{port}"),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn local() -> Self {
        let location = web_sys::window().expect("browser window").location();
        let http = location.origin().expect("browser origin");
        // The smoke site proxies read-only HTTP paths. Its WS connects directly
        // to the backend so the server's real Origin check is exercised.
        let backend_port = location
            .search()
            .ok()
            .and_then(|query| query.strip_prefix("?backend_port=")?.parse::<u16>().ok());
        let ws = match backend_port {
            Some(port) => format!("ws://127.0.0.1:{port}"),
            None => http.replacen("http", "ws", 1),
        };
        Self { http, ws }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.request("GET", path, None).await
    }

    pub(crate) async fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<T, String> {
        self.request_with_error::<T, IgnoredAny>(method, path, body)
            .await
            .map_err(|error| error.message)
    }

    pub(crate) async fn request_with_error<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<T, ApiFailure<E>> {
        let mut request = HttpRequest::new(method, format!("{}{}", self.http, path));
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        let response = fetch(request).await.map_err(|message| ApiFailure {
            message,
            data: None,
        })?;
        let data = decode_response(path, response.status(), &response.body)?;
        log::info!("HTTP {} {path}", response.status());
        Ok(data)
    }

    pub(crate) fn stream(
        &self,
        path: &str,
    ) -> Result<(socket::Frames, socket::Connection), String> {
        socket::connect(self.websocket_url(path))
    }

    pub(crate) fn websocket_url(&self, path: &str) -> String {
        format!("{}{}", self.ws, path)
    }

    pub(crate) async fn workspaces(&self) -> Result<Vec<Workspace>, String> {
        self.get("/api/workspaces").await
    }

    pub(crate) async fn diffs(
        &self,
        workspace: &str,
        files: Signal<Files>,
        status: Signal<SharedString>,
    ) -> Result<(), String> {
        let (mut frames, _connection) =
            self.stream(&format!("/api/workspaces/{workspace}/git/diff/ws"))?;
        let mut document = json!({"entries": {}});
        while let Some(frame) = frames.next().await {
            let frame = parse_frame(&frame?)?;
            if let Some(patch) = frame.patch {
                apply_diff_patch(&mut document, patch)?;
                let mut next = Vec::new();
                if let Some(repos) = document["entries"].as_object() {
                    for (repo, contents) in repos {
                        if let Some(entries) = contents.as_object() {
                            for (path, entry) in entries {
                                let diff = Diff::deserialize(&entry["content"])
                                    .map_err(|e| e.to_string())?;
                                next.push((format!("{repo}/{path}"), Rc::new(diff)));
                            }
                        }
                    }
                }
                next.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                log::info!("diff {workspace}: {} changed files", next.len());
                files.set(next);
            }
            if frame.ready {
                status.set("Watching worktree".into());
            }
        }
        status.set("Diff stream closed".into());
        Ok(())
    }

    pub(crate) async fn logs(
        &self,
        workspace: &str,
        lines: Signal<Vec<SharedString>>,
        status: Signal<SharedString>,
    ) -> Result<(), String> {
        let sessions: Vec<Session> = self
            .get(&format!("/api/sessions?workspace_id={workspace}"))
            .await?;
        let Some(session) = sessions.first() else {
            status.set("No sessions in this workspace".into());
            return Ok(());
        };
        let (mut frames, _connection) = self.stream(&format!(
            "/api/execution-processes/stream/session/ws?session_id={}",
            session.id
        ))?;
        let mut document = json!({"execution_processes": {}});
        let mut seen = HashSet::new();
        let mut readers = FuturesUnordered::new();
        loop {
            let frame = if readers.is_empty() {
                frames.next().await
            } else {
                futures_util::select_biased! {
                    frame = frames.next().fuse() => frame,
                    result = readers.next().fuse() => {
                        if let Some(Err(error)) = result {
                            status.set(SharedString::from(error));
                        }
                        continue;
                    }
                }
            };
            let Some(frame) = frame else { break };
            let frame = parse_frame(&frame?)?;
            if let Some(patch) = frame.patch {
                json_patch::patch(&mut document, &patch).map_err(|e| e.to_string())?;
                if let Some(processes) = document["execution_processes"].as_object() {
                    for id in processes.keys() {
                        if seen.insert(id.clone()) {
                            readers.push(self.process_logs(id.clone(), lines));
                        }
                    }
                }
                status.set(
                    format!(
                        "Session {} · {} processes",
                        session.id.split('-').next().unwrap_or(&session.id),
                        seen.len()
                    )
                    .into(),
                );
            }
        }
        status.set("Session stream closed".into());
        Ok(())
    }

    async fn process_logs(
        &self,
        id: String,
        lines: Signal<Vec<SharedString>>,
    ) -> Result<(), String> {
        let (mut frames, _connection) =
            self.stream(&format!("/api/execution-processes/{id}/raw-logs/ws"))?;
        while let Some(frame) = frames.next().await {
            let frame = parse_frame(&frame?)?;
            if let Some(patch) = frame.patch {
                for operation in patch.0 {
                    if let json_patch::PatchOperation::Add(op) = operation {
                        if matches!(op.value["type"].as_str(), Some("STDOUT" | "STDERR"))
                            && let Some(output) = op.value["content"].as_str()
                        {
                            lines.update(|lines| {
                                lines.push(output.trim_end_matches(['\r', '\n']).to_owned().into())
                            });
                            let count = lines.with(Vec::len);
                            if count <= 2 || count.is_multiple_of(10) {
                                log::info!("execution {id}: {count} live log entries");
                            }
                        }
                    }
                }
            }
            if frame.finished {
                log::info!("execution {id}: raw log stream finished");
                break;
            }
        }
        Ok(())
    }
}

pub(crate) fn parse_frame(input: &str) -> Result<PatchFrame, String> {
    serde_json::from_str(input).map_err(|e| e.to_string())
}

fn apply_diff_patch(
    document: &mut serde_json::Value,
    mut patch: json_patch::Patch,
) -> Result<(), String> {
    use json_patch::{AddOperation, PatchOperation};

    // The server uses replace for a whole repo snapshot, including its first one.
    // RFC 6902 add has the upsert semantics of that stream operation.
    for operation in &mut patch.0 {
        if let PatchOperation::Replace(replace) = operation
            && replace
                .path
                .as_str()
                .strip_prefix("/entries/")
                .is_some_and(|key| !key.contains('/'))
        {
            *operation = PatchOperation::Add(AddOperation {
                path: std::mem::take(&mut replace.path),
                value: std::mem::take(&mut replace.value),
            });
        }
    }
    json_patch::patch(document, &patch).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletes_accept_explicit_null_and_preserve_error_messages() {
        let path = "/api/projects/p/issues/i";
        decode_response::<(), IgnoredAny>(path, 200, br#"{"success":true,"data":null}"#).unwrap();
        assert!(decode_response::<(), IgnoredAny>(path, 200, br#"{"success":true}"#).is_err());
        let body = br#"{"success":false,"data":null,"message":"invalid status"}"#;
        for status in [400, 200] {
            assert_eq!(
                decode_response::<String, IgnoredAny>(path, status, body)
                    .unwrap_err()
                    .message,
                "invalid status"
            );
        }
        let failure = decode_response::<(), serde_json::Value>(
            path,
            200,
            br#"{"success":false,"error_data":{"type":"rebase_in_progress"}}"#,
        )
        .unwrap_err();
        assert_eq!(failure.data.unwrap()["type"], "rebase_in_progress");
    }

    #[test]
    fn diff_snapshots_upsert_the_repo_and_keep_incremental_updates() {
        let mut document = json!({"entries": {}});
        let snapshot = parse_frame(
            r#"{"JsonPatch":[{"op":"replace","path":"/entries/my~1repo","value":{}}]}"#,
        )
        .unwrap()
        .patch
        .unwrap();
        apply_diff_patch(&mut document, snapshot.clone()).unwrap();

        let patch = parse_frame(r#"{"JsonPatch":[{"op":"add","path":"/entries/my~1repo/file.txt","value":{"type":"DIFF","content":{}}}]}"#)
            .unwrap().patch.unwrap();
        apply_diff_patch(&mut document, patch).unwrap();
        assert_eq!(document["entries"]["my/repo"]["file.txt"]["type"], "DIFF");

        apply_diff_patch(&mut document, snapshot).unwrap();
        assert_eq!(document, json!({"entries": {"my/repo": {}}}));
    }
}
