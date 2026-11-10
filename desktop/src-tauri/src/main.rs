// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use std::sync::Arc;
use state::AppState;

fn main() {
    let state = Arc::new(AppState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_connection_status,
            commands::get_devices,
            commands::get_activities,
            commands::send_clipboard,
            commands::send_file,
            commands::pair_device,
            commands::ring_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zima Connect application");
}
