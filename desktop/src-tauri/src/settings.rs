//! Small per-computer preferences, kept next to the identity.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Closing the window keeps Zima Connect running in the tray so that files, clipboard and
    /// "find my device" keep working.
    #[serde(default = "yes")]
    pub background: bool,
    /// Start with the computer (hidden in the tray).
    #[serde(default = "yes")]
    pub autostart: bool,
    /// Other devices may control this computer (touchpad, keys, media) when the policy allows.
    #[serde(default = "yes")]
    pub allow_remote: bool,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self { background: true, autostart: true, allow_remote: true }
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

pub fn load(data_dir: &Path) -> Settings {
    std::fs::read(path(data_dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(data_dir: &Path, s: &Settings) {
    let _ = std::fs::write(path(data_dir), serde_json::to_vec_pretty(s).unwrap_or_default());
}
