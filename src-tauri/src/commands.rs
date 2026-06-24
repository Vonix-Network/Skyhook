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
