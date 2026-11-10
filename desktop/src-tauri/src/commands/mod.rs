use std::path::PathBuf;
use std::time::Duration;

use futures_util::future::join_all;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use zima_connect_net::{AdminOp, Contact, Delivery, HouseholdInfo, Snapshot};

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
        async move {
            let r = tokio::time::timeout(Duration::from_secs(3), node.ping(&c.id)).await;
            let rtt = match r {
                Ok(Ok(d)) => Some(d.as_millis() as u64),
                _ => None,
            };
            ContactView { contact: c, online: rtt.is_some(), rtt_ms: rtt }
        }
    });
    Ok(join_all(futs).await)
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
