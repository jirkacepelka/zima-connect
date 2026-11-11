//! Talks to a *different* Quick Share implementation (run by hand, see the env variables).
//!   QS_SEND_TO=ip:port QS_FILE=path   → send the file to that device
//!   QS_RECEIVE_DIR=dir                → wait for one transfer and save it there (prints PORT=n)
use std::time::Duration;
use zima_connect_net::quickshare::{Config, QsEvent, QuickShare};

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn interop_send() {
    let (Ok(to), Ok(file)) = (std::env::var("QS_SEND_TO"), std::env::var("QS_FILE")) else {
        return;
    };
    let dir = std::env::temp_dir().join("zc-qs-interop");
    let qs = QuickShare::start(Config {
        receive_dir: dir,
        alias: "zima-test".into(),
        device_type: 3,
        discovery: false,
    })
    .await
    .unwrap();
    let (ip, port) = to.rsplit_once(':').unwrap();
    let peer = qs.peer_at(ip.parse().unwrap(), port.parse().unwrap());
    qs.send(&peer, &[file.into()]).await.unwrap();
    println!("SENT");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn interop_receive() {
    let Ok(dir) = std::env::var("QS_RECEIVE_DIR") else {
        return;
    };
    let qs = QuickShare::start(Config {
        receive_dir: dir.into(),
        alias: "zima-test".into(),
        device_type: 3,
        discovery: false,
    })
    .await
    .unwrap();
    let mut ev = qs.subscribe();
    println!("PORT={}", qs.port());
    let me = qs.clone();
    let r = tokio::time::timeout(Duration::from_secs(90), async move {
        while let Ok(e) = ev.recv().await {
            match e {
                QsEvent::Incoming {
                    session,
                    from,
                    pin,
                    files,
                } => {
                    println!("INCOMING from {from} pin {pin} files {files:?}");
                    me.decide(&session, true).unwrap();
                }
                QsEvent::Received { name, size, .. } => {
                    println!("RECEIVED {name} {size}");
                    return;
                }
                QsEvent::Text { text, .. } => {
                    println!("TEXT {text}");
                    return;
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(r.is_ok(), "nothing arrived");
}
