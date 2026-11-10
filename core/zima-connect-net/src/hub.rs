//! Hub-side household state: users, devices, invites, policy. Persisted as JSON.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use iroh::SecretKey;
use serde::{Deserialize, Serialize};

use crate::household::{AdminOp, Device, Role, Rule, SignedSnapshot, Snapshot, User};

const INVITE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum InviteTarget {
    NewUser { name: Option<String>, role: Role },
    Existing { user_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Invite {
    target: InviteTarget,
    expires: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HubData {
    household: String,
    version: u64,
    users: Vec<User>,
    devices: Vec<Device>,
    rules: Vec<Rule>,
    invites: HashMap<String, Invite>,
}

/// Pending invite as shown in the dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct InviteView {
    pub secret: String,
    /// Human description, e.g. "new person: Mom" or "device for Jiří".
    pub label: String,
    pub expires: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HubView {
    pub household: String,
    pub version: u64,
    pub users: Vec<User>,
    pub devices: Vec<Device>,
    pub rules: Vec<Rule>,
    pub invites: Vec<InviteView>,
}

/// Result of an admin operation.
pub enum Outcome {
    Changed,
    /// Secret of a freshly created invite (the node wraps it into a join code).
    Invite(String),
}

pub struct HubCore {
    path: PathBuf,
    key: SecretKey,
    data: Mutex<HubData>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rand_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl HubCore {
    pub fn load(path: PathBuf, key: SecretKey, household: &str) -> Result<Self> {
        let mut data: HubData = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if data.household.is_empty() {
            data.household = household.to_string();
        }
        let hub = Self {
            path,
            key,
            data: Mutex::new(data),
        };
        hub.prune_and_bootstrap()?;
        Ok(hub)
    }

    fn save(&self, d: &HubData) -> Result<()> {
        std::fs::write(&self.path, serde_json::to_vec_pretty(d)?)?;
        Ok(())
    }

    /// Drop expired invites; make sure a setup invite exists until an owner has joined.
    fn prune_and_bootstrap(&self) -> Result<()> {
        let mut d = self.data.lock().unwrap();
        let t = now();
        d.invites.retain(|_, i| i.expires > t);
        let has_owner = d.users.iter().any(|u| u.role == Role::Owner);
        let has_owner_invite = d.invites.values().any(|i| {
            matches!(
                i.target,
                InviteTarget::NewUser {
                    role: Role::Owner,
                    ..
                }
            )
        });
        if !has_owner && !has_owner_invite {
            d.invites.insert(
                rand_hex(16),
                Invite {
                    target: InviteTarget::NewUser {
                        name: None,
                        role: Role::Owner,
                    },
                    expires: t + INVITE_TTL_SECS,
                },
            );
        }
        self.save(&d)
    }

    fn snapshot_of(&self, d: &HubData) -> Snapshot {
        Snapshot {
            version: d.version,
            household: d.household.clone(),
            hub: self.key.public().to_string(),
            users: d.users.clone(),
            devices: d.devices.clone(),
            rules: d.rules.clone(),
        }
    }

    pub fn signed(&self) -> Result<SignedSnapshot> {
        let d = self.data.lock().unwrap();
        SignedSnapshot::sign(&self.snapshot_of(&d), &self.key)
    }

    pub fn view(&self) -> HubView {
        let mut d = self.data.lock().unwrap();
        let t = now();
        d.invites.retain(|_, i| i.expires > t);
        let mut invites: Vec<InviteView> = d
            .invites
            .iter()
            .map(|(s, i)| InviteView {
                secret: s.clone(),
                expires: i.expires,
                label: match &i.target {
                    InviteTarget::NewUser {
                        name,
                        role: Role::Owner,
                    } => {
                        format!(
                            "setup: first owner{}",
                            name.as_ref().map(|n| format!(" ({n})")).unwrap_or_default()
                        )
                    }
                    InviteTarget::NewUser { name, .. } => {
                        format!(
                            "new person: {}",
                            name.clone().unwrap_or_else(|| "(named at join)".into())
                        )
                    }
                    InviteTarget::Existing { user_id } => {
                        let n = d
                            .users
                            .iter()
                            .find(|u| &u.id == user_id)
                            .map(|u| u.name.clone())
                            .unwrap_or_default();
                        format!("new device for {n}")
                    }
                },
            })
            .collect();
        invites.sort_by_key(|i| i.expires);
        HubView {
            household: d.household.clone(),
            version: d.version,
            users: d.users.clone(),
            devices: d.devices.clone(),
            rules: d.rules.clone(),
            invites,
        }
    }

    pub fn is_owner(&self, id: &str) -> bool {
        let d = self.data.lock().unwrap();
        d.devices
            .iter()
            .find(|x| x.id == id)
            .and_then(|x| d.users.iter().find(|u| u.id == x.user_id))
            .map(|u| u.role == Role::Owner)
            .unwrap_or(false)
    }

    /// May `from` queue `cap` for `to`? Returns the sender's device name.
    pub fn check_deposit(
        &self,
        from: &str,
        to: &str,
        cap: crate::household::Cap,
    ) -> Result<String> {
        let d = self.data.lock().unwrap();
        let snap = self.snapshot_of(&d);
        let f = snap
            .device(from)
            .ok_or_else(|| anyhow!("device is not part of this household"))?;
        let t = snap.device(to).ok_or_else(|| anyhow!("unknown device"))?;
        if !snap.allowed(&f.user_id, &t.user_id, cap) {
            bail!("not allowed by your household policy");
        }
        Ok(f.name.clone())
    }

    /// A device joins using an invite secret.
    pub fn join(
        &self,
        secret: &str,
        device_id: &str,
        name: &str,
        platform: &str,
        user_name: Option<&str>,
    ) -> Result<SignedSnapshot> {
        let mut d = self.data.lock().unwrap();
        let invite = d
            .invites
            .get(secret)
            .cloned()
            .ok_or_else(|| anyhow!("invalid or expired invite"))?;
        if invite.expires <= now() {
            d.invites.remove(secret);
            bail!("invalid or expired invite");
        }
        let user_id = match invite.target {
            InviteTarget::Existing { user_id } => {
                if !d.users.iter().any(|u| u.id == user_id) {
                    bail!("the person for this invite no longer exists");
                }
                user_id
            }
            InviteTarget::NewUser {
                name: invited,
                role,
            } => {
                let uname = invited
                    .or_else(|| user_name.map(str::to_string))
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| name.to_string());
                let id = rand_hex(4);
                d.users.push(User {
                    id: id.clone(),
                    name: uname.trim().to_string(),
                    role,
                });
                id
            }
        };
        d.invites.remove(secret);
        d.devices.retain(|x| x.id != device_id);
        d.devices.push(Device {
            id: device_id.to_string(),
            user_id,
            name: name.to_string(),
            platform: platform.to_string(),
        });
        d.version += 1;
        self.save(&d)?;
        SignedSnapshot::sign(&self.snapshot_of(&d), &self.key)
    }

    /// Latest snapshot if newer than `have`.
    pub fn sync(&self, device_id: &str, have: u64) -> Result<Option<SignedSnapshot>> {
        let d = self.data.lock().unwrap();
        if !d.devices.iter().any(|x| x.id == device_id) {
            bail!("device is not part of this household");
        }
        if d.version <= have {
            return Ok(None);
        }
        Ok(Some(SignedSnapshot::sign(
            &self.snapshot_of(&d),
            &self.key,
        )?))
    }

    /// Apply an admin operation (authorisation is checked by the caller).
    pub fn apply(&self, op: &AdminOp) -> Result<Outcome> {
        self.apply_with(op, false)
    }

    /// Same, for the server administrator (dashboard): safety guards for owner devices are lifted,
    /// e.g. to remove a lost owner phone.
    pub fn apply_as_admin(&self, op: &AdminOp) -> Result<Outcome> {
        self.apply_with(op, true)
    }

    /// A device removes itself.
    pub fn leave(&self, device_id: &str) -> Result<()> {
        let mut d = self.data.lock().unwrap();
        let before = d.devices.len();
        d.devices.retain(|x| x.id != device_id);
        if d.devices.len() != before {
            d.version += 1;
            self.save(&d)?;
        }
        Ok(())
    }

    fn apply_with(&self, op: &AdminOp, admin: bool) -> Result<Outcome> {
        let mut d = self.data.lock().unwrap();
        let outcome = match op {
            AdminOp::CreateInvite {
                user_id,
                new_user_name,
            } => {
                let target = match user_id {
                    Some(u) => {
                        if !d.users.iter().any(|x| &x.id == u) {
                            bail!("unknown person");
                        }
                        InviteTarget::Existing { user_id: u.clone() }
                    }
                    None => InviteTarget::NewUser {
                        name: new_user_name.clone().filter(|n| !n.trim().is_empty()),
                        role: Role::Member,
                    },
                };
                let secret = rand_hex(16);
                d.invites.insert(
                    secret.clone(),
                    Invite {
                        target,
                        expires: now() + INVITE_TTL_SECS,
                    },
                );
                self.save(&d)?; // invites don't change the signed snapshot
                return Ok(Outcome::Invite(secret));
            }
            AdminOp::SetRule {
                from_user,
                to_user,
                cap,
                allow,
            } => {
                self.check_users(&d, &[from_user, to_user])?;
                d.rules.retain(|r| {
                    !(r.from_user == *from_user && r.to_user == *to_user && r.cap == *cap)
                });
                d.rules.push(Rule {
                    from_user: from_user.clone(),
                    to_user: to_user.clone(),
                    cap: *cap,
                    allow: *allow,
                });
                Outcome::Changed
            }
            AdminOp::ClearRule {
                from_user,
                to_user,
                cap,
            } => {
                d.rules.retain(|r| {
                    !(r.from_user == *from_user && r.to_user == *to_user && r.cap == *cap)
                });
                Outcome::Changed
            }
            AdminOp::RemoveDevice { id } => {
                let owner_dev = d
                    .devices
                    .iter()
                    .find(|x| &x.id == id)
                    .map(|x| x.user_id.clone());
                let Some(uid) = owner_dev else {
                    bail!("unknown device")
                };
                let owner_devices = d.devices.iter().filter(|x| x.user_id == uid).count();
                let is_owner = d.users.iter().any(|u| u.id == uid && u.role == Role::Owner);
                if is_owner && owner_devices == 1 && !admin {
                    bail!("cannot remove the last device of the owner");
                }
                d.devices.retain(|x| &x.id != id);
                Outcome::Changed
            }
            AdminOp::RemoveUser { id } => {
                let Some(u) = d.users.iter().find(|u| &u.id == id) else {
                    bail!("unknown person")
                };
                if u.role == Role::Owner {
                    bail!("the owner cannot be removed");
                }
                d.users.retain(|u| &u.id != id);
                d.devices.retain(|x| &x.user_id != id);
                d.rules.retain(|r| &r.from_user != id && &r.to_user != id);
                Outcome::Changed
            }
            AdminOp::RevokeInvite { secret } => {
                d.invites.remove(secret);
                self.save(&d)?;
                return Ok(Outcome::Changed);
            }
            AdminOp::RenameUser { id, name } => {
                let n = name.trim();
                if n.is_empty() {
                    bail!("name cannot be empty");
                }
                let Some(u) = d.users.iter_mut().find(|u| &u.id == id) else {
                    bail!("unknown person")
                };
                u.name = n.to_string();
                Outcome::Changed
            }
        };
        d.version += 1;
        self.save(&d)?;
        Ok(outcome)
    }

    fn check_users(&self, d: &HubData, ids: &[&String]) -> Result<()> {
        for id in ids {
            if !d.users.iter().any(|u| &u.id == *id) {
                bail!("unknown person");
            }
        }
        Ok(())
    }
}
