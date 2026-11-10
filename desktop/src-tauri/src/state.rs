use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use zima_connect_net::Node;

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
    pub activities: Mutex<Vec<Activity>>,
    next_id: Mutex<u64>,
}

impl AppState {
    pub fn new(node: Node) -> Self {
        Self { node, activities: Mutex::new(Vec::new()), next_id: Mutex::new(1) }
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
