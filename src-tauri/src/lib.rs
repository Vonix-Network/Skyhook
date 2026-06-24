mod commands;
mod error;
mod session;
mod sftp;
pub mod transfers;
mod vault;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

/// Application-wide shared state. Held by Tauri via `manage`.
pub struct AppState {
    /// Session registry — actor-per-session SFTP layer.
    pub sessions: Arc<session::SessionManager>,
    /// Encrypted connection vault (host/credentials).
    pub vault: Arc<Mutex<vault::Vault>>,
    /// Background upload/download engine.
    pub transfers: transfers::TransferEngine,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "skyhook=info,russh=warn".into()),
        )
        .init();

    let vault = Arc::new(Mutex::new(
        vault::Vault::load_or_default().expect("vault load"),
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let sessions = Arc::new(session::SessionManager::new(app.handle().clone()));
            let transfers = transfers::TransferEngine::new(sessions.clone());
            let engine_for_setup = transfers.clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                engine_for_setup.set_app_handle(handle).await;
            });
            app.manage(AppState {
                sessions,
                vault: vault.clone(),
                transfers,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::connect,
            commands::disconnect,
            commands::reconnect,
            commands::list_dir,
            commands::stat,
            commands::walk,
            commands::read_file,
            commands::write_file,
            commands::download_file,
            commands::upload_file,
            commands::make_dir,
            commands::remove_path,
            commands::rename,
            commands::session_status,
            commands::transfer_enqueue,
            commands::transfer_pause,
            commands::transfer_resume,
            commands::transfer_cancel,
            commands::transfer_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
