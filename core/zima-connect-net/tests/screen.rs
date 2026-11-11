use std::time::Duration;
use zima_connect_net::{Event, Node, NodeConfig, ScreenOp};

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

/// A computer asks a phone for its screen and sends touches; the phone's app answers.
#[tokio::test(flavor = "multi_thread")]
async fn screen_view_and_touch() {
    let tmp = std::env::temp_dir().join(format!("zc-screen-{}", std::process::id()));
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

    let answering = phone.clone();
    let mut ev = phone.subscribe();
    tokio::spawn(async move {
        while let Ok(e) = ev.recv().await {
            if let Event::ScreenRequest { req_id, op, .. } = e {
                let v = match op {
                    ScreenOp::Frame { .. } => {
                        serde_json::json!({ "jpg": "AAAA", "width": 720, "height": 1600 })
                    }
                    ScreenOp::Tap { x, y } if x > 1.0 || y > 1.0 => {
                        serde_json::json!({ "error": "outside the screen" })
                    }
                    ScreenOp::Tap { x, y } => serde_json::json!({ "tapped": [x, y] }),
                    _ => serde_json::json!({ "ok": true }),
                };
                let _ = answering.screen_reply(req_id, v);
            }
        }
    });

    let f = pc
        .screen(
            &phone_id,
            ScreenOp::Frame {
                max_width: Some(480),
            },
        )
        .await
        .unwrap();
    assert_eq!(f["height"], 1600);
    let t = pc
        .screen(&phone_id, ScreenOp::Tap { x: 0.5, y: 0.25 })
        .await
        .unwrap();
    assert_eq!(t["tapped"][1], 0.25);
    pc.screen(
        &phone_id,
        ScreenOp::Key {
            name: "back".into(),
        },
    )
    .await
    .unwrap();
    let e = pc
        .screen(&phone_id, ScreenOp::Tap { x: 2.0, y: 0.0 })
        .await
        .unwrap_err();
    assert!(e.to_string().contains("outside"));
    let _ = std::fs::remove_dir_all(&tmp);
}
