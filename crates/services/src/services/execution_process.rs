use std::{
    collections::HashMap,
    io::{IsTerminal, Write},
    sync::Arc,
};

use anyhow::{Context, Result};
use db::{
    DBService,
    models::{
        coding_agent_turn::CodingAgentTurn, execution_process::ExecutionProcess,
        execution_process_logs::ExecutionProcessLogs, execution_usage::ExecutionUsage,
    },
};
use executors::logs::ExecutionTokenUsage;
use futures::{StreamExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use json_patch::{Patch, PatchOperation};
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::{
    io::AsyncWriteExt,
    sync::{broadcast, oneshot},
    task::JoinHandle,
};
use utils::{
    assets::prod_asset_dir_path,
    execution_logs::{
        ExecutionLogWriter, process_log_file_path, process_log_file_path_in_root,
        read_execution_log_file,
    },
    log_msg::LogMsg,
};
use uuid::Uuid;

pub async fn migrate_execution_logs_to_files() -> Result<()> {
    let pool = DBService::new_migration_pool()
        .await
        .map_err(|e| anyhow::anyhow!("Migration DB pool error: {}", e))?;

    if !ExecutionProcessLogs::has_any(&pool).await? {
        return Ok(());
    }

    let is_tty = std::io::stderr().is_terminal();
    if is_tty {
        let _ = writeln!(
            std::io::stderr(),
            "Performing one time database migration to move logs from SQLite to flat file to improve performance, data remains local, may take a few minutes, please don't exit while this process is running..."
        );
    }

    let pb = if is_tty {
        Some(new_spinner("Migrating"))
    } else {
        None
    };

    let total_processes = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let count_task = {
        let pool = pool.clone();
        let pb = pb.clone();
        let total_processes = total_processes.clone();
        tokio::spawn(async move {
            if let Ok(count) = ExecutionProcessLogs::count_distinct_processes(&pool).await {
                total_processes.store(count as usize, std::sync::atomic::Ordering::Relaxed);
                if let Some(pb) = pb {
                    pb.set_length(count as u64);
                    pb.set_style(
                        ProgressStyle::default_bar()
                            .template("{bar:36.yellow} {percent:>3}% {msg:<12.dim}")
                            .unwrap_or_else(|_| ProgressStyle::default_bar())
                            .progress_chars("■⬝"),
                    );
                }
            }
        })
    };

    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    ExecutionProcessLogs::stream_distinct_processes(&pool)
        .map_err(anyhow::Error::from)
        .map(|res| {
            let pool = pool.clone();
            let pb = pb.clone();
            let completed = completed.clone();
            let total_processes = total_processes.clone();
            async move {
                let p = res?;

                let path = process_log_file_path(p.session_id, p.execution_id);
                if path.exists() {
                    if let Some(pb) = &pb {
                        pb.inc(1);
                    }
                    return Ok::<(), anyhow::Error>(());
                }

                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }

                let temp_path = path.with_extension("jsonl.tmp");
                let mut file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&temp_path)
                    .await?;

                let mut logs_stream =
                    ExecutionProcessLogs::stream_log_lines_by_execution_id(&pool, &p.execution_id);
                let mut has_logs = false;
                while let Some(log_res) = logs_stream.next().await {
                    let log = log_res?;
                    has_logs = true;
                    let mut line = log;
                    if !line.ends_with('\n') {
                        line.push('\n');
                    }
                    file.write_all(line.as_bytes()).await?;
                }

                if !has_logs {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    if let Some(pb) = &pb {
                        pb.inc(1);
                    }
                    return Ok::<(), anyhow::Error>(());
                }

                file.sync_all().await?;
                tokio::fs::rename(temp_path, path).await?;

                let c = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

                if let Some(pb) = &pb {
                    pb.inc(1);
                } else if c.is_multiple_of(100) {
                    let t = total_processes.load(std::sync::atomic::Ordering::Relaxed);
                    let _ = writeln!(
                        std::io::stderr(),
                        "sqlite-migration:{}",
                        (c * 100)
                            .checked_div(t)
                            .map_or_else(|| "?".to_string(), |pct| pct.to_string())
                    );
                }

                Ok::<(), anyhow::Error>(())
            }
        })
        .buffer_unordered(64)
        .try_collect::<Vec<_>>()
        .await?;

    let _ = count_task.await;

    if let Some(pb) = pb {
        pb.finish_and_clear();
    } else {
        let _ = writeln!(std::io::stderr(), "sqlite-migration:done");
    }

    let vacuum_pb = if is_tty {
        Some(new_spinner("Compacting"))
    } else {
        let _ = writeln!(std::io::stderr(), "Compacting database...");
        None
    };

    ExecutionProcessLogs::delete_all(&pool).await?;
    sqlx::query("VACUUM").execute(&pool).await?;

    if let Some(pb) = vacuum_pb {
        pb.finish_and_clear();
    }

    let _ = writeln!(std::io::stderr(), "Database migration complete.");

    pool.close().await;

    Ok(())
}

pub async fn remove_session_process_logs(session_id: Uuid) -> Result<()> {
    let dir = utils::execution_logs::process_logs_session_dir(session_id);
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            Err(e).with_context(|| format!("remove session process logs at {}", dir.display()))
        }
    }
}

pub async fn load_raw_log_messages(pool: &SqlitePool, execution_id: Uuid) -> Option<Vec<LogMsg>> {
    if let Some(jsonl) = read_execution_logs_for_execution(pool, execution_id)
        .await
        .inspect_err(|e| {
            tracing::warn!(
                "Failed to read execution log file for execution {}: {:#}",
                execution_id,
                e
            );
        })
        .ok()
        .flatten()
    {
        let messages = utils::execution_logs::parse_log_jsonl_lossy(execution_id, &jsonl);
        if !messages.is_empty() {
            return Some(messages);
        }
    }

    let db_log_records = match ExecutionProcessLogs::find_by_execution_id(pool, execution_id).await
    {
        Ok(records) if !records.is_empty() => records,
        Ok(_) => return None,
        Err(e) => {
            tracing::error!(
                "Failed to fetch DB logs for execution {}: {}",
                execution_id,
                e
            );
            return None;
        }
    };

    match ExecutionProcessLogs::parse_logs(&db_log_records) {
        Ok(msgs) => Some(msgs),
        Err(e) => {
            tracing::error!(
                "Failed to parse DB logs for execution {}: {}",
                execution_id,
                e
            );
            None
        }
    }
}

pub async fn append_log_message(session_id: Uuid, execution_id: Uuid, msg: &LogMsg) -> Result<()> {
    let mut log_writer = ExecutionLogWriter::new_for_execution(session_id, execution_id)
        .await
        .with_context(|| format!("create log writer for execution {}", execution_id))?;
    let json_line = serde_json::to_string(msg)
        .with_context(|| format!("serialize log message for execution {}", execution_id))?;
    let mut json_line_with_newline = json_line;
    json_line_with_newline.push('\n');
    log_writer
        .append_jsonl_line(&json_line_with_newline)
        .await
        .with_context(|| format!("append log message for execution {}", execution_id))?;
    Ok(())
}

pub fn spawn_stream_raw_logs_to_storage(
    mut receiver: broadcast::Receiver<LogMsg>,
    db: DBService,
    execution_id: Uuid,
    session_id: Uuid,
    executor: Option<(String, Option<String>)>,
    mut normalization_done: oneshot::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut log_writer =
            match ExecutionLogWriter::new_for_execution(session_id, execution_id).await {
                Ok(writer) => Some(writer),
                Err(err) => {
                    tracing::error!(%execution_id, %err, "Failed to create execution log writer");
                    None
                }
            };
        let mut provider_session = None;
        let mut snapshots = HashMap::new();
        loop {
            let msg = tokio::select! {
                biased;
                result = receiver.recv() => match result {
                    Ok(msg) => msg,
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        tracing::error!(%execution_id, count, "Execution log subscriber lagged");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = &mut normalization_done => break,
            };
            match &msg {
                LogMsg::Stdout(_) | LogMsg::Stderr(_) => match serde_json::to_string(&msg) {
                    Ok(jsonl_line) => {
                        let mut jsonl_line_with_newline = jsonl_line;
                        jsonl_line_with_newline.push('\n');

                        if let Some(writer) = &mut log_writer
                            && let Err(e) = writer.append_jsonl_line(&jsonl_line_with_newline).await
                        {
                            tracing::error!(
                                "Failed to append log line for execution {}: {}",
                                execution_id,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to serialize log message for execution {}: {}",
                            execution_id,
                            e
                        );
                    }
                },
                LogMsg::SessionId(agent_session_id) => {
                    provider_session = Some(agent_session_id.to_owned());
                    if let Err(e) = CodingAgentTurn::update_agent_session_id(
                        &db.pool,
                        execution_id,
                        agent_session_id,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to update agent_session_id {} for execution process {}: {}",
                            agent_session_id,
                            execution_id,
                            e
                        );
                    }
                }
                LogMsg::MessageId(agent_message_id) => {
                    if let Err(e) = CodingAgentTurn::update_agent_message_id(
                        &db.pool,
                        execution_id,
                        agent_message_id,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to update agent_message_id {} for execution process {}: {}",
                            agent_message_id,
                            execution_id,
                            e
                        );
                    }
                }
                LogMsg::JsonPatch(patch) => {
                    if let Some((executor, model)) = &executor
                        && let Some(usage) = billing_from_patch(patch)
                    {
                        if usage.cumulative {
                            snapshots.insert(usage.model.clone(), usage);
                        } else if let Err(err) = record_usage(
                            &db.pool,
                            execution_id,
                            session_id,
                            executor,
                            model.as_deref(),
                            None,
                            &usage,
                        )
                        .await
                        {
                            tracing::error!(%execution_id, %err, "Failed to record execution usage");
                        }
                    }
                }
                LogMsg::Finished | LogMsg::Ready => {}
            }
        }
        if let Some((executor, model)) = executor {
            for usage in snapshots.values() {
                if let Err(err) = record_usage(
                    &db.pool,
                    execution_id,
                    session_id,
                    &executor,
                    model.as_deref(),
                    provider_session.as_deref(),
                    usage,
                )
                .await
                {
                    tracing::error!(%execution_id, %err, "Failed to record cumulative execution usage");
                }
            }
        }
    })
}

fn billing_from_patch(patch: &Patch) -> Option<ExecutionTokenUsage> {
    let value = match patch.0.first()? {
        PatchOperation::Add(op) => &op.value,
        PatchOperation::Replace(op) => &op.value,
        _ => return None,
    };
    let entry = value.get("content")?.get("entry_type")?;
    if entry.get("type")?.as_str()? != "token_usage_info" {
        return None;
    }
    ExecutionTokenUsage::deserialize(entry.get("billing")?).ok()
}

async fn record_usage(
    pool: &SqlitePool,
    execution_id: Uuid,
    session_id: Uuid,
    executor: &str,
    model: Option<&str>,
    provider_session: Option<&str>,
    usage: &ExecutionTokenUsage,
) -> Result<(), sqlx::Error> {
    let to_i64 = |count: u64| i64::try_from(count).unwrap_or(i64::MAX);
    let record = ExecutionUsage {
        execution_process_id: execution_id,
        session_id,
        executor,
        model: usage.model.as_deref().or(model),
        input_tokens: to_i64(usage.input_tokens),
        output_tokens: to_i64(usage.output_tokens),
        cache_read_tokens: to_i64(usage.cache_read_tokens),
        cache_write_tokens: to_i64(usage.cache_write_tokens),
        cost_usd: usage.cost_usd,
    };
    if let Some(provider_session) = provider_session {
        record.record_snapshot(pool, provider_session, usage).await
    } else {
        record.record(pool).await
    }
}

async fn read_execution_logs_for_execution(
    pool: &SqlitePool,
    execution_id: Uuid,
) -> Result<Option<String>> {
    let session_id = if let Some(process) = ExecutionProcess::find_by_id(pool, execution_id).await?
    {
        process.session_id
    } else {
        return Ok(None);
    };
    let path = process_log_file_path(session_id, execution_id);

    match tokio::fs::metadata(&path).await {
        Ok(_) => Ok(Some(read_execution_log_file(&path).await.with_context(
            || format!("read execution log file for execution {execution_id}"),
        )?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if cfg!(debug_assertions) {
                // Convenience for local development with a clone of a prod db. Read only access to prod logs.
                let prod_path =
                    process_log_file_path_in_root(&prod_asset_dir_path(), session_id, execution_id);
                match read_execution_log_file(&prod_path).await {
                    Ok(contents) => return Ok(Some(contents)),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => {
                        return Err(err).with_context(|| {
                            format!(
                                "read execution log file for execution {execution_id} from {}",
                                prod_path.display()
                            )
                        });
                    }
                }
            }
            Ok(None)
        }
        Err(e) => Err(e).with_context(|| {
            format!(
                "check execution log file exists for execution {execution_id} at {}",
                path.display()
            )
        }),
    }
}

fn new_spinner(message: &'static str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.yellow} {msg:<12.dim}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    pb.set_message(message);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}
