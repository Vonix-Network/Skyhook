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
