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
use zima_connect_net::{AdminOp, Event, Node, NodeConfig, RemoteInput, SmsOp};

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
    #[serde(default)]
    input: Option<RemoteInput>,
    #[serde(default)]
    percent: u8,
    #[serde(default)]
    hue: u16,
    #[serde(default)]
    call_id: u64,
    #[serde(default)]
    sms_op: Option<SmsOp>,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    charging: bool,
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
        "rename_device" => {
            n.rename_device(&a.device_name).await?;
            Value::Null
        }
        "call_start" => json!(n.call_start(&a.id).await?),
        "call_accept" => {
            n.call_accept(a.call_id)?;
            Value::Null
        }
        "call_decline" => {
            n.call_decline(a.call_id)?;
            Value::Null
        }
        "call_hangup" => {
            n.call_hangup(a.call_id);
            Value::Null
        }
        "call_send" => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(&a.text)?;
            n.call_send(a.call_id, bytes)?;
            Value::Null
        }
        // Waits (up to 2 s) for the next audio frame; null when nothing came, "end" when the call is over.
        "call_recv" => {
            use base64::Engine;
            match tokio::time::timeout(Duration::from_secs(2), n.call_recv(a.call_id)).await {
                Ok(Some(d)) => json!(base64::engine::general_purpose::STANDARD.encode(d)),
                Ok(None) => json!("end"),
                Err(_) => Value::Null,
            }
        }
        // The phone answers a question another device asked about its text messages.
        "sms_reply" => {
            n.sms_reply(a.call_id, a.value.unwrap_or(Value::Null))?;
            Value::Null
        }
        "sms_push" => {
            n.sms_push(&a.id, &a.path, a.user_name.clone(), &a.text)
                .await?;
            Value::Null
        }
        "sms" => {
            let op = a.sms_op.ok_or_else(|| anyhow::anyhow!("missing op"))?;
            n.sms(&a.id, op).await?
        }
        "set_color" => {
            n.set_my_color(a.hue).await?;
            Value::Null
        }
        "send_url" => {
            n.send_url(&a.id, &a.text).await?;
            Value::Null
        }
        "send_battery" => {
            n.send_battery(&a.id, a.percent, a.charging).await?;
            Value::Null
        }
        "send_remote" => {
            let input = a.input.ok_or_else(|| anyhow::anyhow!("missing input"))?;
            n.send_remote(&a.id, input).await?;
            Value::Null
        }
        "shelf_list" => serde_json::to_value(n.shelf_list().await?)?,
        "shelf_put" => serde_json::to_value(n.shelf_put(Path::new(&a.path)).await?)?,
        "shelf_get" => json!(n.shelf_get(&a.id).await?.to_string_lossy()),
        // Download into `path` (the app's cache folder) for previews.
        "shelf_cache" => json!(n
            .shelf_get_in(&a.id, Path::new(&a.path))
            .await?
            .to_string_lossy()),
        "shelf_delete" => {
            n.shelf_delete(&a.id).await?;
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
