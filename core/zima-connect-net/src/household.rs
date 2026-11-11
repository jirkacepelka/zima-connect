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
    /// Phone calls through a phone: dial from here, answer, see who is calling.
    Calls,
    /// Remote control: touchpad, keyboard, media keys, presentations.
    Remote,
    /// Status: battery level and the like.
    Status,
    /// Read and send text messages (SMS) of a phone.
    Sms,
    /// See and control a phone's screen.
    Screen,
}

impl Cap {
    pub const ALL: [Cap; 9] = [
        Cap::Files,
        Cap::Clipboard,
        Cap::Notifications,
        Cap::Ring,
        Cap::Calls,
        Cap::Remote,
        Cap::Status,
        Cap::Sms,
        Cap::Screen,
    ];

    /// Between two *different* people only these are allowed unless a rule says otherwise.
    pub fn default_cross_user(self) -> bool {
        // Things a person does on purpose: send a file or clipboard, find a device.
        // Automatic mirroring (notifications) and calls need an explicit rule.
        matches!(self, Cap::Files | Cap::Clipboard | Cap::Ring)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Member,
}

/// Colours people can have (hue in degrees). The same everywhere: server, computers, phones.
pub const PALETTE: [u16; 12] = [8, 28, 46, 98, 150, 176, 200, 222, 252, 282, 312, 340];

/// The palette colour that is furthest from the colours people already have (the first person
/// gets blue), so neighbours in a household never look alike.
pub fn pick_hue(users: &[User]) -> u16 {
    let used: Vec<u16> = users.iter().filter_map(|u| u.hue).collect();
    if used.is_empty() {
        return 222;
    }
    let dist = |a: u16, b: u16| {
        let d = (i32::from(a) - i32::from(b)).abs();
        d.min(360 - d)
    };
    PALETTE
        .iter()
        .copied()
        .max_by_key(|h| used.iter().map(|u| dist(*h, *u)).min().unwrap_or(360))
        .unwrap_or(222)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub role: Role,
    /// The person's colour (hue, 0-359); older servers did not have one.
    #[serde(default)]
    pub hue: Option<u16>,
}

/// The server's own relay, announced to every device in the signed snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelaySetting {
    /// e.g. `http://203.0.113.7:3340` (traffic is end-to-end encrypted, TLS adds nothing).
    pub url: String,
    /// Use only this relay (not the public ones).
    pub exclusive: bool,
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
    #[serde(default)]
    pub relay: Option<RelaySetting>,
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
    /// Create a person (no devices yet). The first person may manage the household from their app.
    CreateUser {
        name: String,
    },
    /// Allow or forbid a person to manage the household from their app.
    SetRole {
        id: String,
        owner: bool,
    },
    /// Cancel an invitation that was not used yet.
    RevokeInvite {
        secret: String,
    },
    RenameUser {
        id: String,
        name: String,
    },
    RenameDevice {
        id: String,
        name: String,
    },
    SetUserColor {
        id: String,
        hue: u16,
    },
    /// Announce (or withdraw) the server's own relay.
    SetRelay {
        relay: Option<RelaySetting>,
    },
}
