use crate::state::{ActivityItem, ConnectionStatus, DeviceInfo, SharedState};
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub async fn get_connection_status(state: State<'_, SharedState>) -> Result<ConnectionStatus, String> {
    let status = state.connection_status.read().await;
    Ok(status.clone())
}

#[tauri::command]
pub async fn get_devices(state: State<'_, SharedState>) -> Result<Vec<DeviceInfo>, String> {
    let devices = state.paired_devices.read().await;
    if devices.is_empty() {
        // Mock default preview devices for development/demo
        return Ok(vec![
            DeviceInfo {
                id: "dev-pixel8-1234".to_string(),
                name: "Pixel 8 Pro".to_string(),
                platform: "android".to_string(),
                is_online: true,
                last_seen: Some("Just now".to_string()),
                battery: Some(84),
            },
            DeviceInfo {
                id: "dev-thinkpad-5678".to_string(),
                name: "ThinkPad X1".to_string(),
                platform: "linux".to_string(),
                is_online: false,
                last_seen: Some("2 hours ago".to_string()),
                battery: Some(62),
            },
        ]);
    }
    Ok(devices.clone())
}

#[tauri::command]
pub async fn get_activities(state: State<'_, SharedState>) -> Result<Vec<ActivityItem>, String> {
    let activities = state.activities.read().await;
    if activities.is_empty() {
        return Ok(vec![
            ActivityItem {
                id: Uuid::now_v7().to_string(),
                timestamp: "2 min ago".to_string(),
                icon: "file".to_string(),
                title: "File sent: financial_report_q3.pdf".to_string(),
                description: "Delivered to Pixel 8 Pro (14.2 MB)".to_string(),
                status: "success".to_string(),
            },
            ActivityItem {
                id: Uuid::now_v7().to_string(),
                timestamp: "15 min ago".to_string(),
                icon: "clipboard".to_string(),
                title: "Clipboard synced".to_string(),
                description: "Copied 240 chars from Pixel 8 Pro".to_string(),
                status: "success".to_string(),
            },
            ActivityItem {
                id: Uuid::now_v7().to_string(),
                timestamp: "1 hour ago".to_string(),
                icon: "bell".to_string(),
                title: "Notification mirrored".to_string(),
                description: "Signal: New message from Sarah".to_string(),
                status: "success".to_string(),
            },
        ]);
    }
    Ok(activities.clone())
}

#[tauri::command]
pub async fn send_clipboard(
    target_device_id: String,
    content: String,
    state: State<'_, SharedState>,
) -> Result<String, String> {
    let mut activities = state.activities.write().await;
    activities.insert(
        0,
        ActivityItem {
            id: Uuid::now_v7().to_string(),
            timestamp: "Just now".to_string(),
            icon: "clipboard".to_string(),
            title: "Clipboard sent".to_string(),
            description: format!("Sent to device {} ({} chars)", target_device_id, content.len()),
            status: "success".to_string(),
        },
    );
    Ok("Clipboard synchronized".to_string())
}

#[tauri::command]
pub async fn send_file(
    target_device_id: String,
    file_path: String,
    state: State<'_, SharedState>,
) -> Result<String, String> {
    let mut activities = state.activities.write().await;
    activities.insert(
        0,
        ActivityItem {
            id: Uuid::now_v7().to_string(),
            timestamp: "Just now".to_string(),
            icon: "file".to_string(),
            title: format!("File offered: {}", file_path),
            description: format!("Target: {}", target_device_id),
            status: "pending".to_string(),
        },
    );
    Ok("File queued for transmission".to_string())
}

#[tauri::command]
pub async fn pair_device(code: String, state: State<'_, SharedState>) -> Result<bool, String> {
    if code.len() != 6 {
        return Err("Pairing PIN must be 6 digits".to_string());
    }
    let mut status = state.connection_status.write().await;
    status.connected = true;
    Ok(true)
}

#[tauri::command]
pub async fn ring_device(target_device_id: String) -> Result<String, String> {
    Ok(format!("Ring command sent to device {}", target_device_id))
}
