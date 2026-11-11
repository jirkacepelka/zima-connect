use std::time::Duration;
use zima_connect_net::localsend::{Config, LocalSend, LsEvent};

async fn node(dir: &std::path::Path, name: &str) -> LocalSend {
    LocalSend::start(Config {
        data_dir: dir.join(name).join("data"),
        receive_dir: dir.join(name).join("in"),
        alias: name.into(),
        model: "test".into(),
        device_type: "desktop".into(),
        port: 0,
        discovery: false,
    })
    .await
    .unwrap()
}

/// Two nodes speak the LocalSend protocol: ask, accept, upload; and a refusal.
#[tokio::test(flavor = "multi_thread")]
async fn send_and_receive() {
    let tmp = std::env::temp_dir().join(format!("zc-ls-{}", std::process::id()));
    let a = node(&tmp, "alice").await;
    let b = node(&tmp, "bob").await;
    let peer = a.peer_at("127.0.0.1".parse().unwrap(), b.port(), true);

    let src = tmp.join("hello.txt");
    std::fs::create_dir_all(&tmp).unwrap();
    let payload = "hello localsend\n".repeat(50_000); // ~800 KB
    std::fs::write(&src, &payload).unwrap();

    // Bob accepts.
    let bob = b.clone();
    let mut ev = b.subscribe();
    let acc = tokio::spawn(async move {
        let mut got = None;
        while let Ok(e) = ev.recv().await {
            match e {
                LsEvent::Incoming {
                    session,
                    from,
                    files,
                } => {
                    assert_eq!(from, "alice");
                    assert_eq!(files[0].name, "hello.txt");
                    bob.decide(&session, true).unwrap();
                }
                LsEvent::Received { path, size, .. } => {
                    got = Some((path, size));
                    break;
                }
                _ => {}
            }
        }
        got.unwrap()
    });
    a.send(&peer, std::slice::from_ref(&src)).await.unwrap();
    let (path, size) = tokio::time::timeout(Duration::from_secs(10), acc)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(size as usize, payload.len());
    assert_eq!(std::fs::read_to_string(path).unwrap(), payload);

    // Bob declines.
    let bob = b.clone();
    let mut ev = b.subscribe();
    tokio::spawn(async move {
        while let Ok(e) = ev.recv().await {
            if let LsEvent::Incoming { session, .. } = e {
                bob.decide(&session, false).unwrap();
                break;
            }
        }
    });
    let err = a.send(&peer, &[src]).await.unwrap_err();
    assert!(err.to_string().contains("declined"), "{err}");
}

/// Needs working multicast on this machine, so it is not part of the normal run.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn discovery_by_multicast() {
    let tmp = std::env::temp_dir().join(format!("zc-ls-d-{}", std::process::id()));
    let mk = |name: &str| Config {
        data_dir: tmp.join(name).join("data"),
        receive_dir: tmp.join(name).join("in"),
        alias: name.into(),
        model: "test".into(),
        device_type: "desktop".into(),
        port: 0,
        discovery: true,
    };
    let a = LocalSend::start(mk("alice")).await.unwrap();
    let b = LocalSend::start(mk("bob")).await.unwrap();
    for _ in 0..20 {
        if a.peers().iter().any(|p| p.alias == "bob")
            && b.peers().iter().any(|p| p.alias == "alice")
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("not found: {:?} / {:?}", a.peers(), b.peers());
}
