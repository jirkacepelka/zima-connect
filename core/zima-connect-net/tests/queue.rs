use std::time::Duration;
use zima_connect_net::{AdminOp, Delivery, Event, Node, NodeConfig};

fn cfg(dir: &std::path::Path, name: &str, hub: bool) -> NodeConfig {
    NodeConfig {
        data_dir: dir.join(name).join("data"),
        download_dir: dir.join(name).join("dl"),
        device_name: name.into(),
        platform: "test".into(),
        internet: false,
        hub: hub.then(|| "Home".to_string()),
    }
}

/// A device that is offline gets its clipboard and files from the server when it is back.
/// The administrator creates the first person in the dashboard and an invitation for their device.
async fn first_code(hub: &Node) -> String {
    let id = hub
        .hub_admin(AdminOp::CreateUser {
            name: "Jiri".into(),
        })
        .await
        .unwrap()
        .unwrap();
    hub.hub_admin(AdminOp::CreateInvite {
        user_id: Some(id),
        new_user_name: None,
    })
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_queue() {
    let tmp = std::env::temp_dir().join(format!("zc-queue-{}", std::process::id()));
    let hub = Node::start(cfg(&tmp, "hub", true)).await.unwrap();
    let pc = Node::start(cfg(&tmp, "pc", false)).await.unwrap();
    let phone = Node::start(cfg(&tmp, "phone", false)).await.unwrap();

    pc.join(&first_code(&hub).await, Some("Jiri".into()))
        .await
        .unwrap();
    let me = pc.household_info().unwrap().me.id;
    let code = pc
        .admin(AdminOp::CreateInvite {
            user_id: Some(me),
            new_user_name: None,
        })
        .await
        .unwrap()
        .unwrap();
    phone.join(&code, None).await.unwrap();
    pc.sync().await.unwrap();
    let phone_id = phone.id().to_string();

    // The phone goes offline.
    phone.shutdown().await;
    drop(phone);

    let file = tmp.join("photo.jpg");
    std::fs::write(&file, vec![9u8; 700_000]).unwrap();
    assert_eq!(
        pc.send_clipboard(&phone_id, "while you were away".into())
            .await
            .unwrap(),
        Delivery::Queued
    );
    assert_eq!(
        pc.send_file(&phone_id, &file).await.unwrap(),
        Delivery::Queued
    );

    // The server only holds ciphertext: neither the clipboard text nor the file content is readable there.
    let mailbox = tmp.join("hub").join("data").join("mailbox");
    let mut stored = Vec::new();
    for e in std::fs::read_dir(&mailbox).unwrap().flatten() {
        stored.extend(std::fs::read(e.path()).unwrap());
    }
    assert!(stored.len() > 700_000, "file was not stored");
    assert!(!stored.windows(19).any(|w| w == b"while you were away"));
    assert!(
        !stored.windows(64).any(|w| w.iter().all(|b| *b == 9)),
        "file content stored in clear"
    );

    // The phone comes back and collects everything.
    let phone = Node::start(cfg(&tmp, "phone", false)).await.unwrap();
    let mut events = phone.subscribe();
    assert_eq!(phone.pull_inbox().await.unwrap(), 2);

    let mut got = (false, false);
    while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_secs(3), events.recv()).await {
        match ev {
            Event::Clipboard { text, .. } => got.0 = text == "while you were away",
            Event::FileReceived { size, path, .. } => {
                got.1 = size == 700_000 && std::fs::metadata(path).unwrap().len() == 700_000
            }
            _ => {}
        }
        if got == (true, true) {
            break;
        }
    }
    assert_eq!(got, (true, true));
    // Delivered items are removed from the server.
    assert_eq!(phone.pull_inbox().await.unwrap(), 0);
}
