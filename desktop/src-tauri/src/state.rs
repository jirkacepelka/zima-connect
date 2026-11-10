use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use zima_connect_core::crypto::keys::DeviceKeyPair;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub is_online: bool,
    pub last_seen: Option<String>,
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityItem {
    pub id: String,
    pub timestamp: String,
    pub icon: String,
    pub title: String,
    pub description: String,
    pub status: String, // "success", "pending", "failed"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub server_url: String,
    pub server_name: String,
    pub active_devices: usize,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

pub struct AppState {
    pub keypair: DeviceKeyPair,
    pub server_url: RwLock<String>,
    pub auth_token: RwLock<Option<String>>,
    pub paired_devices: RwLock<Vec<DeviceInfo>>,
    pub activities: RwLock<Vec<ActivityItem>>,
    pub connection_status: RwLock<ConnectionStatus>,
}

impl AppState {
    pub fn new() -> Self {
        let keypair = DeviceKeyPair::generate();
        Self {
            keypair,
            server_url: RwLock::new("http://localhost:9120".to_string()),
            auth_token: RwLock::new(None),
            paired_devices: RwLock::new(Vec::new()),
            activities: RwLock::new(Vec::new()),
            connection_status: RwLock::new(ConnectionStatus {
                connected: false,
                server_url: "http://localhost:9120".to_string(),
                server_name: "ZimaOS Hub".to_string(),
                active_devices: 0,
                tx_bytes: 0,
                rx_bytes: 0,
            }),
        }
    }
}

pub type SharedState = Arc<AppState>;
