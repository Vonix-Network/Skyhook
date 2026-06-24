//! Interactive SSH shell (PTY) channels attached to a [`SessionActor`].
//!
//! # Architecture
//!
//! Each shell owns its own `russh::Channel<client::Msg>` and its own Tokio
//! task. That task drives `channel.wait()` in a loop, forwarding inbound
//! bytes to the frontend as `shell-output` events, while a mpsc rx half of
//! the task also drains user input ([`ShellInput`]) and writes it to the
//! channel.
//!
//! Shells run **outside** the [`SessionActor`]'s serial op queue: once the
//! actor opens the PTY and returns a [`ShellHandle`], all further write /
//! resize / close calls hit the handle directly. This keeps interactive
//! latency low even while the SFTP channel is busy walking a large tree.
//!
//! # Lifecycle vs. parent session
//!
//! * When the parent transport dies (`Degraded` or `Closed` transitions),
//!   the actor force-closes every child shell. Interactive shells can NOT
//!   be transparently resumed across an SSH reconnect — the remote PTY
//!   process is gone — so the user is expected to open a fresh shell once
//!   the session reconnects.
//! * When a shell's own task observes `Eof` / `Close` / `ExitStatus`, it
//!   emits `shell-closed` and unregisters itself.

use std::collections::HashMap;
use std::sync::Arc;

use russh::client;
use russh::ChannelMsg;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

use crate::error::{Result, SkyhookError};

/// Default terminal type advertised to the remote.
const DEFAULT_TERM: &str = "xterm-256color";

/// Capacity of the per-shell input queue. 256 is plenty — xterm produces
/// at most a few dozen keystrokes/sec even in a paste storm.
const SHELL_INPUT_QUEUE: usize = 256;

/// Public snapshot returned to the frontend when a shell is opened.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInfo {
    /// Unique shell id (UUID).
    pub id: String,
    /// Parent session id this shell belongs to.
    pub session_id: String,
}

/// Internal control message routed from a [`ShellHandle`] to its task.
enum ShellInput {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

/// Cheap, cloneable handle to one running shell.
#[derive(Clone)]
pub struct ShellHandle {
    /// Shell id (UUID).
    pub id: String,
    /// Parent session id.
    pub session_id: String,
    tx: mpsc::Sender<ShellInput>,
}

impl ShellHandle {
    /// Forward raw bytes (typically xterm.js keystrokes) to the remote PTY.
    pub async fn write(&self, data: Vec<u8>) -> Result<()> {
        self.tx
            .send(ShellInput::Data(data))
            .await
            .map_err(|_| SkyhookError::Other("shell task stopped".into()))
    }

    /// Inform the remote PTY of a window-size change.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.tx
            .send(ShellInput::Resize { cols, rows })
            .await
            .map_err(|_| SkyhookError::Other("shell task stopped".into()))
    }

    /// Ask the shell task to close cleanly. Idempotent; the task will emit
    /// `shell-closed` exactly once regardless of how many times this is
    /// called (subsequent sends after the task exits are best-effort).
    pub async fn close(&self) -> Result<()> {
        // Best-effort — if the task already exited, the receiver is dropped
        // and this returns Err which we treat as success (already closed).
        let _ = self.tx.send(ShellInput::Close).await;
        Ok(())
    }
}

/// Shared map of `shell_id -> ShellHandle` used by both the manager (for
/// command routing) and the parent [`SessionActor`] (for teardown).
pub type ShellRegistry = Arc<Mutex<HashMap<String, ShellHandle>>>;

/// Build an empty shell registry.
pub fn new_registry() -> ShellRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Open a new interactive shell on `ssh`. Returns a [`ShellHandle`] once
/// the PTY + shell requests have succeeded; spawns a background task that
/// drives the channel until close.
///
/// On natural close (`Eof` / `Close` / `ExitStatus` from the server), the
/// task emits a `shell-closed` event and removes itself from `registry`.
pub async fn open_shell(
    app: AppHandle,
    ssh: &client::Handle<super::actor::ClientHandler>,
    session_id: String,
    cols: u16,
    rows: u16,
    registry: ShellRegistry,
) -> Result<ShellHandle> {
    let channel = ssh
        .channel_open_session()
        .await
        .map_err(|e| SkyhookError::Ssh(format!("shell channel: {e}")))?;

    channel
        .request_pty(
            true,
            DEFAULT_TERM,
            cols as u32,
            rows as u32,
            0,
            0,
            &[],
        )
        .await
        .map_err(|e| SkyhookError::Ssh(format!("request_pty: {e}")))?;

    channel
        .request_shell(true)
        .await
        .map_err(|e| SkyhookError::Ssh(format!("request_shell: {e}")))?;

    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel::<ShellInput>(SHELL_INPUT_QUEUE);

    let handle = ShellHandle {
        id: id.clone(),
        session_id: session_id.clone(),
        tx,
    };

    {
        let mut g = registry.lock().await;
        g.insert(id.clone(), handle.clone());
    }

    tokio::spawn(run_shell(
        app,
        channel,
        id,
        session_id,
        rx,
        registry,
    ));

    Ok(handle)
}

/// Main loop for one shell. Owns the channel for its full lifetime.
async fn run_shell(
    app: AppHandle,
    mut channel: russh::Channel<client::Msg>,
    shell_id: String,
    _session_id: String,
    mut rx: mpsc::Receiver<ShellInput>,
    registry: ShellRegistry,
) {
    let mut exit_code: Option<i32> = None;

    loop {
        tokio::select! {
            biased;

            input = rx.recv() => {
                match input {
                    Some(ShellInput::Data(bytes)) => {
                        if let Err(e) = channel.data(&bytes[..]).await {
                            tracing::warn!(shell = %shell_id, error = %e, "shell write failed");
                            // Transport is gone — fall through to close.
                            break;
                        }
                    }
                    Some(ShellInput::Resize { cols, rows }) => {
                        if let Err(e) = channel
                            .window_change(cols as u32, rows as u32, 0, 0)
                            .await
                        {
                            tracing::warn!(shell = %shell_id, error = %e, "shell resize failed");
                        }
                    }
                    Some(ShellInput::Close) | None => {
                        let _ = channel.eof().await;
                        let _ = channel.close().await;
                        break;
                    }
                }
            }

            msg = channel.wait() => {
                let Some(msg) = msg else {
                    // Channel ended without an explicit Close — treat as closed.
                    break;
                };
                match msg {
                    ChannelMsg::Data { data } => {
                        emit_output(&app, &shell_id, &data);
                    }
                    ChannelMsg::ExtendedData { data, ext: _ } => {
                        // stderr (ext == 1) and any other ext channels: surface
                        // as terminal output. xterm tolerates raw bytes.
                        emit_output(&app, &shell_id, &data);
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status as i32);
                        // ExitStatus is informational; server usually sends
                        // Eof/Close right after. Keep looping to drain.
                    }
                    ChannelMsg::Eof => {
                        // Don't break yet — close will follow. But if it
                        // doesn't, we'd hang; force teardown.
                        let _ = channel.close().await;
                    }
                    ChannelMsg::Close => {
                        break;
                    }
                    _ => {
                        // Ignore unrelated control msgs (Success/Failure for
                        // earlier pty/shell requests, WindowAdjusted, ...).
                    }
                }
            }
        }
    }

    finalize(&app, &shell_id, exit_code, &registry).await;
}

fn emit_output(app: &AppHandle, shell_id: &str, data: &[u8]) {
    // xterm.js handles raw bytes via `Terminal.write(string|Uint8Array)`.
    // We ship the bytes as a UTF-8 lossy string for JSON compatibility;
    // invalid sequences become U+FFFD which xterm tolerates.
    let payload = serde_json::json!({
        "shell_id": shell_id,
        "data": String::from_utf8_lossy(data),
    });
    let _ = app.emit("shell-output", payload);
}

async fn finalize(
    app: &AppHandle,
    shell_id: &str,
    exit_code: Option<i32>,
    registry: &ShellRegistry,
) {
    {
        let mut g = registry.lock().await;
        g.remove(shell_id);
    }
    let payload = serde_json::json!({
        "shell_id": shell_id,
        "exit_code": exit_code,
    });
    let _ = app.emit("shell-closed", payload);
}
