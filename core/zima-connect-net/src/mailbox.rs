//! Hub-side mailbox: holds clipboard/notification/file items for devices that are
//! offline and hands them over when the device comes back.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// What kind of item it is. The server sees this (for policy and quotas), not the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Clipboard,
    Notify,
    File,
}

/// An item as the server sees it: everything readable is sealed for the recipient device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub kind: Kind,
    /// Base64 of the sealed [`Payload`].
    pub meta: String,
    /// The sender's signature over the sealed meta bytes (proves who sent it).
    pub sig: iroh::Signature,
    /// Sealed body bytes that follow on the stream (files only, otherwise 0).
    pub size: u64,
}

impl Item {
    pub fn is_file(&self) -> bool {
        self.kind == Kind::File
    }
}

/// What only the recipient can read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Payload {
    Clipboard {
        text: String,
    },
    Notify {
        app: String,
        title: String,
        body: String,
    },
    File {
        name: String,
        size: u64,
        /// sha256 of the sealed body header, binds the body to this signed item.
        body: String,
    },
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

    /// (items waiting, bytes of files waiting) over all devices.
    pub fn stats(&self) -> (usize, u64) {
        let all = self.all();
        let bytes = all
            .iter()
            .map(|e| if e.item.is_file() { e.item.size } else { 0 })
            .sum();
        (all.len(), bytes)
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
        if item.is_file() {
            let size = &item.size;
            if *size > MAX_FILE {
                bail!("file too large for the server queue (limit 1 GB)");
            }
            let waiting: u64 = self
                .for_device(to)
                .iter()
                .map(|e| if e.item.is_file() { e.item.size } else { 0 })
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
