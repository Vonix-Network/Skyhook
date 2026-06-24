mod commands;
mod error;
mod sftp;
mod vault;

use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub sessions: Arc<Mutex<sftp::SessionRegistry>>,
    pub vault: Arc<Mutex<vault::Vault>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "skyhook=info,russh=warn".into()),
        )
        .init();

    let state = AppState {
        sessions: Arc::new(Mutex::new(sftp::SessionRegistry::new())),
        vault: Arc::new(Mutex::new(
            vault::Vault::load_or_default().expect("vault load"),
        )),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::connect,
            commands::disconnect,
            commands::list_dir,
            commands::read_file,
            commands::write_file,
            commands::download_file,
            commands::upload_file,
            commands::make_dir,
            commands::remove_path,
            commands::rename,
            commands::session_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
