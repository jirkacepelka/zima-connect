//! Zima Connect household server.
//!
//! * Speaks the Zima Connect P2P protocol (devices join, sync people and policy, offline queue).
//! * Serves the administration dashboard (password protected) for people, devices and policy.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zima_connect_net::{AdminOp, Node, NodeConfig, RelaySetting};

const DASHBOARD: &str = include_str!("dashboard.html");
const FONT_SANS: &[u8] = include_bytes!("fonts/Geist-Variable.woff2");
const FONT_MONO: &[u8] = include_bytes!("fonts/GeistMono-Variable.woff2");

fn font(bytes: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response()
}
const SESSION_COOKIE: &str = "zc_session";

#[derive(Serialize, Deserialize, Clone)]
struct AdminRecord {
    salt: String,
    hash: String,
}

/// The server's own relay (so devices need no public relay at all).
#[derive(Serialize, Deserialize, Clone, Default)]
struct RelayCfg {
    enabled: bool,
    /// Public address devices use to reach this relay (IP or name).
    host: String,
    port: u16,
    /// Devices use only this relay, never the public ones.
    exclusive: bool,
}

struct AppState {
    node: Node,
    relay_path: PathBuf,
    relay_cfg: Mutex<RelayCfg>,
    relay_srv: tokio::sync::Mutex<Option<iroh_relay::server::Server>>,
    admin_path: PathBuf,
    admin: Mutex<Option<AdminRecord>>,
    sessions: Mutex<HashSet<String>>,
    /// device id -> round-trip ms (None = unreachable), refreshed in the background.
    probes: Mutex<HashMap<String, Option<u64>>>,
    started: Instant,
}

impl AppState {
    /// (Re)start or stop the relay server according to the settings.
    async fn apply_relay(&self) -> Result<(), String> {
        let cfg = self.relay_cfg.lock().unwrap().clone();
        let mut srv = self.relay_srv.lock().await;
        if let Some(old) = srv.take() {
            let _ = old.shutdown().await;
        }
        if !cfg.enabled {
            return Ok(());
        }
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], cfg.port));
        let mut c = iroh_relay::server::ServerConfig::default();
        c.relay = Some(iroh_relay::server::RelayConfig::new(addr));
        match iroh_relay::server::Server::spawn(c).await {
            Ok(s) => {
                *srv = Some(s);
                Ok(())
            }
            Err(e) => Err(format!(
                "could not start the relay on port {}: {e}",
                cfg.port
            )),
        }
    }

    fn relay_setting(&self) -> Option<RelaySetting> {
        let c = self.relay_cfg.lock().unwrap();
        if !c.enabled || c.host.trim().is_empty() {
            return None;
        }
        Some(RelaySetting {
            url: format!("http://{}:{}", c.host.trim(), c.port),
            exclusive: c.exclusive,
        })
    }
}

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn rand_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hash_password(salt: &str, password: &str) -> String {
    let mut h = Sha256::digest(format!("{salt}{password}").as_bytes());
    for _ in 0..50_000 {
        let mut d = Sha256::new();
        d.update(h);
        d.update(salt.as_bytes());
        d.update(password.as_bytes());
        h = d.finalize();
    }
    h.iter().map(|x| format!("{x:02x}")).collect()
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};
    QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L)
        .map(|c| {
            c.render::<svg::Color>()
                .min_dimensions(260, 260)
                .quiet_zone(true)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_default()
}

impl AppState {
    fn authed(&self, headers: &HeaderMap) -> bool {
        let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else {
            return false;
        };
        cookie
            .split(';')
            .filter_map(|p| p.trim().split_once('='))
            .any(|(k, v)| k == SESSION_COOKIE && self.sessions.lock().unwrap().contains(v))
    }

    fn new_session(&self) -> String {
        let token = rand_hex(32);
        self.sessions.lock().unwrap().insert(token.clone());
        token
    }

    fn save_admin(&self, rec: &AdminRecord) {
        let _ = std::fs::write(
            &self.admin_path,
            serde_json::to_vec(rec).unwrap_or_default(),
        );
        *self.admin.lock().unwrap() = Some(rec.clone());
    }
}

fn with_session(token: &str, body: Value) -> Response {
    let cookie =
        format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000");
    (StatusCode::OK, [(header::SET_COOKIE, cookie)], Json(body)).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

type St = State<Arc<AppState>>;

async fn session(State(st): St, headers: HeaderMap) -> Response {
    Json(json!({
        "configured": st.admin.lock().unwrap().is_some(),
        "authed": st.authed(&headers),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct Password {
    password: String,
}

async fn setup(State(st): St, Json(b): Json<Password>) -> Response {
    if st.admin.lock().unwrap().is_some() {
        return err(
            StatusCode::CONFLICT,
            "an administrator password is already set",
        );
    }
    if b.password.chars().count() < 8 {
        return err(StatusCode::BAD_REQUEST, "use at least 8 characters");
    }
    let salt = rand_hex(16);
    let hash = hash_password(&salt, &b.password);
    st.save_admin(&AdminRecord { salt, hash });
    let token = st.new_session();
    with_session(&token, json!({ "ok": true }))
}

async fn login(State(st): St, Json(b): Json<Password>) -> Response {
    let rec = st.admin.lock().unwrap().clone();
    let Some(rec) = rec else {
        return err(StatusCode::CONFLICT, "no administrator password set yet");
    };
    let ok = constant_eq(&hash_password(&rec.salt, &b.password), &rec.hash);
    if !ok {
        tokio::time::sleep(Duration::from_secs(1)).await; // slows down guessing
        return err(StatusCode::UNAUTHORIZED, "wrong password");
    }
    let token = st.new_session();
    with_session(&token, json!({ "ok": true }))
}

async fn logout(State(st): St, headers: HeaderMap) -> Response {
    if let Some(cookie) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for (k, v) in cookie.split(';').filter_map(|p| p.trim().split_once('=')) {
            if k == SESSION_COOKIE {
                st.sessions.lock().unwrap().remove(v);
            }
        }
    }
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct ChangePassword {
    current: String,
    new: String,
}

async fn change_password(
    State(st): St,
    headers: HeaderMap,
    Json(b): Json<ChangePassword>,
) -> Response {
    if !st.authed(&headers) {
        return err(StatusCode::UNAUTHORIZED, "sign in first");
    }
    let rec = st.admin.lock().unwrap().clone();
    let Some(rec) = rec else {
        return err(StatusCode::CONFLICT, "no password set");
    };
    if !constant_eq(&hash_password(&rec.salt, &b.current), &rec.hash) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        return err(StatusCode::UNAUTHORIZED, "current password is wrong");
    }
    if b.new.chars().count() < 8 {
        return err(StatusCode::BAD_REQUEST, "use at least 8 characters");
    }
    let salt = rand_hex(16);
    let hash = hash_password(&salt, &b.new);
    st.save_admin(&AdminRecord { salt, hash });
    Json(json!({ "ok": true })).into_response()
}

async fn state(State(st): St, headers: HeaderMap) -> Response {
    if !st.authed(&headers) {
        return err(StatusCode::UNAUTHORIZED, "sign in first");
    }
    let Some(d) = st.node.hub_dashboard() else {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "not a household server");
    };
    let probes = st.probes.lock().unwrap().clone();
    let devices: Vec<Value> = d
        .devices
        .iter()
        .map(|x| {
            let p = probes.get(&x.id).copied().flatten();
            json!({
                "id": x.id, "user_id": x.user_id, "name": x.name, "platform": x.platform,
                "online": p.is_some(), "rtt_ms": p,
            })
        })
        .collect();
    let invites: Vec<Value> = d
        .invites
        .iter()
        .map(|i| json!({ "secret": i.secret, "user": i.user, "label": i.label, "code": i.code, "expires": i.expires, "qr": qr_svg(&i.code) }))
        .collect();
    let (queued, queued_bytes) = st.node.hub_mailbox_stats().unwrap_or((0, 0));
    Json(json!({
        "id": d.id,
        "household": d.household,
        "version": d.version,
        "users": d.users,
        "devices": devices,
        "rules": d.rules,
        "invites": invites,
        "shelf": st.node.hub_shelf().unwrap_or_default(),
        "queue": { "items": queued, "bytes": queued_bytes },
        "uptime_secs": st.started.elapsed().as_secs(),
        "relay": st.node.wait_online(Duration::from_millis(1)).await,
        "own_relay": {
            "enabled": st.relay_cfg.lock().unwrap().enabled,
            "host": st.relay_cfg.lock().unwrap().host,
            "port": st.relay_cfg.lock().unwrap().port,
            "exclusive": st.relay_cfg.lock().unwrap().exclusive,
            "running": st.relay_srv.lock().await.is_some(),
            "detected": st.node.public_ips(),
            "announced": d.relay.as_ref().map(|r| r.url.clone()),
        },
        "now": now(),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct AdminBody {
    op: AdminOp,
}

async fn admin(State(st): St, headers: HeaderMap, Json(b): Json<AdminBody>) -> Response {
    if !st.authed(&headers) {
        return err(StatusCode::UNAUTHORIZED, "sign in first");
    }
    let is_invite = matches!(b.op, AdminOp::CreateInvite { .. });
    match st.node.hub_admin(b.op).await {
        Ok(Some(code)) if is_invite => {
            Json(json!({ "code": code, "qr": qr_svg(&code) })).into_response()
        }
        Ok(Some(id)) => Json(json!({ "id": id })).into_response(),
        Ok(None) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &format!("{e}")),
    }
}

#[derive(Deserialize)]
struct RelayBody {
    enabled: bool,
    host: String,
    port: u16,
    exclusive: bool,
}

async fn set_relay(State(st): St, headers: HeaderMap, Json(b): Json<RelayBody>) -> Response {
    if !st.authed(&headers) {
        return err(StatusCode::UNAUTHORIZED, "sign in first");
    }
    let host = b.host.trim().to_string();
    if b.enabled {
        if host.is_empty() || host.contains(['/', ' ', '@']) {
            return err(
                StatusCode::BAD_REQUEST,
                "enter the public address of this server",
            );
        }
        if b.port < 1024 {
            return err(StatusCode::BAD_REQUEST, "use a port from 1024 up");
        }
    }
    {
        let mut c = st.relay_cfg.lock().unwrap();
        *c = RelayCfg {
            enabled: b.enabled,
            host,
            port: if b.port == 0 { 3340 } else { b.port },
            exclusive: b.exclusive,
        };
        let _ = std::fs::write(
            &st.relay_path,
            serde_json::to_vec_pretty(&*c).unwrap_or_default(),
        );
    }
    if let Err(e) = st.apply_relay().await {
        return err(StatusCode::BAD_REQUEST, &e);
    }
    match st
        .node
        .hub_admin(AdminOp::SetRelay {
            relay: st.relay_setting(),
        })
        .await
    {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &format!("{e}")),
    }
}

#[derive(Deserialize)]
struct ShelfDelete {
    id: String,
}

async fn shelf_delete(State(st): St, headers: HeaderMap, Json(b): Json<ShelfDelete>) -> Response {
    if !st.authed(&headers) {
        return err(StatusCode::UNAUTHORIZED, "sign in first");
    }
    st.node.hub_shelf_delete(&b.id);
    Json(json!({ "ok": true })).into_response()
}

/// Keep device reachability fresh without blocking the dashboard.
fn spawn_prober(st: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            let ids: Vec<String> = st
                .node
                .hub_dashboard()
                .map(|d| d.devices.into_iter().map(|x| x.id).collect())
                .unwrap_or_default();
            let mut tasks = Vec::new();
            for id in ids {
                let node = st.node.clone();
                tasks.push(tokio::spawn(async move {
                    let r = node.hub_probe(&id).await;
                    (id, r)
                }));
            }
            let mut fresh = HashMap::new();
            for t in tasks {
                if let Ok((id, r)) = t.await {
                    fresh.insert(id, r);
                }
            }
            *st.probes.lock().unwrap() = fresh;
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = PathBuf::from(env("ZC_DATA_DIR", "./data"));
    let port: u16 = env("ZC_DASHBOARD_PORT", "9121").parse().unwrap_or(9121);
    let household = env("ZC_HOUSEHOLD", "Home");

    let node = Node::start(NodeConfig {
        download_dir: data_dir.join("files"),
        data_dir: data_dir.clone(),
        device_name: env("ZC_SERVER_NAME", "Zima Connect Hub"),
        platform: "server".into(),
        internet: env("ZC_OFFLINE", "") != "1",
        hub: Some(household),
    })
    .await?;

    let admin_path = data_dir.join("admin.json");
    let admin_rec: Option<AdminRecord> = std::fs::read(&admin_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());

    let relay_path = data_dir.join("relay.json");
    let relay_cfg: RelayCfg = std::fs::read(&relay_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(RelayCfg {
            enabled: false,
            host: String::new(),
            port: 3340,
            exclusive: false,
        });

    let st = Arc::new(AppState {
        node: node.clone(),
        relay_path,
        relay_cfg: Mutex::new(relay_cfg),
        relay_srv: tokio::sync::Mutex::new(None),
        admin_path,
        admin: Mutex::new(admin_rec),
        sessions: Mutex::new(HashSet::new()),
        probes: Mutex::new(HashMap::new()),
        started: Instant::now(),
    });

    if st.relay_cfg.lock().unwrap().enabled {
        match st.apply_relay().await {
            Ok(()) => println!(
                "Own relay running on port {}.",
                st.relay_cfg.lock().unwrap().port
            ),
            Err(e) => println!("Warning: {e}"),
        }
    }

    let app = Router::new()
        .route("/", get(|| async { Html(DASHBOARD) }))
        .route("/health", get(|| async { "ok" }))
        .route(
            "/fonts/Geist-Variable.woff2",
            get(|| async { font(FONT_SANS) }),
        )
        .route(
            "/fonts/GeistMono-Variable.woff2",
            get(|| async { font(FONT_MONO) }),
        )
        .route("/api/session", get(session))
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/password", post(change_password))
        .route("/api/state", get(state))
        .route("/api/admin", post(admin))
        .route("/api/relay", post(set_relay))
        .route("/api/shelf/delete", post(shelf_delete))
        .with_state(st.clone());

    // Serve the page right away; the relay connection can take a moment.
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("Dashboard: http://0.0.0.0:{port}/");
    println!("Server id: {}", node.id());
    spawn_prober(st.clone());

    let n2 = node.clone();
    tokio::spawn(async move {
        if n2.wait_online(Duration::from_secs(15)).await {
            println!("Connected to the relay network.");
        } else {
            println!("Warning: could not reach the relay network yet; devices outside your home network may not find this server.");
        }
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    node.shutdown().await;
    Ok(())
}
