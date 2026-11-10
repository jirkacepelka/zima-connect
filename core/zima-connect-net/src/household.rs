//! Household model: users, devices and the policy between users.
//!
//! The hub signs a [`Snapshot`] of the whole household. Every device keeps the
//! latest verified snapshot and enforces the policy locally, so devices keep
//! working (and talk directly) even while the hub is offline.

use anyhow::{anyhow, bail, Result};
use iroh::{PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};

/// What one device may do to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap {
    /// Send files / photos.
    Files,
    /// Send clipboard text.
    Clipboard,
    /// Mirror notifications.
    Notifications,
    /// "Find my device" ring.
    Ring,
    /// Voice / video calls (reserved).
    Calls,
}

impl Cap {
    pub const ALL: [Cap; 5] = [
        Cap::Files,
        Cap::Clipboard,
        Cap::Notifications,
        Cap::Ring,
        Cap::Calls,
    ];

    /// Between two *different* users only these are allowed unless a rule says otherwise.
    pub fn default_cross_user(self) -> bool {
        matches!(self, Cap::Files)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Member,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// iroh endpoint id (public key, hex/base32 string form).
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub platform: String,
}

/// Explicit rule: `from_user` may (or may not) use `cap` on devices of `to_user`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub from_user: String,
    pub to_user: String,
    pub cap: Cap,
    pub allow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Snapshot {
    pub version: u64,
    pub household: String,
    /// Hub endpoint id (the trust anchor).
    pub hub: String,
    pub users: Vec<User>,
    pub devices: Vec<Device>,
    pub rules: Vec<Rule>,
}

impl Snapshot {
    pub fn device(&self, id: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn user(&self, id: &str) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    pub fn user_of(&self, device_id: &str) -> Option<&User> {
        self.device(device_id).and_then(|d| self.user(&d.user_id))
    }

    /// Policy decision. Explicit rule wins; otherwise same user = allowed,
    /// different users = [`Cap::default_cross_user`].
    pub fn allowed(&self, from_user: &str, to_user: &str, cap: Cap) -> bool {
        if let Some(r) = self
            .rules
            .iter()
            .find(|r| r.from_user == from_user && r.to_user == to_user && r.cap == cap)
        {
            return r.allow;
        }
        from_user == to_user || cap.default_cross_user()
    }
}

/// A snapshot plus the hub's signature over the exact `body` text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedSnapshot {
    pub body: String,
    pub sig: Signature,
}

impl SignedSnapshot {
    pub fn sign(snapshot: &Snapshot, key: &SecretKey) -> Result<Self> {
        let body = serde_json::to_string(snapshot)?;
        let sig = key.sign(body.as_bytes());
        Ok(Self { body, sig })
    }

    /// Verify against the pinned hub key and parse.
    pub fn open(&self, hub: &PublicKey) -> Result<Snapshot> {
        hub.verify(self.body.as_bytes(), &self.sig)
            .map_err(|_| anyhow!("household data is not signed by your server"))?;
        let s: Snapshot = serde_json::from_str(&self.body)?;
        if s.hub != hub.to_string() {
            bail!("household data belongs to a different server");
        }
        Ok(s)
    }
}

/// Changes an owner can make.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AdminOp {
    /// Invite a device. `user_id` = add a device to an existing person,
    /// otherwise a new person called `new_user_name` is created when they join.
    CreateInvite {
        user_id: Option<String>,
        new_user_name: Option<String>,
    },
    SetRule {
        from_user: String,
        to_user: String,
        cap: Cap,
        allow: bool,
    },
    /// Remove an explicit rule (back to the default).
    ClearRule {
        from_user: String,
        to_user: String,
        cap: Cap,
    },
    RemoveDevice {
        id: String,
    },
    RemoveUser {
        id: String,
    },
    RenameUser {
        id: String,
        name: String,
    },
}
