use std::path::PathBuf;
use std::time::Duration;

use futures_util::future::join_all;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use crate::state::{Activity, SharedState};

#[derive(Serialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub platform: String,
}

#[derive(Serialize)]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub online: bool,
    pub rtt_ms: Option<u64>,
}

#[tauri::command]
pub fn get_identity(state: State<'_, SharedState>) -> Identity {
    Identity {
        id: state.node.id().to_string(),
        name: state.node.device_name().to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

/// Paired devices with live reachability (parallel ping, 3 s budget each).
#[tauri::command]
pub async fn get_peers(state: State<'_, SharedState>) -> Result<Vec<PeerView>, String> {
    let peers = state.node.peers();
    let futs = peers.into_iter().map(|p| {
        let node = state.node.clone();
        async move {
            let r = tokio::time::timeout(Duration::from_secs(3), node.ping(&p.id)).await;
            let rtt = match r {
                Ok(Ok(d)) => Some(d.as_millis() as u64),
                _ => None,
            };
            PeerView { id: p.id, name: p.name, platform: p.platform, online: rtt.is_some(), rtt_ms: rtt }
        }
    });
    Ok(join_all(futs).await)
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
) -> Result<(), String> {
    let text = app.clipboard().read_text().map_err(|e| e.to_string())?;
    if text.is_empty() {
        return Err("Clipboard is empty".into());
    }
    let name = peer_name(&state, &peer_id);
    let n = text.chars().count();
    match state.node.send_clipboard(&peer_id, text).await {
        Ok(()) => {
            state.log("clipboard", "Clipboard sent", format!("{n} characters to {name}"), true, None);
            Ok(())
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
) -> Result<bool, String> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().blocking_pick_file())
        .await
        .map_err(|e| e.to_string())?;
    let Some(fp) = picked else { return Ok(false) };
    let path: PathBuf = fp.into_path().map_err(|e| e.to_string())?;
    let fname = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = peer_name(&state, &peer_id);
    match state.node.send_file(&peer_id, &path).await {
        Ok(()) => {
            state.log("file", format!("Sent {fname}"), format!("to {name}"), true, None);
            Ok(true)
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
    state.node.peers().into_iter().find(|p| p.id == id).map(|p| p.name).unwrap_or_else(|| "device".into())
}
