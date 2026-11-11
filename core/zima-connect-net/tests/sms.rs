use std::time::Duration;
use zima_connect_net::{Event, Node, NodeConfig, SmsOp};

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

/// A computer asks a phone for its text messages; the phone's app answers; new messages are pushed.
#[tokio::test(flavor = "multi_thread")]
async fn text_messages() {
    let tmp = std::env::temp_dir().join(format!("zc-sms-{}", std::process::id()));
    let pc = Node::start(cfg(&tmp, "pc")).await.unwrap();
    let phone = Node::start(cfg(&tmp, "phone")).await.unwrap();
    let ticket = pc.pairing_ticket().await.unwrap();
    phone.pair(&ticket).await.unwrap();
    let phone_id = phone.id().to_string();
    let mut ok = false;
    for _ in 0..20 {
        if pc.ping(&phone_id).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(ok);

    // The phone's side: answer every question.
    let answering = phone.clone();
    let mut phone_ev = phone.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = phone_ev.recv().await {
            if let Event::SmsRequest { req_id, op, .. } = ev {
                let v = match op {
                    SmsOp::Threads => {
                        serde_json::json!([{ "id": "7", "address": "+420123", "snippet": "Hi" }])
                    }
                    SmsOp::Thread { id } => {
                        serde_json::json!([{ "id": id, "body": "Hi", "outgoing": false }])
                    }
                    SmsOp::Send { to, .. } if to == "bad" => {
                        serde_json::json!({ "error": "no signal" })
                    }
                    SmsOp::Send { .. } => serde_json::json!({ "ok": true }),
                };
                let _ = answering.sms_reply(req_id, v);
            }
        }
    });

    let threads = pc.sms(&phone_id, SmsOp::Threads).await.unwrap();
    assert_eq!(threads[0]["address"], "+420123");
    let msgs = pc
        .sms(&phone_id, SmsOp::Thread { id: "7".into() })
        .await
        .unwrap();
    assert_eq!(msgs[0]["body"], "Hi");
    pc.sms(
        &phone_id,
        SmsOp::Send {
            to: "+420123".into(),
            body: "yo".into(),
        },
    )
    .await
    .unwrap();
    let e = pc
        .sms(
            &phone_id,
            SmsOp::Send {
                to: "bad".into(),
                body: "x".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(format!("{e}").contains("no signal"));

    // Push: a new message arrived on the phone.
    let mut pc_ev = pc.subscribe();
    phone
        .sms_push(
            &pc.id().to_string(),
            "+420123",
            Some("Mom".into()),
            "Dinner at 7?",
        )
        .await
        .unwrap();
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(5), pc_ev.recv())
            .await
            .unwrap()
            .unwrap();
        if let Event::SmsReceived { body, name, .. } = ev {
            assert_eq!(body, "Dinner at 7?");
            assert_eq!(name.as_deref(), Some("Mom"));
            break;
        }
    }
}
