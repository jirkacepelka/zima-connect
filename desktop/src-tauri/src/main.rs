// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use std::sync::Arc;

use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_notification::NotificationExt;
use zima_connect_net::{Event, Node, NodeConfig};

use state::AppState;

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "This computer".to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            // ZC_DATA_DIR lets you run several instances on one machine (testing).
            let data_dir = match std::env::var_os("ZC_DATA_DIR") {
                Some(d) => std::path::PathBuf::from(d),
                None => app.path().app_data_dir()?,
            };
            let download_dir = app
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.clone())
                .join("Zima Connect");
            let cfg = NodeConfig {
                data_dir,
                download_dir,
                device_name: device_name(),
                platform: std::env::consts::OS.to_string(),
                internet: true,
                hub: None,
            };
            let node = tauri::async_runtime::block_on(Node::start(cfg))
                .map_err(|e| format!("could not start P2P node: {e:#}"))?;
            let state = Arc::new(AppState::new(node.clone()));
            app.manage(state.clone());

            // Turn incoming P2P events into clipboard writes, notifications and activity entries.
            let handle = app.handle().clone();
            let mut rx = node.subscribe();
            tauri::async_runtime::spawn(async move {
                while let Ok(ev) = rx.recv().await {
                    match ev {
                        Event::Clipboard { from_name, text, .. } => {
                            let _ = handle.clipboard().write_text(text.clone());
                            state.log(
                                "clipboard",
                                "Clipboard received",
                                format!("{} characters from {from_name}", text.chars().count()),
                                true,
                                None,
                            );
                        }
                        Event::FileReceived { from_name, name, path, size, .. } => {
                            state.log(
                                "file",
                                format!("Received {name}"),
                                format!("from {from_name} ({} KB)", size / 1024),
                                true,
                                Some(path),
                            );
                            let _ = handle
                                .notification()
                                .builder()
                                .title("File received")
                                .body(format!("{name} from {from_name}"))
                                .show();
                        }
                        Event::Ring { from_name, .. } => {
                            state.log("ring", "Found by device", from_name.clone(), true, None);
                            *state.ringing.lock().unwrap() = Some((from_name.clone(), std::time::Instant::now()));
                            if let Some(w) = handle.get_webview_window("main") {
                                let _ = w.unminimize();
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                            let _ = handle
                                .notification()
                                .builder()
                                .title("Zima Connect")
                                .body(format!("{from_name} is looking for this computer"))
                                .show();
                        }
                        Event::RingStop { .. } => {
                            *state.ringing.lock().unwrap() = None;
                        }
                        Event::Notification { from_name, app, title, body, .. } => {
                            state.log("notification", format!("{app}: {title}"), format!("from {from_name}"), true, None);
                            let _ = handle.notification().builder().title(format!("{title} · {from_name}")).body(body).show();
                        }
                        Event::Joined { household } => {
                            let name = state.node.household_info().map(|i| i.me.name).unwrap_or_default();
                            state.log("pair", format!("Joined {household}"), format!("as {name}"), true, None);
                        }
                        Event::HouseholdUpdated { .. } => {}
                        Event::Paired { peer } => {
                            state.log("pair", format!("Paired with {}", peer.name), peer.platform, true, None);
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_identity,
            commands::get_contacts,
            commands::get_household,
            commands::join_household,
            commands::leave_household,
            commands::household_admin,
            commands::get_activities,
            commands::create_pairing_code,
            commands::pair_device,
            commands::unpair_device,
            commands::send_clipboard,
            commands::send_file,
            commands::ring_device,
            commands::ring_stop_device,
            commands::get_ring_state,
            commands::stop_ring,
            commands::reveal_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zima Connect application");
}
