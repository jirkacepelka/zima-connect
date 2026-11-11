use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use zima_connect_net::localsend::LocalSend;
use zima_connect_net::quickshare::QuickShare;
use zima_connect_net::Node;

use crate::settings::Settings;

#[derive(Debug, Clone, Serialize)]
pub struct Activity {
    pub id: u64,
    /// Unix time in milliseconds.
    pub ts: u64,
    /// "clipboard" | "file" | "ring" | "pair"
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub ok: bool,
    /// Local path (received files) so the UI can reveal it.
    pub path: Option<String>,
}

pub struct AppState {
    pub node: Node,
    pub localsend: LocalSend,
    pub quickshare: QuickShare,
    /// Port of the local WebDAV server that shows other devices in the file browser.
    pub dav_port: Mutex<u16>,
    pub activities: Mutex<Vec<Activity>>,
    next_id: Mutex<u64>,
    /// "Find my device": who is ringing this computer, since when.
    pub ringing: Mutex<Option<(String, Instant)>>,
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    /// device id -> (percent, charging) as last reported by that device.
    pub batteries: Mutex<HashMap<String, (u8, bool)>>,
}

impl AppState {
    pub fn new(node: Node, localsend: LocalSend, quickshare: QuickShare, data_dir: PathBuf, settings: Settings) -> Self {
        Self {
            node,
            localsend,
            quickshare,
            dav_port: Mutex::new(0),
            activities: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
            ringing: Mutex::new(None),
            data_dir,
            settings: Mutex::new(settings),
            batteries: Mutex::new(HashMap::new()),
        }
    }

    pub fn log(&self, kind: &str, title: impl Into<String>, detail: impl Into<String>, ok: bool, path: Option<String>) {
        let mut idg = self.next_id.lock().unwrap();
        let id = *idg;
        *idg += 1;
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let mut a = self.activities.lock().unwrap();
        a.insert(0, Activity { id, ts, kind: kind.into(), title: title.into(), detail: detail.into(), ok, path });
        a.truncate(100);
    }
}

pub type SharedState = Arc<AppState>;
