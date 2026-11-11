use std::time::Duration;
use zima_connect_net::quickshare::{Config, QsEvent, QuickShare};

async fn node(dir: &std::path::Path, name: &str) -> QuickShare {
    QuickShare::start(Config {
        receive_dir: dir.join(name).join("in"),
        alias: name.into(),
        device_type: 3,
        discovery: false,
    })
    .await
    .unwrap()
}

/// Two nodes speak Quick Share to each other: key exchange, PIN, accept, several files; and a refusal.
#[tokio::test(flavor = "multi_thread")]
async fn send_and_receive() {
    let tmp = std::env::temp_dir().join(format!("zc-qs-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let a = node(&tmp, "alice").await;
    let b = node(&tmp, "bob").await;
    let peer = a.peer_at("127.0.0.1".parse().unwrap(), b.port());

    let big = tmp.join("big.bin");
    let payload: Vec<u8> = (0..1_700_000u32).map(|i| (i % 251) as u8).collect(); // several chunks
    std::fs::write(&big, &payload).unwrap();
    let small = tmp.join("note.txt");
    std::fs::write(&small, "hi").unwrap();

    let bob = b.clone();
    let mut ev = b.subscribe();
    let acc = tokio::spawn(async move {
        let mut got = Vec::new();
        while let Ok(e) = ev.recv().await {
            match e {
                QsEvent::Incoming {
                    session,
                    from,
                    pin,
                    files,
                } => {
                    assert_eq!(from, "alice");
                    assert_eq!(pin.len(), 4);
                    assert_eq!(files.len(), 2);
                    bob.decide(&session, true).unwrap();
                }
                QsEvent::Received {
                    path, size, name, ..
                } => {
                    got.push((name, path, size));
                    if got.len() == 2 {
                        break;
                    }
                }
                _ => {}
            }
        }
        got
    });
    a.send(&peer, &[big.clone(), small.clone()]).await.unwrap();
    let got = tokio::time::timeout(Duration::from_secs(20), acc)
        .await
        .unwrap()
        .unwrap();
    for (name, path, size) in got {
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, size);
        match name.as_str() {
            "big.bin" => assert_eq!(bytes, payload),
            "note.txt" => assert_eq!(bytes, b"hi"),
            other => panic!("unexpected {other}"),
        }
    }

    // Bob declines: the sender gets an error and nothing is saved.
    let bob = b.clone();
    let mut ev = b.subscribe();
    let dec = tokio::spawn(async move {
        while let Ok(e) = ev.recv().await {
            if let QsEvent::Incoming { session, .. } = e {
                bob.decide(&session, false).unwrap();
                break;
            }
        }
    });
    let err = a
        .send(&peer, std::slice::from_ref(&small))
        .await
        .unwrap_err();
    dec.await.unwrap();
    assert!(format!("{err:#}").contains("declined"), "{err:#}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Needs a network with multicast, so it only runs on request (`-- --ignored`).
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn discovery_by_mdns() {
    let tmp = std::env::temp_dir().join(format!("zc-qs-mdns-{}", std::process::id()));
    let mk = |name: &str| {
        let cfg = Config {
            receive_dir: tmp.join(name),
            alias: name.into(),
            device_type: 3,
            discovery: true,
        };
        QuickShare::start(cfg)
    };
    let a = mk("alice-lap").await.unwrap();
    let b = mk("bob-lap").await.unwrap();
    b.set_visible(true);
    let mut found = None;
    for _ in 0..40 {
        if let Some(p) = a.peers().into_iter().find(|p| p.alias == "bob-lap") {
            found = Some(p);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let p = found.expect("bob was not discovered");
    assert_eq!(p.port, b.port());
    assert_eq!(p.device_type, "laptop");
}
