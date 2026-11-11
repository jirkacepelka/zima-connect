//! LocalSend compatibility (protocol v2, <https://github.com/localsend/protocol>).
//!
//! Makes this device show up in the LocalSend app and lets it send files to LocalSend devices
//! (phones, computers) on the same network. Discovery is UDP multicast on 224.0.0.167:53317,
//! transfers are HTTPS on a self-signed certificate (its SHA-256 is the device fingerprint).
//! Receiving always asks first: nothing is written until the user accepts.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring as ring_provider, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, ServerConfig, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{broadcast, oneshot};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// The port LocalSend uses for discovery and (by default) transfers.
pub const PORT: u16 = 53317;
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);
const API: &str = "/api/localsend/v2";
const ASK_TIMEOUT: Duration = Duration::from_secs(60);
const PEER_TTL: Duration = Duration::from_secs(10 * 60);

type Body = BoxBody<Bytes, std::io::Error>;

pub struct Config {
    /// Where the certificate is kept (so the fingerprint stays the same).
    pub data_dir: PathBuf,
    /// Where accepted files are saved.
    pub receive_dir: PathBuf,
    pub alias: String,
    pub model: String,
    /// "mobile" | "desktop" | "headless" | …
    pub device_type: String,
    /// TCP port to listen on; 0 picks a free one (the default LocalSend port is [`PORT`]).
    pub port: u16,
    /// Announce on the network and listen for others. Off in tests.
    pub discovery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    alias: String,
    #[serde(default = "version")]
    version: String,
    #[serde(default)]
    device_model: Option<String>,
    #[serde(default)]
    device_type: Option<String>,
    #[serde(default)]
    fingerprint: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    download: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    announce: Option<bool>,
}

fn version() -> String {
    "2.0".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub fingerprint: String,
    pub alias: String,
    pub model: Option<String>,
    pub device_type: String,
    pub ip: String,
    pub port: u16,
    pub https: bool,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LsFile {
    pub id: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LsEvent {
    /// Somebody wants to send files here. Answer with [`LocalSend::decide`].
    Incoming {
        session: String,
        from: String,
        files: Vec<LsFile>,
    },
    /// One file arrived and is saved.
    Received {
        from: String,
        name: String,
        path: String,
        size: u64,
    },
    /// The sender stopped, or everything arrived.
    Done { session: String },
}

#[derive(Deserialize)]
struct PrepareReq {
    info: Info,
    files: HashMap<String, FileMeta>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileMeta {
    #[serde(default)]
    id: String,
    file_name: String,
    size: u64,
    #[serde(default)]
    #[allow(dead_code)]
    file_type: String,
}

struct FileEntry {
    name: String,
    size: u64,
    token: String,
    done: bool,
}

struct Session {
    id: String,
    ip: IpAddr,
    from: String,
    accepted: bool,
    files: HashMap<String, FileEntry>,
    touched: Instant,
}

struct Inner {
    alias: Mutex<String>,
    model: String,
    device_type: String,
    receive_dir: PathBuf,
    fingerprint: String,
    port: u16,
    peers: Mutex<HashMap<String, Peer>>,
    events: broadcast::Sender<LsEvent>,
    session: Mutex<Option<Session>>,
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    tls_client: Arc<ClientConfig>,
    /// Announce this device and accept transfers. Sending to others always works.
    visible: AtomicBool,
}

#[derive(Clone)]
pub struct LocalSend {
    inner: Arc<Inner>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rand_hex(n: usize) -> String {
    use rand::RngCore;
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn json_resp(status: StatusCode, v: serde_json::Value) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(v.to_string())))
        .unwrap()
}

fn plain(status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::new()))
        .unwrap()
}

/// A file name that can only create a file in the target folder.
fn safe_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = p.extension().and_then(|s| s.to_str());
    for n in 1..1000 {
        let cand = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        if !dir.join(&cand).exists() {
            return dir.join(cand);
        }
    }
    dir.join(format!("{}-{name}", rand_hex(4)))
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

/// LocalSend devices present self-signed certificates; the protocol relies on the local
/// network and on asking the receiver, so any certificate is accepted.
#[derive(Debug)]
struct AcceptAny(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn load_or_make_cert(dir: &Path) -> Result<(CertificateDer<'static>, Vec<u8>)> {
    let cert_p = dir.join("localsend-cert.der");
    let key_p = dir.join("localsend-key.der");
    if let (Ok(c), Ok(k)) = (std::fs::read(&cert_p), std::fs::read(&key_p)) {
        return Ok((CertificateDer::from(c), k));
    }
    std::fs::create_dir_all(dir)?;
    let ck = rcgen::generate_simple_self_signed(vec!["localsend".to_string()])?;
    let cert = ck.cert.der().to_vec();
    let key = ck.signing_key.serialize_der();
    std::fs::write(&cert_p, &cert)?;
    std::fs::write(&key_p, &key)?;
    Ok((CertificateDer::from(cert), key))
}

impl LocalSend {
    /// Start listening (and announcing, if `discovery`).
    pub async fn start(cfg: Config) -> Result<LocalSend> {
        let provider = Arc::new(ring_provider::default_provider());
        let (cert, key) = load_or_make_cert(&cfg.data_dir)?;
        let fingerprint: String = Sha256::digest(cert.as_ref())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        let server = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(
                vec![cert],
                PrivateKeyDer::try_from(key).map_err(|e| anyhow!("bad key: {e}"))?,
            )?;
        let acceptor = TlsAcceptor::from(Arc::new(server));
        let client = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAny(provider)))
            .with_no_client_auth();

        std::fs::create_dir_all(&cfg.receive_dir)?;
        let listener = TcpListener::bind(("0.0.0.0", cfg.port))
            .await
            .context("the LocalSend port is in use")?;
        let port = listener.local_addr()?.port();
        let (events, _) = broadcast::channel(64);
        let ls = LocalSend {
            inner: Arc::new(Inner {
                alias: Mutex::new(cfg.alias),
                model: cfg.model,
                device_type: cfg.device_type,
                receive_dir: cfg.receive_dir,
                fingerprint,
                port,
                peers: Mutex::new(HashMap::new()),
                events,
                session: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                tls_client: Arc::new(client),
                visible: AtomicBool::new(true),
            }),
        };

        let me = ls.clone();
        tokio::spawn(async move {
            loop {
                let Ok((sock, addr)) = listener.accept().await else {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                };
                let me = me.clone();
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(sock).await else {
                        return;
                    };
                    let io = TokioIo::new(tls);
                    let svc = service_fn(move |req| {
                        let me = me.clone();
                        async move { Ok::<_, Infallible>(me.route(req, addr.ip()).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });

        if cfg.discovery {
            ls.spawn_discovery();
        }
        Ok(ls)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LsEvent> {
        self.inner.events.subscribe()
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    pub fn fingerprint(&self) -> &str {
        &self.inner.fingerprint
    }

    /// Show this device to LocalSend users (and accept what they send) or hide it.
    pub fn set_visible(&self, on: bool) {
        self.inner.visible.store(on, Ordering::Relaxed);
    }

    pub fn set_alias(&self, alias: &str) {
        *self.inner.alias.lock().unwrap() = alias.to_string();
    }

    /// LocalSend devices seen on the network lately.
    pub fn peers(&self) -> Vec<Peer> {
        let mut g = self.inner.peers.lock().unwrap();
        let cutoff = now_secs().saturating_sub(PEER_TTL.as_secs());
        g.retain(|_, p| p.last_seen >= cutoff);
        let mut v: Vec<Peer> = g.values().cloned().collect();
        v.sort_by_key(|p| p.alias.to_lowercase());
        v
    }

    /// A peer at a known address (for devices multicast did not find).
    pub fn peer_at(&self, ip: IpAddr, port: u16, https: bool) -> Peer {
        Peer {
            fingerprint: String::new(),
            alias: ip.to_string(),
            model: None,
            device_type: "desktop".into(),
            ip: ip.to_string(),
            port,
            https,
            last_seen: now_secs(),
        }
    }

    fn info(&self, announce: Option<bool>) -> Info {
        Info {
            alias: self.inner.alias.lock().unwrap().clone(),
            version: version(),
            device_model: Some(self.inner.model.clone()),
            device_type: Some(self.inner.device_type.clone()),
            fingerprint: self.inner.fingerprint.clone(),
            port: Some(self.inner.port),
            protocol: Some("https".into()),
            download: Some(false),
            announce,
        }
    }

    fn add_peer(&self, ip: IpAddr, info: &Info) {
        if info.fingerprint == self.inner.fingerprint || info.fingerprint.is_empty() {
            return;
        }
        let p = Peer {
            fingerprint: info.fingerprint.clone(),
            alias: info.alias.clone(),
            model: info.device_model.clone(),
            device_type: info.device_type.clone().unwrap_or_else(|| "desktop".into()),
            ip: ip.to_string(),
            port: info.port.unwrap_or(PORT),
            https: info.protocol.as_deref() != Some("http"),
            last_seen: now_secs(),
        };
        self.inner
            .peers
            .lock()
            .unwrap()
            .insert(info.fingerprint.clone(), p);
    }

    /// Answer a [`LsEvent::Incoming`].
    pub fn decide(&self, session: &str, accept: bool) -> Result<()> {
        match self.inner.pending.lock().unwrap().remove(session) {
            Some(tx) => {
                let _ = tx.send(accept);
                Ok(())
            }
            None => bail!("this request is no longer open"),
        }
    }

    // -- discovery ----------------------------------------------------------------------

    fn spawn_discovery(&self) {
        use socket2::{Domain, Protocol, Socket, Type};
        let make = || -> Result<UdpSocket> {
            let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            s.set_reuse_address(true)?;
            #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
            let _ = s.set_reuse_port(true);
            s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;
            s.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
            s.set_multicast_loop_v4(true)?;
            s.set_nonblocking(true)?;
            Ok(UdpSocket::from_std(s.into())?)
        };
        let sock = match make() {
            Ok(s) => Arc::new(s),
            Err(e) => {
                tracing::warn!("LocalSend discovery is off: {e}");
                return;
            }
        };

        let me = self.clone();
        let rx = sock.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            loop {
                let Ok((n, from)) = rx.recv_from(&mut buf).await else {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                };
                let Ok(info) = serde_json::from_slice::<Info>(&buf[..n]) else {
                    continue;
                };
                if info.fingerprint == me.inner.fingerprint {
                    continue;
                }
                me.add_peer(from.ip(), &info);
                if info.announce == Some(true) && me.inner.visible.load(Ordering::Relaxed) {
                    // Tell the announcer about us over HTTP, as the protocol says.
                    let me = me.clone();
                    tokio::spawn(async move {
                        let peer = me.peer_at(
                            from.ip(),
                            info.port.unwrap_or(PORT),
                            info.protocol.as_deref() != Some("http"),
                        );
                        let body = serde_json::to_vec(&me.info(None)).unwrap_or_default();
                        let _ = me
                            .request(
                                &peer,
                                Method::POST,
                                &format!("{API}/register"),
                                full(body),
                                Some(10),
                            )
                            .await;
                    });
                }
            }
        });

        let me = self.clone();
        tokio::spawn(async move {
            let dest = SocketAddr::from((GROUP, PORT));
            let mut round = 0u32;
            loop {
                if me.inner.visible.load(Ordering::Relaxed) {
                    let msg = serde_json::to_vec(&me.info(Some(true))).unwrap_or_default();
                    let _ = sock.send_to(&msg, dest).await;
                }
                round += 1;
                tokio::time::sleep(if round < 4 {
                    Duration::from_secs(2)
                } else {
                    Duration::from_secs(30)
                })
                .await;
            }
        });
    }

    // -- server -------------------------------------------------------------------------

    async fn route(&self, req: Request<Incoming>, ip: IpAddr) -> Response<Full<Bytes>> {
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        let Some(rest) = path.strip_prefix(API) else {
            return plain(StatusCode::NOT_FOUND);
        };
        match (req.method(), rest) {
            (&Method::GET, "/info") => json_resp(
                StatusCode::OK,
                serde_json::to_value(self.info(None)).unwrap_or_default(),
            ),
            (&Method::POST, "/register") => {
                let Some(body) = read_small(req).await else {
                    return plain(StatusCode::BAD_REQUEST);
                };
                let Ok(info) = serde_json::from_slice::<Info>(&body) else {
                    return plain(StatusCode::BAD_REQUEST);
                };
                self.add_peer(ip, &info);
                json_resp(
                    StatusCode::OK,
                    serde_json::to_value(self.info(None)).unwrap_or_default(),
                )
            }
            (&Method::POST, "/prepare-upload") => {
                let Some(body) = read_small(req).await else {
                    return plain(StatusCode::BAD_REQUEST);
                };
                match serde_json::from_slice::<PrepareReq>(&body) {
                    Ok(p) => self.prepare(p, ip).await,
                    Err(_) => plain(StatusCode::BAD_REQUEST),
                }
            }
            (&Method::POST, "/upload") => self.upload(req, &query, ip).await,
            (&Method::POST, "/cancel") => {
                if let Some(id) = query_param(&query, "sessionId") {
                    let mut g = self.inner.session.lock().unwrap();
                    if g.as_ref().is_some_and(|s| s.id == id && s.ip == ip) {
                        *g = None;
                        drop(g);
                        let _ = self.inner.events.send(LsEvent::Done { session: id });
                    }
                }
                plain(StatusCode::OK)
            }
            _ => plain(StatusCode::NOT_FOUND),
        }
    }

    async fn prepare(&self, p: PrepareReq, ip: IpAddr) -> Response<Full<Bytes>> {
        if p.files.is_empty() {
            return plain(StatusCode::BAD_REQUEST);
        }
        if !self.inner.visible.load(Ordering::Relaxed) {
            return plain(StatusCode::FORBIDDEN);
        }
        let sid = rand_hex(12);
        let mut files = HashMap::new();
        let mut shown = Vec::new();
        for (key, f) in p.files {
            let id = if f.id.is_empty() { key } else { f.id };
            let name = safe_name(&f.file_name);
            shown.push(LsFile {
                id: id.clone(),
                name: name.clone(),
                size: f.size,
            });
            files.insert(
                id,
                FileEntry {
                    name,
                    size: f.size,
                    token: rand_hex(16),
                    done: false,
                },
            );
        }
        {
            let mut g = self.inner.session.lock().unwrap();
            if g.as_ref()
                .is_some_and(|s| s.touched.elapsed() < Duration::from_secs(30 * 60))
            {
                return plain(StatusCode::CONFLICT);
            }
            *g = Some(Session {
                id: sid.clone(),
                ip,
                from: p.info.alias.clone(),
                accepted: false,
                files,
                touched: Instant::now(),
            });
        }
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().unwrap().insert(sid.clone(), tx);
        let _ = self.inner.events.send(LsEvent::Incoming {
            session: sid.clone(),
            from: p.info.alias,
            files: shown,
        });
        let yes = matches!(tokio::time::timeout(ASK_TIMEOUT, rx).await, Ok(Ok(true)));
        self.inner.pending.lock().unwrap().remove(&sid);
        let mut g = self.inner.session.lock().unwrap();
        if !yes {
            if g.as_ref().is_some_and(|s| s.id == sid) {
                *g = None;
            }
            return plain(StatusCode::FORBIDDEN);
        }
        let Some(s) = g.as_mut().filter(|s| s.id == sid) else {
            return plain(StatusCode::CONFLICT);
        };
        s.accepted = true;
        s.touched = Instant::now();
        let tokens: HashMap<&String, &String> =
            s.files.iter().map(|(k, f)| (k, &f.token)).collect();
        json_resp(
            StatusCode::OK,
            serde_json::json!({ "sessionId": sid, "files": tokens }),
        )
    }

    async fn upload(
        &self,
        req: Request<Incoming>,
        query: &str,
        ip: IpAddr,
    ) -> Response<Full<Bytes>> {
        let (Some(sid), Some(fid), Some(token)) = (
            query_param(query, "sessionId"),
            query_param(query, "fileId"),
            query_param(query, "token"),
        ) else {
            return plain(StatusCode::BAD_REQUEST);
        };
        let (name, size, from) = {
            let mut g = self.inner.session.lock().unwrap();
            let Some(s) = g
                .as_mut()
                .filter(|s| s.id == sid && s.accepted && s.ip == ip)
            else {
                return plain(StatusCode::FORBIDDEN);
            };
            let Some(f) = s.files.get(&fid).filter(|f| f.token == token && !f.done) else {
                return plain(StatusCode::FORBIDDEN);
            };
            s.touched = Instant::now();
            (f.name.clone(), f.size, s.from.clone())
        };

        let dest = unique_path(&self.inner.receive_dir, &name);
        let part = dest.with_extension(format!(
            "{}part",
            dest.extension()
                .and_then(|e| e.to_str())
                .map(|e| format!("{e}."))
                .unwrap_or_default()
        ));
        let result: Result<u64> = async {
            let mut file = tokio::fs::File::create(&part).await?;
            let mut body = req.into_body();
            let mut got = 0u64;
            while let Some(frame) = body.frame().await {
                let frame = frame?;
                if let Some(d) = frame.data_ref() {
                    got += d.len() as u64;
                    if got > size {
                        bail!("more data than announced");
                    }
                    file.write_all(d).await?;
                }
            }
            file.flush().await?;
            if got != size {
                bail!("the file ended early");
            }
            Ok(got)
        }
        .await;
        match result {
            Ok(got) => {
                if tokio::fs::rename(&part, &dest).await.is_err() {
                    let _ = tokio::fs::remove_file(&part).await;
                    return plain(StatusCode::INTERNAL_SERVER_ERROR);
                }
                let all_done = {
                    let mut g = self.inner.session.lock().unwrap();
                    let mut finished = false;
                    if let Some(s) = g.as_mut().filter(|s| s.id == sid) {
                        if let Some(f) = s.files.get_mut(&fid) {
                            f.done = true;
                        }
                        finished = s.files.values().all(|f| f.done);
                        if finished {
                            *g = None;
                        }
                    }
                    finished
                };
                let _ = self.inner.events.send(LsEvent::Received {
                    from,
                    name: dest
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .into(),
                    path: dest.to_string_lossy().into(),
                    size: got,
                });
                if all_done {
                    let _ = self.inner.events.send(LsEvent::Done { session: sid });
                }
                plain(StatusCode::OK)
            }
            Err(e) => {
                tracing::warn!("LocalSend upload failed: {e}");
                let _ = tokio::fs::remove_file(&part).await;
                plain(StatusCode::BAD_REQUEST)
            }
        }
    }

    // -- client -------------------------------------------------------------------------

    async fn request(
        &self,
        peer: &Peer,
        method: Method,
        path_and_query: &str,
        body: Body,
        timeout_s: Option<u64>,
    ) -> Result<(StatusCode, Bytes)> {
        let ip: IpAddr = peer.ip.parse().map_err(|_| anyhow!("bad address"))?;
        let req = Request::builder()
            .method(method)
            .uri(path_and_query)
            .header("host", format!("{}:{}", peer.ip, peer.port))
            .header("content-type", "application/json")
            .body(body)?;
        let work = async {
            let tcp =
                tokio::time::timeout(Duration::from_secs(5), TcpStream::connect((ip, peer.port)))
                    .await
                    .map_err(|_| anyhow!("{} did not answer", peer.alias))??;
            if peer.https {
                let tls = TlsConnector::from(self.inner.tls_client.clone())
                    .connect(ServerName::IpAddress(ip.into()), tcp)
                    .await?;
                send_over(TokioIo::new(tls), req).await
            } else {
                send_over(TokioIo::new(tcp), req).await
            }
        };
        match timeout_s {
            Some(s) => tokio::time::timeout(Duration::from_secs(s), work)
                .await
                .map_err(|_| anyhow!("{} did not answer", peer.alias))?,
            None => work.await,
        }
    }

    /// Send files to a LocalSend device. Returns once everything arrived (or it was refused).
    pub async fn send(&self, peer: &Peer, paths: &[PathBuf]) -> Result<()> {
        let mut files = serde_json::Map::new();
        let mut order: Vec<(String, PathBuf)> = Vec::new();
        for p in paths {
            let meta = tokio::fs::metadata(p).await?;
            if !meta.is_file() {
                bail!("{} is not a file", p.display());
            }
            let id = rand_hex(8);
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            files.insert(
                id.clone(),
                serde_json::json!({
                    "id": id,
                    "fileName": name,
                    "size": meta.len(),
                    "fileType": mime_guess::from_path(p).first_or_octet_stream().to_string(),
                }),
            );
            order.push((id, p.clone()));
        }
        if order.is_empty() {
            bail!("nothing to send");
        }
        let body = serde_json::to_vec(&serde_json::json!({
            "info": self.info(None),
            "files": files,
        }))?;
        let (status, resp) = self
            .request(
                peer,
                Method::POST,
                &format!("{API}/prepare-upload"),
                full(body),
                Some(ASK_TIMEOUT.as_secs() + 15),
            )
            .await?;
        match status {
            StatusCode::OK => {}
            StatusCode::NO_CONTENT => return Ok(()),
            StatusCode::FORBIDDEN => bail!("{} declined the files", peer.alias),
            StatusCode::UNAUTHORIZED => {
                bail!("{} needs a PIN, which is not supported yet", peer.alias)
            }
            StatusCode::CONFLICT => bail!("{} is busy with another transfer", peer.alias),
            s => bail!("{} answered with {s}", peer.alias),
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Accepted {
            session_id: String,
            files: HashMap<String, String>,
        }
        let acc: Accepted = serde_json::from_slice(&resp).context("unexpected answer")?;
        for (id, path) in order {
            let Some(token) = acc.files.get(&id) else {
                continue;
            };
            let file = tokio::fs::File::open(&path).await?;
            let stream = tokio_util::io::ReaderStream::new(file).map_ok(Frame::data);
            let body: Body = StreamBody::new(stream).boxed();
            let q = format!(
                "{API}/upload?sessionId={}&fileId={id}&token={token}",
                acc.session_id
            );
            let (st, _) = self.request(peer, Method::POST, &q, body, None).await?;
            if st != StatusCode::OK {
                let _ = self
                    .request(
                        peer,
                        Method::POST,
                        &format!("{API}/cancel?sessionId={}", acc.session_id),
                        full(Vec::new()),
                        Some(5),
                    )
                    .await;
                bail!("{} stopped the transfer ({st})", peer.alias);
            }
        }
        Ok(())
    }
}

fn full(b: Vec<u8>) -> Body {
    Full::new(Bytes::from(b)).map_err(|e| match e {}).boxed()
}

async fn read_small(req: Request<Incoming>) -> Option<Bytes> {
    let b = req.into_body().collect().await.ok()?.to_bytes();
    (b.len() <= 4 * 1024 * 1024).then_some(b)
}

async fn send_over<T>(io: TokioIo<T>, req: Request<Body>) -> Result<(StatusCode, Bytes)>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let resp = sender.send_request(req).await?;
    let status = resp.status();
    let body = resp.into_body().collect().await?.to_bytes();
    Ok((status, body))
}
