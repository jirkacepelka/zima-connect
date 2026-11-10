//! The shelf: a personal place on the household server for files you want to reach from any of
//! your own devices. Only the person who put a file there can see it.
//!
//! Unlike the offline queue these are not sealed for one device: the shelf is a folder on your own
//! server, so the server can see them.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB per file
pub const MAX_TOTAL: u64 = 20 * 1024 * 1024 * 1024; // 20 GiB on the shelf
const TTL_SECS: u64 = 14 * 24 * 60 * 60;

/// Kept for older apps that still send it. The shelf is personal: this is ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Audience {
    /// Everybody in the household the sender may share files with.
    Everyone,
    /// Only these people (user ids).
    Users { ids: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfItem {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub owner_user: String,
    pub owner_user_name: String,
    pub owner_device: String,
    pub audience: Audience,
    pub created: u64,
    /// Filled in per viewer: this item was put on the shelf by the viewer's own person.
    #[serde(default)]
    pub mine: bool,
}

impl Default for Audience {
    fn default() -> Self {
        Audience::Users { ids: Vec::new() }
    }
}

impl ShelfItem {
    /// Only the person who put the file on the shelf may see it.
    pub fn visible_to(&self, viewer_user: &str) -> bool {
        self.owner_user == viewer_user
    }
}

pub struct Shelf {
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

/// A file name that cannot escape a folder.
pub fn clean_name(name: &str) -> String {
    let n: String = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() {
        "file".into()
    } else {
        n
    }
}

impl Shelf {
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let s = Self { dir };
        s.prune();
        Ok(s)
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn data_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.bin"))
    }

    pub fn all(&self) -> Vec<ShelfItem> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("json") {
                    if let Some(i) = std::fs::read(&p)
                        .ok()
                        .and_then(|b| serde_json::from_slice::<ShelfItem>(&b).ok())
                    {
                        out.push(i);
                    }
                }
            }
        }
        out.sort_by_key(|i| std::cmp::Reverse(i.created));
        out
    }

    pub fn prune(&self) {
        let t = now();
        for i in self.all() {
            if i.created + TTL_SECS < t {
                self.remove(&i.id);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<ShelfItem> {
        if !safe_id(id) {
            return None;
        }
        std::fs::read(self.meta_path(id))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn remove(&self, id: &str) {
        if safe_id(id) {
            let _ = std::fs::remove_file(self.meta_path(id));
            let _ = std::fs::remove_file(self.data_path(id));
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.all().iter().map(|i| i.size).sum()
    }

    /// Check limits and create the (not yet saved) item.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        &self,
        name: &str,
        size: u64,
        owner_user: &str,
        owner_user_name: &str,
        owner_device: &str,
        audience: Audience,
    ) -> Result<ShelfItem> {
        if size > MAX_FILE {
            bail!("file too large for the shelf (limit 4 GB)");
        }
        if self.total_bytes() + size > MAX_TOTAL {
            bail!("the shelf on the server is full");
        }
        Ok(ShelfItem {
            id: rand_id(),
            name: clean_name(name),
            size,
            owner_user: owner_user.to_string(),
            owner_user_name: owner_user_name.to_string(),
            owner_device: owner_device.to_string(),
            audience,
            created: now(),
            mine: false,
        })
    }

    pub fn commit(&self, item: &ShelfItem) -> Result<()> {
        std::fs::write(self.meta_path(&item.id), serde_json::to_vec(item)?)?;
        Ok(())
    }
}
