//! Peer-to-peer transport and household model for Zima Connect.
//!
//! Devices dial each other by public key ([`iroh`]); the path (same network via
//! mDNS, hole-punched direct, relay fallback) is chosen automatically. Nobody
//! needs an account, a domain or an open port.
//!
//! Two ways to connect devices:
//! * **Household** (recommended): every device joins the home server once
//!   ([`Node::join`]). The server signs a list of people, their devices and the
//!   policy between people; devices find and authorise each other from it.
//! * **Direct pairing** (no server): [`Node::pairing_ticket`] / [`Node::pair`].

mod household;
mod hub;
mod mailbox;
mod proto;
mod seal;
mod shelf;
mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use iroh::{endpoint::presets, Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

pub use household::RelaySetting;
pub use household::{AdminOp, Cap, Device, Role, Rule, SignedSnapshot, Snapshot, User};
use hub::{HubCore, Outcome};
use mailbox::{Item, Kind, Mailbox, Payload};
pub use proto::RemoteInput;
use proto::{read_msg, write_msg, Request, Response};
pub use shelf::{Audience, ShelfItem};
pub use store::Peer;
use store::Store;

/// ALPN identifying the Zima Connect protocol.
pub const ALPN: &[u8] = b"zima-connect/1";
const TICKET_PREFIX: &str = "zc1-";
const JOIN_PREFIX: &str = "zch1-";
const TICKET_TTL: Duration = Duration::from_secs(15 * 60);
const SYNC_EVERY: Duration = Duration::from_secs(45);

fn ctx<T, E: std::fmt::Display>(r: std::result::Result<T, E>) -> Result<T> {
    r.map_err(|e| anyhow!("{e}"))
}

/// Node configuration.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Where identity key, peers and household state are persisted.
    pub data_dir: PathBuf,
    /// Where received files are saved.
    pub download_dir: PathBuf,
    /// Human-friendly device name shown to peers.
    pub device_name: String,
    /// "windows" | "linux" | "macos" | "android" | "server"
    pub platform: String,
    /// Use public relays + DNS address lookup (internet). `false` = LAN/direct only (tests).
    pub internet: bool,
    /// `Some(household name)` makes this node the household server (hub).
    pub hub: Option<String>,
}

/// Something that happened that the UI should show.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Paired {
        peer: Peer,
    },
    Joined {
        household: String,
    },
    /// Household list or policy changed (or this device was removed: `member` = false).
    HouseholdUpdated {
        member: bool,
    },
    Clipboard {
        from: String,
        from_name: String,
        text: String,
    },
    FileReceived {
        from: String,
        from_name: String,
        name: String,
        path: String,
        size: u64,
    },
    Ring {
        from: String,
        from_name: String,
    },
    /// A link to open on this device.
    OpenUrl {
        from: String,
        from_name: String,
        url: String,
    },
    /// Battery level of another device.
    Battery {
        from: String,
        from_name: String,
        percent: u8,
        charging: bool,
    },
    /// Remote control from another device (touchpad, keys, media).
    Remote {
        from: String,
        from_name: String,
        input: RemoteInput,
    },
    /// The sender stopped its "find my device" ring.
    RingStop {
        from: String,
        from_name: String,
    },
    Notification {
        from: String,
        from_name: String,
        app: String,
        title: String,
        body: String,
    },
}

/// Someone this device can talk to.
#[derive(Debug, Clone, Serialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    /// Belongs to the same person as this device.
    pub mine: bool,
    /// Paired directly (not through a household).
    pub direct: bool,
    /// What this device is allowed to send to them.
    pub caps: Vec<Cap>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HouseholdInfo {
    pub household: String,
    pub version: u64,
    pub me: User,
    pub users: Vec<User>,
    pub can_admin: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InviteOut {
    pub secret: String,
    pub user: Option<String>,
    pub label: String,
    pub code: String,
    pub expires: u64,
}

/// Everything the server dashboard shows.
#[derive(Debug, Clone, Serialize)]
pub struct HubDashboard {
    pub id: String,
    pub household: String,
    pub version: u64,
    pub users: Vec<User>,
    pub devices: Vec<Device>,
    pub rules: Vec<Rule>,
    pub invites: Vec<InviteOut>,
    pub relay: Option<crate::household::RelaySetting>,
}

#[derive(Serialize, Deserialize)]
struct Ticket {
    addr: EndpointAddr,
    secret: String,
}

#[derive(Serialize, Deserialize)]
struct JoinCode {
    addr: EndpointAddr,
    secret: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Membership {
    hub: EndpointAddr,
    signed: SignedSnapshot,
    #[serde(skip)]
    snapshot: Snapshot,
}

/// The peer could not be contacted (offline, no route).
#[derive(Debug)]
struct Unreachable(String);

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "device unreachable: {}", self.0)
    }
}

impl std::error::Error for Unreachable {}

/// What follows a request on the stream.
enum Body {
    /// Raw file bytes (direct, already end-to-end encrypted by QUIC).
    Plain(tokio::fs::File, u64),
    /// File sealed for the recipient (for the server queue).
    Sealed {
        file: tokio::fs::File,
        size: u64,
        sealer: Box<seal::Sealer>,
    },
}

/// How a send ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Handed to the device directly.
    Delivered,
    /// Device is offline: the server keeps it until the device is back.
    Queued,
}

/// The peer could not be contacted (offline, no route).
#[derive(Debug)]
struct Who {
    id: String,
    name: String,
}

struct Inner {
    endpoint: Endpoint,
    cfg: NodeConfig,
    store: Mutex<Store>,
    pending: Mutex<HashMap<String, Instant>>, // direct-pair secret -> created
    membership: Mutex<Option<Membership>>,
    hub: Option<HubCore>,
    mailbox: Option<Mailbox>,
    shelf: Option<shelf::Shelf>,
    events: broadcast::Sender<Event>,
    /// Current device name (user can rename it).
    name: Mutex<String>,
    /// Own relay currently applied to the endpoint.
    relay_applied: Mutex<Option<crate::household::RelaySetting>>,
}

/// A running Zima Connect peer.
#[derive(Clone)]
pub struct Node {
    inner: Arc<Inner>,
}

impl Node {
    /// Start the node: load/create identity, bind, begin accepting.
    pub async fn start(mut cfg: NodeConfig) -> Result<Node> {
        tokio::fs::create_dir_all(&cfg.data_dir).await?;
        // A name the user chose earlier wins over the default one.
        if let Ok(n) = std::fs::read_to_string(cfg.data_dir.join("device_name")) {
            if !n.trim().is_empty() {
                cfg.device_name = n.trim().to_string();
            }
        }
        tokio::fs::create_dir_all(&cfg.download_dir).await?;
        let secret = load_or_create_key(&cfg.data_dir.join("identity.key"))?;

        let builder = if cfg.internet {
            Endpoint::builder(presets::N0)
        } else {
            // Relay transport present but empty: a relay can still be added later (own relay).
            Endpoint::builder(presets::Minimal)
                .relay_mode(iroh::RelayMode::Custom(iroh_relay::RelayMap::empty()))
        };
        let endpoint = ctx(builder
            .secret_key(secret.clone())
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await)?;

        // Same-network discovery without any internet.
        match iroh_mdns_address_lookup::MdnsAddressLookup::builder().build(endpoint.id()) {
            Ok(mdns) => {
                if let Ok(al) = endpoint.address_lookup() {
                    al.add(mdns);
                }
            }
            Err(e) => tracing::warn!("mDNS unavailable: {e}"),
        }

        let hub = match &cfg.hub {
            Some(name) => Some(HubCore::load(cfg.data_dir.join("hub.json"), secret, name)?),
            None => None,
        };
        let mailbox = match &cfg.hub {
            Some(_) => Some(Mailbox::open(cfg.data_dir.join("mailbox"))?),
            None => None,
        };
        let shelf = match &cfg.hub {
            Some(_) => Some(shelf::Shelf::open(cfg.data_dir.join("shelf"))?),
            None => None,
        };
        let membership = load_membership(&cfg.data_dir.join("household.json"));

        let name = cfg.device_name.clone();
        let (events, _) = broadcast::channel(64);
        let store = Store::load(cfg.data_dir.join("peers.json"))?;
        let node = Node {
            inner: Arc::new(Inner {
                endpoint,
                cfg,
                store: Mutex::new(store),
                pending: Mutex::new(HashMap::new()),
                membership: Mutex::new(membership),
                hub,
                mailbox,
                shelf,
                events,
                name: Mutex::new(name),
                relay_applied: Mutex::new(None),
            }),
        };
        node.refresh_relay();
        node.spawn_accept_loop();
        node.spawn_sync_loop();
        Ok(node)
    }

    /// Wait until the home relay is connected (so shared codes contain it). `false` on timeout.
    pub async fn wait_online(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, self.inner.endpoint.online())
            .await
            .is_ok()
    }

    /// This device's public identity.
    pub fn id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }

    /// This device's display name.
    pub fn device_name(&self) -> String {
        self.inner.name.lock().unwrap().clone()
    }

    /// Rename this device (kept across restarts, announced to the server).
    pub async fn rename_device(&self, name: &str) -> Result<()> {
        let n: String = name.trim().chars().take(40).collect();
        if n.is_empty() {
            bail!("name cannot be empty");
        }
        std::fs::write(self.inner.cfg.data_dir.join("device_name"), &n)?;
        *self.inner.name.lock().unwrap() = n.clone();
        if let Some(hub) = &self.inner.hub {
            let _ = hub.rename_device(&self.id().to_string(), &n);
            return Ok(());
        }
        let hub = self
            .inner
            .membership
            .lock()
            .unwrap()
            .as_ref()
            .map(|m| m.hub.clone());
        if let Some(hub) = hub {
            match self.request(hub, Request::Rename { name: n }, None).await? {
                Response::Snapshot { snapshot } => self.apply_snapshot(snapshot)?,
                Response::Err { message } => bail!("{message}"),
                _ => {}
            }
        }
        Ok(())
    }

    /// Relay the server announced (members) or configured (server itself).
    fn desired_relay(&self) -> Option<crate::household::RelaySetting> {
        if let Some(hub) = &self.inner.hub {
            return hub.view().relay;
        }
        self.inner
            .membership
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.snapshot.relay.clone())
    }

    /// Apply the server's own relay: add it to the endpoint, and drop the public ones if exclusive.
    fn refresh_relay(&self) {
        let node = self.clone();
        tokio::spawn(async move {
            let want = node.desired_relay();
            let prev = node.inner.relay_applied.lock().unwrap().clone();
            if want == prev {
                return;
            }
            let ep = &node.inner.endpoint;
            let public: Vec<RelayUrl> = if node.inner.cfg.internet {
                iroh::defaults::prod::default_relay_map().urls()
            } else {
                Vec::new()
            };
            if let Some(old) = prev.as_ref().and_then(|p| p.url.parse::<RelayUrl>().ok()) {
                ep.remove_relay(&old).await;
            }
            // Back to the defaults, then apply what is wanted.
            for u in &public {
                ep.insert_relay(
                    u.clone(),
                    Arc::new(iroh_relay::RelayConfig::from(u.clone())),
                )
                .await;
            }
            if let Some(w) = &want {
                match w.url.parse::<RelayUrl>() {
                    Ok(u) => {
                        ep.insert_relay(u.clone(), Arc::new(iroh_relay::RelayConfig::new(u, None)))
                            .await;
                        if w.exclusive {
                            for u in &public {
                                ep.remove_relay(u).await;
                            }
                        }
                    }
                    Err(e) => tracing::warn!("bad relay url {}: {e}", w.url),
                }
            }
            *node.inner.relay_applied.lock().unwrap() = want;
        });
    }

    /// Subscribe to incoming events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    // -- household (client side) -------------------------------------------

    /// Join the household of the server that created `code`.
    pub async fn join(&self, code: &str, user_name: Option<String>) -> Result<HouseholdInfo> {
        let jc = parse_join_code(code)?;
        let hub_id = jc.addr.id;
        let resp = self
            .request(
                jc.addr.clone(),
                Request::Join {
                    code: jc.secret,
                    name: self.device_name(),
                    platform: self.inner.cfg.platform.clone(),
                    user_name: user_name.filter(|n| !n.trim().is_empty()),
                },
                None,
            )
            .await?;
        let signed = match resp {
            Response::Joined { snapshot } => snapshot,
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        };
        let snapshot = signed.open(&hub_id)?;
        if snapshot.device(&self.id().to_string()).is_none() {
            bail!("server did not register this device");
        }
        let m = Membership {
            hub: jc.addr,
            signed,
            snapshot,
        };
        save_membership(&self.inner.cfg.data_dir.join("household.json"), &m)?;
        let name = m.snapshot.household.clone();
        *self.inner.membership.lock().unwrap() = Some(m);
        let _ = self.inner.events.send(Event::Joined { household: name });
        self.household_info().ok_or_else(|| anyhow!("join failed"))
    }

    /// Leave the household (this device only).
    /// Leave the household: tell the server (best effort) and forget it locally.
    pub async fn leave_household_async(&self) {
        let hub = self
            .inner
            .membership
            .lock()
            .unwrap()
            .as_ref()
            .map(|m| m.hub.clone());
        if let Some(hub) = hub {
            let _ = self.request(hub, Request::Leave, None).await;
        }
        self.leave_household();
    }

    /// Forget the household on this device only (does not tell the server).
    pub fn leave_household(&self) {
        *self.inner.membership.lock().unwrap() = None;
        let _ = std::fs::remove_file(self.inner.cfg.data_dir.join("household.json"));
        let _ = self
            .inner
            .events
            .send(Event::HouseholdUpdated { member: false });
    }

    pub fn is_joined(&self) -> bool {
        self.inner.membership.lock().unwrap().is_some()
    }

    /// Household overview for the UI.
    pub fn household_info(&self) -> Option<HouseholdInfo> {
        let g = self.inner.membership.lock().unwrap();
        let s = &g.as_ref()?.snapshot;
        let me = s.user_of(&self.id().to_string())?.clone();
        Some(HouseholdInfo {
            household: s.household.clone(),
            version: s.version,
            can_admin: me.role == Role::Owner,
            me,
            users: s.users.clone(),
        })
    }

    /// Fetch the latest household data from the server. `true` if something changed.
    pub async fn sync(&self) -> Result<bool> {
        let (hub, have) = {
            let g = self.inner.membership.lock().unwrap();
            let m = g.as_ref().ok_or_else(|| anyhow!("not joined"))?;
            (m.hub.clone(), m.snapshot.version)
        };
        match self.request(hub, Request::Sync { have }, None).await? {
            Response::Snapshot { snapshot } => {
                self.apply_snapshot(snapshot)?;
                Ok(true)
            }
            Response::NotModified => Ok(false),
            Response::Err { message } => {
                if message.contains("not part of this household") {
                    self.leave_household();
                }
                bail!("{message}")
            }
            _ => bail!("unexpected response"),
        }
    }

    fn apply_snapshot(&self, signed: SignedSnapshot) -> Result<()> {
        let mut g = self.inner.membership.lock().unwrap();
        let m = g.as_mut().ok_or_else(|| anyhow!("not joined"))?;
        let snapshot = signed.open(&m.hub.id)?;
        if snapshot.version < m.snapshot.version {
            return Ok(()); // never go back in time
        }
        m.signed = signed;
        m.snapshot = snapshot;
        save_membership(&self.inner.cfg.data_dir.join("household.json"), m)?;
        drop(g);
        self.refresh_relay();
        let _ = self
            .inner
            .events
            .send(Event::HouseholdUpdated { member: true });
        Ok(())
    }

    /// Owner only: change people, devices or policy on the server.
    /// Returns a join code when the operation was `CreateInvite`.
    pub async fn admin(&self, op: AdminOp) -> Result<Option<String>> {
        if let Some(hub) = &self.inner.hub {
            return self.hub_admin_local(hub, &op, false).await;
        }
        let hub = {
            let g = self.inner.membership.lock().unwrap();
            g.as_ref().ok_or_else(|| anyhow!("not joined"))?.hub.clone()
        };
        match self.request(hub, Request::Admin { op }, None).await? {
            Response::Snapshot { snapshot } => {
                self.apply_snapshot(snapshot)?;
                Ok(None)
            }
            Response::InviteCode { code } => Ok(Some(code)),
            Response::Created { id } => Ok(Some(id)),
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        }
    }

    // -- hub (server side) ----------------------------------------------------

    /// Relay(s) this device is currently reachable through.
    pub fn relay_urls(&self) -> Vec<String> {
        self.inner
            .endpoint
            .addr()
            .relay_urls()
            .map(|u| u.to_string())
            .collect()
    }

    /// Public IPv4 addresses this device was seen on (hint for the relay setup).
    pub fn public_ips(&self) -> Vec<String> {
        self.inner
            .endpoint
            .addr()
            .ip_addrs()
            .filter_map(|a| match a.ip() {
                std::net::IpAddr::V4(v)
                    if !(v.is_private()
                        || v.is_loopback()
                        || v.is_link_local()
                        || v.is_unspecified()) =>
                {
                    Some(v.to_string())
                }
                _ => None,
            })
            .collect()
    }

    /// Server dashboard data (only on a hub node).
    pub fn hub_dashboard(&self) -> Option<HubDashboard> {
        let hub = self.inner.hub.as_ref()?;
        let v = hub.view();
        Some(HubDashboard {
            id: self.id().to_string(),
            household: v.household,
            version: v.version,
            users: v.users,
            devices: v.devices,
            rules: v.rules,
            relay: v.relay,
            invites: v
                .invites
                .into_iter()
                .map(|i| InviteOut {
                    user: i.user.clone(),
                    code: self.wrap_join_code(&i.secret),
                    secret: i.secret.clone(),
                    label: i.label,
                    expires: i.expires,
                })
                .collect(),
        })
    }

    /// Server dashboard: is this household device reachable right now? (round-trip ms)
    pub async fn hub_probe(&self, device_id: &str) -> Option<u64> {
        let id: EndpointId = device_id.parse().ok()?;
        let start = Instant::now();
        // Any reply (even "not paired", the hub is no member) proves the device is online.
        self.request(EndpointAddr::from(id), Request::Ping, None)
            .await
            .ok()
            .map(|_| start.elapsed().as_millis() as u64)
    }

    /// Server dashboard: (items waiting, bytes of files waiting) in the offline queue.
    pub fn hub_mailbox_stats(&self) -> Option<(usize, u64)> {
        self.inner.mailbox.as_ref().map(|m| m.stats())
    }

    /// Server dashboard: apply an operation directly.
    pub async fn hub_admin(&self, op: AdminOp) -> Result<Option<String>> {
        let hub = self
            .inner
            .hub
            .as_ref()
            .ok_or_else(|| anyhow!("not a household server"))?;
        self.hub_admin_local(hub, &op, true).await
    }

    async fn hub_admin_local(
        &self,
        hub: &HubCore,
        op: &AdminOp,
        admin: bool,
    ) -> Result<Option<String>> {
        let outcome = if admin {
            hub.apply_as_admin(op)?
        } else {
            hub.apply(op)?
        };
        self.refresh_relay();
        match outcome {
            Outcome::Invite(secret) => Ok(Some(self.wrap_join_code(&secret))),
            Outcome::Created(id) => Ok(Some(id)),
            Outcome::Changed => Ok(None),
        }
    }

    fn wrap_join_code(&self, secret: &str) -> String {
        let jc = JoinCode {
            addr: self.shareable_addr(),
            secret: secret.to_string(),
        };
        let json = serde_json::to_vec(&jc).unwrap_or_default();
        format!(
            "{JOIN_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
        )
    }

    // -- contacts and authorisation ------------------------------------------

    /// Everyone this device can talk to (household members + direct pairings).
    pub fn contacts(&self) -> Vec<Contact> {
        let me = self.id().to_string();
        let mut out: Vec<Contact> = Vec::new();
        if let Some(m) = self.inner.membership.lock().unwrap().as_ref() {
            let s = &m.snapshot;
            let my_user = s.device(&me).map(|d| d.user_id.clone());
            for d in s.devices.iter().filter(|d| d.id != me) {
                let caps = match &my_user {
                    Some(mu) => Cap::ALL
                        .into_iter()
                        .filter(|c| s.allowed(mu, &d.user_id, *c))
                        .collect(),
                    None => vec![],
                };
                out.push(Contact {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    platform: d.platform.clone(),
                    user_id: Some(d.user_id.clone()),
                    user_name: s.user(&d.user_id).map(|u| u.name.clone()),
                    mine: my_user.as_deref() == Some(d.user_id.as_str()),
                    direct: false,
                    caps,
                });
            }
        }
        for p in self.inner.store.lock().unwrap().peers() {
            if !out.iter().any(|c| c.id == p.id) {
                out.push(Contact {
                    id: p.id,
                    name: p.name,
                    platform: p.platform,
                    user_id: None,
                    user_name: None,
                    mine: false,
                    direct: true,
                    caps: Cap::ALL.to_vec(),
                });
            }
        }
        out
    }

    /// May this device send `cap` to `peer`? (Household policy; direct pairings are always allowed.)
    fn can_send(&self, peer_id: &str, cap: Option<Cap>) -> Result<()> {
        if self.inner.store.lock().unwrap().contains(peer_id) {
            return Ok(());
        }
        let g = self.inner.membership.lock().unwrap();
        let s = &g
            .as_ref()
            .ok_or_else(|| anyhow!("unknown device"))?
            .snapshot;
        let to = s.device(peer_id).ok_or_else(|| anyhow!("unknown device"))?;
        let me = s
            .device(&self.id().to_string())
            .ok_or_else(|| anyhow!("this device is no longer in the household"))?;
        if let Some(cap) = cap {
            if !s.allowed(&me.user_id, &to.user_id, cap) {
                bail!("not allowed by your household policy");
            }
        }
        Ok(())
    }

    /// Decide whether `remote` may use `cap` on this device.
    async fn authorize(&self, remote: &str, cap: Option<Cap>) -> std::result::Result<Who, String> {
        if let Some(p) = self.inner.store.lock().unwrap().get(remote) {
            return Ok(Who {
                id: p.id,
                name: p.name,
            });
        }
        for attempt in 0..2 {
            let r = self.check_household(remote, cap);
            if let Some(Ok(w)) = r {
                return Ok(w);
            }
            if attempt == 1 || !self.is_joined() {
                return r.unwrap_or_else(|| Err("not paired".into()));
            }
            // Unknown or denied: our copy may be stale (new device / policy change).
            let _ = self.sync().await;
        }
        Err("not paired".into())
    }

    fn check_household(
        &self,
        remote: &str,
        cap: Option<Cap>,
    ) -> Option<std::result::Result<Who, String>> {
        let g = self.inner.membership.lock().unwrap();
        let s = &g.as_ref()?.snapshot;
        let dev = s.device(remote)?;
        let Some(me) = s.device(&self.id().to_string()) else {
            return Some(Err("this device is no longer in the household".into()));
        };
        if let Some(cap) = cap {
            if !s.allowed(&dev.user_id, &me.user_id, cap) {
                let owner = s
                    .user(&me.user_id)
                    .map(|u| u.name.clone())
                    .unwrap_or_default();
                return Some(Err(format!("{owner}'s policy does not allow this")));
            }
        }
        Some(Ok(Who {
            id: dev.id.clone(),
            name: dev.name.clone(),
        }))
    }

    // -- direct pairing (no server) -------------------------------------------

    /// Direct pairs only.
    pub fn peers(&self) -> Vec<Peer> {
        self.inner.store.lock().unwrap().peers()
    }

    /// Forget a direct pairing.
    pub fn unpair(&self, id: &str) -> Result<()> {
        self.inner.store.lock().unwrap().remove(id)
    }

    /// Create a one-time pairing ticket (valid 15 min) to show to another device.
    pub async fn pairing_ticket(&self) -> Result<String> {
        if self.inner.cfg.internet {
            // Wait briefly so the ticket contains the home relay.
            let _ =
                tokio::time::timeout(Duration::from_secs(5), self.inner.endpoint.online()).await;
        }
        let mut raw = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut raw);
        let secret: String = raw.iter().map(|x| format!("{x:02x}")).collect();
        {
            let mut p = self.inner.pending.lock().unwrap();
            p.retain(|_, t| t.elapsed() < TICKET_TTL);
            p.insert(secret.clone(), Instant::now());
        }
        let t = Ticket {
            addr: self.shareable_addr(),
            secret,
        };
        let json = serde_json::to_vec(&t)?;
        Ok(format!(
            "{TICKET_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
        ))
    }

    /// Keep codes short: relay + a few IPv4 addresses are enough; everything
    /// else (IPv6, virtual adapters) is found through mDNS / address lookup.
    fn shareable_addr(&self) -> EndpointAddr {
        let full = self.inner.endpoint.addr();
        let mut addr = EndpointAddr::new(full.id);
        for r in full.relay_urls() {
            addr = addr.with_relay_url(r.clone());
        }
        for ip in full.ip_addrs().filter(|a| a.is_ipv4()).take(3) {
            addr = addr.with_ip_addr(*ip);
        }
        addr
    }

    /// Pair with the device that produced `ticket`.
    pub async fn pair(&self, ticket: &str) -> Result<Peer> {
        let t = parse_ticket(ticket)?;
        let id = t.addr.id;
        if id == self.id() {
            bail!("cannot pair with itself");
        }
        let req = Request::Pair {
            secret: t.secret,
            name: self.device_name(),
            platform: self.inner.cfg.platform.clone(),
        };
        match self.request(t.addr, req, None).await? {
            Response::PairOk { name, platform } => {
                let peer = Peer {
                    id: id.to_string(),
                    name,
                    platform,
                };
                self.inner.store.lock().unwrap().add(peer.clone())?;
                let _ = self.inner.events.send(Event::Paired { peer: peer.clone() });
                Ok(peer)
            }
            Response::Err { message } => bail!("pairing refused: {message}"),
            _ => bail!("unexpected response"),
        }
    }

    // -- device to device actions ------------------------------------------------

    /// Round-trip time to a peer; also the "is online" check.
    pub async fn ping(&self, peer_id: &str) -> Result<Duration> {
        let start = Instant::now();
        self.expect_ok(
            self.request_peer(peer_id, Request::Ping, None, None)
                .await?,
        )?;
        Ok(start.elapsed())
    }

    /// Send clipboard text to a peer (sealed and kept on the server if the peer is offline).
    pub async fn send_clipboard(&self, peer_id: &str, text: String) -> Result<Delivery> {
        let req = Request::Clipboard { text: text.clone() };
        self.deliver(peer_id, Cap::Clipboard, req, Payload::Clipboard { text })
            .await
    }

    /// Make the peer ring (live only; a queued ring would be useless).
    pub async fn ring(&self, peer_id: &str) -> Result<()> {
        self.expect_ok(
            self.request_peer(peer_id, Request::Ring, Some(Cap::Ring), None)
                .await?,
        )
    }

    /// Open a link on the peer's device (live only).
    pub async fn send_url(&self, peer_id: &str, url: &str) -> Result<()> {
        let u = url.trim();
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            bail!("only web links can be sent");
        }
        self.expect_ok(
            self.request_peer(
                peer_id,
                Request::OpenUrl { url: u.to_string() },
                Some(Cap::Clipboard),
                None,
            )
            .await?,
        )
    }

    /// Tell the peer this device's battery level (live only).
    pub async fn send_battery(&self, peer_id: &str, percent: u8, charging: bool) -> Result<()> {
        self.expect_ok(
            self.request_peer(
                peer_id,
                Request::Battery { percent, charging },
                Some(Cap::Status),
                None,
            )
            .await?,
        )
    }

    /// Control the peer: touchpad, keys, media (live only).
    pub async fn send_remote(&self, peer_id: &str, input: RemoteInput) -> Result<()> {
        self.expect_ok(
            self.request_peer(peer_id, Request::Remote { input }, Some(Cap::Remote), None)
                .await?,
        )
    }

    /// Stop a ring started with [`Node::ring`].
    pub async fn ring_stop(&self, peer_id: &str) -> Result<()> {
        self.expect_ok(
            self.request_peer(peer_id, Request::RingStop, Some(Cap::Ring), None)
                .await?,
        )
    }

    /// Mirror a notification to a peer (sealed and kept on the server if the peer is offline).
    pub async fn send_notification(
        &self,
        peer_id: &str,
        app: &str,
        title: &str,
        body: &str,
    ) -> Result<Delivery> {
        let req = Request::Notify {
            app: app.into(),
            title: title.into(),
            body: body.into(),
        };
        let payload = Payload::Notify {
            app: app.into(),
            title: title.into(),
            body: body.into(),
        };
        self.deliver(peer_id, Cap::Notifications, req, payload)
            .await
    }

    /// Stream a file to a peer (sealed and kept on the server if the peer is offline).
    pub async fn send_file(&self, peer_id: &str, path: &Path) -> Result<Delivery> {
        let meta = tokio::fs::metadata(path).await.context("reading file")?;
        let size = meta.len();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("bad file name"))?
            .to_string();
        let file = tokio::fs::File::open(path).await?;
        let req = Request::File {
            name: name.clone(),
            size,
        };
        match self
            .request_peer(
                peer_id,
                req,
                Some(Cap::Files),
                Some(Body::Plain(file, size)),
            )
            .await
        {
            Ok(r) => {
                self.expect_ok(r)?;
                Ok(Delivery::Delivered)
            }
            Err(e) if self.should_queue(peer_id, &e) => {
                let rcpt: EndpointId = peer_id.parse().map_err(|e| anyhow!("bad peer id: {e}"))?;
                let sealer = seal::Sealer::new(&rcpt)?;
                let payload = Payload::File {
                    name,
                    size,
                    body: seal::header_hash(&sealer.header),
                };
                let file = tokio::fs::File::open(path).await?;
                let body = Body::Sealed {
                    file,
                    size,
                    sealer: Box::new(sealer),
                };
                self.deposit(peer_id, Kind::File, payload, Some(body)).await
            }
            Err(e) => Err(e),
        }
    }

    /// Try the device directly; if it is offline hand the sealed item to the server.
    async fn deliver(
        &self,
        peer_id: &str,
        cap: Cap,
        req: Request,
        payload: Payload,
    ) -> Result<Delivery> {
        match self.request_peer(peer_id, req, Some(cap), None).await {
            Ok(r) => {
                self.expect_ok(r)?;
                Ok(Delivery::Delivered)
            }
            Err(e) if self.should_queue(peer_id, &e) => {
                let kind = if cap == Cap::Notifications {
                    Kind::Notify
                } else {
                    Kind::Clipboard
                };
                self.deposit(peer_id, kind, payload, None).await
            }
            Err(e) => Err(e),
        }
    }

    /// Queue only for household devices, when the failure was "unreachable".
    fn should_queue(&self, peer_id: &str, e: &anyhow::Error) -> bool {
        e.downcast_ref::<Unreachable>().is_some()
            && self
                .inner
                .membership
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|m| m.snapshot.device(peer_id).is_some())
    }

    /// Seal `payload` for the recipient, sign it, and leave it on the server.
    async fn deposit(
        &self,
        peer_id: &str,
        kind: Kind,
        payload: Payload,
        body: Option<Body>,
    ) -> Result<Delivery> {
        let hub = {
            let g = self.inner.membership.lock().unwrap();
            g.as_ref().ok_or_else(|| anyhow!("not joined"))?.hub.clone()
        };
        let rcpt: EndpointId = peer_id.parse().map_err(|e| anyhow!("bad peer id: {e}"))?;
        let meta = seal::seal_bytes(&rcpt, &serde_json::to_vec(&payload)?)?;
        let sig = self.inner.endpoint.secret_key().sign(&meta);
        let size = match &body {
            Some(Body::Sealed { size, .. }) => seal::sealed_size(*size),
            _ => 0,
        };
        let item = Item {
            kind,
            meta: base64::engine::general_purpose::STANDARD.encode(&meta),
            sig,
            size,
        };
        let req = Request::Deposit {
            to: peer_id.to_string(),
            item,
        };
        match self.request(hub, req, body).await {
            Ok(Response::Queued) => Ok(Delivery::Queued),
            Ok(Response::Err { message }) => bail!("{message}"),
            Ok(_) => bail!("unexpected response"),
            Err(e) if e.downcast_ref::<Unreachable>().is_some() => {
                bail!("device and server are both unreachable")
            }
            Err(e) => Err(e),
        }
    }

    fn hub_addr(&self) -> Result<EndpointAddr> {
        let g = self.inner.membership.lock().unwrap();
        Ok(g.as_ref().ok_or_else(|| anyhow!("not joined"))?.hub.clone())
    }

    /// Put a file on your personal shelf on the server (reachable from all your devices).
    pub async fn shelf_put(&self, path: &Path) -> Result<ShelfItem> {
        let meta = tokio::fs::metadata(path).await.context("reading file")?;
        let size = meta.len();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("bad file name"))?
            .to_string();
        let file = tokio::fs::File::open(path).await?;
        let req = Request::ShelfPut {
            name,
            size,
            audience: shelf::Audience::default(),
        };
        match self
            .request(self.hub_addr()?, req, Some(Body::Plain(file, size)))
            .await?
        {
            Response::ShelfItem { item } => Ok(item),
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        }
    }

    /// What is on the shelf for this person.
    pub async fn shelf_list(&self) -> Result<Vec<ShelfItem>> {
        match self
            .request(self.hub_addr()?, Request::ShelfList, None)
            .await?
        {
            Response::Shelf { items } => Ok(items),
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        }
    }

    /// Download a shelf item into the download folder. Returns the saved path.
    pub async fn shelf_get(&self, id: &str) -> Result<PathBuf> {
        self.shelf_get_in(id, &self.inner.cfg.download_dir).await
    }

    /// Download a shelf item into `dir` (e.g. a preview cache). Returns the saved path.
    pub async fn shelf_get_in(&self, id: &str, dir: &Path) -> Result<PathBuf> {
        tokio::fs::create_dir_all(dir).await?;
        let hub = self.hub_addr()?;
        let conn = tokio::time::timeout(
            Duration::from_secs(8),
            self.inner.endpoint.connect(hub, ALPN),
        )
        .await
        .map_err(|_| anyhow!("server unreachable"))?;
        let conn = ctx(conn)?;
        let (mut send, mut recv) = ctx(conn.open_bi().await)?;
        write_msg(&mut send, &Request::ShelfGet { id: id.to_string() }).await?;
        ctx(send.finish())?;
        let item = match read_msg::<_, Response>(&mut recv).await? {
            Response::ShelfItem { item } => item,
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        };
        let path = unique_path(dir, &item.name).await;
        let mut f = tokio::fs::File::create(&path).await?;
        let mut limited = (&mut recv).take(item.size);
        let n = tokio::io::copy(&mut limited, &mut f).await?;
        f.flush().await?;
        conn.close(0u32.into(), b"done");
        if n != item.size {
            let _ = tokio::fs::remove_file(&path).await;
            bail!("download was interrupted");
        }
        Ok(path)
    }

    /// Take a file off the shelf (the person who put it there, or the owner).
    pub async fn shelf_delete(&self, id: &str) -> Result<()> {
        match self
            .request(
                self.hub_addr()?,
                Request::ShelfDelete { id: id.to_string() },
                None,
            )
            .await?
        {
            Response::Ok => Ok(()),
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        }
    }

    /// Server dashboard: everything on the shelf.
    pub fn hub_shelf(&self) -> Option<Vec<ShelfItem>> {
        self.inner.shelf.as_ref().map(|s| s.all())
    }

    /// Server dashboard: remove a shelf item.
    pub fn hub_shelf_delete(&self, id: &str) {
        if let Some(s) = &self.inner.shelf {
            s.remove(id);
        }
    }

    /// Collect everything the server kept for this device while it was offline.
    pub async fn pull_inbox(&self) -> Result<usize> {
        let hub = {
            let g = self.inner.membership.lock().unwrap();
            match g.as_ref() {
                Some(m) => m.hub.clone(),
                None => return Ok(0),
            }
        };
        let entries = match self.request(hub.clone(), Request::Inbox, None).await? {
            Response::Inbox { entries } => entries,
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        };
        let mut n = 0;
        for e in entries {
            match self.fetch_entry(&hub, &e).await {
                Ok(()) => n += 1,
                Err(err) => tracing::debug!("inbox item {} failed: {err:#}", e.id),
            }
        }
        Ok(n)
    }

    async fn fetch_entry(&self, hub: &EndpointAddr, e: &mailbox::Entry) -> Result<()> {
        let conn = tokio::time::timeout(
            Duration::from_secs(8),
            self.inner.endpoint.connect(hub.clone(), ALPN),
        )
        .await
        .map_err(|_| anyhow!("server unreachable"))?;
        let conn = ctx(conn)?;
        let (mut send, mut recv) = ctx(conn.open_bi().await)?;
        write_msg(&mut send, &Request::Fetch { id: e.id.clone() }).await?;
        ctx(send.finish())?;
        let entry = match read_msg::<_, Response>(&mut recv).await? {
            Response::Entry { entry } => entry,
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        };
        let result = self.open_entry(&entry, &mut recv).await;
        conn.close(0u32.into(), b"done");
        // Delivered, or permanently unusable (bad signature, not allowed): the server can forget it.
        let _ = self
            .request(hub.clone(), Request::Ack { id: entry.id }, None)
            .await;
        result
    }

    /// Verify, authorise and decrypt one queued item, then emit it as a normal event.
    async fn open_entry<R: tokio::io::AsyncRead + Unpin>(
        &self,
        entry: &mailbox::Entry,
        recv: &mut R,
    ) -> Result<()> {
        let from: EndpointId = entry.from.parse().map_err(|_| anyhow!("bad sender id"))?;
        let meta = base64::engine::general_purpose::STANDARD
            .decode(&entry.item.meta)
            .map_err(|_| anyhow!("corrupted item"))?;
        from.verify(&meta, &entry.item.sig)
            .map_err(|_| anyhow!("item is not signed by its sender"))?;
        let cap = match entry.item.kind {
            Kind::Clipboard => Cap::Clipboard,
            Kind::Notify => Cap::Notifications,
            Kind::File => Cap::Files,
        };
        // Same rules as for live delivery, checked on this device.
        let who = self
            .authorize(&entry.from, Some(cap))
            .await
            .map_err(|m| anyhow!(m))?;
        let payload: Payload =
            serde_json::from_slice(&seal::open_bytes(self.inner.endpoint.secret_key(), &meta)?)?;
        let ev = &self.inner.events;
        match payload {
            Payload::Clipboard { text } if entry.item.kind == Kind::Clipboard => {
                let _ = ev.send(Event::Clipboard {
                    from: who.id,
                    from_name: who.name,
                    text,
                });
            }
            Payload::Notify { app, title, body } if entry.item.kind == Kind::Notify => {
                let _ = ev.send(Event::Notification {
                    from: who.id,
                    from_name: who.name,
                    app,
                    title,
                    body,
                });
            }
            Payload::File { name, size, body } if entry.item.kind == Kind::File => {
                let total = entry.item.size;
                if total != seal::sealed_size(size) {
                    bail!("corrupted item");
                }
                let mut header = [0u8; seal::HEADER];
                recv.read_exact(&mut header).await?;
                if seal::header_hash(&header) != body {
                    bail!("file does not belong to this item");
                }
                let mut opener = seal::Opener::new(self.inner.endpoint.secret_key(), &header)?;
                let path = unique_path(&self.inner.cfg.download_dir, &name).await;
                let mut f = tokio::fs::File::create(&path).await?;
                let mut consumed = seal::HEADER as u64;
                let mut written = 0u64;
                let ok: Result<()> = async {
                    loop {
                        let mut len = [0u8; 4];
                        recv.read_exact(&mut len).await?;
                        let len = u32::from_be_bytes(len) as usize;
                        if len > seal::Opener::MAX_CT {
                            bail!("corrupted item");
                        }
                        let mut ct = vec![0u8; len];
                        recv.read_exact(&mut ct).await?;
                        consumed += 4 + len as u64;
                        if consumed > total {
                            bail!("corrupted item");
                        }
                        let last = consumed == total;
                        let pt = opener.open(&ct, last)?;
                        written += pt.len() as u64;
                        f.write_all(&pt).await?;
                        if last {
                            break;
                        }
                    }
                    if written != size {
                        bail!("incomplete file");
                    }
                    f.flush().await?;
                    Ok(())
                }
                .await;
                if let Err(e) = ok {
                    drop(f);
                    let _ = tokio::fs::remove_file(&path).await;
                    return Err(e);
                }
                let _ = ev.send(Event::FileReceived {
                    from: who.id,
                    from_name: who.name,
                    name,
                    path: path.to_string_lossy().into_owned(),
                    size,
                });
            }
            _ => bail!("item kind does not match its content"),
        }
        Ok(())
    }

    /// Close the endpoint gracefully.
    pub async fn shutdown(&self) {
        self.inner.endpoint.close().await;
    }

    // -- internals ---------------------------------------------------------------

    fn expect_ok(&self, r: Response) -> Result<()> {
        match r {
            Response::Ok => Ok(()),
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        }
    }

    async fn request_peer(
        &self,
        peer_id: &str,
        req: Request,
        cap: Option<Cap>,
        body: Option<Body>,
    ) -> Result<Response> {
        self.can_send(peer_id, cap)?;
        let id: EndpointId = peer_id.parse().map_err(|e| anyhow!("bad peer id: {e}"))?;
        self.request(EndpointAddr::from(id), req, body).await
    }

    async fn request(
        &self,
        addr: EndpointAddr,
        req: Request,
        body: Option<Body>,
    ) -> Result<Response> {
        let conn = tokio::time::timeout(
            Duration::from_secs(8),
            self.inner.endpoint.connect(addr, ALPN),
        )
        .await
        .map_err(|_| anyhow::Error::new(Unreachable("timeout".into())))?;
        let conn = conn.map_err(|e| anyhow::Error::new(Unreachable(e.to_string())))?;
        let (mut send, mut recv) = ctx(conn.open_bi().await)?;
        write_msg(&mut send, &req).await?;
        match body {
            Some(Body::Plain(mut file, len)) => {
                let copied = tokio::io::copy(&mut file, &mut send).await?;
                if copied != len {
                    bail!("file changed while sending");
                }
            }
            Some(Body::Sealed {
                mut file,
                size,
                mut sealer,
            }) => {
                send.write_all(&sealer.header).await?;
                let mut remaining = size;
                let mut buf = vec![0u8; seal::CHUNK];
                loop {
                    let n = remaining.min(seal::CHUNK as u64) as usize;
                    file.read_exact(&mut buf[..n]).await?;
                    remaining -= n as u64;
                    let last = remaining == 0;
                    send.write_all(&sealer.seal(&buf[..n], last)?).await?;
                    if last {
                        break;
                    }
                }
            }
            None => {}
        }
        ctx(send.finish())?;
        let resp: Response = read_msg(&mut recv).await?;
        conn.close(0u32.into(), b"done");
        Ok(resp)
    }

    fn spawn_accept_loop(&self) {
        let node = self.clone();
        tokio::spawn(async move {
            while let Some(incoming) = node.inner.endpoint.accept().await {
                let node = node.clone();
                tokio::spawn(async move {
                    if let Err(e) = node.handle_incoming(incoming).await {
                        tracing::debug!("incoming connection ended: {e:#}");
                    }
                });
            }
        });
    }

    fn spawn_sync_loop(&self) {
        let node = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            loop {
                if node.is_joined() {
                    if let Err(e) = node.sync().await {
                        tracing::debug!("household sync failed: {e:#}");
                    }
                    if let Err(e) = node.pull_inbox().await {
                        tracing::debug!("inbox pull failed: {e:#}");
                    }
                }
                tokio::time::sleep(SYNC_EVERY).await;
            }
        });
    }

    async fn handle_incoming(&self, incoming: iroh::endpoint::Incoming) -> Result<()> {
        let conn = ctx(incoming.await)?;
        let remote = conn.remote_id().to_string();
        let (mut send, mut recv) = ctx(conn.accept_bi().await)?;
        let req: Request = read_msg(&mut recv).await?;

        let mut stream_after: Option<PathBuf> = None;
        let resp = match &req {
            Request::Deposit { .. }
            | Request::Inbox
            | Request::Fetch { .. }
            | Request::Ack { .. } => {
                let (r, p) = self
                    .handle_mailbox_request(&req, &mut recv, &remote)
                    .await?;
                stream_after = p;
                r
            }
            Request::ShelfPut { .. }
            | Request::ShelfList
            | Request::ShelfGet { .. }
            | Request::ShelfDelete { .. } => {
                let (r, p) = self.handle_shelf_request(&req, &mut recv, &remote).await?;
                stream_after = p;
                r
            }
            Request::Join { .. }
            | Request::Leave
            | Request::Rename { .. }
            | Request::Sync { .. }
            | Request::Admin { .. } => self.handle_hub_request(&req, &remote),
            Request::Pair {
                secret,
                name,
                platform,
            } => {
                let ok = {
                    let mut p = self.inner.pending.lock().unwrap();
                    p.retain(|_, t| t.elapsed() < TICKET_TTL);
                    p.remove(secret).is_some()
                };
                if ok {
                    let peer = Peer {
                        id: remote.clone(),
                        name: name.clone(),
                        platform: platform.clone(),
                    };
                    self.inner.store.lock().unwrap().add(peer.clone())?;
                    let _ = self.inner.events.send(Event::Paired { peer });
                    Response::PairOk {
                        name: self.device_name(),
                        platform: self.inner.cfg.platform.clone(),
                    }
                } else {
                    Response::Err {
                        message: "invalid or expired pairing code".into(),
                    }
                }
            }
            _ => self.handle_device_request(&req, &mut recv, &remote).await?,
        };
        write_msg(&mut send, &resp).await?;
        if let Some(p) = stream_after {
            let mut f = tokio::fs::File::open(p).await?;
            tokio::io::copy(&mut f, &mut send).await?;
        }
        ctx(send.finish())?;
        // Let the peer read the reply before we drop the connection.
        let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
        Ok(())
    }

    async fn handle_mailbox_request<R: tokio::io::AsyncRead + Unpin>(
        &self,
        req: &Request,
        recv: &mut R,
        remote: &str,
    ) -> Result<(Response, Option<PathBuf>)> {
        let (Some(hub), Some(mb)) = (self.inner.hub.as_ref(), self.inner.mailbox.as_ref()) else {
            return Ok((
                Response::Err {
                    message: "this device is not a household server".into(),
                },
                None,
            ));
        };
        let err = |m: String| (Response::Err { message: m }, None);
        Ok(match req {
            Request::Deposit { to, item } => {
                let cap = match item.kind {
                    Kind::Clipboard => Cap::Clipboard,
                    Kind::Notify => Cap::Notifications,
                    Kind::File => Cap::Files,
                };
                let from_name = match hub.check_deposit(remote, to, cap) {
                    Ok(n) => n,
                    Err(e) => return Ok(err(format!("{e}"))),
                };
                let entry = match mb.reserve(to, remote, &from_name, item.clone()) {
                    Ok(e) => e,
                    Err(e) => return Ok(err(format!("{e}"))),
                };
                if entry.item.is_file() {
                    // Sealed bytes: the server stores what it cannot read.
                    let size = entry.item.size;
                    let path = mb.data_path(&entry.id);
                    let mut f = tokio::fs::File::create(&path).await?;
                    let mut limited = recv.take(size);
                    let n = tokio::io::copy(&mut limited, &mut f).await?;
                    f.flush().await?;
                    if n != size {
                        let _ = tokio::fs::remove_file(&path).await;
                        return Ok(err("incomplete file".into()));
                    }
                }
                mb.commit(&entry)?;
                (Response::Queued, None)
            }
            Request::Inbox => (
                Response::Inbox {
                    entries: mb.for_device(remote),
                },
                None,
            ),
            Request::Fetch { id } => match mb.get(id, remote) {
                Some(entry) => {
                    let data = entry.item.is_file().then(|| mb.data_path(&entry.id));
                    (Response::Entry { entry }, data)
                }
                None => err("item not found".into()),
            },
            Request::Ack { id } => {
                if mb.get(id, remote).is_some() {
                    mb.remove(id);
                }
                (Response::Ok, None)
            }
            _ => err("unexpected request".into()),
        })
    }

    async fn handle_shelf_request<R: tokio::io::AsyncRead + Unpin>(
        &self,
        req: &Request,
        recv: &mut R,
        remote: &str,
    ) -> Result<(Response, Option<PathBuf>)> {
        let (Some(hub), Some(shelf)) = (self.inner.hub.as_ref(), self.inner.shelf.as_ref()) else {
            return Ok((
                Response::Err {
                    message: "this device is not a household server".into(),
                },
                None,
            ));
        };
        let err = |m: String| (Response::Err { message: m }, None);
        let snap = hub.snapshot();
        let Some(me) = snap.device(remote) else {
            return Ok(err("device is not part of this household".into()));
        };
        let my_user = me.user_id.clone();
        Ok(match req {
            Request::ShelfPut { name, size, .. } => {
                let Some(user) = snap.user(&my_user) else {
                    return Ok(err("unknown person".into()));
                };
                let item = match shelf.reserve(
                    name,
                    *size,
                    &my_user,
                    &user.name,
                    remote,
                    shelf::Audience::default(),
                ) {
                    Ok(i) => i,
                    Err(e) => return Ok(err(format!("{e}"))),
                };
                let path = shelf.data_path(&item.id);
                let mut f = tokio::fs::File::create(&path).await?;
                let mut limited = recv.take(*size);
                let n = tokio::io::copy(&mut limited, &mut f).await?;
                f.flush().await?;
                if n != *size {
                    let _ = tokio::fs::remove_file(&path).await;
                    return Ok(err("incomplete file".into()));
                }
                shelf.commit(&item)?;
                (Response::ShelfItem { item }, None)
            }
            Request::ShelfList => {
                shelf.prune();
                let items = shelf
                    .all()
                    .into_iter()
                    .filter(|i| i.visible_to(&my_user))
                    .map(|mut i| {
                        i.mine = i.owner_user == my_user;
                        i
                    })
                    .collect();
                (Response::Shelf { items }, None)
            }
            Request::ShelfGet { id } => match shelf.get(id) {
                Some(item) if item.visible_to(&my_user) => {
                    let p = shelf.data_path(&item.id);
                    (Response::ShelfItem { item }, Some(p))
                }
                _ => err("not on the shelf".into()),
            },
            Request::ShelfDelete { id } => match shelf.get(id) {
                Some(item) if item.owner_user == my_user || hub.is_owner(remote) => {
                    shelf.remove(&item.id);
                    (Response::Ok, None)
                }
                _ => err("not on the shelf".into()),
            },
            _ => err("unexpected request".into()),
        })
    }

    fn handle_hub_request(&self, req: &Request, remote: &str) -> Response {
        let Some(hub) = self.inner.hub.as_ref() else {
            return Response::Err {
                message: "this device is not a household server".into(),
            };
        };
        let err = |e: anyhow::Error| Response::Err {
            message: format!("{e}"),
        };
        match req {
            Request::Join {
                code,
                name,
                platform,
                user_name,
            } => match hub.join(code, remote, name, platform, user_name.as_deref()) {
                Ok(s) => Response::Joined { snapshot: s },
                Err(e) => err(e),
            },
            Request::Leave => match hub.leave(remote) {
                Ok(()) => Response::Ok,
                Err(e) => err(e),
            },
            Request::Rename { name } => {
                match hub.rename_device(remote, name).and_then(|_| hub.signed()) {
                    Ok(s) => Response::Snapshot { snapshot: s },
                    Err(e) => err(e),
                }
            }
            Request::Sync { have } => match hub.sync(remote, *have) {
                Ok(Some(s)) => Response::Snapshot { snapshot: s },
                Ok(None) => Response::NotModified,
                Err(e) => err(e),
            },
            Request::Admin { op } => {
                if !hub.is_owner(remote) {
                    return Response::Err {
                        message: "only the owner can change the household".into(),
                    };
                }
                match hub.apply(op) {
                    Ok(Outcome::Created(id)) => Response::Created { id },
                    Ok(Outcome::Invite(secret)) => Response::InviteCode {
                        code: self.wrap_join_code(&secret),
                    },
                    Ok(Outcome::Changed) => {
                        self.refresh_relay();
                        match hub.signed() {
                            Ok(s) => Response::Snapshot { snapshot: s },
                            Err(e) => err(e),
                        }
                    }
                    Err(e) => err(e),
                }
            }
            _ => Response::Err {
                message: "unexpected request".into(),
            },
        }
    }

    async fn handle_device_request<R: tokio::io::AsyncRead + Unpin>(
        &self,
        req: &Request,
        recv: &mut R,
        remote: &str,
    ) -> Result<Response> {
        let cap = match req {
            Request::Clipboard { .. } => Some(Cap::Clipboard),
            Request::Ring | Request::RingStop => Some(Cap::Ring),
            Request::Notify { .. } => Some(Cap::Notifications),
            Request::File { .. } => Some(Cap::Files),
            Request::OpenUrl { .. } => Some(Cap::Clipboard),
            Request::Battery { .. } => Some(Cap::Status),
            Request::Remote { .. } => Some(Cap::Remote),
            _ => None, // Ping
        };
        let who = match self.authorize(remote, cap).await {
            Ok(w) => w,
            Err(message) => return Ok(Response::Err { message }),
        };
        let ev = &self.inner.events;
        Ok(match req {
            Request::Ping => Response::Ok,
            Request::Clipboard { text } => {
                let _ = ev.send(Event::Clipboard {
                    from: who.id,
                    from_name: who.name,
                    text: text.clone(),
                });
                Response::Ok
            }
            Request::RingStop => {
                let _ = ev.send(Event::RingStop {
                    from: who.id,
                    from_name: who.name,
                });
                Response::Ok
            }
            Request::Ring => {
                let _ = ev.send(Event::Ring {
                    from: who.id,
                    from_name: who.name,
                });
                Response::Ok
            }
            Request::OpenUrl { url } => {
                if url.starts_with("http://") || url.starts_with("https://") {
                    let _ = ev.send(Event::OpenUrl {
                        from: who.id,
                        from_name: who.name,
                        url: url.clone(),
                    });
                    Response::Ok
                } else {
                    Response::Err {
                        message: "only web links can be opened".into(),
                    }
                }
            }
            Request::Battery { percent, charging } => {
                let _ = ev.send(Event::Battery {
                    from: who.id,
                    from_name: who.name,
                    percent: (*percent).min(100),
                    charging: *charging,
                });
                Response::Ok
            }
            Request::Remote { input } => {
                let _ = ev.send(Event::Remote {
                    from: who.id,
                    from_name: who.name,
                    input: input.clone(),
                });
                Response::Ok
            }
            Request::Notify { app, title, body } => {
                let _ = ev.send(Event::Notification {
                    from: who.id,
                    from_name: who.name,
                    app: app.clone(),
                    title: title.clone(),
                    body: body.clone(),
                });
                Response::Ok
            }
            Request::File { name, size } => {
                let max = 20u64 * 1024 * 1024 * 1024;
                if *size > max {
                    return Ok(Response::Err {
                        message: "file too large".into(),
                    });
                }
                let path = unique_path(&self.inner.cfg.download_dir, name).await;
                let mut f = tokio::fs::File::create(&path).await?;
                let mut limited = recv.take(*size);
                let n = tokio::io::copy(&mut limited, &mut f).await?;
                f.flush().await?;
                if n != *size {
                    let _ = tokio::fs::remove_file(&path).await;
                    Response::Err {
                        message: "incomplete file".into(),
                    }
                } else {
                    let _ = ev.send(Event::FileReceived {
                        from: who.id,
                        from_name: who.name,
                        name: name.clone(),
                        path: path.to_string_lossy().into_owned(),
                        size: n,
                    });
                    Response::Ok
                }
            }
            _ => Response::Err {
                message: "unexpected request".into(),
            },
        })
    }
}

fn parse_ticket(ticket: &str) -> Result<Ticket> {
    decode_code(ticket, TICKET_PREFIX, "pairing code")
}

fn parse_join_code(code: &str) -> Result<JoinCode> {
    if code.trim().starts_with(TICKET_PREFIX) {
        bail!("this is a direct pairing code; use “Pair directly” instead");
    }
    decode_code(code, JOIN_PREFIX, "server code")
}

fn decode_code<T: serde::de::DeserializeOwned>(s: &str, prefix: &str, what: &str) -> Result<T> {
    let body = s
        .trim()
        .strip_prefix(prefix)
        .ok_or_else(|| anyhow!("not a Zima Connect {what}"))?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| anyhow!("{what} is corrupted"))?;
    serde_json::from_slice(&raw).map_err(|_| anyhow!("{what} is corrupted"))
}

fn load_membership(path: &Path) -> Option<Membership> {
    let mut m: Membership = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    m.snapshot = m.signed.open(&m.hub.id).ok()?; // re-verify on every start
    Some(m)
}

fn save_membership(path: &Path, m: &Membership) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(m)?)?;
    Ok(())
}

fn load_or_create_key(path: &Path) -> Result<SecretKey> {
    if let Ok(bytes) = std::fs::read(path) {
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return Ok(SecretKey::from_bytes(&arr));
        }
    }
    let key = SecretKey::generate();
    std::fs::write(path, key.to_bytes())?;
    Ok(key)
}

/// Sanitised, collision-free destination path inside `dir`.
async fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let clean: String = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut p = dir.join(&clean);
    let mut i = 1;
    while tokio::fs::try_exists(&p).await.unwrap_or(false) {
        let stem = Path::new(&clean)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(&clean).extension().and_then(|s| s.to_str());
        let n = match ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        p = dir.join(n);
        i += 1;
    }
    p
}

impl Node {
    /// The verified household data this device currently holds (people, devices, rules).
    pub fn household_snapshot(&self) -> Option<Snapshot> {
        self.inner
            .membership
            .lock()
            .unwrap()
            .as_ref()
            .map(|m| m.snapshot.clone())
    }
}
