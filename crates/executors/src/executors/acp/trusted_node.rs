use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use acp::schema::v1::{ToolCallContent, ToolCallLocation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};
use uuid::Uuid;

use crate::{command::CommandParts, env::RepoContext, executors::ExecutorError};

const SESSION_PREFIX: &str = "vk-ssh:";
const PROBE_PREFIX: &str = ".vk-acp-probe-";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TrustedNode {
    registry_id: String,
    host: String,
    user: String,
    port: u16,
    identity_file: PathBuf,
    known_hosts_file: PathBuf,
    local_root: PathBuf,
    remote_root: String,
}

#[derive(Serialize)]
struct Preflight<'a> {
    registry_id: &'a str,
    cwd: &'a str,
    workspace: &'a str,
    repositories: Vec<String>,
    probes: Vec<(&'a str, &'a str)>,
}

pub(super) struct RemoteSession {
    prefix: String,
    workspace: String,
}

impl RemoteSession {
    pub(super) fn new(scope_descriptor: &[u8], workspace: String) -> Self {
        Self {
            prefix: format!("{SESSION_PREFIX}{:x}:", Sha256::digest(scope_descriptor)),
            workspace,
        }
    }

    pub(super) fn load_id(&self, persisted: &str) -> Result<String, ExecutorError> {
        persisted
            .strip_prefix(self.prefix.as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| invalid("ACP session belongs to a different node or working directory"))
    }

    pub(super) fn display_id(&self, id: &str) -> String {
        format!("{}{id}", self.prefix)
    }

    pub(super) fn rebase_tool_paths(
        &self,
        locations: &mut [ToolCallLocation],
        content: &mut [ToolCallContent],
    ) {
        let to_relative = |path: &mut PathBuf| {
            let Some(path_str) = path.to_str() else {
                return;
            };
            let Some(suffix) = path_str.strip_prefix(self.workspace.as_str()) else {
                return;
            };
            if suffix.is_empty() {
                *path = PathBuf::from(".");
            } else if let Some(relative) = suffix.strip_prefix('/')
                && valid_relative(relative)
            {
                *path = PathBuf::from(relative);
            }
        };
        for location in locations {
            to_relative(&mut location.path);
        }
        for entry in content {
            if let ToolCallContent::Diff(diff) = entry {
                to_relative(&mut diff.path);
            }
        }
    }
}

pub(in crate::executors) fn is_remote_session(id: &str) -> bool {
    id.starts_with(SESSION_PREFIX)
}

pub(super) async fn connect(
    name: &str,
    registry_id: &str,
    current_dir: &Path,
    repo_context: &RepoContext,
    session_id: Option<&str>,
) -> Result<(Child, RemoteSession, PathBuf, Option<String>), ExecutorError> {
    let config_path = std::env::var_os("VK_ACP_TRUSTED_NODES_FILE")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| invalid("VK_ACP_TRUSTED_NODES_FILE must name an absolute local file"))?;
    let config_path = tokio::fs::canonicalize(config_path).await?;
    let config = tokio::fs::read(&config_path).await?;
    let mut nodes: HashMap<String, TrustedNode> = serde_json::from_slice(&config)?;
    let node = nodes
        .remove(name)
        .ok_or_else(|| invalid("Unknown trusted ACP node"))?;
    if node.registry_id != registry_id {
        return Err(invalid(
            "Trusted ACP node is configured for a different registry agent",
        ));
    }
    if !node.local_root.is_absolute() || !valid_absolute(&node.remote_root) {
        return Err(invalid(
            "Trusted ACP node requires absolute local and POSIX remote roots",
        ));
    }
    let root = tokio::fs::canonicalize(&node.local_root).await?;
    if config_path.starts_with(&root) {
        return Err(invalid(
            "Trusted ACP node configuration must be outside the shared root",
        ));
    }
    let workspace = tokio::fs::canonicalize(&repo_context.workspace_root).await?;
    if workspace == root {
        return Err(invalid(
            "ACP workspace must be a directory below the trusted node root",
        ));
    }
    let cwd = tokio::fs::canonicalize(current_dir).await?;
    if !cwd.starts_with(&workspace) {
        return Err(invalid("ACP working directory is outside the workspace"));
    }
    let remote_workspace = map_directory(&node.remote_root, &root, &workspace)?;
    let remote_cwd = map_directory(&node.remote_root, &root, &cwd)?;
    // Bind persisted session IDs to the configured host, registry agent and mapped cwd.
    let scope_descriptor = serde_json::to_vec(&(name, &node, &remote_cwd))?;
    let scope = RemoteSession::new(&scope_descriptor, remote_workspace.clone());
    let existing = session_id.map(|id| scope.load_id(id)).transpose()?;

    let mut probes = Vec::with_capacity(repo_context.repo_names.len() + 2);
    probes.push(Probe::new(&workspace, &remote_workspace).await?);
    if cwd != workspace {
        probes.push(Probe::new(&cwd, &remote_cwd).await?);
    }
    let mut remote_repos = Vec::with_capacity(repo_context.repo_names.len());
    for name in &repo_context.repo_names {
        let repo = tokio::fs::canonicalize(workspace.join(name)).await?;
        if repo.parent() != Some(workspace.as_path())
            || repo.file_name() != Some(std::ffi::OsStr::new(name))
        {
            return Err(invalid(
                "ACP repository directory must be directly inside its workspace",
            ));
        }
        let remote = map_directory(&node.remote_root, &root, &repo)?;
        if repo != cwd {
            probes.push(Probe::new(&repo, &remote).await?);
        }
        remote_repos.push(remote);
    }
    let request = Preflight {
        registry_id,
        cwd: &remote_cwd,
        workspace: &remote_workspace,
        repositories: remote_repos,
        probes: probes
            .iter()
            .map(|p| (p.directory.as_str(), p.name.as_str()))
            .collect(),
    };
    let mut packet = serde_json::to_vec(&request)?;
    packet.push(b'\n');
    let mut ssh = node.spawn(&root).await?;
    tokio::time::timeout(CONNECT_TIMEOUT, preflight(&mut ssh, &packet, probes))
        .await
        .map_err(|_| invalid("Trusted ACP node did not verify the shared workspace in time"))??;
    Ok((ssh, scope, PathBuf::from(remote_cwd), existing))
}

impl TrustedNode {
    async fn spawn(&self, shared_root: &Path) -> Result<Child, ExecutorError> {
        if self.port == 0 || !valid_host(&self.host) || !valid_user(&self.user) {
            return Err(invalid(
                "Trusted ACP node host, port or SSH user is invalid",
            ));
        }
        let identity = ssh_file(&self.identity_file, shared_root).await?;
        let known_hosts = ssh_file(&self.known_hosts_file, shared_root).await?;
        let port = self.port.to_string();
        let known_hosts = format!("UserKnownHostsFile=\"{known_hosts}\"");
        let exe = if cfg!(windows) { "ssh.exe" } else { "ssh" };
        let (exe, _) = CommandParts::new(exe.to_owned(), Vec::new())
            .into_resolved()
            .await?;
        let mut command = Command::new(exe);
        command
            .env_clear()
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args([
                "-F",
                "none",
                "-T",
                "-s",
                "-i",
                &identity,
                "-l",
                &self.user,
                "-p",
                &port,
                "-o",
                "BatchMode=yes",
                "-o",
                "PreferredAuthentications=publickey",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "IdentityAgent=none",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "GlobalKnownHostsFile=none",
                "-o",
                &known_hosts,
                "-o",
                "UpdateHostKeys=no",
                "-o",
                "ForwardAgent=no",
                "-o",
                "ForwardX11=no",
                "-o",
                "ClearAllForwardings=yes",
                "-o",
                "ConnectTimeout=10",
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                &self.host,
                "vibe-acp",
            ]);
        #[cfg(windows)]
        {
            use workspace_utils::command_ext::NoWindowExt;

            command.no_window();
            if let Some(root) = std::env::var_os("SystemRoot") {
                command.env("SystemRoot", root);
            }
        }
        Ok(command.spawn()?)
    }
}

struct Probe {
    file: tempfile::TempPath,
    directory: String,
    name: String,
    nonce: String,
}

impl Probe {
    async fn new(local: &Path, remote: &str) -> Result<Self, ExecutorError> {
        let file = tempfile::Builder::new()
            .prefix(PROBE_PREFIX)
            .tempfile_in(local)?
            .into_temp_path();
        let name = file
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or_default()
            .to_owned();
        let nonce = Uuid::new_v4().simple().to_string();
        tokio::fs::write(&file, &nonce).await?;
        Ok(Self {
            file,
            directory: remote.to_owned(),
            name,
            nonce,
        })
    }
}

async fn preflight(
    ssh: &mut Child,
    request: &[u8],
    probes: Vec<Probe>,
) -> Result<(), ExecutorError> {
    let stdin = ssh
        .stdin
        .as_mut()
        .ok_or_else(|| invalid("SSH stdin is unavailable"))?;
    let stdout = ssh
        .stdout
        .as_mut()
        .ok_or_else(|| invalid("SSH stdout is unavailable"))?;
    stdin.write_all(request).await?;
    stdin.flush().await?;
    let mut reply = [0; 17];
    if stdout.read_exact(&mut reply).await.is_err() || &reply != b"vibe-acp/1 PROOF\n" {
        return Err(invalid(
            "Trusted ACP node rejected the shared workspace (check SSH host key, subsystem and Git paths)",
        ));
    }
    for probe in probes {
        let result = tokio::fs::read_to_string(&probe.file).await?;
        if result.strip_prefix("ack:") != Some(probe.nonce.as_str()) {
            return Err(invalid(
                "Trusted ACP node could not write the same shared workspace",
            ));
        }
        probe.file.close()?;
    }
    stdin.write_all(b"vibe-acp/1 START\n").await?;
    stdin.flush().await?;
    Ok(())
}

fn map_directory(remote: &str, local_root: &Path, path: &Path) -> Result<String, ExecutorError> {
    let relative = path
        .strip_prefix(local_root)
        .map_err(|_| invalid("ACP workspace is outside the configured local root"))?;
    let mut result = remote.to_owned();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(invalid("ACP workspace contains an invalid path component"));
        };
        let name = name
            .to_str()
            .filter(|&s| valid_relative(s))
            .ok_or_else(|| invalid("ACP workspace has a non-portable path component"))?;
        result.push('/');
        result.push_str(name);
    }
    Ok(result)
}

fn valid_relative(path: &str) -> bool {
    !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn valid_absolute(path: &str) -> bool {
    path.strip_prefix('/').is_some_and(valid_relative)
}

fn valid_host(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok()
        || host.split('.').all(|part| {
            part.as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
                && part
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

fn valid_user(user: &str) -> bool {
    !user.starts_with('-')
        && !user.is_empty()
        && user
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

async fn ssh_file(path: &Path, shared_root: &Path) -> Result<String, ExecutorError> {
    let value = path
        .to_str()
        .filter(|s| {
            path.is_absolute() && !s.contains(['%', '$', '"']) && !s.chars().any(char::is_control)
        })
        .ok_or_else(|| {
            invalid("Trusted ACP SSH files need absolute paths without OpenSSH expansions")
        })?;
    let resolved = tokio::fs::canonicalize(path).await?;
    if resolved.starts_with(shared_root) || !tokio::fs::metadata(resolved).await?.is_file() {
        return Err(invalid(
            "Trusted ACP SSH identity and known_hosts must be files outside the shared root",
        ));
    }
    #[cfg(windows)]
    {
        Ok(value.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        Ok(value.to_owned())
    }
}

fn invalid(message: &str) -> ExecutorError {
    ExecutorError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_node_sessions_and_paths_stay_with_their_workspace() {
        let scope = RemoteSession::new(b"node-a", "/shared/workspace".into());
        let other = RemoteSession::new(b"node-b", "/shared/workspace".into());
        let session = scope.display_id("session:1");
        assert_eq!(scope.load_id(&session).unwrap(), "session:1");
        assert!(other.load_id(&session).is_err());
        assert!(scope.load_id("local-id").is_err());
        let mut locations = [
            ToolCallLocation::new("/shared/workspace/repo/file"),
            ToolCallLocation::new("/shared/workspace-other/file"),
        ];
        scope.rebase_tool_paths(&mut locations, &mut []);
        assert_eq!(locations[0].path, Path::new("repo/file"));
        assert_eq!(locations[1].path, Path::new("/shared/workspace-other/file"));
    }

    // The node must be POSIX and have Python 3. This test uses real process pipes
    // with the deployed wrapper and a child that never reads stdin.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires /usr/bin/python3 on the node"]
    async fn trusted_node_subsystem_proves_mount_and_stops_on_disconnect() {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let cwd = root_path.join("workspace");
        tokio::fs::create_dir(&cwd).await.unwrap();
        let cwd_str = cwd.to_str().unwrap();
        let probe = Probe::new(&cwd, cwd_str).await.unwrap();
        let mut packet = serde_json::to_vec(&Preflight {
            registry_id: "test-agent",
            cwd: cwd_str,
            workspace: cwd_str,
            repositories: Vec::new(),
            probes: vec![(cwd_str, &probe.name)],
        })
        .unwrap();
        packet.push(b'\n');
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/executors/acp/trusted_node_subsystem.py");
        let spawn = || {
            Command::new("/usr/bin/python3")
                .arg(&script)
                .arg(&root_path)
                .args([
                    "test-agent",
                    "/bin/sh",
                    "-c",
                    "printf '%s\n%s\n' \"$PWD\" \"$$\"; exec /bin/sleep 60",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .unwrap()
        };
        let mut node = spawn();
        tokio::time::timeout(Duration::from_secs(5), async {
            preflight(&mut node, &packet, vec![probe]).await.unwrap();
            let mut lines = BufReader::new(node.stdout.take().unwrap()).lines();
            assert_eq!(lines.next_line().await.unwrap().as_deref(), Some(cwd_str));
            let pid = lines.next_line().await.unwrap().unwrap();
            let proc = Path::new("/proc").join(pid);
            assert!(proc.is_dir());
            drop(node.stdin.take());
            assert!(node.wait().await.unwrap().success());
            assert!(!proc.exists());
            assert_eq!(std::fs::read_dir(cwd).unwrap().count(), 0);

            let unrelated = root_path.join("different-workspace");
            tokio::fs::create_dir(&unrelated).await.unwrap();
            let wrong = unrelated.to_str().unwrap();
            let probe = Probe::new(&cwd, wrong).await.unwrap();
            let mut packet = serde_json::to_vec(&Preflight {
                registry_id: "test-agent",
                cwd: wrong,
                workspace: wrong,
                repositories: Vec::new(),
                probes: vec![(wrong, &probe.name)],
            })
            .unwrap();
            packet.push(b'\n');
            let mut wrong_node = spawn();
            assert!(
                preflight(&mut wrong_node, &packet, vec![probe])
                    .await
                    .is_err()
            );
            assert!(!wrong_node.wait().await.unwrap().success());
        })
        .await
        .expect("trusted ACP subsystem did not prove or release the workspace");
    }
}
