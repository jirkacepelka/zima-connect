use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A paired device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub platform: String,
}

pub struct Store {
    path: PathBuf,
    peers: Vec<Peer>,
}

impl Store {
    pub fn load(path: PathBuf) -> Result<Self> {
        let peers = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Ok(Self { path, peers })
    }

    fn save(&self) -> Result<()> {
        std::fs::write(&self.path, serde_json::to_vec_pretty(&self.peers)?)?;
        Ok(())
    }

    pub fn peers(&self) -> Vec<Peer> {
        self.peers.clone()
    }
    pub fn contains(&self, id: &str) -> bool {
        self.peers.iter().any(|p| p.id == id)
    }
    pub fn get(&self, id: &str) -> Option<Peer> {
        self.peers.iter().find(|p| p.id == id).cloned()
    }
    pub fn add(&mut self, peer: Peer) -> Result<()> {
        self.peers.retain(|p| p.id != peer.id);
        self.peers.push(peer);
        self.save()
    }
    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.peers.retain(|p| p.id != id);
        self.save()
    }
}
