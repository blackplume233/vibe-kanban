use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::api::Api;

pub(super) const INFO: &str = "/api/info";
pub(super) const REPOS: &str = "/api/repos";

#[derive(Deserialize)]
pub(super) struct Info {
    pub config: Value,
    pub executors: BTreeMap<String, Profile>,
}

#[derive(Deserialize)]
pub(super) struct Profile {
    #[serde(flatten)]
    variants: BTreeMap<String, serde::de::IgnoredAny>,
}

impl Profile {
    pub fn variants(&self) -> impl Iterator<Item = &str> {
        self.variants
            .keys()
            .map(String::as_str)
            .filter(|name| *name != "recently_used_models")
    }

    pub fn has_variant(&self, name: &str) -> bool {
        self.variants().any(|key| key == name)
    }
}

#[derive(Clone, PartialEq, Deserialize)]
pub(super) struct RepoSummary {
    pub id: String,
    pub display_name: String,
}

#[derive(Deserialize)]
pub(super) struct Branch {
    pub name: String,
    pub is_current: bool,
}

#[derive(Deserialize)]
pub(super) struct Directory {
    pub current_path: String,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Deserialize)]
pub(super) struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

pub(super) async fn write<T: serde::de::DeserializeOwned>(
    api: &Api,
    method: &str,
    path: &str,
    body: &impl Serialize,
) -> Result<T, String> {
    api.request(
        method,
        path,
        Some(serde_json::to_vec(body).map_err(|error| error.to_string())?),
    )
    .await
}

#[derive(Clone, PartialEq, Default)]
pub(super) struct Draft {
    base: Value,
    edits: BTreeMap<&'static str, Value>,
}

impl Draft {
    pub fn new(base: Value) -> Self {
        Self {
            base,
            edits: BTreeMap::new(),
        }
    }

    pub fn get(&self, path: &str) -> &Value {
        self.edits
            .get(path)
            .or_else(|| self.base.pointer(path))
            .unwrap_or(&Value::Null)
    }

    pub fn edit(&mut self, path: &'static str, value: Value) {
        if self.base.pointer(path).unwrap_or(&Value::Null) == &value {
            self.edits.remove(path);
        } else {
            self.edits.insert(path, value);
        }
    }

    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn discard(&mut self) {
        self.edits.clear();
    }

    /// PUT /config takes the whole Config DTO. Overlay only touched leaves on a fresh copy.
    pub fn config_body(&self, latest: &mut Value) {
        for (&path, value) in &self.edits {
            let (parent, key) = path.rsplit_once('/').expect("settings field path");
            if let Some(parent) = latest.pointer_mut(parent).and_then(Value::as_object_mut) {
                parent.insert(key.to_owned(), value.clone());
            }
        }
    }

    /// PUT /repos/{id} distinguishes an omitted property from an explicit null.
    pub fn repo_body(&self) -> Map<String, Value> {
        self.edits
            .iter()
            .map(|(&path, value)| {
                let value = value
                    .as_str()
                    .map_or_else(|| value.clone(), |value| optional(value.trim()));
                (path[1..].to_owned(), value)
            })
            .collect()
    }
}

pub(super) fn parent_dir(path: &str) -> Option<&str> {
    let path = path.trim_end_matches(['/', '\\']);
    let index = path.rfind(['/', '\\'])?;
    // The path belongs to the backend host. WebAssembly's native Path parser
    // cannot recognize a Windows drive root or backslash separators.
    let parent = if index == 0 || (index == 2 && path.as_bytes()[1] == b':') {
        index + 1
    } else {
        index
    };
    if path.starts_with(r"\\") && path[..index].matches('\\').count() < 3 {
        return None;
    }
    Some(&path[..parent])
}

pub(super) fn optional(value: &str) -> Value {
    if value.is_empty() {
        Value::Null
    } else {
        Value::String(value.to_owned())
    }
}

pub(super) fn prefix_error(value: &str) -> Option<&'static str> {
    if value.contains('/') {
        Some("前缀不能包含斜杠。")
    } else if value.starts_with('.')
        || value.starts_with('-')
        || value.ends_with('.')
        || value.ends_with(".lock")
    {
        Some("前缀的开头或结尾不符合 Git 分支命名规则。")
    } else if value.contains("..") || value.contains("@{") {
        Some("前缀不能包含 .. 或 @{。")
    } else if value
        .bytes()
        .any(|byte| byte <= b' ' || byte == 127 || b"~^:?*[\\".contains(&byte))
    {
        Some("前缀不能包含空格、控制字符或 Git 保留字符。")
    } else {
        None
    }
}

pub(super) fn directory_url(path: &str) -> String {
    if path.is_empty() {
        return "/api/filesystem/directory".to_owned();
    }
    use std::fmt::Write as _;
    let mut url = String::from("/api/filesystem/directory?path=");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            url.push(char::from(byte));
        } else {
            write!(url, "%{byte:02X}").expect("format into String");
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn config_overlay_preserves_latest_unrelated_values_and_accepts_missing_variant() {
        let mut draft = Draft::new(
            json!({"editor":{"editor_type":"VS_CODE","remote_ssh_host":"previous"},"executor_profile":{"executor":"CODEX"}}),
        );
        draft.edit("/editor/editor_type", json!("CUSTOM"));
        draft.edit("/editor/remote_ssh_host", json!("previous"));
        draft.edit("/executor_profile/variant", json!("PLAN"));
        let mut latest = json!({"editor":{"editor_type":"VS_CODE","remote_ssh_host":"other-client"},"relay_enabled":true,"executor_profile":{"executor":"CODEX"}});
        draft.config_body(&mut latest);
        assert_eq!(
            latest,
            json!({"editor":{"editor_type":"CUSTOM","remote_ssh_host":"other-client"},"relay_enabled":true,"executor_profile":{"executor":"CODEX","variant":"PLAN"}})
        );
    }

    #[test]
    fn repo_delta_omits_unchanged_fields_and_clears_optional_values() {
        let mut draft = Draft::new(
            json!({"setup_script":"echo setup", "cleanup_script":"echo cleanup", "parallel_setup_script":true}),
        );
        draft.edit("/setup_script", optional(""));
        draft.edit("/cleanup_script", json!("echo cleanup"));
        draft.edit("/parallel_setup_script", json!(false));
        assert_eq!(
            Value::Object(draft.repo_body()),
            json!({"setup_script":null,"parallel_setup_script":false})
        );
        draft.discard();
        assert!(!draft.dirty());
    }

    #[test]
    fn branch_and_directory_values_follow_server_contract() {
        for value in ["", "vibe-own", "local.dev", "团队"] {
            assert_eq!(prefix_error(value), None, "{value}");
        }
        for value in [
            "bad/name",
            ".hidden",
            "-bad",
            "bad..name",
            "bad.lock",
            "bad?",
            "bad.",
            "bad\tname",
        ] {
            assert!(prefix_error(value).is_some(), "{value}");
        }
        assert_eq!(
            directory_url("F:\\仓库+one & two"),
            "/api/filesystem/directory?path=F%3A%5C%E4%BB%93%E5%BA%93%2Bone%20%26%20two"
        );
    }
}
