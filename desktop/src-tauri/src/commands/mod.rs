use std::path::PathBuf;
use std::time::Duration;

use futures_util::future::join_all;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use tauri_plugin_autostart::ManagerExt;
use zima_connect_net::{AdminOp, PhoneOp, SmsOp, Contact, Delivery, HouseholdInfo, RemoteInput, ShelfItem, Snapshot};

use crate::settings::{self, Settings};

use crate::state::{Activity, SharedState};

#[derive(Serialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub platform: String,
}

#[derive(Serialize)]
pub struct ContactView {
    #[serde(flatten)]
    pub contact: Contact,
    pub online: bool,
    pub rtt_ms: Option<u64>,
    pub battery: Option<(u8, bool)>,
}

#[derive(Serialize)]
pub struct HouseholdView {
    pub info: HouseholdInfo,
    pub snapshot: Snapshot,
}

#[tauri::command]
pub fn get_identity(state: State<'_, SharedState>) -> Identity {
    Identity {
        id: state.node.id().to_string(),
        name: state.node.device_name().to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

/// Everyone this device can talk to, with live reachability (parallel ping, 3 s budget each).
#[tauri::command]
pub async fn get_contacts(state: State<'_, SharedState>) -> Result<Vec<ContactView>, String> {
    let futs = state.node.contacts().into_iter().map(|c| {
        let node = state.node.clone();
        let battery = state.batteries.lock().unwrap().get(&c.id).copied();
        async move {
            let r = tokio::time::timeout(Duration::from_secs(3), node.ping(&c.id)).await;
            let rtt = match r {
                Ok(Ok(d)) => Some(d.as_millis() as u64),
                _ => None,
            };
            ContactView { contact: c, online: rtt.is_some(), rtt_ms: rtt, battery }
        }
    });
    Ok(join_all(futs).await)
}

/// Open a web page in the default browser (https only).
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links can be opened".into());
    }
    open::that(url).map_err(|e| e.to_string())
}

/// Open the system's Bluetooth settings, to connect a phone as a headset.
#[tauri::command]
pub fn bluetooth_settings() -> Result<(), String> {
    #[cfg(windows)]
    {
        open::that("ms-settings:bluetooth").map_err(|e| e.to_string())
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        for (cmd, args) in [
            ("gnome-control-center", vec!["bluetooth"]),
            ("systemsettings", vec!["kcm_bluetooth"]),
            ("blueman-manager", vec![]),
        ] {
            if Command::new(cmd).args(&args).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("Open your system's Bluetooth settings and connect the phone there".into())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err("Open your system's Bluetooth settings and connect the phone there".into())
    }
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// A newer release on GitHub, with its release notes. `None` when up to date.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<crate::updater::UpdateInfo>, String> {
    crate::updater::check(&app.package_info().version.to_string()).await
}

/// Download and install the update, then restart. Progress arrives as `update-progress` events.
#[tauri::command]
pub async fn install_update(info: crate::updater::UpdateInfo, app: AppHandle) -> Result<(), String> {
    crate::updater::install(app, info).await
}

/// Use a phone's line: { op: "dial", number } | "answer" | "decline" | "hang_up" | { op: "mute", on } | "state".
#[tauri::command]
pub async fn phone_op(peer_id: String, op: PhoneOp, state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    let name = peer_name(&state, &peer_id);
    let dialed = if let PhoneOp::Dial { number } = &op { Some(number.clone()) } else { None };
    match state.node.phone(&peer_id, op).await {
        Ok(v) => {
            if let Some(n) = dialed {
                state.log("call", "Calling", format!("{n} through {name}"), true, None);
            }
            Ok(v)
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Look at or control a phone's screen: { op: "status" } | { op: "frame", max_width } | tap | hold | swipe | { op: "key", name }.
#[tauri::command]
pub async fn screen_op(peer_id: String, op: zima_connect_net::ScreenOp, state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.node.screen(&peer_id, op).await.map_err(|e| format!("{e:#}"))
}

/// What is on this computer's clipboard (text), for the Send panel.
#[tauri::command]
pub fn clipboard_text(app: AppHandle) -> Result<String, String> {
    app.clipboard().read_text().map_err(|e| e.to_string())
}

/// The phone's screen in its own window (one per device).
#[tauri::command]
pub fn open_screen_window(peer_id: String, name: String, app: AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let label = format!("screen-{}", peer_id.chars().filter(|c| c.is_ascii_alphanumeric()).take(24).collect::<String>());
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let enc = |s: &str| -> String { s.bytes().map(|b| if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' { (b as char).to_string() } else { format!("%{b:02X}") }).collect() };
    let url = format!("screen?peer={}&name={}", enc(&peer_id), enc(&name));
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App(url.into()))
        .title(format!("{name} · Zima Connect"))
        .inner_size(420.0, 860.0)
        .min_inner_size(260.0, 420.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// A folder of another device: { entries: [{ name, dir, size, mtime }] }.
#[tauri::command]
pub async fn files_list(peer_id: String, path: String, state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.node.files(&peer_id, zima_connect_net::FilesOp::List { path }).await.map_err(|e| format!("{e:#}"))
}

/// Copy a file of another device into Downloads/Zima Connect; returns where it is now.
#[tauri::command]
pub async fn files_save(peer_id: String, path: String, app: AppHandle, state: State<'_, SharedState>) -> Result<String, String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?.join("Zima Connect");
    let from = peer_name(&state, &peer_id);
    let target = crate::files::download(&state, &peer_id, &path, dir).await.map_err(|e| format!("{e:#}"))?;
    let shown = target.to_string_lossy().into_owned();
    state.log("file", format!("Copied {}", target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), format!("from {from}"), true, Some(shown.clone()));
    Ok(shown)
}

/// Open a device's files in this computer's own file browser (a WebDAV location on this computer).
#[tauri::command]
pub fn files_explorer(peer_id: String, state: State<'_, SharedState>) -> Result<(), String> {
    let port = *state.dav_port.lock().unwrap();
    if port == 0 {
        return Err("The file browser link could not start".into());
    }
    let name = state
        .node
        .contacts()
        .into_iter()
        .find(|c| c.id == peer_id)
        .map(|c| c.name.chars().map(|ch| if "\\/:*?\"<>|".contains(ch) { '_' } else { ch }).collect::<String>())
        .ok_or("device not found")?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let unc = format!("\\\\127.0.0.1@{port}\\DavWWWRoot\\{name}");
        std::process::Command::new("explorer.exe").raw_arg(format!("\"{unc}\"")).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        let url = format!("dav://127.0.0.1:{port}/{}", name.replace(' ', "%20"));
        std::process::Command::new("xdg-open").arg(url).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The conversations of a phone's text messages.
#[tauri::command]
pub async fn sms_threads(peer_id: String, state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.node.sms(&peer_id, SmsOp::Threads).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn sms_thread(peer_id: String, id: String, state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.node.sms(&peer_id, SmsOp::Thread { id }).await.map_err(|e| format!("{e:#}"))
}

/// Send a text message through the phone.
#[tauri::command]
pub async fn sms_send(peer_id: String, to: String, body: String, state: State<'_, SharedState>) -> Result<(), String> {
    let name = peer_name(&state, &peer_id);
    match state.node.sms(&peer_id, SmsOp::Send { to: to.clone(), body }).await {
        Ok(_) => {
            state.log("sms", "Text message sent", format!("to {to} through {name}"), true, None);
            Ok(())
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Choose the colour of the person using this computer (the same on every screen).
#[tauri::command]
pub async fn set_my_color(hue: u16, state: State<'_, SharedState>) -> Result<(), String> {
    state.node.set_my_color(hue).await.map_err(|e| format!("{e:#}"))
}

/// Give this computer a new name (shown on all other devices).
#[tauri::command]
pub async fn rename_device(name: String, state: State<'_, SharedState>) -> Result<String, String> {
    state.node.rename_device(&name).await.map_err(|e| format!("{e:#}"))?;
    state.localsend.set_alias(&state.node.device_name());
    state.quickshare.set_alias(&state.node.device_name());
    Ok(state.node.device_name())
}

/// LocalSend devices nearby.
#[tauri::command]
pub fn ls_peers(state: State<'_, SharedState>) -> Vec<zima_connect_net::localsend::Peer> {
    let mut all = state.localsend.peers();
    // Quick Share devices look the same to the UI; their id starts with "qs:".
    all.extend(state.quickshare.peers().into_iter().map(|p| zima_connect_net::localsend::Peer {
        fingerprint: format!("qs:{}", p.id),
        alias: p.alias,
        model: Some("Quick Share".into()),
        device_type: if p.device_type == "phone" || p.device_type == "tablet" { "mobile".into() } else { "desktop".into() },
        ip: p.ip,
        port: p.port,
        https: false,
        last_seen: 0,
    }));
    all
}

/// Send files to a LocalSend device (it asks its user first).
#[tauri::command]
pub async fn ls_send(fingerprint: String, paths: Vec<String>, state: State<'_, SharedState>) -> Result<(), String> {
    let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).filter(|p| p.is_file()).collect();
    if files.is_empty() {
        return Err("Folders cannot be sent yet. Choose files instead.".into());
    }
    if let Some(id) = fingerprint.strip_prefix("qs:") {
        let peer = state.quickshare.peers().into_iter().find(|p| p.id == id).ok_or("That device is no longer nearby")?;
        let label = if files.len() == 1 { files[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { format!("{} files", files.len()) };
        return match state.quickshare.send(&peer, &files).await {
            Ok(()) => {
                state.log("file", format!("Sent {label}"), format!("to {} with Quick Share", peer.alias), true, None);
                Ok(())
            }
            Err(e) => Err(format!("{e:#}")),
        };
    }
    let peer = state
        .localsend
        .peers()
        .into_iter()
        .find(|p| p.fingerprint == fingerprint)
        .ok_or("That device is no longer nearby")?;
    let label = if files.len() == 1 {
        files[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        format!("{} files", files.len())
    };
    match state.localsend.send(&peer, &files).await {
        Ok(()) => {
            state.log("file", format!("Sent {label}"), format!("to {} with LocalSend", peer.alias), true, None);
            Ok(())
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Accept or decline files a LocalSend device wants to send here.
#[tauri::command]
pub fn ls_decide(session: String, accept: bool, state: State<'_, SharedState>) -> Result<(), String> {
    if session.starts_with("qs:") {
        return state.quickshare.decide(&session, accept).map_err(|e| format!("{e:#}"));
    }
    state.localsend.decide(&session, accept).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn get_settings(state: State<'_, SharedState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(new: Settings, app: AppHandle, state: State<'_, SharedState>) -> Result<Settings, String> {
    let prev = state.settings.lock().unwrap().clone();
    if new.autostart != prev.autostart {
        let launch = app.autolaunch();
        let r = if new.autostart { launch.enable() } else { launch.disable() };
        r.map_err(|e| e.to_string())?;
    }
    state.localsend.set_visible(new.localsend);
    state.quickshare.set_visible(new.quickshare);
    settings::save(&state.data_dir, &new);
    *state.settings.lock().unwrap() = new.clone();
    Ok(new)
}

#[tauri::command]
pub async fn shelf_list(state: State<'_, SharedState>) -> Result<Vec<ShelfItem>, String> {
    state.node.shelf_list().await.map_err(|e| format!("{e:#}"))
}

/// Pick a file and leave it on the server's shelf. `None` if the picker was cancelled.
#[tauri::command]
pub async fn shelf_put(app: AppHandle, state: State<'_, SharedState>) -> Result<Option<ShelfItem>, String> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_file())
        .await
        .map_err(|e| e.to_string())?;
    let Some(fp) = picked else { return Ok(None) };
    let path: PathBuf = fp.into_path().map_err(|e| e.to_string())?;
    match state.node.shelf_put(&path).await {
        Ok(item) => {
            state.log("shelf", format!("Put {} on the shelf", item.name), "reachable from all your devices", true, None);
            Ok(Some(item))
        }
        Err(e) => {
            state.log("shelf", "Could not use the shelf", format!("{e:#}"), false, None);
            Err(format!("{e:#}"))
        }
    }
}

/// Download a shelf item into the Zima Connect folder. Returns where it was saved.
#[tauri::command]
pub async fn shelf_get(id: String, state: State<'_, SharedState>) -> Result<String, String> {
    let path = state.node.shelf_get(&id).await.map_err(|e| format!("{e:#}"))?;
    let p = path.to_string_lossy().into_owned();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    state.log("file", format!("Took {name} from the shelf"), "saved in your Zima Connect folder", true, Some(p.clone()));
    Ok(p)
}

fn safe_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c)).take(120).collect();
    if n.trim().is_empty() { "file".into() } else { n }
}

/// Make sure a shelf item is in the local preview cache. Returns its path.
async fn cached(state: &SharedState, id: &str, name: &str, size: u64) -> Result<PathBuf, String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("bad id".into());
    }
    let dir = state.data_dir.join("shelf-cache").join(id);
    let want = dir.join(safe_name(name));
    if std::fs::metadata(&want).map(|m| m.len() == size).unwrap_or(false) {
        return Ok(want);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let got = state.node.shelf_get_in(id, &dir).await.map_err(|e| format!("{e:#}"))?;
    Ok(got)
}

/// Download a shelf item (once) into the preview cache; the UI shows it from there.
#[tauri::command]
pub async fn shelf_cache(id: String, name: String, size: u64, state: State<'_, SharedState>) -> Result<String, String> {
    Ok(cached(&state, &id, &name, size).await?.to_string_lossy().into_owned())
}

/// Start a native drag of a shelf file out of the window (to the desktop, a folder, another app).
#[tauri::command]
pub async fn drag_out(id: String, name: String, size: u64, window: tauri::WebviewWindow, state: State<'_, SharedState>) -> Result<(), String> {
    let path = cached(&state, &id, &name, size).await?;
    let w = window.clone();
    window
        .run_on_main_thread(move || {
            let icon = include_bytes!("../../icons/32x32.png").to_vec();
            // GTK (Linux) wants its own window type.
            #[cfg(target_os = "linux")]
            let target = w.gtk_window();
            #[cfg(not(target_os = "linux"))]
            let target: Result<&tauri::WebviewWindow, ()> = Ok(&w);
            if let Ok(target) = target {
                let _ = drag::start_drag(&target, drag::DragItem::Files(vec![path]), drag::Image::Raw(icon), |_, _| {}, drag::Options::default());
            }
        })
        .map_err(|e| e.to_string())
}

/// Let the user pick files (several). Empty when cancelled.
#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Result<Vec<String>, String> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_files())
        .await
        .map_err(|e| e.to_string())?;
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[derive(Serialize)]
pub struct PathInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_file: bool,
}

/// Names and sizes of dropped paths, so the UI can show what is about to be sent.
#[tauri::command]
pub fn stat_paths(paths: Vec<String>) -> Vec<PathInfo> {
    paths
        .into_iter()
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok()?;
            let name = std::path::Path::new(&p).file_name()?.to_string_lossy().into_owned();
            Some(PathInfo { path: p, name, size: if m.is_file() { m.len() } else { 0 }, is_file: m.is_file() })
        })
        .collect()
}

/// Send text (not the clipboard) to a peer.
#[tauri::command]
pub async fn send_text(peer_id: String, text: String, state: State<'_, SharedState>) -> Result<Delivery, String> {
    let name = peer_name(&state, &peer_id);
    let n = text.chars().count();
    match state.node.send_clipboard(&peer_id, text).await {
        Ok(d) => {
            state.log("clipboard", "Text sent", format!("{n} characters to {name}"), true, None);
            Ok(d)
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

#[tauri::command]
pub async fn shelf_delete(id: String, state: State<'_, SharedState>) -> Result<(), String> {
    state.node.shelf_delete(&id).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn send_url(peer_id: String, url: String, state: State<'_, SharedState>) -> Result<(), String> {
    let name = peer_name(&state, &peer_id);
    match state.node.send_url(&peer_id, &url).await {
        Ok(()) => {
            state.log("link", "Link sent", format!("to {name}"), true, None);
            Ok(())
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

#[tauri::command]
pub async fn send_remote(peer_id: String, input: RemoteInput, state: State<'_, SharedState>) -> Result<(), String> {
    state.node.send_remote(&peer_id, input).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn get_household(state: State<'_, SharedState>) -> Option<HouseholdView> {
    Some(HouseholdView { info: state.node.household_info()?, snapshot: state.node.household_snapshot()? })
}

/// Join the household of the server that showed this code.
#[tauri::command]
pub async fn join_household(code: String, user_name: Option<String>, state: State<'_, SharedState>) -> Result<String, String> {
    let info = state.node.join(code.trim(), user_name).await.map_err(|e| format!("{e:#}"))?;
    Ok(info.household)
}

#[tauri::command]
pub async fn leave_household(state: State<'_, SharedState>) -> Result<(), String> {
    // Tells the server too, so this device disappears from the household list.
    state.node.leave_household_async().await;
    Ok(())
}

/// Owner only: change people, devices or policy. Returns a join code for invites.
#[tauri::command]
pub async fn household_admin(op: AdminOp, state: State<'_, SharedState>) -> Result<Option<String>, String> {
    state.node.admin(op).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn get_activities(state: State<'_, SharedState>) -> Vec<Activity> {
    state.activities.lock().unwrap().clone()
}

/// New one-time pairing code to show on this device.
#[tauri::command]
pub async fn create_pairing_code(state: State<'_, SharedState>) -> Result<String, String> {
    state.node.pairing_ticket().await.map_err(|e| format!("{e:#}"))
}

/// Pair with the device that showed `code`.
#[tauri::command]
pub async fn pair_device(code: String, state: State<'_, SharedState>) -> Result<String, String> {
    match state.node.pair(&code).await {
        // The "Paired" activity entry is logged by the node event handler.
        Ok(p) => Ok(p.name),
        Err(e) => Err(format!("{e:#}")),
    }
}

#[tauri::command]
pub fn unpair_device(id: String, state: State<'_, SharedState>) -> Result<(), String> {
    state.node.unpair(&id).map_err(|e| format!("{e:#}"))
}

/// Send whatever is on this computer's clipboard to a peer.
#[tauri::command]
pub async fn send_clipboard(
    peer_id: String,
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<Delivery, String> {
    let text = app.clipboard().read_text().map_err(|e| e.to_string())?;
    if text.is_empty() {
        return Err("Clipboard is empty".into());
    }
    let name = peer_name(&state, &peer_id);
    let n = text.chars().count();
    match state.node.send_clipboard(&peer_id, text).await {
        Ok(d) => {
            let (title, detail) = match d {
                Delivery::Delivered => ("Clipboard sent", format!("{n} characters to {name}")),
                Delivery::Queued => ("Clipboard queued", format!("{name} is offline, the server will deliver it")),
            };
            state.log("clipboard", title, detail, true, None);
            Ok(d)
        }
        Err(e) => {
            state.log("clipboard", "Clipboard not sent", format!("{name}: {e:#}"), false, None);
            Err(format!("{e:#}"))
        }
    }
}

/// Let the user pick a file and send it to a peer. Returns false if cancelled.
#[tauri::command]
pub async fn send_file(
    peer_id: String,
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<Option<Delivery>, String> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_file())
        .await
        .map_err(|e| e.to_string())?;
    let Some(fp) = picked else { return Ok(None) };
    let path: PathBuf = fp.into_path().map_err(|e| e.to_string())?;
    let fname = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = peer_name(&state, &peer_id);
    match state.node.send_file(&peer_id, &path).await {
        Ok(d) => {
            match d {
                Delivery::Delivered => state.log("file", format!("Sent {fname}"), format!("to {name}"), true, None),
                Delivery::Queued => state.log(
                    "file",
                    format!("Queued {fname}"),
                    format!("{name} is offline, the server will deliver it"),
                    true,
                    None,
                ),
            }
            Ok(Some(d))
        }
        Err(e) => {
            state.log("file", format!("Could not send {fname}"), format!("{name}: {e:#}"), false, None);
            Err(format!("{e:#}"))
        }
    }
}

/// Send a file that was dropped onto the window.
#[tauri::command]
pub async fn send_path(peer_id: String, path: String, state: State<'_, SharedState>) -> Result<Delivery, String> {
    let path = PathBuf::from(path);
    let fname = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !path.is_file() {
        return Err("Folders cannot be sent yet. Drop files instead.".into());
    }
    let name = peer_name(&state, &peer_id);
    match state.node.send_file(&peer_id, &path).await {
        Ok(d) => {
            match d {
                Delivery::Delivered => state.log("file", format!("Sent {fname}"), format!("to {name}"), true, None),
                Delivery::Queued => state.log("file", format!("Queued {fname}"), format!("{name} is offline, the server will deliver it"), true, None),
            }
            Ok(d)
        }
        Err(e) => {
            state.log("file", format!("Could not send {fname}"), format!("{name}: {e:#}"), false, None);
            Err(format!("{e:#}"))
        }
    }
}

/// Put a dropped file on the shelf.
#[tauri::command]
pub async fn shelf_put_path(path: String, state: State<'_, SharedState>) -> Result<ShelfItem, String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err("Folders cannot be put on the shelf yet. Drop files instead.".into());
    }
    match state.node.shelf_put(&path).await {
        Ok(item) => {
            state.log("shelf", format!("Put {} on the shelf", item.name), "reachable from all your devices", true, None);
            Ok(item)
        }
        Err(e) => {
            state.log("shelf", "Could not use the shelf", format!("{e:#}"), false, None);
            Err(format!("{e:#}"))
        }
    }
}

#[tauri::command]
pub async fn ring_device(peer_id: String, state: State<'_, SharedState>) -> Result<(), String> {
    let name = peer_name(&state, &peer_id);
    match state.node.ring(&peer_id).await {
        Ok(()) => {
            state.log("ring", "Ringing device", name, true, None);
            Ok(())
        }
        Err(e) => {
            state.log("ring", "Could not reach device", format!("{name}: {e:#}"), false, None);
            Err(format!("{e:#}"))
        }
    }
}

/// Stop a ring this computer started on another device.
#[tauri::command]
pub async fn ring_stop_device(peer_id: String, state: State<'_, SharedState>) -> Result<(), String> {
    state.node.ring_stop(&peer_id).await.map_err(|e| format!("{e:#}"))
}

/// Who is ringing this computer right now (rings stop by themselves after 30 s).
#[tauri::command]
pub fn get_ring_state(state: State<'_, SharedState>) -> Option<String> {
    let mut g = state.ringing.lock().unwrap();
    match g.as_ref() {
        Some((_, since)) if since.elapsed() > Duration::from_secs(30) => {
            *g = None;
            None
        }
        Some((who, _)) => Some(who.clone()),
        None => None,
    }
}

/// Silence "find my device" on this computer.
#[tauri::command]
pub fn stop_ring(state: State<'_, SharedState>) {
    *state.ringing.lock().unwrap() = None;
}

/// Reveal a received file in the system file manager.
#[tauri::command]
pub fn reveal_path(path: String) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").args(["-R", &path]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = std::path::Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

fn peer_name(state: &SharedState, id: &str) -> String {
    state.node.contacts().into_iter().find(|c| c.id == id).map(|c| c.name).unwrap_or_else(|| "device".into())
}
