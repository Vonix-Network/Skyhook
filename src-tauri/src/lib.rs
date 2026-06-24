mod commands;
mod error;
mod known_hosts;
mod session;
mod settings;
mod sftp;
pub mod transfers;
mod vault;

use std::sync::Arc;
use std::time::Duration;
use tauri::{LogicalPosition, LogicalSize, Manager, WindowEvent};
use tokio::sync::Mutex;

/// Application-wide shared state. Held by Tauri via `manage`.
pub struct AppState {
    /// Session registry — actor-per-session SFTP layer.
    pub sessions: Arc<session::SessionManager>,
    /// Encrypted connection vault (host/credentials).
    pub vault: Arc<Mutex<vault::Vault>>,
    /// Background upload/download engine.
    pub transfers: transfers::TransferEngine,
    /// Trust-on-first-use known hosts store.
    pub known_hosts: Arc<Mutex<known_hosts::KnownHosts>>,
    /// User-facing settings (theme, transfer concurrency, window state).
    pub settings: Arc<Mutex<settings::SettingsStore>>,
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
    let known_hosts = Arc::new(Mutex::new(
        known_hosts::KnownHosts::load().expect("known_hosts load"),
    ));
    let settings = Arc::new(Mutex::new(
        settings::SettingsStore::load().expect("settings load"),
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
                known_hosts: known_hosts.clone(),
                settings: settings.clone(),
            });

            // Restore + persist window state.
            if let Some(window) = app.get_webview_window("main") {
                // Apply saved geometry on startup (best-effort; ignore errors).
                let saved = {
                    // Block briefly on the settings mutex (uncontested here at startup).
                    let s = tauri::async_runtime::block_on(settings.lock());
                    s.get().window
                };
                if let (Some(w), Some(h)) = (saved.width, saved.height) {
                    if w >= 200 && h >= 150 {
                        let _ = window.set_size(LogicalSize::new(w as f64, h as f64));
                    }
                }
                if let (Some(x), Some(y)) = (saved.x, saved.y) {
                    let _ = window.set_position(LogicalPosition::new(x as f64, y as f64));
                }
                if saved.maximized {
                    let _ = window.maximize();
                }

                // Debounced save loop.
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<bool>();
                let settings_for_save = settings.clone();
                let window_for_save = window.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(force) = rx.recv().await {
                        // Coalesce bursts: drain with a debounce window unless forced.
                        if !force {
                            let debounce = tokio::time::sleep(Duration::from_millis(500));
                            tokio::pin!(debounce);
                            loop {
                                tokio::select! {
                                    _ = &mut debounce => break,
                                    msg = rx.recv() => {
                                        match msg {
                                            Some(true) => break, // forced save (close)
                                            Some(false) => continue,
                                            None => return,
                                        }
                                    }
                                }
                            }
                        }
                        let scale = window_for_save.scale_factor().unwrap_or(1.0);
                        let size = window_for_save.inner_size().ok();
                        let pos = window_for_save.outer_position().ok();
                        let maximized = window_for_save.is_maximized().unwrap_or(false);
                        let mut store = settings_for_save.lock().await;
                        let mut cur = store.get();
                        if !maximized {
                            if let Some(sz) = size {
                                let logical = sz.to_logical::<f64>(scale);
                                cur.window.width = Some(logical.width.round() as u32);
                                cur.window.height = Some(logical.height.round() as u32);
                            }
                            if let Some(p) = pos {
                                let logical = p.to_logical::<f64>(scale);
                                cur.window.x = Some(logical.x.round() as i32);
                                cur.window.y = Some(logical.y.round() as i32);
                            }
                        }
                        cur.window.maximized = maximized;
                        if let Err(e) = store.save(cur) {
                            tracing::warn!("window state save failed: {e:?}");
                        }
                    }
                });

                let tx_for_events = tx.clone();
                window.on_window_event(move |ev| match ev {
                    WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
                        let _ = tx_for_events.send(false);
                    }
                    WindowEvent::CloseRequested { .. } => {
                        let _ = tx_for_events.send(true);
                    }
                    _ => {}
                });
            }

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
            commands::known_hosts_list,
            commands::known_hosts_remove,
            commands::known_hosts_trust,
            commands::get_settings,
            commands::save_settings,
            commands::shell_open,
            commands::shell_write,
            commands::shell_resize,
            commands::shell_close,
            commands::set_last_active_connection,
            commands::export_connections,
            commands::import_connections,
            commands::save_window_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
