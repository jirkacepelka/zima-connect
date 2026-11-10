//! Hub-side mailbox: holds clipboard/notification/file items for devices that are
//! offline and hands them over when the device comes back.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// What can wait in the mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    Clipboard {
        text: String,
    },
    Notify {
        app: String,
        title: String,
        body: String,
    },
    /// Raw bytes follow on the stream when depositing / fetching.
    File {
        name: String,
        size: u64,
    },
}

impl Item {
    pub fn is_file(&self) -> bool {
        matches!(self, Item::File { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub to: String,
    pub from: String,
    pub from_name: String,
    pub item: Item,
    pub created: u64,
}

pub const MAX_FILE: u64 = 1024 * 1024 * 1024; // 1 GiB per file
pub const MAX_PER_DEVICE: u64 = 5 * 1024 * 1024 * 1024; // 5 GiB waiting per device
const TTL_SECS: u64 = 48 * 60 * 60;

pub struct Mailbox {
    dir: PathBuf,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rand_id() -> String {
    let mut b = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

impl Mailbox {
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let m = Self { dir };
        m.prune();
        Ok(m)
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn data_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.bin"))
    }

    fn all(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("json") {
                    if let Some(entry) = std::fs::read(&p)
                        .ok()
                        .and_then(|b| serde_json::from_slice::<Entry>(&b).ok())
                    {
                        out.push(entry);
                    }
                }
            }
        }
        out.sort_by_key(|e| e.created);
        out
    }

    /// Remove expired entries.
    pub fn prune(&self) {
        let t = now();
        for e in self.all() {
            if e.created + TTL_SECS < t {
                self.remove(&e.id);
            }
        }
    }

    pub fn remove(&self, id: &str) {
        if safe_id(id) {
            let _ = std::fs::remove_file(self.meta_path(id));
            let _ = std::fs::remove_file(self.data_path(id));
        }
    }

    pub fn for_device(&self, to: &str) -> Vec<Entry> {
        self.prune();
        self.all().into_iter().filter(|e| e.to == to).collect()
    }

    pub fn get(&self, id: &str, to: &str) -> Option<Entry> {
        if !safe_id(id) {
            return None;
        }
        std::fs::read(self.meta_path(id))
            .ok()
            .and_then(|b| serde_json::from_slice::<Entry>(&b).ok())
            .filter(|e| e.to == to)
    }

    /// Reserve a slot for a new item. For files the caller then writes `data_path(id)`.
    pub fn reserve(&self, to: &str, from: &str, from_name: &str, item: Item) -> Result<Entry> {
        if let Item::File { size, .. } = &item {
            if *size > MAX_FILE {
                bail!("file too large for the server queue (limit 1 GB)");
            }
            let waiting: u64 = self
                .for_device(to)
                .iter()
                .map(|e| {
                    if let Item::File { size, .. } = e.item {
                        size
                    } else {
                        0
                    }
                })
                .sum();
            if waiting + size > MAX_PER_DEVICE {
                bail!("the device's queue on the server is full");
            }
        }
        let entry = Entry {
            id: rand_id(),
            to: to.to_string(),
            from: from.to_string(),
            from_name: from_name.to_string(),
            item,
            created: now(),
        };
        Ok(entry)
    }

    /// Persist metadata (after file bytes, if any, are safely written).
    pub fn commit(&self, entry: &Entry) -> Result<()> {
        std::fs::write(self.meta_path(&entry.id), serde_json::to_vec(entry)?)?;
        Ok(())
    }
}
