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

async fn next_call_event(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Event {
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("no call event")
            .unwrap();
        if matches!(ev, Event::CallIncoming { .. } | Event::CallState { .. }) {
            return ev;
        }
    }
}

async fn incoming(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> u64 {
    loop {
        if let Event::CallIncoming { call_id, .. } = next_call_event(rx).await {
            return call_id;
        }
    }
}

/// A call rings, is answered, carries audio both ways and ends when one side hangs up.
#[tokio::test(flavor = "multi_thread")]
async fn voice_call() {
    let tmp = std::env::temp_dir().join(format!("zc-call-{}", std::process::id()));
    let a = Node::start(cfg(&tmp, "alice")).await.unwrap();
    let b = Node::start(cfg(&tmp, "bob")).await.unwrap();
    let ticket = a.pairing_ticket().await.unwrap();
    b.pair(&ticket).await.unwrap();
    let bid = b.id().to_string();
    let mut ok = false;
    for _ in 0..20 {
        if a.ping(&bid).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(ok, "alice could not reach bob");

    let mut a_ev = a.subscribe();
    let mut b_ev = b.subscribe();

    // Declined.
    let id = a.call_start(&bid).await.unwrap();
    let bid1 = incoming(&mut b_ev).await;
    b.call_decline(bid1).unwrap();
    let mut declined = false;
    for _ in 0..4 {
        if let Event::CallState { state, call_id, .. } = next_call_event(&mut a_ev).await {
            if call_id == id && state == "declined" {
                declined = true;
                break;
            }
        }
    }
    assert!(declined);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(a.active_call().is_none());

    // Answered: audio flows both ways.
    let id = a.call_start(&bid).await.unwrap();
    let bcall = incoming(&mut b_ev).await;
    b.call_accept(bcall).unwrap();
    let mut connected = false;
    for _ in 0..6 {
        if let Event::CallState { state, call_id, .. } = next_call_event(&mut a_ev).await {
            if call_id == id && state == "connected" {
                connected = true;
                break;
            }
        }
    }
    assert!(connected);

    let frame: Vec<u8> = (0..640u32).map(|i| (i % 251) as u8).collect();
    a.call_send(id, frame.clone()).unwrap();
    let got = tokio::time::timeout(Duration::from_secs(5), b.call_recv(bcall))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, frame);
    b.call_send(bcall, vec![7u8; 640]).unwrap();
    let back = tokio::time::timeout(Duration::from_secs(5), a.call_recv(id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(back, vec![7u8; 640]);

    // Hanging up ends both sides.
    a.call_hangup(id);
    let end = tokio::time::timeout(Duration::from_secs(5), b.call_recv(bcall))
        .await
        .unwrap();
    assert!(end.is_none());
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(a.active_call().is_none() && b.active_call().is_none());
}
