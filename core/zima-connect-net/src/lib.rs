//! Serverless peer-to-peer transport for Zima Connect.
//!
//! Devices dial each other by public key ([`iroh`]). The connection path is
//! chosen automatically: same network (mDNS) -> hole-punched direct -> relay
//! fallback. Nobody needs an account, a domain or an open port.
//!
//! Pairing: one device shows a ticket ([`Node::pairing_ticket`]), the other
//! pastes or scans it ([`Node::pair`]). Afterwards only paired peers are served.

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

use proto::{read_msg, write_msg, Request, Response};
pub use store::Peer;
use store::Store;

/// ALPN identifying the Zima Connect protocol.
pub const ALPN: &[u8] = b"zima-connect/1";
const TICKET_PREFIX: &str = "zc1-";
const TICKET_TTL: Duration = Duration::from_secs(15 * 60);

fn ctx<T, E: std::fmt::Display>(r: std::result::Result<T, E>) -> Result<T> {
    r.map_err(|e| anyhow!("{e}"))
}

/// Node configuration.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Where identity key and paired peers are persisted.
    pub data_dir: PathBuf,
    /// Where received files are saved.
    pub download_dir: PathBuf,
    /// Human-friendly device name shown to peers.
    pub device_name: String,
    /// "windows" | "linux" | "macos" | "android" | "server"
    pub platform: String,
    /// Use public relays + DNS address lookup (internet). `false` = LAN/direct only (tests).
    pub internet: bool,
}

/// Something that happened that the UI should show.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Paired {
        peer: Peer,
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
}

#[derive(Serialize, Deserialize)]
struct Ticket {
    addr: EndpointAddr,
    secret: String,
}

struct Inner {
    endpoint: Endpoint,
    cfg: NodeConfig,
    store: Mutex<Store>,
    pending: Mutex<HashMap<String, Instant>>, // secret -> created
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
            .secret_key(secret)
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

        let (events, _) = broadcast::channel(64);
        let store = Store::load(cfg.data_dir.join("peers.json"))?;
        let node = Node {
            inner: Arc::new(Inner {
                endpoint,
                cfg,
                store: Mutex::new(store),
                pending: Mutex::new(HashMap::new()),
                events,
            }),
        };
        node.spawn_accept_loop();
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

    /// Paired peers.
    pub fn peers(&self) -> Vec<Peer> {
        self.inner.store.lock().unwrap().peers()
    }

    /// Forget a peer.
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
        // Keep the code short: relay + a few IPv4 addresses are enough; everything
        // else (IPv6, virtual adapters) is found through mDNS / address lookup.
        let full = self.inner.endpoint.addr();
        let mut addr = EndpointAddr::new(full.id);
        for r in full.relay_urls() {
            addr = addr.with_relay_url(r.clone());
        }
        for ip in full.ip_addrs().filter(|a| a.is_ipv4()).take(3) {
            addr = addr.with_ip_addr(*ip);
        }
        let t = Ticket { addr, secret };
        let json = serde_json::to_vec(&t)?;
        Ok(format!(
            "{TICKET_PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
        ))
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

    /// Round-trip time to a peer; also the "is online" check.
    pub async fn ping(&self, peer_id: &str) -> Result<Duration> {
        let start = Instant::now();
        self.expect_ok(self.request_peer(peer_id, Request::Ping, None).await?)?;
        Ok(start.elapsed())
    }

    /// Send clipboard text to a peer.
    pub async fn send_clipboard(&self, peer_id: &str, text: String) -> Result<()> {
        self.expect_ok(
            self.request_peer(peer_id, Request::Clipboard { text }, None)
                .await?,
        )
    }

    /// Make the peer ring / notify.
    pub async fn ring(&self, peer_id: &str) -> Result<()> {
        self.expect_ok(self.request_peer(peer_id, Request::Ring, None).await?)
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
            self.request_peer(peer_id, req, Some((file, meta.len())))
                .await?,
        )
    }

    /// Close the endpoint gracefully.
    pub async fn shutdown(&self) {
        self.inner.endpoint.close().await;
    }

    // -- internals ---------------------------------------------------------

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
        body: Option<(tokio::fs::File, u64)>,
    ) -> Result<Response> {
        if !self.inner.store.lock().unwrap().contains(peer_id) {
            bail!("unknown peer");
        }
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

    async fn handle_incoming(&self, incoming: iroh::endpoint::Incoming) -> Result<()> {
        let conn = ctx(incoming.await)?;
        let remote = conn.remote_id().to_string();
        let (mut send, mut recv) = ctx(conn.accept_bi().await)?;
        let req: Request = read_msg(&mut recv).await?;

        let known = self.inner.store.lock().unwrap().get(&remote);
        let resp = match (&req, known) {
            (
                Request::Pair {
                    secret,
                    name,
                    platform,
                },
                _,
            ) => {
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
            (_, None) => Response::Err {
                message: "not paired".into(),
            },
            (Request::Ping, Some(_)) => Response::Ok,
            (Request::Clipboard { text }, Some(p)) => {
                let _ = self.inner.events.send(Event::Clipboard {
                    from: p.id,
                    from_name: p.name,
                    text: text.clone(),
                });
                Response::Ok
            }
            (Request::Ring, Some(p)) => {
                let _ = self.inner.events.send(Event::Ring {
                    from: p.id,
                    from_name: p.name,
                });
                Response::Ok
            }
            (Request::File { name, size }, Some(p)) => {
                let max = 20u64 * 1024 * 1024 * 1024;
                if *size > max {
                    Response::Err {
                        message: "file too large".into(),
                    }
                } else {
                    let path = unique_path(&self.inner.cfg.download_dir, name).await;
                    let mut f = tokio::fs::File::create(&path).await?;
                    let mut limited = (&mut recv).take(*size);
                    let n = tokio::io::copy(&mut limited, &mut f).await?;
                    f.flush().await?;
                    if n != *size {
                        let _ = tokio::fs::remove_file(&path).await;
                        Response::Err {
                            message: "incomplete file".into(),
                        }
                    } else {
                        let _ = self.inner.events.send(Event::FileReceived {
                            from: p.id,
                            from_name: p.name,
                            name: name.clone(),
                            path: path.to_string_lossy().into_owned(),
                            size: n,
                        });
                        Response::Ok
                    }
                }
            }
        };
        write_msg(&mut send, &resp).await?;
        ctx(send.finish())?;
        // Let the peer read the reply before we drop the connection.
        let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
        Ok(())
    }
}

fn parse_ticket(ticket: &str) -> Result<Ticket> {
    let body = ticket
        .trim()
        .strip_prefix(TICKET_PREFIX)
        .ok_or_else(|| anyhow!("not a Zima Connect pairing code"))?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| anyhow!("pairing code is corrupted"))?;
    serde_json::from_slice(&raw).map_err(|_| anyhow!("pairing code is corrupted"))
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
