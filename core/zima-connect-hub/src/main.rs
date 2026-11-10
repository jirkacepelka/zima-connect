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
use zima_connect_net::{AdminOp, Node, NodeConfig};

const DASHBOARD: &str = include_str!("dashboard.html");
const SESSION_COOKIE: &str = "zc_session";

#[derive(Serialize, Deserialize, Clone)]
struct AdminRecord {
    salt: String,
    hash: String,
}

struct AppState {
    node: Node,
    admin_path: PathBuf,
    admin: Mutex<Option<AdminRecord>>,
    sessions: Mutex<HashSet<String>>,
    /// device id -> round-trip ms (None = unreachable), refreshed in the background.
    probes: Mutex<HashMap<String, Option<u64>>>,
    started: Instant,
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
        .map(|i| json!({ "secret": i.secret, "label": i.label, "code": i.code, "expires": i.expires, "qr": qr_svg(&i.code) }))
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
        "queue": { "items": queued, "bytes": queued_bytes },
        "uptime_secs": st.started.elapsed().as_secs(),
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
    match st.node.hub_admin(b.op).await {
        Ok(Some(code)) => Json(json!({ "code": code, "qr": qr_svg(&code) })).into_response(),
        Ok(None) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &format!("{e}")),
    }
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

    let st = Arc::new(AppState {
        node: node.clone(),
        admin_path,
        admin: Mutex::new(admin_rec),
        sessions: Mutex::new(HashSet::new()),
        probes: Mutex::new(HashMap::new()),
        started: Instant::now(),
    });

    let app = Router::new()
        .route("/", get(|| async { Html(DASHBOARD) }))
        .route("/health", get(|| async { "ok" }))
        .route("/api/session", get(session))
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/password", post(change_password))
        .route("/api/state", get(state))
        .route("/api/admin", post(admin))
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
