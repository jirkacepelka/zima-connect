// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod remote;
mod settings;
mod state;
mod updater;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
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

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn main() {
    let mut builder = tauri::Builder::default();
    // One copy per computer (the tray copy is reused). Several copies on purpose: ZC_DATA_DIR.
    if std::env::var_os("ZC_DATA_DIR").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)));
    }
    builder
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
            let prefs = settings::load(&cfg.data_dir);
            let first_run = !settings::path(&cfg.data_dir).exists();
            let data_dir_copy = cfg.data_dir.clone();
            // Previews of shelf files are served from the cache folder through the asset protocol.
            let _ = app.asset_protocol_scope().allow_directory(cfg.data_dir.join("shelf-cache"), true);
            let node = tauri::async_runtime::block_on(Node::start(cfg))
                .map_err(|e| format!("could not start P2P node: {e:#}"))?;
            if first_run {
                settings::save(&data_dir_copy, &prefs);
                if prefs.autostart {
                    let _ = app.autolaunch().enable();
                }
            }
            let state = Arc::new(AppState::new(node.clone(), data_dir_copy, prefs));
            app.manage(state.clone());

            // Tray: Zima Connect keeps running in the background.
            let open = MenuItem::with_id(app, "open", "Open Zima Connect", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Zima Connect")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Started by the system at login: stay in the tray. Otherwise show the window.
            let hidden = std::env::args().any(|a| a == "--minimized");
            if !hidden {
                show_main(app.handle());
            }

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
                        Event::SmsReceived { from_name, address, name, body, .. } => {
                            let who = name.unwrap_or(address);
                            state.log("sms", format!("Text message from {who}"), format!("on {from_name}"), true, None);
                            let _ = handle.notification().builder().title(format!("{who} · {from_name}")).body(body).show();
                            let _ = handle.emit("sms-received", ());
                        }
                        Event::SmsRequest { .. } => {}
                        Event::CallIncoming { call_id, from, from_name } => {
                            state.log("call", "Incoming call", from_name.clone(), true, None);
                            show_main(&handle);
                            let _ = handle
                                .notification()
                                .builder()
                                .title("Zima Connect")
                                .body(format!("{from_name} is calling"))
                                .show();
                            let _ = handle.emit("call-event", serde_json::json!({ "kind": "incoming", "call_id": call_id, "from": from, "peer": from_name }));
                        }
                        Event::CallState { call_id, state: st, peer_name } => {
                            if st == "connected" {
                                // Pull the other side's audio and hand it to the window to play.
                                let node = state.node.clone();
                                let h = handle.clone();
                                tauri::async_runtime::spawn(async move {
                                    use base64::Engine;
                                    while let Some(d) = node.call_recv(call_id).await {
                                        let _ = h.emit("call-audio", (call_id, base64::engine::general_purpose::STANDARD.encode(d)));
                                    }
                                });
                            }
                            if matches!(st.as_str(), "ended" | "declined" | "busy" | "unreachable") {
                                state.log("call", format!("Call {st}"), peer_name.clone(), st == "ended", None);
                            }
                            let _ = handle.emit("call-event", serde_json::json!({ "kind": "state", "call_id": call_id, "state": st, "peer": peer_name }));
                        }
                        Event::OpenUrl { from_name, url, .. } => {
                            state.log("link", "Link opened", format!("from {from_name}"), true, None);
                            let _ = open::that(&url);
                        }
                        Event::Battery { from, percent, charging, .. } => {
                            state.batteries.lock().unwrap().insert(from, (percent, charging));
                        }
                        Event::Remote { input, .. } => {
                            if state.settings.lock().unwrap().allow_remote {
                                remote::apply(&input);
                            }
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
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let keep = window
                    .app_handle()
                    .try_state::<state::SharedState>()
                    .map(|s| s.settings.lock().unwrap().background)
                    .unwrap_or(false);
                if keep {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::rename_device,
            commands::set_my_color,
            commands::sms_threads,
            commands::sms_thread,
            commands::sms_send,
            commands::call_start,
            commands::call_accept,
            commands::call_decline,
            commands::call_hangup,
            commands::call_send,
            commands::call_active,
            commands::app_version,
            commands::open_external,
            commands::check_update,
            commands::install_update,
            commands::get_settings,
            commands::set_settings,
            commands::shelf_list,
            commands::shelf_put,
            commands::shelf_get,
            commands::shelf_delete,
            commands::send_url,
            commands::shelf_cache,
            commands::drag_out,
            commands::stat_paths,
            commands::pick_files,
            commands::send_text,
            commands::send_path,
            commands::shelf_put_path,
            commands::send_remote,
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
