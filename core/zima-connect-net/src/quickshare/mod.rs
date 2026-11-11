//! Quick Share (Google's "Nearby Share", also built into Samsung phones), over the local network.
//!
//! The protocol is not published; this follows what the community has worked out
//! (NearDrop, rquickshare): devices announce `_FC9F5ED42C8A._tcp` over mDNS, the sender opens a
//! TCP connection, both sides run a UKEY2 key exchange (P-256), talk in AES-256-CBC +
//! HMAC-SHA256 messages, and the receiver's user accepts after seeing a four digit PIN.
//! Only the Wi-Fi LAN path is implemented (no Bluetooth, no Wi-Fi Direct upgrade).
//!
//! Receiving and sending files, and receiving texts and links, work in both directions.

mod proto;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aes::Aes256;
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::rand_core::OsRng;
use p256::elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use p256::{EncodedPoint, PublicKey, SecretKey};
use prost::Message;
use rand::{Rng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot};

use proto::*;

pub const SERVICE: &str = "_FC9F5ED42C8A._tcp.local.";
const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";
const SALT_D2D: [u8; 32] =
    hex32("82AA55A0D397F88346CA1CEE8D3909B95F13FA7DEB1D4AB38376B8256DA85510");
const SALT_KEYS: [u8; 32] =
    hex32("BF9D2A53C63616D75DB0A7165B91C1EF73E537F2427405FA23610A4BE657642E");
const MAX_FRAME: usize = 8 * 1024 * 1024;
const CHUNK: usize = 512 * 1024;
const ASK_TIMEOUT: Duration = Duration::from_secs(60);
const IDLE: Duration = Duration::from_secs(45);
const CONNECT: Duration = Duration::from_secs(10);

type HmacSha256 = Hmac<Sha256>;

const fn hex32(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nib(b[2 * i]) << 4) | nib(b[2 * i + 1]);
        i += 1;
    }
    out
}
const fn nib(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'F' => c - b'A' + 10,
        _ => c - b'a' + 10,
    }
}

pub struct Config {
    /// Where accepted files are saved.
    pub receive_dir: PathBuf,
    /// The name other people see.
    pub alias: String,
    /// 1 phone, 2 tablet, 3 laptop
    pub device_type: u8,
    /// Announce on the network and look for others. Off in tests.
    pub discovery: bool,
}

/// A device nearby that is ready to receive.
#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    /// `ip:port`
    pub id: String,
    pub alias: String,
    pub device_type: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct QsFile {
    pub id: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QsEvent {
    /// Somebody wants to send here. Show the PIN so they can compare, then call [`QuickShare::decide`].
    Incoming {
        session: String,
        from: String,
        pin: String,
        files: Vec<QsFile>,
    },
    /// One file arrived and is saved.
    Received {
        from: String,
        name: String,
        path: String,
        size: u64,
    },
    /// A text or a link arrived.
    Text {
        from: String,
        text: String,
        url: bool,
    },
    Done {
        session: String,
    },
}

struct Inner {
    cfg: Mutex<(String, PathBuf)>, // alias, receive_dir
    device_type: u8,
    endpoint_id: [u8; 4],
    port: u16,
    events: broadcast::Sender<QsEvent>,
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    peers: Mutex<HashMap<String, (Peer, std::time::Instant)>>,
    mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
    visible: Mutex<bool>,
}

#[derive(Clone)]
pub struct QuickShare {
    inner: Arc<Inner>,
}

impl QuickShare {
    pub async fn start(cfg: Config) -> Result<QuickShare> {
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .await
            .context("listening for Quick Share")?;
        let port = listener.local_addr()?.port();
        let (events, _) = broadcast::channel(64);
        let mut id = [0u8; 4];
        let alnum = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        for b in id.iter_mut() {
            *b = alnum[rand::thread_rng().gen_range(0..alnum.len())];
        }
        let inner = Arc::new(Inner {
            cfg: Mutex::new((cfg.alias.clone(), cfg.receive_dir)),
            device_type: cfg.device_type,
            endpoint_id: id,
            port,
            events,
            pending: Mutex::new(HashMap::new()),
            peers: Mutex::new(HashMap::new()),
            mdns: Mutex::new(None),
            visible: Mutex::new(false),
        });
        let qs = QuickShare {
            inner: inner.clone(),
        };

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let inner = inner.clone();
                tokio::spawn(async move {
                    if let Err(e) = serve(inner, stream).await {
                        tracing::debug!("quick share receive ended: {e:#}");
                    }
                });
            }
        });

        if cfg.discovery {
            qs.start_discovery()?;
        }
        Ok(qs)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<QsEvent> {
        self.inner.events.subscribe()
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    pub fn set_alias(&self, alias: &str) {
        self.inner.cfg.lock().unwrap().0 = alias.to_string();
        if *self.inner.visible.lock().unwrap() {
            self.advertise(true);
        }
    }

    /// Show this computer to Quick Share users nearby (they still see a PIN and you still accept).
    pub fn set_visible(&self, on: bool) {
        *self.inner.visible.lock().unwrap() = on;
        self.advertise(on);
    }

    pub fn peers(&self) -> Vec<Peer> {
        let mut peers = self.inner.peers.lock().unwrap();
        peers.retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(120));
        let mut v: Vec<Peer> = peers.values().map(|(p, _)| p.clone()).collect();
        v.sort_by(|a, b| a.alias.cmp(&b.alias));
        v
    }

    pub fn peer_at(&self, ip: IpAddr, port: u16) -> Peer {
        Peer {
            id: format!("{ip}:{port}"),
            alias: ip.to_string(),
            device_type: "unknown".into(),
            ip: ip.to_string(),
            port,
        }
    }

    pub fn decide(&self, session: &str, accept: bool) -> Result<()> {
        let tx = self
            .inner
            .pending
            .lock()
            .unwrap()
            .remove(session)
            .ok_or_else(|| anyhow!("That request is no longer waiting"))?;
        let _ = tx.send(accept);
        Ok(())
    }

    // ───── mDNS ─────

    fn instance_name(&self) -> String {
        let mut b = vec![0x23];
        b.extend_from_slice(&self.inner.endpoint_id);
        b.extend_from_slice(&[0xFC, 0x9F, 0x5E, 0, 0]);
        URL_SAFE_NO_PAD.encode(b)
    }

    fn endpoint_info(&self) -> Vec<u8> {
        let alias = self.inner.cfg.lock().unwrap().0.clone();
        let mut name = alias.into_bytes();
        name.truncate(200);
        // version 0, visible to everyone, device type
        let mut info = vec![self.inner.device_type << 1];
        let mut random = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random);
        info.extend_from_slice(&random);
        info.push(name.len() as u8);
        info.extend_from_slice(&name);
        info
    }

    fn start_discovery(&self) -> Result<()> {
        let daemon = mdns_sd::ServiceDaemon::new().map_err(|e| anyhow!("mDNS: {e}"))?;
        let browse = daemon
            .browse(SERVICE)
            .map_err(|e| anyhow!("mDNS browse: {e}"))?;
        *self.inner.mdns.lock().unwrap() = Some(daemon);
        let inner = self.inner.clone();
        let me = self.instance_name();
        tokio::spawn(async move {
            while let Ok(ev) = browse.recv_async().await {
                match ev {
                    mdns_sd::ServiceEvent::ServiceResolved(info) => {
                        if info.get_fullname().starts_with(&me) {
                            continue;
                        }
                        let Some(ip) = info.get_addresses_v4().into_iter().next().copied() else {
                            continue;
                        };
                        let Some(n) = info.get_property_val_str("n") else {
                            continue;
                        };
                        let Some((kind, alias)) = parse_endpoint_info(n) else {
                            continue;
                        };
                        let port = info.get_port();
                        let peer = Peer {
                            id: format!("{ip}:{port}"),
                            alias,
                            device_type: kind.into(),
                            ip: ip.to_string(),
                            port,
                        };
                        inner.peers.lock().unwrap().insert(
                            info.get_fullname().to_string(),
                            (peer, std::time::Instant::now()),
                        );
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, full) => {
                        inner.peers.lock().unwrap().remove(&full);
                    }
                    _ => {}
                }
            }
        });
        Ok(())
    }

    fn advertise(&self, on: bool) {
        let guard = self.inner.mdns.lock().unwrap();
        let Some(daemon) = guard.as_ref() else { return };
        let name = self.instance_name();
        let full = format!("{name}.{SERVICE}");
        let _ = daemon.unregister(&full);
        if !on {
            return;
        }
        let alias = self.inner.cfg.lock().unwrap().0.clone();
        let host: String = alias
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let host = if host.is_empty() {
            "zima-connect".to_string()
        } else {
            host
        };
        let props = [("n", URL_SAFE_NO_PAD.encode(self.endpoint_info()))];
        match mdns_sd::ServiceInfo::new(
            SERVICE,
            &name,
            &format!("{host}.local."),
            "",
            self.inner.port,
            &props[..],
        ) {
            Ok(info) => {
                if let Err(e) = daemon.register(info.enable_addr_auto()) {
                    tracing::warn!("Quick Share: could not announce: {e}");
                }
            }
            Err(e) => tracing::warn!("Quick Share: bad service info: {e}"),
        }
    }

    // ───── sending ─────

    /// Send files to a device that shows up in [`peers`](Self::peers). Its user sees a PIN and accepts.
    pub async fn send(&self, peer: &Peer, paths: &[PathBuf]) -> Result<()> {
        let alias = self.inner.cfg.lock().unwrap().0.clone();
        let stream =
            tokio::time::timeout(CONNECT, TcpStream::connect((peer.ip.as_str(), peer.port)))
                .await
                .map_err(|_| anyhow!("{} did not answer", peer.alias))?
                .with_context(|| format!("could not reach {}", peer.alias))?;
        stream.set_nodelay(true).ok();
        let mut c = Conn::new(stream);

        // Open the connection and run the key exchange.
        let eid = String::from_utf8_lossy(&self.inner.endpoint_id).to_string();
        let info = self.endpoint_info();
        let mut req = v1(FrameType::ConnectionRequest);
        req.connection_request = Some(ConnectionRequestFrame {
            endpoint_id: Some(eid),
            endpoint_name: Some(alias.clone().into_bytes()),
            endpoint_info: Some(info),
            mediums: vec![5],
            ..Default::default()
        });
        c.send(&offline(req).encode_to_vec()).await?;

        let secret = SecretKey::random(&mut OsRng);
        let finish = Ukey2Message {
            message_type: Some(Ukey2Type::ClientFinish as i32),
            message_data: Some(
                Ukey2ClientFinished {
                    public_key: Some(public_key_message(&secret.public_key())),
                }
                .encode_to_vec(),
            ),
        }
        .encode_to_vec();
        let mut random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random);
        let init = Ukey2Message {
            message_type: Some(Ukey2Type::ClientInit as i32),
            message_data: Some(
                Ukey2ClientInit {
                    version: Some(1),
                    random: Some(random.to_vec()),
                    cipher_commitments: vec![CipherCommitment {
                        handshake_cipher: Some(CIPHER_P256_SHA512),
                        commitment: Some(Sha512::digest(&finish).to_vec()),
                    }],
                    next_protocol: Some(NEXT_PROTOCOL.into()),
                }
                .encode_to_vec(),
            ),
        }
        .encode_to_vec();
        c.send(&init).await?;

        let server_init_raw = c.recv().await?;
        let m = Ukey2Message::decode(&server_init_raw[..])?;
        if m.message_type != Some(Ukey2Type::ServerInit as i32) {
            bail!("the other device did not continue the key exchange");
        }
        let si = Ukey2ServerInit::decode(m.message_data.as_deref().unwrap_or_default())?;
        if si.version != Some(1) || si.handshake_cipher != Some(CIPHER_P256_SHA512) {
            bail!("the other device uses a key exchange we do not know");
        }
        let peer_key = parse_public_key(si.public_key.as_deref().unwrap_or_default())?;
        c.send(&finish).await?;
        let (keys, pin) = derive(&secret, &peer_key, &init, &server_init_raw, false)?;
        c.keys = Some(keys);

        c.send(&offline(response_frame()).encode_to_vec()).await?;
        let raw = c.recv().await?;
        let f = OfflineFrame::decode(&raw[..])?;
        let ok =
            f.v1.as_ref()
                .and_then(|v| v.connection_response.as_ref())
                .map(|r| r.response == Some(ResponseStatus::Accept as i32));
        if ok != Some(true) {
            bail!("{} did not accept the connection", peer.alias);
        }
        tracing::debug!("Quick Share to {}: PIN {pin}", peer.alias);
        c.send_sharing(&sharing(SharingType::PairedKeyEncryption, |v| {
            v.paired_key_encryption = Some(PairedKeyEncryptionFrame {
                signed_data: Some(random_bytes(72)),
                secret_id_hash: Some(random_bytes(6)),
            });
        }))
        .await?;

        // What we offer.
        let mut files: Vec<(FileMetadata, PathBuf)> = Vec::new();
        for p in paths {
            let meta = tokio::fs::metadata(p)
                .await
                .with_context(|| format!("cannot read {}", p.display()))?;
            if !meta.is_file() {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            let mime = mime_guess::from_path(p).first_or_octet_stream().to_string();
            let kind = if mime.starts_with("image/") {
                1
            } else if mime.starts_with("video/") {
                2
            } else if mime.starts_with("audio/") {
                4
            } else if p.extension().is_some_and(|e| e == "apk") {
                3
            } else {
                0
            };
            let id: i64 = rand::thread_rng().gen();
            files.push((
                FileMetadata {
                    name: Some(name),
                    r#type: Some(kind),
                    payload_id: Some(id),
                    size: Some(meta.len() as i64),
                    mime_type: Some(mime),
                    id: Some(id),
                },
                p.clone(),
            ));
        }
        if files.is_empty() {
            bail!("nothing to send");
        }

        let mut got_pkr = false;
        let mut introduced = false;
        loop {
            let item = tokio::time::timeout(ASK_TIMEOUT + IDLE, c.next())
                .await
                .map_err(|_| anyhow!("{} stopped answering", peer.alias))??;
            match item {
                Item::KeepAlive => c.keep_alive_ack().await?,
                Item::Bye => bail!("{} closed the connection", peer.alias),
                Item::Chunk { .. } => {}
                Item::Bytes { data, .. } => {
                    let Some(v) = Frame::decode(&data[..]).ok().and_then(|f| f.v1) else {
                        continue;
                    };
                    match v.r#type.and_then(|t| SharingType::try_from(t).ok()) {
                        Some(SharingType::PairedKeyEncryption) => {
                            c.send_sharing(&sharing(SharingType::PairedKeyResult, |v| {
                                v.paired_key_result = Some(PairedKeyResultFrame { status: Some(3) })
                            }))
                            .await?;
                        }
                        Some(SharingType::PairedKeyResult) => {
                            got_pkr = true;
                        }
                        Some(SharingType::Cancel) => bail!("{} cancelled", peer.alias),
                        Some(SharingType::Response) => {
                            match v.connection_response.and_then(|r| r.status) {
                                Some(1) => break,
                                Some(3) => bail!("{} has not enough space", peer.alias),
                                Some(4) => bail!("{} cannot take this kind of file", peer.alias),
                                Some(5) => bail!("{} did not answer in time", peer.alias),
                                _ => bail!("{} declined", peer.alias),
                            }
                        }
                        _ => {}
                    }
                }
            }
            if got_pkr && !introduced {
                introduced = true;
                let metas: Vec<FileMetadata> = files.iter().map(|(m, _)| m.clone()).collect();
                c.send_sharing(&sharing(SharingType::Introduction, |v| {
                    v.introduction = Some(IntroductionFrame {
                        file_metadata: metas,
                        ..Default::default()
                    });
                }))
                .await?;
            }
        }

        for (meta, path) in &files {
            c.send_file(meta, path).await?;
        }
        let mut bye = v1(FrameType::Disconnection);
        bye.disconnection = Some(DisconnectionFrame::default());
        c.send_secure(&offline(bye)).await?;
        c.finish().await;
        Ok(())
    }
}

// ───────────────────────── receiving ─────────────────────────

struct Incoming {
    name: String,
    path: PathBuf,
    size: i64,
    got: i64,
    file: Option<tokio::fs::File>,
}

async fn serve(inner: Arc<Inner>, stream: TcpStream) -> Result<()> {
    stream.set_nodelay(true).ok();
    let mut c = Conn::new(stream);

    let raw = tokio::time::timeout(IDLE, c.recv()).await??;
    let req = OfflineFrame::decode(&raw[..])?
        .v1
        .and_then(|v| v.connection_request)
        .ok_or_else(|| anyhow!("not a connection request"))?;
    let from = req
        .endpoint_info
        .as_deref()
        .and_then(|i| parse_endpoint_info(&URL_SAFE_NO_PAD.encode(i)))
        .map(|(_, n)| n)
        .or_else(|| {
            req.endpoint_name
                .as_ref()
                .and_then(|n| String::from_utf8(n.clone()).ok())
        })
        .unwrap_or_else(|| "Unknown device".into());

    // Key exchange (we are the server).
    let init_raw = tokio::time::timeout(IDLE, c.recv()).await??;
    let m = Ukey2Message::decode(&init_raw[..])?;
    if m.message_type != Some(Ukey2Type::ClientInit as i32) {
        c.alert(alert::BAD_MESSAGE_TYPE).await;
        bail!("expected a key exchange start");
    }
    let ci = Ukey2ClientInit::decode(m.message_data.as_deref().unwrap_or_default())?;
    if ci.version != Some(1) {
        c.alert(alert::BAD_VERSION).await;
        bail!("unknown key exchange version");
    }
    if ci.random.as_ref().map(|r| r.len()) != Some(32) {
        c.alert(alert::BAD_RANDOM).await;
        bail!("bad random");
    }
    let Some(commitment) = ci
        .cipher_commitments
        .iter()
        .find(|c| c.handshake_cipher == Some(CIPHER_P256_SHA512))
        .and_then(|c| c.commitment.clone())
    else {
        c.alert(alert::BAD_HANDSHAKE_CIPHER).await;
        bail!("no cipher we support");
    };
    if ci.next_protocol.as_deref() != Some(NEXT_PROTOCOL) {
        c.alert(alert::BAD_NEXT_PROTOCOL).await;
        bail!("unknown next protocol");
    }
    let secret = SecretKey::random(&mut OsRng);
    let mut random = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random);
    let server_init = Ukey2Message {
        message_type: Some(Ukey2Type::ServerInit as i32),
        message_data: Some(
            Ukey2ServerInit {
                version: Some(1),
                random: Some(random.to_vec()),
                handshake_cipher: Some(CIPHER_P256_SHA512),
                public_key: Some(public_key_message(&secret.public_key())),
            }
            .encode_to_vec(),
        ),
    }
    .encode_to_vec();
    c.send(&server_init).await?;

    let fin_raw = tokio::time::timeout(IDLE, c.recv()).await??;
    let m = Ukey2Message::decode(&fin_raw[..])?;
    if m.message_type != Some(Ukey2Type::ClientFinish as i32) {
        c.alert(alert::BAD_MESSAGE_TYPE).await;
        bail!("expected the key exchange to finish");
    }
    if Sha512::digest(&fin_raw).as_slice() != commitment.as_slice() {
        bail!("the key exchange does not match what was promised");
    }
    let fin = Ukey2ClientFinished::decode(m.message_data.as_deref().unwrap_or_default())?;
    let peer_key = parse_public_key(fin.public_key.as_deref().unwrap_or_default())?;
    let (keys, pin) = derive(&secret, &peer_key, &init_raw, &server_init, true)?;
    c.keys = Some(keys);

    // The sender's connection response, ours, and our half of the pairing check.
    let raw = tokio::time::timeout(IDLE, c.recv()).await??;
    let ok = OfflineFrame::decode(&raw[..])?
        .v1
        .and_then(|v| v.connection_response)
        .map(|r| r.response == Some(ResponseStatus::Accept as i32));
    if ok != Some(true) {
        bail!("the sender did not accept the connection");
    }
    c.send(&offline(response_frame()).encode_to_vec()).await?;
    c.send_sharing(&sharing(SharingType::PairedKeyEncryption, |v| {
        v.paired_key_encryption = Some(PairedKeyEncryptionFrame {
            signed_data: Some(random_bytes(72)),
            secret_id_hash: Some(random_bytes(6)),
        });
    }))
    .await?;

    let session = format!("qs:{:016x}", rand::thread_rng().gen::<u64>());
    let mut decision: Option<oneshot::Receiver<bool>> = None;
    let mut files: HashMap<i64, Incoming> = HashMap::new();
    let mut texts: HashMap<i64, bool> = HashMap::new(); // payload id -> is a link
    let mut waiting_for = 0usize;
    let mut accepted = false;
    let result: Result<()> = async {
        loop {
            let asking = decision.is_some();
            let item = tokio::select! {
                d = async { decision.as_mut().unwrap().await }, if decision.is_some() => {
                    decision = None;
                    let yes = d.unwrap_or(false);
                    inner.pending.lock().unwrap().remove(&session);
                    let status = if yes { 1 } else { 2 };
                    if yes {
                        for f in files.values_mut() {
                            if let Some(dir) = f.path.parent() {
                                tokio::fs::create_dir_all(dir).await.ok();
                            }
                            f.file = Some(tokio::fs::File::create(&f.path).await.with_context(|| format!("cannot save {}", f.path.display()))?);
                        }
                        accepted = true;
                    }
                    c.send_sharing(&sharing(SharingType::Response, |v| v.connection_response = Some(SharingResponse { status: Some(status) }))).await?;
                    if !yes {
                        let mut bye = v1(FrameType::Disconnection);
                        bye.disconnection = Some(DisconnectionFrame::default());
                        c.send_secure(&offline(bye)).await.ok();
                        return Ok(());
                    }
                    continue;
                }
                i = tokio::time::timeout(if asking { ASK_TIMEOUT + IDLE } else { IDLE }, c.next()) => i.map_err(|_| anyhow!("the sender stopped talking"))??,
            };
            match item {
                Item::KeepAlive => c.keep_alive_ack().await?,
                Item::Bye => return Ok(()),
                Item::Bytes { id, data } => {
                    if let Some(&url) = texts.get(&id) {
                        texts.remove(&id);
                        let text = String::from_utf8_lossy(&data).trim_end_matches('\u{10}').to_string();
                        let _ = inner.events.send(QsEvent::Text { from: from.clone(), text, url });
                        waiting_for = waiting_for.saturating_sub(1);
                        if waiting_for == 0 && files.is_empty() {
                            return Ok(());
                        }
                        continue;
                    }
                    let Some(v) = Frame::decode(&data[..]).ok().and_then(|f| f.v1) else { continue };
                    match v.r#type.and_then(|t| SharingType::try_from(t).ok()) {
                        Some(SharingType::PairedKeyEncryption) => {
                            c.send_sharing(&sharing(SharingType::PairedKeyResult, |v| v.paired_key_result = Some(PairedKeyResultFrame { status: Some(3) }))).await?;
                        }
                        Some(SharingType::Cancel) => return Ok(()),
                        Some(SharingType::Introduction) if decision.is_none() && !accepted => {
                            let intro = v.introduction.unwrap_or_default();
                            if !intro.wifi_credentials_metadata.is_empty() && intro.file_metadata.is_empty() && intro.text_metadata.is_empty() {
                                c.send_sharing(&sharing(SharingType::Response, |v| v.connection_response = Some(SharingResponse { status: Some(4) }))).await?;
                                return Ok(());
                            }
                            let dir = inner.cfg.lock().unwrap().1.clone();
                            let mut shown = Vec::new();
                            for fm in &intro.file_metadata {
                                let name = clean_name(fm.name.as_deref().unwrap_or("file"));
                                let path = unique_path(&dir, &name);
                                let size = fm.size.unwrap_or(0).max(0);
                                let id = fm.payload_id.unwrap_or(0);
                                shown.push(QsFile { id: id.to_string(), name: name.clone(), size: size as u64 });
                                files.insert(id, Incoming { name, path, size, got: 0, file: None });
                            }
                            for tm in &intro.text_metadata {
                                let url = tm.r#type == Some(2);
                                texts.insert(tm.payload_id.unwrap_or(0), url);
                                waiting_for += 1;
                                shown.push(QsFile {
                                    id: tm.payload_id.unwrap_or(0).to_string(),
                                    name: tm.text_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| if url { "A link".into() } else { "A text".into() }),
                                    size: tm.size.unwrap_or(0).max(0) as u64,
                                });
                            }
                            if shown.is_empty() {
                                return Ok(());
                            }
                            let (tx, rx) = oneshot::channel();
                            inner.pending.lock().unwrap().insert(session.clone(), tx);
                            decision = Some(rx);
                            let _ = inner.events.send(QsEvent::Incoming { session: session.clone(), from: from.clone(), pin: pin.clone(), files: shown });
                        }
                        _ => {}
                    }
                }
                Item::Chunk { id, offset, body, last } => {
                    if !accepted {
                        bail!("data arrived before it was accepted");
                    }
                    let Some(f) = files.get_mut(&id) else { continue };
                    if offset != f.got {
                        bail!("a piece of {} is missing", f.name);
                    }
                    if f.got + body.len() as i64 > f.size {
                        bail!("{} is bigger than announced", f.name);
                    }
                    if !body.is_empty() {
                        f.file.as_mut().unwrap().write_all(&body).await?;
                        f.got += body.len() as i64;
                    }
                    if last {
                        let mut f = files.remove(&id).unwrap();
                        if let Some(mut file) = f.file.take() {
                            file.flush().await?;
                        }
                        if f.got != f.size {
                            tokio::fs::remove_file(&f.path).await.ok();
                            bail!("{} arrived incomplete", f.name);
                        }
                        let _ = inner.events.send(QsEvent::Received { from: from.clone(), name: f.name.clone(), path: f.path.to_string_lossy().into_owned(), size: f.size as u64 });
                        if files.is_empty() && waiting_for == 0 {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
    .await;

    inner.pending.lock().unwrap().remove(&session);
    // Half-written files are not worth keeping.
    for f in files.values() {
        if f.file.is_some() {
            tokio::fs::remove_file(&f.path).await.ok();
        }
    }
    let _ = inner.events.send(QsEvent::Done { session });
    let mut bye = v1(FrameType::Disconnection);
    bye.disconnection = Some(DisconnectionFrame::default());
    c.send_secure(&offline(bye)).await.ok();
    c.finish().await;
    result
}

// ───────────────────────── the encrypted connection ─────────────────────────

struct Keys {
    enc: [u8; 32],
    enc_mac: [u8; 32],
    dec: [u8; 32],
    dec_mac: [u8; 32],
}

enum Item {
    KeepAlive,
    Bye,
    /// A finished BYTES payload.
    Bytes {
        id: i64,
        data: Vec<u8>,
    },
    /// A piece of a FILE payload.
    Chunk {
        id: i64,
        offset: i64,
        body: Vec<u8>,
        last: bool,
    },
}

struct Conn {
    wr: OwnedWriteHalf,
    rx: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    keys: Option<Keys>,
    send_seq: i32,
    recv_seq: i32,
    partial: HashMap<i64, Vec<u8>>,
}

impl Conn {
    fn new(stream: TcpStream) -> Conn {
        let (mut rd, wr) = stream.into_split();
        let (tx, rx) = mpsc::channel(8);
        // Reading lives in its own task so waiting for a frame never loses half of one.
        tokio::spawn(async move {
            loop {
                let mut len = [0u8; 4];
                if let Err(e) = rd.read_exact(&mut len).await {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
                let n = u32::from_be_bytes(len) as usize;
                if n > MAX_FRAME {
                    let _ = tx.send(Err(std::io::Error::other("frame too large"))).await;
                    break;
                }
                let mut buf = vec![0u8; n];
                if let Err(e) = rd.read_exact(&mut buf).await {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
                if tx.send(Ok(buf)).await.is_err() {
                    break;
                }
            }
        });
        Conn {
            wr,
            rx,
            keys: None,
            send_seq: 0,
            recv_seq: 0,
            partial: HashMap::new(),
        }
    }

    async fn recv(&mut self) -> Result<Vec<u8>> {
        match self.rx.recv().await {
            Some(Ok(b)) => Ok(b),
            Some(Err(e)) => Err(e.into()),
            None => bail!("connection closed"),
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        let mut out = Vec::with_capacity(data.len() + 4);
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
        self.wr.write_all(&out).await?;
        self.wr.flush().await?;
        Ok(())
    }

    async fn alert(&mut self, code: i32) {
        let msg = Ukey2Message {
            message_type: Some(Ukey2Type::Alert as i32),
            message_data: Some(
                Ukey2Alert {
                    r#type: Some(code),
                    error_message: None,
                }
                .encode_to_vec(),
            ),
        };
        let _ = self.send(&msg.encode_to_vec()).await;
    }

    async fn send_secure(&mut self, frame: &OfflineFrame) -> Result<()> {
        let keys = self.keys.as_ref().ok_or_else(|| anyhow!("no keys yet"))?;
        self.send_seq += 1;
        let d2d = DeviceToDeviceMessage {
            sequence_number: Some(self.send_seq),
            message: Some(frame.encode_to_vec()),
        }
        .encode_to_vec();
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);
        let body = cbc::Encryptor::<Aes256>::new((&keys.enc).into(), (&iv).into())
            .encrypt_padded_vec_mut::<Pkcs7>(&d2d);
        let hb = HeaderAndBody {
            header: Some(Header {
                signature_scheme: Some(1),
                encryption_scheme: Some(2),
                iv: Some(iv.to_vec()),
                public_metadata: Some(
                    GcmMetadata {
                        r#type: Some(13),
                        version: Some(1),
                    }
                    .encode_to_vec(),
                ),
            }),
            body: Some(body),
        }
        .encode_to_vec();
        let mut mac = HmacSha256::new_from_slice(&keys.enc_mac)?;
        mac.update(&hb);
        let sm = SecureMessage {
            header_and_body: Some(hb),
            signature: Some(mac.finalize().into_bytes().to_vec()),
        };
        self.send(&sm.encode_to_vec()).await
    }

    fn open(&mut self, raw: &[u8]) -> Result<OfflineFrame> {
        let keys = self.keys.as_ref().ok_or_else(|| anyhow!("no keys yet"))?;
        let sm = SecureMessage::decode(raw)?;
        let hb_raw = sm.header_and_body.unwrap_or_default();
        let mut mac = HmacSha256::new_from_slice(&keys.dec_mac)?;
        mac.update(&hb_raw);
        mac.verify_slice(sm.signature.as_deref().unwrap_or_default())
            .map_err(|_| anyhow!("a message was altered on the way"))?;
        let hb = HeaderAndBody::decode(&hb_raw[..])?;
        let iv = hb.header.and_then(|h| h.iv).unwrap_or_default();
        if iv.len() != 16 {
            bail!("bad message header");
        }
        let plain = cbc::Decryptor::<Aes256>::new((&keys.dec).into(), iv.as_slice().into())
            .decrypt_padded_vec_mut::<Pkcs7>(&hb.body.unwrap_or_default())
            .map_err(|_| anyhow!("a message could not be decrypted"))?;
        let d2d = DeviceToDeviceMessage::decode(&plain[..])?;
        self.recv_seq += 1;
        if d2d.sequence_number != Some(self.recv_seq) {
            bail!("messages arrived out of order");
        }
        Ok(OfflineFrame::decode(
            d2d.message.unwrap_or_default().as_slice(),
        )?)
    }

    /// The next thing that matters. Safe to drop while waiting (reading never loses data).
    async fn next(&mut self) -> Result<Item> {
        loop {
            let raw = self.recv().await?;
            let frame = self.open(&raw)?;
            let Some(v) = frame.v1 else { continue };
            match v.r#type.and_then(|t| FrameType::try_from(t).ok()) {
                Some(FrameType::KeepAlive) => {
                    // Our own acks come back as keep-alives with ack set: nothing to answer.
                    if v.keep_alive.and_then(|k| k.ack) == Some(true) {
                        continue;
                    }
                    return Ok(Item::KeepAlive);
                }
                Some(FrameType::Disconnection) => return Ok(Item::Bye),
                Some(FrameType::PayloadTransfer) => {
                    let Some(pt) = v.payload_transfer else {
                        continue;
                    };
                    let (Some(h), Some(ch)) = (pt.payload_header, pt.payload_chunk) else {
                        continue;
                    };
                    let id = h.id.unwrap_or(0);
                    let last = ch.flags.unwrap_or(0) & 1 == 1;
                    let body = ch.body.unwrap_or_default();
                    match h.r#type.and_then(|t| PayloadType::try_from(t).ok()) {
                        Some(PayloadType::Bytes) => {
                            let buf = self.partial.entry(id).or_default();
                            if ch.offset.unwrap_or(0) != buf.len() as i64 {
                                bail!("a piece of a message is missing");
                            }
                            if buf.len() + body.len() > MAX_FRAME {
                                bail!("message too large");
                            }
                            buf.extend_from_slice(&body);
                            if last {
                                let data = self.partial.remove(&id).unwrap_or_default();
                                return Ok(Item::Bytes { id, data });
                            }
                        }
                        Some(PayloadType::File) => {
                            return Ok(Item::Chunk {
                                id,
                                offset: ch.offset.unwrap_or(0),
                                body,
                                last,
                            })
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    async fn keep_alive_ack(&mut self) -> Result<()> {
        let mut f = v1(FrameType::KeepAlive);
        f.keep_alive = Some(KeepAliveFrame { ack: Some(true) });
        self.send_secure(&offline(f)).await
    }

    /// A small message of the sharing layer, as a BYTES payload in two pieces.
    async fn send_sharing(&mut self, frame: &Frame) -> Result<()> {
        let data = frame.encode_to_vec();
        let header = PayloadHeader {
            id: Some(rand::thread_rng().gen()),
            r#type: Some(PayloadType::Bytes as i32),
            total_size: Some(data.len() as i64),
            is_sensitive: Some(false),
            ..Default::default()
        };
        let len = data.len() as i64;
        self.send_chunk(&header, 0, 0, data).await?;
        self.send_chunk(&header, len, 1, Vec::new()).await
    }

    async fn send_chunk(
        &mut self,
        header: &PayloadHeader,
        offset: i64,
        flags: i32,
        body: Vec<u8>,
    ) -> Result<()> {
        let mut f = v1(FrameType::PayloadTransfer);
        f.payload_transfer = Some(PayloadTransferFrame {
            packet_type: Some(PacketType::Data as i32),
            payload_header: Some(header.clone()),
            payload_chunk: Some(PayloadChunk {
                flags: Some(flags),
                offset: Some(offset),
                body: Some(body),
            }),
            control_message: None,
        });
        self.send_secure(&offline(f)).await
    }

    async fn send_file(&mut self, meta: &FileMetadata, path: &Path) -> Result<()> {
        let size = meta.size.unwrap_or(0);
        let header = PayloadHeader {
            id: meta.payload_id,
            r#type: Some(PayloadType::File as i32),
            total_size: Some(size),
            is_sensitive: Some(false),
            file_name: meta.name.clone(),
            ..Default::default()
        };
        let mut file = tokio::fs::File::open(path).await?;
        let mut sent = 0i64;
        let mut buf = vec![0u8; CHUNK];
        while sent < size {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                bail!("{} got shorter while sending", path.display());
            }
            self.send_chunk(&header, sent, 0, buf[..n].to_vec()).await?;
            sent += n as i64;
        }
        self.send_chunk(&header, sent, 1, Vec::new()).await
    }

    /// Close politely: tell the other side we are done, then let it finish reading.
    async fn finish(&mut self) {
        let _ = self.wr.shutdown().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            while self.rx.recv().await.is_some() {}
        })
        .await;
    }
}

// ───────────────────────── helpers ─────────────────────────

fn v1(t: FrameType) -> V1Frame {
    V1Frame {
        r#type: Some(t as i32),
        ..Default::default()
    }
}

fn offline(v: V1Frame) -> OfflineFrame {
    OfflineFrame {
        version: Some(1),
        v1: Some(v),
    }
}

fn response_frame() -> V1Frame {
    let mut v = v1(FrameType::ConnectionResponse);
    let os = if cfg!(windows) { 3 } else { 100 };
    v.connection_response = Some(ConnectionResponseFrame {
        response: Some(ResponseStatus::Accept as i32),
        os_info: Some(OsInfo { r#type: Some(os) }),
    });
    v
}

fn sharing(t: SharingType, fill: impl FnOnce(&mut SharingV1)) -> Frame {
    let mut v = SharingV1 {
        r#type: Some(t as i32),
        ..Default::default()
    };
    fill(&mut v);
    Frame {
        version: Some(1),
        v1: Some(v),
    }
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    b
}

/// Numbers in these messages are signed big-endian integers: a leading zero keeps them positive.
fn signed_be(raw: &[u8]) -> Vec<u8> {
    let trimmed: &[u8] = match raw.iter().position(|&b| b != 0) {
        Some(i) => &raw[i..],
        None => &[0],
    };
    let mut out = Vec::with_capacity(trimmed.len() + 1);
    if trimmed[0] & 0x80 != 0 {
        out.push(0);
    }
    out.extend_from_slice(trimmed);
    out
}

fn public_key_message(key: &PublicKey) -> Vec<u8> {
    let p = key.to_encoded_point(false);
    GenericPublicKey {
        r#type: Some(1),
        ec_p256_public_key: Some(EcP256PublicKey {
            x: Some(signed_be(p.x().unwrap())),
            y: Some(signed_be(p.y().unwrap())),
        }),
    }
    .encode_to_vec()
}

fn parse_public_key(raw: &[u8]) -> Result<PublicKey> {
    let g = GenericPublicKey::decode(raw)?;
    let k = g
        .ec_p256_public_key
        .ok_or_else(|| anyhow!("no public key"))?;
    let fit = |v: Vec<u8>| -> Vec<u8> {
        let v = if v.len() > 32 {
            v[v.len() - 32..].to_vec()
        } else {
            v
        };
        let mut out = vec![0u8; 32 - v.len()];
        out.extend_from_slice(&v);
        out
    };
    let mut bytes = vec![4u8];
    bytes.extend(fit(k.x.unwrap_or_default()));
    bytes.extend(fit(k.y.unwrap_or_default()));
    let point = EncodedPoint::from_bytes(&bytes).map_err(|_| anyhow!("bad public key"))?;
    Option::from(PublicKey::from_encoded_point(&point))
        .ok_or_else(|| anyhow!("public key is not on the curve"))
}

fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .map_err(|_| anyhow!("key derivation failed"))?;
    Ok(out)
}

/// Keys for both directions and the PIN both people compare.
fn derive(
    secret: &SecretKey,
    peer: &PublicKey,
    client_init: &[u8],
    server_init: &[u8],
    server: bool,
) -> Result<(Keys, String)> {
    let dh = diffie_hellman(secret.to_nonzero_scalar(), peer.as_affine());
    let shared = Sha256::digest(dh.raw_secret_bytes());
    let mut info = client_init.to_vec();
    info.extend_from_slice(server_init);
    let auth = hkdf32(b"UKEY2 v1 auth", &shared, &info)?;
    let next = hkdf32(b"UKEY2 v1 next", &shared, &info)?;
    let d2d_client = hkdf32(&SALT_D2D, &next, b"client")?;
    let d2d_server = hkdf32(&SALT_D2D, &next, b"server")?;
    let client_enc = hkdf32(&SALT_KEYS, &d2d_client, b"ENC:2")?;
    let client_mac = hkdf32(&SALT_KEYS, &d2d_client, b"SIG:1")?;
    let server_enc = hkdf32(&SALT_KEYS, &d2d_server, b"ENC:2")?;
    let server_mac = hkdf32(&SALT_KEYS, &d2d_server, b"SIG:1")?;
    let keys = if server {
        Keys {
            enc: server_enc,
            enc_mac: server_mac,
            dec: client_enc,
            dec_mac: client_mac,
        }
    } else {
        Keys {
            enc: client_enc,
            enc_mac: client_mac,
            dec: server_enc,
            dec_mac: server_mac,
        }
    };
    Ok((keys, pin(&auth)))
}

/// The four digits both screens show.
fn pin(auth: &[u8]) -> String {
    let (modulo, base) = (9973i32, 31i32);
    let mut hash = 0i32;
    let mut mult = 1i32;
    for &b in auth {
        hash = (hash + (b as i8 as i32) * mult) % modulo;
        mult = (mult * base) % modulo;
    }
    format!("{:04}", hash.abs())
}

/// `(device kind, name)` from the mDNS text record or the connection request.
fn parse_endpoint_info(b64: &str) -> Option<(&'static str, String)> {
    let raw = URL_SAFE_NO_PAD.decode(b64.trim_end_matches('=')).ok()?;
    if raw.len() < 19 {
        return None;
    }
    let kind = match (raw[0] >> 1) & 7 {
        1 => "phone",
        2 => "tablet",
        3 => "laptop",
        _ => "unknown",
    };
    let n = raw[17] as usize;
    let name = String::from_utf8(raw.get(18..18 + n)?.to_vec()).ok()?;
    Some((kind, name))
}

/// A file name without folders or characters Windows refuses.
fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let s: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        "file".into()
    } else {
        s
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.into());
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|c| !c.exists())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_numbers_keep_the_sign_bit_clear() {
        assert_eq!(signed_be(&[0x80, 1]), vec![0, 0x80, 1]);
        assert_eq!(signed_be(&[0, 0, 5]), vec![5]);
        assert_eq!(signed_be(&[0, 0]), vec![0]);
    }

    #[test]
    fn endpoint_info_round_trips() {
        let qs_info = {
            let mut v = vec![3u8 << 1];
            v.extend_from_slice(&[7u8; 16]);
            v.push(5);
            v.extend_from_slice(b"Laptp");
            URL_SAFE_NO_PAD.encode(v)
        };
        assert_eq!(
            parse_endpoint_info(&qs_info),
            Some(("laptop", "Laptp".into()))
        );
    }

    #[test]
    fn salts_decode() {
        assert_eq!(SALT_D2D[0], 0x82);
        assert_eq!(SALT_D2D[31], 0x10);
        assert_eq!(SALT_KEYS[0], 0xBF);
    }
}
