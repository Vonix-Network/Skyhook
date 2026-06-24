use std::path::PathBuf;

use tauri::State;

use crate::error::Result;
use crate::session::SessionInfo;
use crate::sftp::DirEntry;
use crate::vault::Connection;
use crate::AppState;

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> Result<Vec<Connection>> {
    let vault = state.vault.lock().await;
    Ok(vault.list())
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    connection: Connection,
) -> Result<Connection> {
    let mut vault = state.vault.lock().await;
    vault.upsert(connection)
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut vault = state.vault.lock().await;
    vault.remove(&id)
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<SessionInfo> {
    let conn = {
        let vault = state.vault.lock().await;
        vault
            .get(&connection_id)
            .cloned()
            .ok_or(crate::error::SkyhookError::ConnectionNotFound(connection_id))?
    };
    let handle = state.sessions.connect(conn).await?;
    Ok(handle.info().await)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.sessions.disconnect(&session_id).await
}

#[tauri::command]
pub async fn session_status(state: State<'_, AppState>) -> Result<Vec<SessionInfo>> {
    Ok(state.sessions.list().await)
}

#[tauri::command]
pub async fn reconnect(state: State<'_, AppState>, session_id: String) -> Result<()> {
    let h = state.sessions.require(&session_id).await?;
    h.reconnect().await
}

#[tauri::command]
pub async fn list_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<Vec<DirEntry>> {
    let h = state.sessions.require(&session_id).await?;
    h.list_dir(path).await
}

#[tauri::command]
pub async fn stat(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<DirEntry> {
    let h = state.sessions.require(&session_id).await?;
    h.stat(path).await
}

#[tauri::command]
pub async fn walk(
    state: State<'_, AppState>,
    session_id: String,
    root: String,
) -> Result<Vec<DirEntry>> {
    let h = state.sessions.require(&session_id).await?;
    h.walk(root).await
}

#[tauri::command]
pub async fn read_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String> {
    let h = state.sessions.require(&session_id).await?;
    let bytes = h.read_file(path).await?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[tauri::command]
pub async fn write_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    content: String,
) -> Result<()> {
    let h = state.sessions.require(&session_id).await?;
    h.write_file(path, content.into_bytes()).await
}

#[tauri::command]
pub async fn download_file(
    state: State<'_, AppState>,
    session_id: String,
    remote: String,
    local: String,
) -> Result<u64> {
    let h = state.sessions.require(&session_id).await?;
    h.download(remote, PathBuf::from(local)).await
}

#[tauri::command]
pub async fn upload_file(
    state: State<'_, AppState>,
    session_id: String,
    local: String,
    remote: String,
) -> Result<u64> {
    let h = state.sessions.require(&session_id).await?;
    h.upload(PathBuf::from(local), remote).await
}

#[tauri::command]
pub async fn make_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<()> {
    let h = state.sessions.require(&session_id).await?;
    h.mkdir(path).await
}

#[tauri::command]
pub async fn remove_path(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<()> {
    let h = state.sessions.require(&session_id).await?;
    h.remove(path).await
}

#[tauri::command]
pub async fn rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> Result<()> {
    let h = state.sessions.require(&session_id).await?;
    h.rename(from, to).await
}

// NOTE: Transfer engine commands were added by the sibling transfers subagent
// but referenced a `crate::transfers` module that doesn't yet exist in this
// branch. They were removed here so the session rewrite compiles cleanly. The
// parent agent will re-integrate them once the transfers module lands.

// ============================================================================
// Transfer engine commands
// ============================================================================

#[tauri::command]
pub async fn transfer_enqueue(
    state: State<'_, AppState>,
    session_id: String,
    jobs: Vec<crate::transfers::TransferRequest>,
) -> Result<Vec<String>> {
    state.transfers.enqueue(session_id, jobs).await
}

#[tauri::command]
pub async fn transfer_pause(state: State<'_, AppState>, id: String) -> Result<()> {
    state.transfers.pause(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_resume(state: State<'_, AppState>, id: String) -> Result<()> {
    state.transfers.resume(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel(state: State<'_, AppState>, id: String) -> Result<()> {
    state.transfers.cancel(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::transfers::Transfer>> {
    Ok(state.transfers.list().await)
}

// ============================================================
// Known hosts (TOFU)
// ============================================================

#[tauri::command]
pub async fn known_hosts_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::known_hosts::KnownHostEntry>> {
    let kh = state.known_hosts.lock().await;
    Ok(kh.list())
}

#[tauri::command]
pub async fn known_hosts_remove(
    state: State<'_, AppState>,
    host: String,
    port: u16,
) -> Result<()> {
    let mut kh = state.known_hosts.lock().await;
    kh.remove(&host, port)
}

#[tauri::command]
pub async fn known_hosts_trust(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    algo: String,
    fingerprint: String,
) -> Result<()> {
    let mut kh = state.known_hosts.lock().await;
    kh.add_raw(&host, port, &algo, &fingerprint)
}

// ============================================================
// Settings
// ============================================================

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<crate::settings::Settings> {
    let s = state.settings.lock().await;
    Ok(s.get())
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: crate::settings::Settings,
) -> Result<()> {
    let mut s = state.settings.lock().await;
    s.save(settings)
}

// ============================================================
// Window state / last-active connection (v0.3.0)
// ============================================================

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStateInput {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default)]
    pub maximized: bool,
}

#[tauri::command]
pub async fn save_window_state(
    state: State<'_, AppState>,
    window: WindowStateInput,
) -> Result<()> {
    let mut s = state.settings.lock().await;
    let mut cur = s.get();
    cur.window = crate::settings::WindowState {
        width: window.width,
        height: window.height,
        x: window.x,
        y: window.y,
        maximized: window.maximized,
    };
    s.save(cur)
}

#[tauri::command]
pub async fn set_last_active_connection(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<()> {
    let mut s = state.settings.lock().await;
    let mut cur = s.get();
    cur.last_active_connection_id = connection_id;
    s.save(cur)
}

// ============================================================
// Connection import/export (no secrets)
// ============================================================

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportAuthKind {
    Password,
    Key,
    Agent,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ExportConnection {
    name: String,
    host: String,
    port: u16,
    username: String,
    default_path: Option<String>,
    color: Option<String>,
    auth_kind: ExportAuthKind,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ExportBundle {
    version: u32,
    exported_at: String,
    connections: Vec<ExportConnection>,
}

#[tauri::command]
pub async fn export_connections(state: State<'_, AppState>) -> Result<String> {
    let vault = state.vault.lock().await;
    let connections: Vec<ExportConnection> = vault
        .list()
        .into_iter()
        .map(|c| ExportConnection {
            name: c.name,
            host: c.host,
            port: c.port,
            username: c.username,
            default_path: c.default_path,
            color: c.color,
            auth_kind: match c.auth {
                crate::vault::AuthMethod::Password { .. } => ExportAuthKind::Password,
                crate::vault::AuthMethod::Key { .. } => ExportAuthKind::Key,
                crate::vault::AuthMethod::Agent => ExportAuthKind::Agent,
            },
        })
        .collect();
    let bundle = ExportBundle {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        connections,
    };
    serde_json::to_string_pretty(&bundle)
        .map_err(|e| crate::error::SkyhookError::Other(format!("export serialize: {e}")))
}

#[derive(serde::Serialize)]
pub struct ImportResult {
    pub added: u32,
    pub skipped: u32,
}

#[tauri::command]
pub async fn import_connections(
    state: State<'_, AppState>,
    json: String,
) -> Result<ImportResult> {
    let bundle: ExportBundle = serde_json::from_str(&json)
        .map_err(|e| crate::error::SkyhookError::Other(format!("invalid bundle: {e}")))?;
    if bundle.version != 1 {
        return Err(crate::error::SkyhookError::Other(format!(
            "unsupported bundle version: {}",
            bundle.version
        )));
    }
    let mut vault = state.vault.lock().await;
    let existing: Vec<(String, u16, String, String)> = vault
        .list()
        .into_iter()
        .map(|c| (c.host, c.port, c.username, c.name))
        .collect();
    let mut added: u32 = 0;
    let mut skipped: u32 = 0;
    for ec in bundle.connections {
        let key = (ec.host.clone(), ec.port, ec.username.clone(), ec.name.clone());
        if existing.iter().any(|e| e == &key) {
            skipped += 1;
            continue;
        }
        let auth = match ec.auth_kind {
            ExportAuthKind::Agent => crate::vault::AuthMethod::Agent,
            ExportAuthKind::Key => crate::vault::AuthMethod::Key {
                private_key: String::new(),
                passphrase: None,
            },
            ExportAuthKind::Password => crate::vault::AuthMethod::Password {
                password: String::new(),
            },
        };
        let mut conn = crate::vault::Connection::new(
            ec.name, ec.host, ec.port, ec.username, auth,
        );
        conn.default_path = ec.default_path;
        conn.color = ec.color;
        vault.upsert(conn)?;
        added += 1;
    }
    Ok(ImportResult { added, skipped })
}

// ============================================================
// Interactive shell (PTY)
// ============================================================

#[tauri::command]
pub async fn shell_open(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<crate::session::ShellInfo> {
    let h = state.sessions.require(&session_id).await?;
    h.open_shell(cols, rows).await
}

#[tauri::command]
pub async fn shell_write(
    state: State<'_, AppState>,
    shell_id: String,
    data: Vec<u8>,
) -> Result<()> {
    let h = state.sessions.require_shell(&shell_id).await?;
    h.write(data).await
}

#[tauri::command]
pub async fn shell_resize(
    state: State<'_, AppState>,
    shell_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let h = state.sessions.require_shell(&shell_id).await?;
    h.resize(cols, rows).await
}

#[tauri::command]
pub async fn shell_close(
    state: State<'_, AppState>,
    shell_id: String,
) -> Result<()> {
    // Idempotent: missing handle means the shell already closed.
    if let Some(h) = state.sessions.get_shell(&shell_id).await {
        h.close().await?;
    }
    Ok(())
}

// ============================================================
// Local file I/O (for connection import/export, etc.)
// ============================================================

#[tauri::command]
pub async fn read_local_text_file(path: String) -> Result<String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| crate::error::SkyhookError::Io(e))
}

#[tauri::command]
pub async fn write_local_text_file(path: String, contents: String) -> Result<()> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| crate::error::SkyhookError::Io(e))?;
        }
    }
    tokio::fs::write(&path, contents)
        .await
        .map_err(|e| crate::error::SkyhookError::Io(e))
}

// ============================================================
// Agent commands (Wave 1 task A — minimal surface; siblings extend)
// ============================================================

/// Return the list of model ids supported by `provider`.
///
/// `provider` is one of `"anthropic"` or `"openai"`. Hardcoded for Anthropic;
/// sibling subagent B fills in the OpenAI branch.
#[tauri::command]
pub async fn agent_list_models(
    _state: tauri::State<'_, crate::AppState>,
    provider: String,
) -> Result<Vec<String>> {
    match provider.as_str() {
        "anthropic" => {
            // We don't need a real key just to enumerate the static list.
            let p = crate::agent::AnthropicProvider::new(String::new(), None);
            crate::agent::Provider::list_models(&p).await
        }
        "openai" => Ok(Vec::new()),
        other => Err(crate::error::SkyhookError::Agent(format!(
            "unknown provider: {other}"
        ))),
    }
}


// ============================================================
// Agent commands (Wave 1 task C — conversations, chat, approval, settings)
// ============================================================
//
// These commands depend on a `tauri::State<Arc<crate::agent::AgentRuntime>>`
// that Wave 3 wires into the Tauri builder via `.manage(...)`. The runtime
// owns the conversation store, approval gate, cancel registry, current
// provider, and persisted settings.

use std::sync::Arc;
use crate::agent::history::{Conversation, ConversationMeta};
use crate::agent::runner::{AgentRuntime, AgentRunner};
use crate::agent::tools::ApprovalMode;
use crate::agent::{AgentSettings, build_system_prompt, PromptContext, Provider as AgentProvider};
use crate::agent::anthropic::AnthropicProvider;
use crate::agent::openai::OpenAIProvider;
use crate::agent::keystore;
use crate::error::SkyhookError;

/// List every saved conversation for `connection_id`.
#[tauri::command]
pub async fn agent_list_conversations(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    connection_id: String,
) -> Result<Vec<ConversationMeta>> {
    runtime.store.list(&connection_id).await
}

/// Load one conversation in full.
#[tauri::command]
pub async fn agent_load_conversation(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    conversation_id: String,
) -> Result<Conversation> {
    runtime.store.load(&conversation_id).await
}

/// Create a fresh empty conversation pinned to a connection.
#[tauri::command]
pub async fn agent_new_conversation(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    connection_id: String,
    title: Option<String>,
    provider: String,
    model: String,
) -> Result<Conversation> {
    let title = title.unwrap_or_else(|| "New conversation".into());
    runtime
        .store
        .create(connection_id, title, provider, model)
        .await
}

/// Delete a conversation. Irreversible.
#[tauri::command]
pub async fn agent_delete_conversation(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    conversation_id: String,
) -> Result<()> {
    runtime.store.delete(&conversation_id).await
}

/// Update a conversation's title.
#[tauri::command]
pub async fn agent_rename_conversation(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    conversation_id: String,
    title: String,
) -> Result<()> {
    runtime.store.rename(&conversation_id, title).await
}

/// Helper: parse an approval-mode string into the enum.
fn parse_approval_mode(s: &str) -> ApprovalMode {
    match s {
        "manual" => ApprovalMode::Manual,
        "yolo" => ApprovalMode::Yolo,
        _ => ApprovalMode::AutoRead,
    }
}

/// Send a user message into `conversation_id` and drive the agent turn.
///
/// Streams output via `agent-*` events. Returns once the turn ends (no more
/// tool calls, `task_complete`, max-turns, or cancellation).
#[tauri::command]
pub async fn agent_send_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    conversation_id: String,
    content: String,
    approval_mode: String,
) -> Result<()> {
    // Snapshot the configured provider.
    let provider = {
        let g = runtime.provider.read().await;
        g.clone()
            .ok_or_else(|| SkyhookError::Agent("no provider configured".into()))?
    };
    let max_turns = runtime.settings.read().await.max_turns_per_invocation;

    // Load conversation + look up session for the connection.
    let mut conv = runtime.store.load(&conversation_id).await?;
    // Find an open session for this connection.
    let conn_id = conv.meta.connection_id.clone();
    // SessionManager only exposes id-based lookup; resolve via list.
    let sessions = state.sessions.clone();
    let infos = sessions.list().await;
    let session_id = infos
        .into_iter()
        .find(|i| i.connection_id == conn_id && i.state.is_live())
        .map(|i| i.id)
        .ok_or_else(|| SkyhookError::Agent("no live session for this conversation's connection".into()))?;

    // Resolve connection metadata for the system prompt.
    let (host, port, username) = {
        let vault = state.vault.lock().await;
        let c = vault
            .get(&conn_id)
            .cloned()
            .ok_or(SkyhookError::ConnectionNotFound(conn_id.clone()))?;
        (c.host, c.port, c.username)
    };
    let approval = parse_approval_mode(&approval_mode);
    let system_text = build_system_prompt(&PromptContext {
        host,
        port,
        username,
        cwd: "/".into(),
        approval_mode: approval_mode.clone(),
    });

    let runner = AgentRunner {
        provider,
        sessions,
        session_id,
        store: runtime.store.clone(),
        approvals: runtime.approvals.clone(),
        cancels: runtime.cancels.clone(),
        app,
        max_turns,
    };
    runner.run_turn(&mut conv, content, approval, system_text).await
}

/// Cancel the in-flight turn for a conversation.
#[tauri::command]
pub async fn agent_cancel(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    conversation_id: String,
) -> Result<()> {
    runtime.cancels.cancel(&conversation_id).await;
    runtime.approvals.cancel_all().await;
    Ok(())
}

/// Approve a pending tool call.
#[tauri::command]
pub async fn agent_approve_tool(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    call_id: String,
) -> Result<()> {
    runtime.approvals.approve(&call_id).await;
    Ok(())
}

/// Reject a pending tool call with a user-supplied reason.
#[tauri::command]
pub async fn agent_reject_tool(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    call_id: String,
    reason: String,
) -> Result<()> {
    runtime.approvals.reject(&call_id, reason).await;
    Ok(())
}

/// Persist an API key for `provider` to the OS keyring and (re)install the
/// provider into the runtime.
#[tauri::command]
pub async fn agent_set_api_key(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    provider: String,
    key: String,
) -> Result<()> {
    keystore::set_key(&provider, &key)?;
    let p: Arc<dyn AgentProvider> = match provider.as_str() {
        "anthropic" => Arc::new(AnthropicProvider::new(key, None)),
        "openai" => Arc::new(OpenAIProvider::new(key, None)),
        other => return Err(SkyhookError::Agent(format!("unknown provider: {other}"))),
    };
    *runtime.provider.write().await = Some(p);
    Ok(())
}

/// `true` iff an API key is currently stored for `provider`.
#[tauri::command]
pub async fn agent_has_api_key(provider: String) -> Result<bool> {
    Ok(keystore::has_key(&provider))
}

/// Remove a stored API key and drop the live provider if it matches.
#[tauri::command]
pub async fn agent_remove_api_key(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    provider: String,
) -> Result<()> {
    keystore::remove_key(&provider)?;
    let drop_it = {
        let g = runtime.provider.read().await;
        g.as_ref().map(|p| p.name() == provider).unwrap_or(false)
    };
    if drop_it {
        *runtime.provider.write().await = None;
    }
    Ok(())
}

/// Return the currently persisted agent settings.
#[tauri::command]
pub async fn agent_get_settings(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
) -> Result<AgentSettings> {
    Ok(runtime.settings.read().await.clone())
}

/// Replace the persisted agent settings.
#[tauri::command]
pub async fn agent_save_settings(
    runtime: tauri::State<'_, Arc<AgentRuntime>>,
    settings: AgentSettings,
) -> Result<()> {
    *runtime.settings.write().await = settings;
    Ok(())
}
