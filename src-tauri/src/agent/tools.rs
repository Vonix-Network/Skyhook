//! Agent tool surface — the set of operations the LLM can invoke against the
//! current SFTP/SSH session.
//!
//! Tools are modelled as a single Rust enum ([`ToolCall`]) that is the
//! canonical post-parse form of the model's `tool_use` block. Dispatching a
//! call routes through [`dispatch_tool`] to the live [`crate::session::SessionHandle`].
//!
//! `shell_exec` opens a fresh ephemeral PTY for each invocation rather than
//! reusing one of the user's interactive shell tabs.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Listener};
use tokio::sync::oneshot;

use crate::agent::provider::ToolSchema;
use crate::error::{Result, SkyhookError};
use crate::session::SessionManager;

/// Default cap on bytes returned by `sftp_read_file` when the model omits
/// `max_bytes`.
const DEFAULT_READ_BYTES: usize = 100_000;
/// Default recursion depth for `sftp_walk`.
const DEFAULT_WALK_DEPTH: usize = 5;
/// Default `shell_exec` timeout (seconds).
const DEFAULT_SHELL_TIMEOUT: u64 = 60;
/// Cap on tool-output payload returned to the model (~32 KiB).
const TOOL_OUTPUT_CAP: usize = 32_000;
/// Sentinel echoed after a `shell_exec` command so we can detect completion
/// + capture the exit code.
const SHELL_EXIT_SENTINEL: &str = "__SKYHOOK_AGENT_EXIT_";

/// Approval requirement mode for the current run.
///
/// Per the v0.6.0 plan, reads are always free; writes/execs depend on mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// Confirm every write and exec.
    Manual,
    /// Reads always allowed; writes/execs confirmed. Default.
    AutoRead,
    /// Everything goes — no prompts.
    Yolo,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        ApprovalMode::AutoRead
    }
}

/// One tool invocation issued by the model.
///
/// Variant names use snake_case to match the over-the-wire tool name used by
/// both Anthropic and OpenAI tool definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name", content = "input", rename_all = "snake_case")]
pub enum ToolCall {
    /// `sftp_list_dir(path)` — list one remote directory.
    SftpListDir { path: String },
    /// `sftp_read_file(path, max_bytes?)` — read a small remote text file.
    SftpReadFile {
        path: String,
        #[serde(default)]
        max_bytes: Option<usize>,
    },
    /// `sftp_stat(path)` — stat a remote path.
    SftpStat { path: String },
    /// `sftp_walk(root, max_depth?)` — recursive directory listing.
    SftpWalk {
        root: String,
        #[serde(default)]
        max_depth: Option<usize>,
    },
    /// `sftp_write_file(path, content)` — overwrite a remote file (UTF-8 text).
    SftpWriteFile { path: String, content: String },
    /// `sftp_make_dir(path)` — create a remote directory.
    SftpMakeDir { path: String },
    /// `sftp_remove(path)` — delete a remote file or empty directory.
    SftpRemove { path: String },
    /// `sftp_rename(from, to)` — rename/move a remote path.
    SftpRename { from: String, to: String },
    /// `sftp_download(remote, local)` — copy remote → user's local machine.
    SftpDownload { remote: String, local: String },
    /// `sftp_upload(local, remote)` — copy local → remote.
    SftpUpload { local: String, remote: String },
    /// `shell_exec(command, timeout_s?)` — run a non-interactive shell command.
    ShellExec {
        command: String,
        #[serde(default)]
        timeout_s: Option<u64>,
    },
    /// `task_complete(summary)` — signal end of turn.
    TaskComplete { summary: String },
}

impl ToolCall {
    /// The user-facing snake_case tool name (matches the schema name).
    pub fn name(&self) -> &'static str {
        match self {
            ToolCall::SftpListDir { .. } => "sftp_list_dir",
            ToolCall::SftpReadFile { .. } => "sftp_read_file",
            ToolCall::SftpStat { .. } => "sftp_stat",
            ToolCall::SftpWalk { .. } => "sftp_walk",
            ToolCall::SftpWriteFile { .. } => "sftp_write_file",
            ToolCall::SftpMakeDir { .. } => "sftp_make_dir",
            ToolCall::SftpRemove { .. } => "sftp_remove",
            ToolCall::SftpRename { .. } => "sftp_rename",
            ToolCall::SftpDownload { .. } => "sftp_download",
            ToolCall::SftpUpload { .. } => "sftp_upload",
            ToolCall::ShellExec { .. } => "shell_exec",
            ToolCall::TaskComplete { .. } => "task_complete",
        }
    }

    /// Whether this call needs explicit user approval given the active mode.
    ///
    /// `task_complete` and pure reads never require approval. In `Yolo` mode
    /// nothing requires approval. In `AutoRead` mode only mutating tools do.
    /// In `Manual` mode every mutating tool does (reads still pass through).
    pub fn needs_approval(&self, mode: ApprovalMode) -> bool {
        if matches!(mode, ApprovalMode::Yolo) {
            return false;
        }
        match self {
            ToolCall::SftpListDir { .. }
            | ToolCall::SftpReadFile { .. }
            | ToolCall::SftpStat { .. }
            | ToolCall::SftpWalk { .. }
            | ToolCall::TaskComplete { .. } => false,
            ToolCall::SftpWriteFile { .. }
            | ToolCall::SftpMakeDir { .. }
            | ToolCall::SftpRemove { .. }
            | ToolCall::SftpRename { .. }
            | ToolCall::SftpDownload { .. }
            | ToolCall::SftpUpload { .. }
            | ToolCall::ShellExec { .. } => true,
        }
    }

    /// Short one-line preview suitable for the approval card.
    pub fn preview(&self) -> String {
        match self {
            ToolCall::SftpListDir { path } => format!("sftp_list_dir: {path}"),
            ToolCall::SftpReadFile { path, max_bytes } => {
                let mb = max_bytes.unwrap_or(DEFAULT_READ_BYTES);
                format!("sftp_read_file: {path} (≤ {mb} bytes)")
            }
            ToolCall::SftpStat { path } => format!("sftp_stat: {path}"),
            ToolCall::SftpWalk { root, max_depth } => {
                format!(
                    "sftp_walk: {root} (depth {})",
                    max_depth.unwrap_or(DEFAULT_WALK_DEPTH)
                )
            }
            ToolCall::SftpWriteFile { path, content } => {
                format!("sftp_write_file: {path} ({} bytes)", content.len())
            }
            ToolCall::SftpMakeDir { path } => format!("sftp_make_dir: {path}"),
            ToolCall::SftpRemove { path } => format!("sftp_remove: {path}"),
            ToolCall::SftpRename { from, to } => format!("sftp_rename: {from} → {to}"),
            ToolCall::SftpDownload { remote, local } => {
                format!("sftp_download: {remote} → {local}")
            }
            ToolCall::SftpUpload { local, remote } => {
                format!("sftp_upload: {local} → {remote}")
            }
            ToolCall::ShellExec { command, timeout_s } => {
                let t = timeout_s.unwrap_or(DEFAULT_SHELL_TIMEOUT);
                let short = if command.len() > 120 {
                    format!("{}…", &command[..120])
                } else {
                    command.clone()
                };
                format!("shell_exec ({}s): {}", t, short)
            }
            ToolCall::TaskComplete { summary } => format!("task_complete: {summary}"),
        }
    }

    /// Parse a `(name, input_json)` pair (as produced by the model's stream)
    /// into a [`ToolCall`].
    pub fn from_name_and_input(name: &str, input: Value) -> Result<Self> {
        // Re-serialise into the tag/content form `serde` expects.
        let wire = json!({ "name": name, "input": input });
        serde_json::from_value(wire).map_err(|e| {
            SkyhookError::Other(format!("bad tool call {name}: {e}"))
        })
    }
}

/// All tool schemas advertised to the model.
///
/// Built fresh per call; cheap (small static JSON). Callers may cache.
pub fn all_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "sftp_list_dir".into(),
            description: "List entries in a remote directory. Returns one entry per line: TYPE NAME SIZE.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute remote directory path." }
                },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "sftp_read_file".into(),
            description: "Read a remote text file. max_bytes defaults to 100000 if unset.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "sftp_stat".into(),
            description: "Stat a remote path. Returns type, size, mtime, mode.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "sftp_walk".into(),
            description: "Recursively list entries under a root. max_depth defaults to 5.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "root": { "type": "string" },
                    "max_depth": { "type": "integer", "minimum": 1 }
                },
                "required": ["root"]
            }),
        },
        ToolSchema {
            name: "sftp_write_file".into(),
            description: "Overwrite a remote file with the given content. Confirm with the user first if the file exists.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolSchema {
            name: "sftp_make_dir".into(),
            description: "Create a directory on the remote (mkdir -p style).".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "sftp_remove".into(),
            description: "Delete a file or empty directory on the remote.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        },
        ToolSchema {
            name: "sftp_rename".into(),
            description: "Rename or move a remote path.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to":   { "type": "string" }
                },
                "required": ["from", "to"]
            }),
        },
        ToolSchema {
            name: "sftp_download".into(),
            description: "Download a remote file to a local path on the user's machine.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "remote": { "type": "string" },
                    "local":  { "type": "string" }
                },
                "required": ["remote", "local"]
            }),
        },
        ToolSchema {
            name: "sftp_upload".into(),
            description: "Upload a local file from the user's machine to the remote.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local":  { "type": "string" },
                    "remote": { "type": "string" }
                },
                "required": ["local", "remote"]
            }),
        },
        ToolSchema {
            name: "shell_exec".into(),
            description: "Run a shell command on the remote and return stdout+stderr+exit_code. timeout_s defaults to 60.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command":  { "type": "string" },
                    "timeout_s": { "type": "integer", "minimum": 1 }
                },
                "required": ["command"]
            }),
        },
        ToolSchema {
            name: "task_complete".into(),
            description: "Call this when you have finished the user's request. Provide a one-line summary.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"]
            }),
        },
    ]
}

/// Execute one tool call against a live session and return a string suitable
/// for inclusion in a `tool_result` message back to the model.
///
/// Errors are returned as `Err`; callers (the runner) typically render them
/// into a `tool_result` with `is_error: true` rather than aborting the turn.
pub async fn dispatch_tool(
    sessions: &Arc<SessionManager>,
    app: &AppHandle,
    session_id: &str,
    call: &ToolCall,
) -> Result<String> {
    let handle = sessions.require(session_id).await?;

    let raw = match call {
        ToolCall::SftpListDir { path } => {
            let entries = handle.list_dir(path.clone()).await?;
            format_entries(&entries)
        }
        ToolCall::SftpReadFile { path, max_bytes } => {
            let bytes = handle.read_file(path.clone()).await?;
            let cap = max_bytes.unwrap_or(DEFAULT_READ_BYTES).min(bytes.len());
            let slice = &bytes[..cap];
            let text = String::from_utf8_lossy(slice).into_owned();
            if bytes.len() > cap {
                format!("{text}\n[... truncated, {} bytes total]", bytes.len())
            } else {
                text
            }
        }
        ToolCall::SftpStat { path } => {
            let e = handle.stat(path.clone()).await?;
            format!(
                "type={} name={} path={} size={} modified={} mode={}",
                if e.is_dir { "dir" } else if e.is_symlink { "symlink" } else { "file" },
                e.name,
                e.path,
                e.size,
                e.modified.map(|t| t.to_string()).unwrap_or_else(|| "?".into()),
                e.mode.map(|m| format!("{:o}", m)).unwrap_or_else(|| "?".into()),
            )
        }
        ToolCall::SftpWalk { root, max_depth } => {
            let entries = handle.walk(root.clone()).await?;
            let depth = max_depth.unwrap_or(DEFAULT_WALK_DEPTH);
            // Filter by path depth relative to root.
            let base_segs = root.trim_end_matches('/').matches('/').count();
            let filtered: Vec<_> = entries
                .into_iter()
                .filter(|e| {
                    let segs = e.path.trim_end_matches('/').matches('/').count();
                    segs.saturating_sub(base_segs) <= depth
                })
                .collect();
            format_entries(&filtered)
        }
        ToolCall::SftpWriteFile { path, content } => {
            handle
                .write_file(path.clone(), content.as_bytes().to_vec())
                .await?;
            format!("wrote {} bytes to {}", content.len(), path)
        }
        ToolCall::SftpMakeDir { path } => {
            handle.mkdir(path.clone()).await?;
            format!("created directory {path}")
        }
        ToolCall::SftpRemove { path } => {
            handle.remove(path.clone()).await?;
            format!("removed {path}")
        }
        ToolCall::SftpRename { from, to } => {
            handle.rename(from.clone(), to.clone()).await?;
            format!("renamed {from} -> {to}")
        }
        ToolCall::SftpDownload { remote, local } => {
            let n = handle.download(remote.clone(), local.into()).await?;
            format!("downloaded {n} bytes: {remote} -> {local}")
        }
        ToolCall::SftpUpload { local, remote } => {
            let n = handle.upload(local.into(), remote.clone()).await?;
            format!("uploaded {n} bytes: {local} -> {remote}")
        }
        ToolCall::ShellExec { command, timeout_s } => {
            let timeout = Duration::from_secs(timeout_s.unwrap_or(DEFAULT_SHELL_TIMEOUT));
            run_shell_exec(sessions, app, session_id, command, timeout).await?
        }
        ToolCall::TaskComplete { summary } => format!("task_complete: {summary}"),
    };

    Ok(truncate_output(raw))
}

/// Render a directory-entry slice as one entry per line.
fn format_entries(entries: &[crate::sftp::DirEntry]) -> String {
    if entries.is_empty() {
        return "(empty)".into();
    }
    let mut out = String::new();
    for e in entries {
        let kind = if e.is_dir {
            "DIR "
        } else if e.is_symlink {
            "LINK"
        } else {
            "FILE"
        };
        out.push_str(&format!("{kind} {} {}\n", e.path, e.size));
    }
    out.pop();
    out
}

fn truncate_output(mut s: String) -> String {
    if s.len() > TOOL_OUTPUT_CAP {
        s.truncate(TOOL_OUTPUT_CAP);
        s.push_str("\n[... truncated]");
    }
    s
}

/// Run `command` in a fresh ephemeral PTY shell, capture output until the
/// sentinel + exit marker (or timeout), and return a formatted block.
///
/// Implementation notes: the existing shell layer emits output on the
/// `shell-output` Tauri event rather than returning it via channel. We listen
/// on that event filtered by our newly-opened shell id, accumulate until we
/// see `__SKYHOOK_AGENT_EXIT_<code>__`, then close. This is admittedly
/// roundabout but avoids touching the session actor in this wave.
async fn run_shell_exec(
    sessions: &Arc<SessionManager>,
    app: &AppHandle,
    session_id: &str,
    command: &str,
    timeout: Duration,
) -> Result<String> {
    let handle = sessions.require(session_id).await?;
    let shell_info = handle.open_shell(120, 30).await?;
    let shell_id = shell_info.id.clone();

    // Subscribe to shell-output before sending the command.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let want_id = shell_id.clone();
    let listener = app.listen("shell-output", move |event| {
        // Payload: { shell_id, data }
        let payload = event.payload();
        // Best-effort parse; ignore malformed events.
        if let Ok(v) = serde_json::from_str::<Value>(payload) {
            if v.get("shell_id").and_then(|s| s.as_str()) == Some(&want_id) {
                if let Some(data) = v.get("data").and_then(|s| s.as_str()) {
                    let _ = tx.send(data.to_string());
                }
            }
        }
    });

    let shell_handle = sessions.require_shell(&shell_id).await?;
    // Compose the command line. The sentinel echo captures the exit status.
    let line = format!(
        "{command}\necho {sentinel}$?__\n",
        sentinel = SHELL_EXIT_SENTINEL
    );
    shell_handle.write(line.into_bytes()).await?;

    let mut buf = String::new();
    let mut exit_code: Option<i32> = None;
    let deadline = tokio::time::Instant::now() + timeout;

    while exit_code.is_none() {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let chunk = tokio::time::timeout(deadline - now, rx.recv()).await;
        match chunk {
            Ok(Some(s)) => {
                buf.push_str(&s);
                if let Some(code) = find_exit(&buf) {
                    exit_code = Some(code);
                    break;
                }
            }
            Ok(None) => break,    // sender dropped — shell closed
            Err(_) => break,      // timeout
        }
    }

    app.unlisten(listener);
    let _ = shell_handle.close().await;

    let cleaned = strip_sentinel_line(&buf);
    let (status_line, formatted_exit) = match exit_code {
        Some(c) => ("exit", c.to_string()),
        None => ("timeout", "-".into()),
    };
    Ok(format!(
        "$ {command}\n{cleaned}\n[{status_line}: {formatted_exit}]"
    ))
}

/// Look for `__SKYHOOK_AGENT_EXIT_<n>__` in `buf` and return `<n>`.
fn find_exit(buf: &str) -> Option<i32> {
    let needle = SHELL_EXIT_SENTINEL;
    let pos = buf.find(needle)?;
    let tail = &buf[pos + needle.len()..];
    let end = tail.find("__")?;
    tail[..end].trim().parse::<i32>().ok()
}

/// Remove any line containing the exit sentinel from the captured output.
fn strip_sentinel_line(buf: &str) -> String {
    buf.lines()
        .filter(|l| !l.contains(SHELL_EXIT_SENTINEL))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Unused-but-exported helper so a future caller can wait on a oneshot for
/// shell output without round-tripping events. Kept as a stub.
#[allow(dead_code)]
fn _channel_stub() -> (oneshot::Sender<()>, oneshot::Receiver<()>) {
    oneshot::channel()
}
