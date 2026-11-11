// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod files;
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
use zima_connect_net::localsend::{self, LocalSend, LsEvent};
use zima_connect_net::quickshare::{self, QsEvent, QuickShare};
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
            let download_dir_copy = cfg.download_dir.clone();
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
            // LocalSend: the open protocol other apps (LocalSend on phones and computers) speak.
            let ls_cfg = |port: u16| localsend::Config {
                data_dir: data_dir_copy.clone(),
                receive_dir: download_dir_copy.clone(),
                alias: node.device_name(),
                model: std::env::consts::OS.to_string(),
                device_type: "desktop".into(),
                port,
                discovery: true,
            };
            // The official app may already hold the usual port: then take any free one.
            let ls = tauri::async_runtime::block_on(async {
                match LocalSend::start(ls_cfg(localsend::PORT)).await {
                    Ok(l) => Ok(l),
                    Err(_) => LocalSend::start(ls_cfg(0)).await,
                }
            })
            .map_err(|e| format!("could not start LocalSend: {e:#}"))?;
            ls.set_visible(prefs.localsend);
            // Quick Share: what Android phones use. Optional: if it cannot start, the rest still works.
            let qs = tauri::async_runtime::block_on(QuickShare::start(quickshare::Config {
                receive_dir: download_dir_copy.clone(),
                alias: node.device_name(),
                device_type: 3,
                discovery: true,
            }))
            .map_err(|e| format!("could not start Quick Share: {e:#}"))?;
            qs.set_visible(prefs.quickshare);
            let state = Arc::new(AppState::new(node.clone(), ls.clone(), qs.clone(), data_dir_copy, prefs));
            {
                let handle = app.handle().clone();
                let state = state.clone();
                let mut rx = qs.subscribe();
                tauri::async_runtime::spawn(async move {
                    while let Ok(ev) = rx.recv().await {
                        match ev {
                            QsEvent::Incoming { session, from, pin, files } => {
                                show_main(&handle);
                                let n = files.len();
                                let _ = handle.notification().builder().title(format!("{from} wants to send you {} item{}", n, if n == 1 { "" } else { "s" })).body(format!("PIN {pin}. Open Zima Connect to accept or decline")).show();
                                let _ = handle.emit("ls-incoming", serde_json::json!({ "session": session, "from": from, "files": files, "pin": pin, "via": "Quick Share" }));
                            }
                            QsEvent::Received { from, name, path, size } => {
                                state.log("file", format!("Received {name}"), format!("from {from} with Quick Share ({} KB)", size / 1024), true, Some(path));
                                let _ = handle.emit("ls-done", ());
                            }
                            QsEvent::Text { from, text, url } => {
                                let shown: String = text.chars().take(200).collect();
                                state.log(if url { "link" } else { "clipboard" }, if url { "Received a link" } else { "Received a text" }, format!("from {from} with Quick Share: {shown}"), true, None);
                                let _ = handle.emit("ls-done", ());
                            }
                            QsEvent::Done { .. } => {
                                let _ = handle.emit("ls-done", ());
                            }
                        }
                    }
                });
            }
            {
                let handle = app.handle().clone();
                let state = state.clone();
                let mut rx = ls.subscribe();
                tauri::async_runtime::spawn(async move {
                    while let Ok(ev) = rx.recv().await {
                        match ev {
                            LsEvent::Incoming { session, from, files } => {
                                show_main(&handle);
                                let n = files.len();
                                let _ = handle.notification().builder().title(format!("{from} wants to send you {} file{}", n, if n == 1 { "" } else { "s" })).body("Open Zima Connect to accept or decline").show();
                                let _ = handle.emit("ls-incoming", serde_json::json!({ "session": session, "from": from, "files": files, "via": "LocalSend" }));
                            }
                            LsEvent::Received { from, name, path, size } => {
                                state.log("file", format!("Received {name}"), format!("from {from} with LocalSend ({} KB)", size / 1024), true, Some(path));
                                let _ = handle.emit("ls-done", ());
                            }
                            LsEvent::Done { .. } => {
                                let _ = handle.emit("ls-done", ());
                            }
                        }
                    }
                });
            }
            app.manage(state.clone());
            // Other devices as folders in the file browser (this computer only).
            {
                let fs: Arc<dyn zima_connect_net::webdav::Fs> = Arc::new(files::DavFs(state.clone()));
                let port = tauri::async_runtime::block_on(async {
                    match zima_connect_net::webdav::serve(fs.clone(), 21980).await {
                        Ok(p) => Some(p),
                        Err(_) => zima_connect_net::webdav::serve(fs, 0).await.ok(),
                    }
                });
                if let Some(p) = port {
                    *state.dav_port.lock().unwrap() = p;
                }
            }

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
                            if !state.settings.lock().unwrap().clipboard_in { continue; }
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
                            if !state.settings.lock().unwrap().sms { continue; }
                            let who = name.unwrap_or(address);
                            state.log("sms", format!("Text message from {who}"), format!("on {from_name}"), true, None);
                            let _ = handle.notification().builder().title(format!("{who} · {from_name}")).body(body).show();
                            let _ = handle.emit("sms-received", ());
                        }
                        Event::SmsRequest { .. } => {}
                        Event::PhoneRequest { .. } | Event::ScreenRequest { .. } => {}
                        Event::FilesRequest { req_id, op, .. } => {
                            let state = state.clone();
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                let share = state.settings.lock().unwrap().share_files;
                                let v = if share {
                                    let roots = files::share_roots(&handle);
                                    tokio::task::spawn_blocking(move || zima_connect_net::localfs::answer(&roots, &op)).await.unwrap_or_else(|_| serde_json::json!({ "error": "failed" }))
                                } else {
                                    serde_json::json!({ "error": "File sharing is off on this computer" })
                                };
                                let _ = state.node.files_reply(req_id, v);
                            });
                        }
                        Event::PhoneState { from, from_name, state: st, number, name } => {
                            if !state.settings.lock().unwrap().phone_calls { continue; }
                            let who = name.clone().unwrap_or_else(|| number.clone());
                            if st == "ringing" {
                                state.log("call", format!("Call from {who}"), format!("on {from_name}"), true, None);
                                show_main(&handle);
                                let _ = handle.notification().builder().title(format!("{who} is calling")).body(format!("on {from_name}")).show();
                            }
                            let _ = handle.emit("phone-state", serde_json::json!({ "from": from, "from_name": from_name, "state": st, "number": number, "name": name }));
                        }
                        Event::OpenUrl { from_name, url, .. } => {
                            if !state.settings.lock().unwrap().links_in { continue; }
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
                            if !state.settings.lock().unwrap().ring_in { continue; }
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
                            if !state.settings.lock().unwrap().notifications_in { continue; }
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
            commands::phone_op,
            commands::screen_op,
            commands::clipboard_text,
            commands::open_screen_window,
            commands::files_list,
            commands::files_save,
            commands::files_explorer,
            commands::app_version,
            commands::open_external,
            commands::ls_peers,
            commands::ls_send,
            commands::ls_decide,
            commands::bluetooth_settings,
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
