//! JNI bridge for the Android app.
//!
//! The whole API is two functions so there is no code generation to keep in sync:
//!
//! * `Native.call(method, argsJson) -> resultJson` — `{"ok": …}` or `{"err": "…"}`
//! * `Native.nextEvent(timeoutMs) -> eventJson | ""` — incoming clipboard, files, notifications…
//!
//! Both block, so Kotlin calls them from a background dispatcher.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use zima_connect_net::{AdminOp, Event, Node, NodeConfig};

static RT: OnceLock<Runtime> = OnceLock::new();
static NODE: OnceLock<Node> = OnceLock::new();
static EVENTS: OnceLock<Mutex<broadcast::Receiver<Event>>> = OnceLock::new();

fn rt() -> &'static Runtime {
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn node() -> anyhow::Result<&'static Node> {
    NODE.get()
        .ok_or_else(|| anyhow::anyhow!("node not started"))
}

#[derive(Deserialize, Default)]
struct Args {
    #[serde(default)]
    data_dir: String,
    #[serde(default)]
    download_dir: String,
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    app: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    op: Option<AdminOp>,
}

/// Run one API call. Public so it can be tested on the desktop.
pub fn call(method: &str, args_json: &str) -> String {
    let result = rt().block_on(async { dispatch(method, args_json).await });
    match result {
        Ok(v) => json!({ "ok": v }).to_string(),
        Err(e) => json!({ "err": format!("{e:#}") }).to_string(),
    }
}

async fn dispatch(method: &str, args_json: &str) -> anyhow::Result<Value> {
    let a: Args = if args_json.trim().is_empty() {
        Args::default()
    } else {
        serde_json::from_str(args_json)?
    };

    if method == "init" {
        if let Some(n) = NODE.get() {
            return Ok(json!({ "id": n.id().to_string() }));
        }
        let n = Node::start(NodeConfig {
            data_dir: PathBuf::from(&a.data_dir),
            download_dir: PathBuf::from(&a.download_dir),
            device_name: a.device_name,
            platform: "android".into(),
            internet: true,
            hub: None,
        })
        .await?;
        let id = n.id().to_string();
        let _ = EVENTS.set(Mutex::new(n.subscribe()));
        let _ = NODE.set(n);
        return Ok(json!({ "id": id }));
    }

    let n = node()?;
    Ok(match method {
        "contacts" => serde_json::to_value(n.contacts())?,
        "household" => match (n.household_info(), n.household_snapshot()) {
            (Some(info), Some(snapshot)) => json!({ "info": info, "snapshot": snapshot }),
            _ => Value::Null,
        },
        "ping" => {
            let d = tokio::time::timeout(Duration::from_secs(3), n.ping(&a.id)).await??;
            json!({ "rtt_ms": d.as_millis() as u64 })
        }
        "join" => {
            let info = n.join(a.code.trim(), a.user_name).await?;
            json!(info.household)
        }
        "leave" => {
            n.leave_household_async().await;
            Value::Null
        }
        "admin" => {
            let op = a.op.ok_or_else(|| anyhow::anyhow!("missing op"))?;
            json!(n.admin(op).await?)
        }
        "pairing_code" => json!(n.pairing_ticket().await?),
        "pair" => json!(n.pair(a.code.trim()).await?.name),
        "unpair" => {
            n.unpair(&a.id)?;
            Value::Null
        }
        "send_clipboard" => serde_json::to_value(n.send_clipboard(&a.id, a.text).await?)?,
        "send_file" => serde_json::to_value(n.send_file(&a.id, Path::new(&a.path)).await?)?,
        "send_notification" => serde_json::to_value(
            n.send_notification(&a.id, &a.app, &a.title, &a.body)
                .await?,
        )?,
        "ring" => {
            n.ring(&a.id).await?;
            Value::Null
        }
        "ring_stop" => {
            n.ring_stop(&a.id).await?;
            Value::Null
        }
        "pull_inbox" => json!(n.pull_inbox().await?),
        other => anyhow::bail!("unknown method {other}"),
    })
}

/// Wait up to `timeout_ms` for the next event; `None` on timeout.
pub fn next_event(timeout_ms: u64) -> Option<String> {
    let rx = EVENTS.get()?;
    let mut rx = rx.lock().ok()?;
    rt().block_on(async {
        match tokio::time::timeout(Duration::from_millis(timeout_ms), rx.recv()).await {
            Ok(Ok(ev)) => serde_json::to_string(&ev).ok(),
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => Some("{\"kind\":\"lagged\"}".into()),
            _ => None,
        }
    })
}

fn get(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|j| j.into()).unwrap_or_default()
}

fn out(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s)
        .map(|j| j.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_dev_zimaconnect_Native_call(
    mut env: JNIEnv,
    _class: JClass,
    method: JString,
    args: JString,
) -> jstring {
    let m = get(&mut env, &method);
    let a = get(&mut env, &args);
    let r = std::panic::catch_unwind(|| call(&m, &a))
        .unwrap_or_else(|_| json!({ "err": "internal error" }).to_string());
    out(&mut env, &r)
}

#[no_mangle]
pub extern "system" fn Java_dev_zimaconnect_Native_nextEvent(
    mut env: JNIEnv,
    _class: JClass,
    timeout_ms: jlong,
) -> jstring {
    let r = std::panic::catch_unwind(|| next_event(timeout_ms.max(0) as u64))
        .ok()
        .flatten()
        .unwrap_or_default();
    out(&mut env, &r)
}
