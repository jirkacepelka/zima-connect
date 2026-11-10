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
mod proto;
mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use iroh::{endpoint::presets, Endpoint, EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

pub use household::{AdminOp, Cap, Device, Role, Rule, SignedSnapshot, Snapshot, User};
use hub::{HubCore, Outcome};
use proto::{read_msg, write_msg, Request, Response};
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
    events: broadcast::Sender<Event>,
}

/// A running Zima Connect peer.
#[derive(Clone)]
pub struct Node {
    inner: Arc<Inner>,
}

impl Node {
    /// Start the node: load/create identity, bind, begin accepting.
    pub async fn start(cfg: NodeConfig) -> Result<Node> {
        tokio::fs::create_dir_all(&cfg.data_dir).await?;
        tokio::fs::create_dir_all(&cfg.download_dir).await?;
        let secret = load_or_create_key(&cfg.data_dir.join("identity.key"))?;

        let builder = if cfg.internet {
            Endpoint::builder(presets::N0)
        } else {
            Endpoint::builder(presets::Minimal)
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
        let membership = load_membership(&cfg.data_dir.join("household.json"));

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
                events,
            }),
        };
        node.spawn_accept_loop();
        node.spawn_sync_loop();
        Ok(node)
    }

    /// This device's public identity.
    pub fn id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }

    /// This device's display name.
    pub fn device_name(&self) -> &str {
        &self.inner.cfg.device_name
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
                    name: self.inner.cfg.device_name.clone(),
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
            return self.hub_admin_local(hub, &op).await;
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
            Response::Err { message } => bail!("{message}"),
            _ => bail!("unexpected response"),
        }
    }

    // -- hub (server side) ----------------------------------------------------

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
            invites: v
                .invites
                .into_iter()
                .map(|i| InviteOut {
                    code: self.wrap_join_code(&i.secret),
                    label: i.label,
                    expires: i.expires,
                })
                .collect(),
        })
    }

    /// Server dashboard: apply an operation directly.
    pub async fn hub_admin(&self, op: AdminOp) -> Result<Option<String>> {
        let hub = self
            .inner
            .hub
            .as_ref()
            .ok_or_else(|| anyhow!("not a household server"))?;
        self.hub_admin_local(hub, &op).await
    }

    async fn hub_admin_local(&self, hub: &HubCore, op: &AdminOp) -> Result<Option<String>> {
        match hub.apply(op)? {
            Outcome::Invite(secret) => Ok(Some(self.wrap_join_code(&secret))),
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
            name: self.inner.cfg.device_name.clone(),
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

    /// Send clipboard text to a peer.
    pub async fn send_clipboard(&self, peer_id: &str, text: String) -> Result<()> {
        let r = self
            .request_peer(
                peer_id,
                Request::Clipboard { text },
                Some(Cap::Clipboard),
                None,
            )
            .await?;
        self.expect_ok(r)
    }

    /// Make the peer ring / notify.
    pub async fn ring(&self, peer_id: &str) -> Result<()> {
        self.expect_ok(
            self.request_peer(peer_id, Request::Ring, Some(Cap::Ring), None)
                .await?,
        )
    }

    /// Mirror a notification to a peer.
    pub async fn send_notification(
        &self,
        peer_id: &str,
        app: &str,
        title: &str,
        body: &str,
    ) -> Result<()> {
        let req = Request::Notify {
            app: app.into(),
            title: title.into(),
            body: body.into(),
        };
        self.expect_ok(
            self.request_peer(peer_id, req, Some(Cap::Notifications), None)
                .await?,
        )
    }

    /// Stream a file to a peer.
    pub async fn send_file(&self, peer_id: &str, path: &Path) -> Result<()> {
        let meta = tokio::fs::metadata(path).await.context("reading file")?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("bad file name"))?
            .to_string();
        let file = tokio::fs::File::open(path).await?;
        let req = Request::File {
            name,
            size: meta.len(),
        };
        self.expect_ok(
            self.request_peer(peer_id, req, Some(Cap::Files), Some((file, meta.len())))
                .await?,
        )
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
        body: Option<(tokio::fs::File, u64)>,
    ) -> Result<Response> {
        self.can_send(peer_id, cap)?;
        let id: EndpointId = peer_id.parse().map_err(|e| anyhow!("bad peer id: {e}"))?;
        self.request(EndpointAddr::from(id), req, body).await
    }

    async fn request(
        &self,
        addr: EndpointAddr,
        req: Request,
        body: Option<(tokio::fs::File, u64)>,
    ) -> Result<Response> {
        let conn = tokio::time::timeout(
            Duration::from_secs(20),
            self.inner.endpoint.connect(addr, ALPN),
        )
        .await
        .map_err(|_| anyhow!("peer unreachable (timeout)"))?;
        let conn = ctx(conn)?;
        let (mut send, mut recv) = ctx(conn.open_bi().await)?;
        write_msg(&mut send, &req).await?;
        if let Some((mut file, len)) = body {
            let copied = tokio::io::copy(&mut file, &mut send).await?;
            if copied != len {
                bail!("file changed while sending");
            }
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

        let resp = match &req {
            Request::Join { .. } | Request::Sync { .. } | Request::Admin { .. } => {
                self.handle_hub_request(&req, &remote)
            }
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
                        name: self.inner.cfg.device_name.clone(),
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
        ctx(send.finish())?;
        // Let the peer read the reply before we drop the connection.
        let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
        Ok(())
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
                    Ok(Outcome::Invite(secret)) => Response::InviteCode {
                        code: self.wrap_join_code(&secret),
                    },
                    Ok(Outcome::Changed) => match hub.signed() {
                        Ok(s) => Response::Snapshot { snapshot: s },
                        Err(e) => err(e),
                    },
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
            Request::Ring => Some(Cap::Ring),
            Request::Notify { .. } => Some(Cap::Notifications),
            Request::File { .. } => Some(Cap::Files),
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
            Request::Ring => {
                let _ = ev.send(Event::Ring {
                    from: who.id,
                    from_name: who.name,
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
