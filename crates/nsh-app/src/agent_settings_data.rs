use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::api::Api;

const REGISTRY: &str = "/api/acp-registry/agents";
const PROFILES: &str = "/api/profiles";

#[derive(Clone, Deserialize, PartialEq)]
pub(super) struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub executor: String,
    pub installed: bool,
}

#[derive(Deserialize)]
struct ProfilesContent {
    content: String,
    path: String,
}

#[derive(PartialEq)]
pub(super) struct Profiles {
    pub path: String,
    document: Value,
}

impl Profiles {
    fn parse(content: ProfilesContent) -> Result<Self, String> {
        let document: Value = serde_json::from_str(&content.content)
            .map_err(|_| "服务返回的 Agent 预设不是有效的 JSON。")?;
        if !document["executors"].is_object() {
            return Err("Agent 预设缺少 executors 对象。".into());
        }
        Ok(Self {
            path: content.path,
            document,
        })
    }

    pub fn executors(&self) -> impl Iterator<Item = &str> {
        self.document["executors"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
    }

    pub fn variants(&self, executor: &str) -> impl Iterator<Item = &str> {
        self.document["executors"][executor]
            .as_object()
            .into_iter()
            .flat_map(Map::iter)
            .filter(|(name, _)| *name != "recently_used_models")
            .map(|(name, _)| name.as_str())
    }

    pub fn variant(&self, target: &Preset) -> Option<&Value> {
        self.document["executors"][&target.executor].get(&target.variant)
    }

    fn apply(&mut self, target: &Preset, change: ProfileChange) -> Result<(), String> {
        let variants = self.document["executors"]
            .get_mut(&target.executor)
            .and_then(Value::as_object_mut)
            .ok_or("该执行器已移除，请重新选择。")?;
        match change {
            ProfileChange::Save { before, after } => {
                if variants.get(&target.variant) != before.as_ref() {
                    return Err("预设已在其他操作中变更。请重新载入后再编辑。".into());
                }
                variants.insert(target.variant.clone(), after);
            }
            ProfileChange::Delete { before } => {
                if target.variant == "DEFAULT" || variants.get(&target.variant) != Some(&before) {
                    return Err("无法删除默认预设或已在其他操作中变更的预设。".into());
                }
                variants.remove(&target.variant);
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct Preset {
    pub executor: String,
    pub variant: String,
}

pub(super) enum ProfileChange {
    Save { before: Option<Value>, after: Value },
    Delete { before: Value },
}

pub(super) fn preset_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty()
        || name.len() > 40
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("名称须包含 1–40 个英文字母、数字、下划线或连字符。".into());
    }
    // Restrict normalized names to the same segments used by the server's
    // SCREAMING_SNAKE_CASE conversion, including digit and camel-case boundaries.
    let chars = name.as_bytes();
    let mut canonical = String::with_capacity(name.len());
    for (index, &c) in chars.iter().enumerate() {
        if !c.is_ascii_alphanumeric() {
            if !canonical.is_empty() && !canonical.ends_with('_') {
                canonical.push('_');
            }
            continue;
        }
        if let Some(&previous) = index.checked_sub(1).and_then(|i| chars.get(i)) {
            let acronym_end = previous.is_ascii_uppercase()
                && c.is_ascii_uppercase()
                && chars.get(index + 1).is_some_and(u8::is_ascii_lowercase);
            if (previous.is_ascii_lowercase() && c.is_ascii_uppercase())
                || acronym_end
                || (previous.is_ascii_alphanumeric()
                    && previous.is_ascii_digit() != c.is_ascii_digit())
            {
                canonical.push('_');
            }
        }
        canonical.push(char::from(c.to_ascii_uppercase()));
    }
    canonical.truncate(canonical.trim_end_matches('_').len());
    if name.starts_with("__")
        || canonical.is_empty()
        || canonical == "DEFAULT"
        || canonical == "RECENTLY_USED_MODELS"
    {
        return Err("此预设名称为保留名称。".into());
    }
    Ok(canonical)
}

pub(super) fn preset_body(value: &Value) -> Result<(&str, &Map<String, Value>), String> {
    value
        .as_object()
        .filter(|tags| tags.len() == 1)
        .and_then(|tags| tags.iter().next())
        .and_then(|(tag, value)| Some((tag.as_str(), value.as_object()?)))
        .ok_or_else(|| "服务返回的预设配置格式无效。".into())
}

pub(super) fn edited_preset(base: &Value, draft: &str) -> Result<Value, String> {
    let (tag, base) = preset_body(base)?;
    let body: Map<String, Value> =
        serde_json::from_str(draft).map_err(|_| "预设内容必须是一个有效的 JSON 对象。")?;
    if tag == "ACP_SERVER" && body.get("registry_id") != base.get("registry_id") {
        return Err("registry_id 标识已安装的适配器，不能在预设中修改。".into());
    }
    Ok(Value::Object(Map::from_iter([(
        tag.to_owned(),
        Value::Object(body),
    )])))
}

pub(super) fn blank_preset(base: &Value) -> Result<Value, String> {
    let (tag, fields) = preset_body(base)?;
    let mut body = Map::new();
    if tag == "ACP_SERVER" {
        body.insert(
            "registry_id".into(),
            fields
                .get("registry_id")
                .ok_or("预设缺少 registry_id。")?
                .clone(),
        );
    }
    Ok(Value::Object(Map::from_iter([(
        tag.to_owned(),
        Value::Object(body),
    )])))
}

#[derive(Deserialize)]
pub(super) struct McpResponse {
    pub config_path: String,
    pub mcp_config: McpConfig,
}

#[derive(Deserialize)]
pub(super) struct McpConfig {
    pub servers_path: Vec<String>,
    servers: Map<String, Value>,
    template: Value,
    #[serde(default)]
    preconfigured: BTreeMap<String, Value>,
}

type McpEditorParts = (String, Vec<String>, BTreeMap<String, Value>);

impl McpConfig {
    pub fn editor(self) -> Result<McpEditorParts, String> {
        let (key, parents) = self
            .servers_path
            .split_last()
            .ok_or("服务没有提供 MCP 配置字段。")?;
        let mut full = self.template.as_object().cloned().unwrap_or_default();
        let mut current = &mut full;
        for key in parents {
            let next = current
                .entry(key.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if !next.is_object() {
                *next = Value::Object(Map::new());
            }
            current = next.as_object_mut().unwrap();
        }
        current.insert(key.clone(), Value::Object(self.servers));
        let providers = self
            .preconfigured
            .into_iter()
            .filter(|(name, _)| name != "meta")
            .collect();
        Ok((pretty(&full), self.servers_path, providers))
    }
}

pub(super) fn mcp_servers<'a>(
    value: &'a mut Value,
    path: &[String],
) -> Result<&'a mut Map<String, Value>, String> {
    let mut current = value;
    for key in path {
        current = current
            .as_object_mut()
            .and_then(|obj| obj.get_mut(key))
            .ok_or_else(|| format!("缺少 MCP 配置字段 {}。", path.join(".")))?;
    }
    current
        .as_object_mut()
        .ok_or_else(|| "MCP 服务列表必须是 JSON 对象。".into())
}

pub(super) fn parse_mcp(draft: &str) -> Result<Value, String> {
    serde_json::from_str(draft).map_err(|_| "MCP 配置不是有效的 JSON。".into())
}

pub(super) fn pretty(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).expect("JSON values serialize")
}

// Encode segments as well as query values. Registry ids originate outside the app.
fn encode(value: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    encoded
}

pub(super) async fn agents(api: &Api) -> Result<Vec<Agent>, String> {
    api.request("GET", REGISTRY, None).await
}

pub(super) async fn set_installed(api: &Api, id: &str, installed: bool) -> Result<(), String> {
    let path = format!("{REGISTRY}/{}", encode(id));
    if installed {
        api.request::<Agent>("POST", &path, None).await?;
    } else {
        api.request::<()>("DELETE", &path, None).await?;
    }
    Ok(())
}

pub(super) async fn profiles(api: &Api) -> Result<Profiles, String> {
    Profiles::parse(api.request("GET", PROFILES, None).await?)
}

pub(super) async fn save_profile(
    api: &Api,
    target: &Preset,
    change: ProfileChange,
) -> Result<(), String> {
    let mut current = profiles(api).await?;
    current.apply(target, change)?;
    let body = serde_json::to_vec(&current.document).expect("JSON values serialize");
    api.request::<String>("PUT", PROFILES, Some(body))
        .await
        .map(|_| ())
        .map_err(|error| {
            if error.contains("cannot be deleted") {
                "内置预设不能删除。".into()
            } else {
                "保存 Agent 预设失败。请确认字段符合该执行器的配置要求。".into()
            }
        })
}

fn mcp_url(executor: &str) -> String {
    format!("/api/mcp-config?executor={}", encode(executor))
}

pub(super) async fn mcp(api: &Api, executor: &str) -> Result<McpResponse, String> {
    api.request("GET", &mcp_url(executor), None)
        .await
        .map_err(|error| {
            if error == "MCP not supported by this executor" {
                "此执行器不支持 MCP。".into()
            } else {
                "无法读取 MCP 配置。请检查服务连接、文件权限和配置格式。".into()
            }
        })
}

pub(super) async fn save_mcp(
    api: &Api,
    executor: &str,
    servers: &Map<String, Value>,
) -> Result<(), String> {
    let body =
        serde_json::to_vec(&BTreeMap::from([("servers", servers)])).expect("JSON values serialize");
    api.request::<String>("POST", &mcp_url(executor), Some(body))
        .await
        .map(|_| ())
        .map_err(|_| "无法保存 MCP 配置。请检查文件权限和执行器支持的格式。".into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn update_preserves_other_profiles_and_metadata() {
        let initial = json!({"executors": {
            "CODEX": {"DEFAULT": {"CODEX": {}}, "recently_used_models": {"models": ["sample"]}},
            "CLAUDE_CODE": {"DEFAULT": {"CLAUDE_CODE": {"model": "sample"}}},
            "CODEX_ACP": {"DEFAULT": {"ACP_SERVER": {"registry_id": "codex-acp"}}}
        }});
        let mut profiles = Profiles::parse(ProfilesContent {
            content: initial.to_string(),
            path: String::new(),
        })
        .unwrap();
        let target = Preset {
            executor: "CODEX".into(),
            variant: "REVIEW".into(),
        };
        let after = json!({"CODEX": {}});
        profiles
            .apply(
                &target,
                ProfileChange::Save {
                    before: None,
                    after: after.clone(),
                },
            )
            .unwrap();
        assert_eq!(
            profiles.variants("CODEX").collect::<Vec<_>>(),
            ["DEFAULT", "REVIEW"]
        );
        assert!(
            profiles
                .apply(
                    &target,
                    ProfileChange::Save {
                        before: None,
                        after: after.clone()
                    }
                )
                .is_err()
        );
        profiles
            .apply(&target, ProfileChange::Delete { before: after })
            .unwrap();
        assert_eq!(profiles.document, initial);
    }

    #[test]
    fn normalized_names_and_acp_id_are_preserved() {
        assert_eq!(preset_name(" reviewMode-2 ").unwrap(), "REVIEW_MODE_2");
        assert!(preset_name("__reserved").is_err());
        assert!(preset_name("default").is_err());
        let base = json!({"ACP_SERVER": {"registry_id": "claude-acp", "env": {}}});
        assert_eq!(
            blank_preset(&base).unwrap(),
            json!({"ACP_SERVER": {"registry_id": "claude-acp"}})
        );
        assert!(edited_preset(&base, r#"{"registry_id":"codex-acp"}"#).is_err());
    }

    #[test]
    fn mcp_editor_validates_the_agent_specific_nested_servers_path() {
        let path = vec!["context".into(), "servers".into()];
        let config = McpConfig {
            servers_path: path,
            template: json!({"unrelated": true}),
            servers: Map::new(),
            preconfigured: BTreeMap::new(),
        };
        let (draft, path, _) = config.editor().unwrap();
        let mut draft = parse_mcp(&draft).unwrap();
        assert!(mcp_servers(&mut draft, &path).unwrap().is_empty());
        assert_eq!(draft["unrelated"], true);
        assert!(mcp_servers(&mut json!({"context": {"servers": []}}), &path).is_err());
        assert!(parse_mcp("{invalid").is_err());
        assert_eq!(encode("id/?&"), "id%2F%3F%26");
    }
}
