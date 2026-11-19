//! Reads registry agents' MCP settings and converts them to ACP session descriptors.

use std::{
    collections::BTreeMap,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use acp::schema::v1::{
    EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use workspace_utils::shell::resolve_executable_path;

use crate::executors::ExecutorError;

pub(in crate::executors) fn config_path(root: &Path, registry_id: &str) -> PathBuf {
    // Registry IDs may contain path separators or differ only by case on Windows.
    root.join("acp-mcp")
        .join(format!("{:x}.json", Sha256::digest(registry_id)))
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Transport {
    #[default]
    Stdio,
    Http,
    Sse,
}

#[derive(Deserialize)]
struct ServerConfig {
    #[serde(default, rename = "type")]
    transport: Transport,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

pub(super) async fn read_servers(
    path: Option<&Path>,
    cwd: &Path,
) -> Result<Vec<McpServer>, ExecutorError> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let content = match tokio::fs::read(path).await {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(ExecutorError::Io(io::Error::new(
                err.kind(),
                format!("Could not read ACP MCP settings ({}).", err.kind()),
            )));
        }
    };
    // JSON/serde errors can include values from the file, which may contain credentials.
    let mut config: Map<String, Value> = serde_json::from_slice(&content).map_err(|err| {
        invalid_config(format_args!(
            "expected a JSON object (line {}, column {})",
            err.line(),
            err.column()
        ))
    })?;
    let Some(servers) = config.remove("mcpServers") else {
        return Ok(Vec::new());
    };
    let Value::Object(servers) = servers else {
        return Err(invalid_config("mcpServers must be an object"));
    };

    let mut descriptors = Vec::with_capacity(servers.len());
    for (index, (name, config)) in servers.into_iter().enumerate() {
        let invalid_server =
            |reason| invalid_config(format_args!("server #{}: {reason}", index + 1));
        if name.trim().is_empty() || !config.is_object() {
            return Err(invalid_server("expected a named server object"));
        }
        let config = serde_json::from_value::<ServerConfig>(config).map_err(|_| {
            invalid_server("expected an stdio, HTTP, or SSE server with valid fields")
        })?;
        descriptors.push(
            config
                .into_descriptor(name, cwd)
                .await
                .map_err(invalid_server)?,
        );
    }
    Ok(descriptors)
}

impl ServerConfig {
    async fn into_descriptor(self, name: String, cwd: &Path) -> Result<McpServer, &'static str> {
        match self.transport {
            Transport::Stdio => {
                let command = self
                    .command
                    .filter(|command| !command.trim().is_empty())
                    .ok_or("stdio requires a command")?;
                let path = Path::new(&command);
                let command = if path.is_absolute() {
                    PathBuf::from(command)
                } else if path.components().count() > 1 {
                    cwd.join(path)
                } else {
                    resolve_executable_path(&command)
                        .await
                        .ok_or("stdio command was not found on PATH")?
                };
                if !command.is_absolute() {
                    return Err("stdio command could not be resolved to an absolute path");
                }
                let env = self
                    .env
                    .into_iter()
                    .map(|(key, value)| EnvVariable::new(key, value));
                Ok(McpServer::Stdio(
                    McpServerStdio::new(name, command)
                        .args(self.args)
                        .env(env.collect()),
                ))
            }
            Transport::Http | Transport::Sse => {
                let url = self.url.ok_or("HTTP and SSE require a URL")?;
                if !reqwest::Url::parse(&url)
                    .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
                {
                    return Err("HTTP and SSE require an absolute HTTP or HTTPS URL");
                }
                let headers = self
                    .headers
                    .into_iter()
                    .map(|(key, value)| HttpHeader::new(key, value))
                    .collect();
                Ok(match self.transport {
                    Transport::Http => {
                        McpServer::Http(McpServerHttp::new(name, url).headers(headers))
                    }
                    _ => McpServer::Sse(McpServerSse::new(name, url).headers(headers)),
                })
            }
        }
    }
}

fn invalid_config(reason: impl std::fmt::Display) -> ExecutorError {
    ExecutorError::Io(io::Error::new(
        ErrorKind::InvalidData,
        format!("Invalid ACP MCP settings: {reason}."),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_paths_are_isolated_and_safe_to_create() {
        let root = Path::new("settings");
        let id = "../Vendor/Tool";
        let path = config_path(root, id);
        assert_eq!(path, config_path(root, id));
        assert_ne!(path, config_path(root, "../vendor/tool"));
        assert_ne!(path, config_path(root, "___Vendor_Tool"));
        assert_eq!(path.parent(), Some(root.join("acp-mcp").as_path()));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("json"));
    }

    #[tokio::test]
    async fn missing_settings_are_empty_but_bad_settings_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        assert!(
            read_servers(Some(&path), dir.path())
                .await
                .unwrap()
                .is_empty()
        );
        for content in [
            r#"{"mcpServers": {"test": "private-value""#,
            r#"{"mcpServers": ["private-value"]}"#,
            r#"{"mcpServers": {"test": {"type": "private-value"}}}"#,
            r#"{"mcpServers": {"test": {"command": ""}}}"#,
            r#"{"mcpServers": {"test": {"type": "http", "url": "private-value"}}}"#,
            r#"{"mcpServers": {"test": {"type": "sse", "url": "https://example.test", "headers": {"X-Test": ["private-value"]}}}}"#,
        ] {
            tokio::fs::write(&path, content).await.unwrap();
            let err = read_servers(Some(&path), dir.path()).await.unwrap_err();
            assert!(matches!(&err, ExecutorError::Io(err) if err.kind() == ErrorKind::InvalidData));
            assert!(err.to_string().contains("Invalid ACP MCP settings"));
            assert!(!err.to_string().contains("private-value"));
        }
    }
}
