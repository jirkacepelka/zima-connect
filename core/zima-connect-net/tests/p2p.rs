use std::time::Duration;
use zima_connect_net::{Event, Node, NodeConfig};

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

#[tokio::test(flavor = "multi_thread")]
async fn pair_clipboard_ring_file() {
    let tmp = std::env::temp_dir().join(format!("zc-test-{}", std::process::id()));
    let a = Node::start(cfg(&tmp, "alice")).await.unwrap();
    let b = Node::start(cfg(&tmp, "bob")).await.unwrap();
    let mut b_events = b.subscribe();

    // Unpaired peers are refused.
    assert!(a.ping(&b.id().to_string()).await.is_err());

    let ticket = a.pairing_ticket().await.unwrap();
    let peer = b.pair(&ticket).await.unwrap();
    assert_eq!(peer.name, "alice");
    assert_eq!(a.peers().len(), 1);
    assert_eq!(b.peers().len(), 1);

    // A ticket is one-time.
    let c = Node::start(cfg(&tmp, "carol")).await.unwrap();
    assert!(c.pair(&ticket).await.is_err());

    let aid = a.id().to_string();
    let bid = b.id().to_string();
    // After pairing, alice dials bob by id only: this relies on mDNS discovery.
    let mut ok = false;
    for _ in 0..20 {
        if a.ping(&bid).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(ok, "alice could not reach bob");

    a.send_clipboard(&bid, "hello".into()).await.unwrap();
    a.ring(&bid).await.unwrap();
    let f = tmp.join("x.bin");
    std::fs::write(&f, vec![7u8; 3_000_000]).unwrap();
    a.send_file(&bid, &f).await.unwrap();

    let mut got = (false, false, false);
    while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_secs(3), b_events.recv()).await {
        match ev {
            Event::Clipboard { text, .. } => got.0 = text == "hello",
            Event::Ring { .. } => got.1 = true,
            Event::FileReceived { size, path, .. } => {
                got.2 = size == 3_000_000 && std::fs::metadata(path).unwrap().len() == 3_000_000
            }
            _ => {}
        }
        if got == (true, true, true) {
            break;
        }
    }
    assert_eq!(got, (true, true, true));
    assert!(b.ping(&aid).await.is_ok());
}
