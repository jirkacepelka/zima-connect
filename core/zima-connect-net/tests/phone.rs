use std::time::Duration;
use zima_connect_net::{Event, Node, NodeConfig, PhoneOp};

fn cfg(dir: &std::path::Path, name: &str) -> NodeConfig {
    NodeConfig {
        data_dir: dir.join(name).join("data"),
        download_dir: dir.join(name).join("dl"),
        device_name: name.into(),
        platform: "test".into(),
        internet: false,
        hub: None,
    }
}

/// A computer dials through a phone, the phone's app answers; call state is pushed back.
#[tokio::test(flavor = "multi_thread")]
async fn phone_line() {
    let tmp = std::env::temp_dir().join(format!("zc-phone-{}", std::process::id()));
    let pc = Node::start(cfg(&tmp, "pc")).await.unwrap();
    let phone = Node::start(cfg(&tmp, "phone")).await.unwrap();
    let ticket = pc.pairing_ticket().await.unwrap();
    phone.pair(&ticket).await.unwrap();
    let phone_id = phone.id().to_string();
    let pc_id = pc.id().to_string();
    let mut ok = false;
    for _ in 0..20 {
        if pc.ping(&phone_id).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(ok);

    let answering = phone.clone();
    let mut phone_ev = phone.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = phone_ev.recv().await {
            if let Event::PhoneRequest { req_id, op, .. } = ev {
                let v = match op {
                    PhoneOp::Dial { number } if number == "bad" => {
                        serde_json::json!({ "error": "no signal" })
                    }
                    PhoneOp::Dial { number } => serde_json::json!({ "dialing": number }),
                    PhoneOp::State => serde_json::json!({ "state": "idle" }),
                    _ => serde_json::json!({ "ok": true }),
                };
                let _ = answering.phone_reply(req_id, v);
            }
        }
    });

    let r = pc
        .phone(&phone_id, PhoneOp::Dial { number: "+420123".into() })
        .await
        .unwrap();
    assert_eq!(r["dialing"], "+420123");
    assert_eq!(pc.phone(&phone_id, PhoneOp::State).await.unwrap()["state"], "idle");
    pc.phone(&phone_id, PhoneOp::HangUp).await.unwrap();
    let e = pc
        .phone(&phone_id, PhoneOp::Dial { number: "bad".into() })
        .await
        .unwrap_err();
    assert!(e.to_string().contains("no signal"));

    // The phone tells the computer somebody is calling.
    let mut pc_ev = pc.subscribe();
    phone
        .phone_push(&pc_id, "ringing", "+420999", Some("Anna".into()))
        .await
        .unwrap();
    let got = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::PhoneState { state, number, name, .. }) = pc_ev.recv().await {
                return (state, number, name);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(got.0, "ringing");
    assert_eq!(got.1, "+420999");
    assert_eq!(got.2.as_deref(), Some("Anna"));
}
