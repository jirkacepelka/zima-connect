use std::time::Duration;
use zima_connect_net::{AdminOp, Node, NodeConfig, RelaySetting};

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

/// The server announces its own relay; members pick it up from the signed household data,
/// and a renamed device shows up under its new name.
#[tokio::test(flavor = "multi_thread")]
async fn own_relay_and_rename() {
    let tmp = std::env::temp_dir().join(format!("zc-relay-{}", std::process::id()));
    let mut c = iroh_relay::server::ServerConfig::default();
    c.relay = Some(iroh_relay::server::RelayConfig::new(([127, 0, 0, 1], 0)));
    let srv = iroh_relay::server::Server::spawn(c).await.unwrap();
    let url = format!("http://{}", srv.http_addr().unwrap());

    let hub = Node::start(cfg(&tmp, "hub", true)).await.unwrap();
    let pc = Node::start(cfg(&tmp, "pc", false)).await.unwrap();
    let id = hub
        .hub_admin(AdminOp::CreateUser {
            name: "Jiri".into(),
        })
        .await
        .unwrap()
        .unwrap();
    let code = hub
        .hub_admin(AdminOp::CreateInvite {
            user_id: Some(id),
            new_user_name: None,
        })
        .await
        .unwrap()
        .unwrap();
    pc.join(&code, None).await.unwrap();
    assert!(pc.household_snapshot().unwrap().relay.is_none());

    hub.hub_admin(AdminOp::SetRelay {
        relay: Some(RelaySetting {
            url: url.clone(),
            exclusive: true,
        }),
    })
    .await
    .unwrap();
    pc.sync().await.unwrap();
    assert_eq!(pc.household_snapshot().unwrap().relay.unwrap().url, url);

    // Both ends end up with the server's relay as their home relay.
    let mut ok = false;
    for _ in 0..40 {
        if pc.relay_urls().iter().any(|u| u.starts_with(&url))
            && hub.relay_urls().iter().any(|u| u.starts_with(&url))
        {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(ok, "pc: {:?} hub: {:?}", pc.relay_urls(), hub.relay_urls());

    // Rename: kept locally and visible to the whole household.
    pc.rename_device("Kitchen laptop").await.unwrap();
    assert_eq!(pc.device_name(), "Kitchen laptop");
    let d = hub.hub_dashboard().unwrap();
    assert!(d.devices.iter().any(|x| x.name == "Kitchen laptop"));
    assert!(pc.rename_device("  ").await.is_err());
    srv.shutdown().await.unwrap();
}
