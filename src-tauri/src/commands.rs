use crate::error::Result;
use crate::sftp::{DirEntry, Session, SessionStatus};
use crate::vault::Connection;
use crate::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

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
) -> Result<SessionStatus> {
    let conn = {
        let vault = state.vault.lock().await;
        vault
            .get(&connection_id)
            .cloned()
            .ok_or(crate::error::SkyhookError::ConnectionNotFound(connection_id))?
    };
    let session = Arc::new(Session::connect(&conn).await?);
    let cwd = session.cwd.lock().await.clone();
    let status = SessionStatus {
        id: session.id.clone(),
        connection_id: session.connection_id.clone(),
        connected: true,
        cwd,
    };
    let mut reg = state.sessions.lock().await;
    reg.insert(session);
    Ok(status)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, session_id: String) -> Result<()> {
    let session = {
        let mut reg = state.sessions.lock().await;
        reg.remove(&session_id)
    };
    if let Some(s) = session {
        s.disconnect().await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn session_status(state: State<'_, AppState>) -> Result<Vec<SessionStatus>> {
    let reg = state.sessions.lock().await;
    Ok(reg.list())
}

async fn session(state: &State<'_, AppState>, id: &str) -> Result<Arc<Session>> {
    let reg = state.sessions.lock().await;
    reg.get(id)
        .ok_or_else(|| crate::error::SkyhookError::SessionNotFound(id.into()))
}

#[tauri::command]
pub async fn list_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<Vec<DirEntry>> {
    let s = session(&state, &session_id).await?;
    s.list_dir(&path).await
}

#[tauri::command]
pub async fn read_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String> {
    let s = session(&state, &session_id).await?;
    let bytes = s.read_file(&path).await?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[tauri::command]
pub async fn write_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    content: String,
) -> Result<()> {
    let s = session(&state, &session_id).await?;
    s.write_file(&path, content.as_bytes()).await
}

#[tauri::command]
pub async fn download_file(
    state: State<'_, AppState>,
    session_id: String,
    remote: String,
    local: String,
) -> Result<u64> {
    let s = session(&state, &session_id).await?;
    s.download(&remote, &PathBuf::from(local)).await
}

#[tauri::command]
pub async fn upload_file(
    state: State<'_, AppState>,
    session_id: String,
    local: String,
    remote: String,
) -> Result<u64> {
    let s = session(&state, &session_id).await?;
    s.upload(&PathBuf::from(local), &remote).await
}

#[tauri::command]
pub async fn make_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<()> {
    let s = session(&state, &session_id).await?;
    s.mkdir(&path).await
}

#[tauri::command]
pub async fn remove_path(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<()> {
    let s = session(&state, &session_id).await?;
    s.remove(&path).await
}

#[tauri::command]
pub async fn rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> Result<()> {
    let s = session(&state, &session_id).await?;
    s.rename(&from, &to).await
}
