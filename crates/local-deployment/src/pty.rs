use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use portable_pty::{ChildKiller, CommandBuilder, NativePtySystem, PtySize, PtySystem};
use thiserror::Error;
use tokio::sync::mpsc;
use utils::shell::get_interactive_shell;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("Failed to create PTY: {0}")]
    CreateFailed(String),
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),
    #[error("Failed to write to PTY: {0}")]
    WriteFailed(String),
    #[error("Failed to resize PTY: {0}")]
    ResizeFailed(String),
    #[error("Session already closed")]
    SessionClosed,
}

struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Clone)]
pub struct PtyService {
    sessions: Arc<Mutex<HashMap<Uuid, PtySession>>>,
}

impl PtyService {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create_session(
        &self,
        working_dir: PathBuf,
        cols: u16,
        rows: u16,
    ) -> Result<(Uuid, mpsc::UnboundedReceiver<Vec<u8>>), PtyError> {
        let session_id = Uuid::new_v4();
        let (output_tx, output_rx) = mpsc::unbounded_channel();
        let shell = get_interactive_shell().await;
        let sessions = Arc::clone(&self.sessions);

        tokio::task::spawn_blocking(move || {
            let pty_system = NativePtySystem::default();

            let pty_pair = pty_system
                .openpty(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| PtyError::CreateFailed(e.to_string()))?;

            let mut cmd = CommandBuilder::new(&shell);
            cmd.cwd(&working_dir);

            // Configure shell-specific options
            let shell_name = shell.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if shell_name == "powershell.exe" || shell_name == "pwsh.exe" {
                // PowerShell: use -NoLogo for cleaner startup
                cmd.arg("-NoLogo");
            } else if shell_name == "cmd.exe" {
                // cmd.exe: no special args needed
            } else {
                // Unix shells
                cmd.env("VIBE_KANBAN_TERMINAL", "1");

                if shell_name == "bash" {
                    cmd.env("PROMPT_COMMAND", r#"PS1='$ '; unset PROMPT_COMMAND"#);
                } else if shell_name == "zsh" {
                    // PROMPT is set after spawning
                } else {
                    cmd.env("PS1", "$ ");
                }
            }

            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");

            let mut child = pty_pair
                .slave
                .spawn_command(cmd)
                .map_err(|e| PtyError::CreateFailed(e.to_string()))?;
            let killer = child.clone_killer();
            let mut writer = pty_pair
                .master
                .take_writer()
                .map_err(|e| PtyError::CreateFailed(e.to_string()))?;
            let mut reader = pty_pair
                .master
                .try_clone_reader()
                .map_err(|e| PtyError::CreateFailed(e.to_string()))?;

            if shell_name == "zsh" {
                let _ = writer.write_all(b" PROMPT='$ '; RPROMPT=''\n");
                let _ = writer.flush();
                let _ = writer.write_all(b"\x0c");
                let _ = writer.flush();
            }

            sessions
                .lock()
                .map_err(|e| PtyError::CreateFailed(e.to_string()))?
                .insert(
                    session_id,
                    PtySession {
                        writer,
                        master: pty_pair.master,
                        killer,
                    },
                );

            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if output_tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // ConPTY keeps its output pipe open until the master is closed.
            // Reap the process and release the master without holding the map
            // lock while the reader drains the last output.
            let sessions = Arc::downgrade(&sessions);
            thread::spawn(move || {
                let _ = child.wait();
                if let Some(sessions) = sessions.upgrade() {
                    let session = sessions
                        .lock()
                        .ok()
                        .and_then(|mut map| map.remove(&session_id));
                    drop(session);
                }
            });
            Ok::<_, PtyError>(())
        })
        .await
        .map_err(|e| PtyError::CreateFailed(e.to_string()))??;

        Ok((session_id, output_rx))
    }

    pub async fn write(&self, session_id: Uuid, data: &[u8]) -> Result<(), PtyError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| PtyError::WriteFailed(e.to_string()))?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or(PtyError::SessionNotFound(session_id))?;

        session
            .writer
            .write_all(data)
            .map_err(|e| PtyError::WriteFailed(e.to_string()))?;

        session
            .writer
            .flush()
            .map_err(|e| PtyError::WriteFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn resize(&self, session_id: Uuid, cols: u16, rows: u16) -> Result<(), PtyError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| PtyError::ResizeFailed(e.to_string()))?;
        let session = sessions
            .get(&session_id)
            .ok_or(PtyError::SessionNotFound(session_id))?;

        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::ResizeFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn close_session(&self, session_id: Uuid) -> Result<(), PtyError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| PtyError::SessionClosed)?
            .remove(&session_id);
        if let Some(mut session) = session {
            tokio::task::spawn_blocking(move || {
                let _ = session.killer.kill();
                drop(session);
            })
            .await
            .map_err(|_| PtyError::SessionClosed)?;
        }
        Ok(())
    }
}

impl Default for PtyService {
    fn default() -> Self {
        Self::new()
    }
}
