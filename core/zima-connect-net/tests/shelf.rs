use std::time::Duration;
use zima_connect_net::{AdminOp, Event, Node, NodeConfig, RemoteInput};

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

async fn invite(hub: &Node, user: Option<String>, name: Option<&str>) -> String {
    hub.hub_admin(AdminOp::CreateInvite {
        user_id: user,
        new_user_name: name.map(|n| n.to_string()),
    })
    .await
    .unwrap()
    .unwrap()
}

/// Shelf visibility follows the household policy; remote control, links and battery are live
/// features gated by their own permissions.
#[tokio::test(flavor = "multi_thread")]
async fn shelf_and_remote() {
    let tmp = std::env::temp_dir().join(format!("zc-shelf-{}", std::process::id()));
    let hub = Node::start(cfg(&tmp, "hub", true)).await.unwrap();
    let me = Node::start(cfg(&tmp, "jiri-pc", false)).await.unwrap();
    let phone = Node::start(cfg(&tmp, "jiri-phone", false)).await.unwrap();
    let mom = Node::start(cfg(&tmp, "mama", false)).await.unwrap();
    let dad = Node::start(cfg(&tmp, "tata", false)).await.unwrap();

    let jiri = hub
        .hub_admin(AdminOp::CreateUser {
            name: "Jiri".into(),
        })
        .await
        .unwrap()
        .unwrap();
    me.join(&invite(&hub, Some(jiri.clone()), None).await, None)
        .await
        .unwrap();
    phone
        .join(&invite(&hub, Some(jiri.clone()), None).await, None)
        .await
        .unwrap();
    mom.join(&invite(&hub, None, Some("Mom")).await, None)
        .await
        .unwrap();
    dad.join(&invite(&hub, None, Some("Dad")).await, None)
        .await
        .unwrap();
    for n in [&me, &phone, &mom, &dad] {
        n.sync().await.unwrap();
    }

    // The shelf is personal: Jiri's phone sees it, Mom and Dad do not.
    let f = tmp.join("holiday.jpg");
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(&f, vec![7u8; 300_000]).unwrap();
    let item = me.shelf_put(&f).await.unwrap();
    assert_eq!(item.name, "holiday.jpg");
    assert!(me
        .shelf_list()
        .await
        .unwrap()
        .iter()
        .any(|i| i.id == item.id && i.mine));
    assert!(phone
        .shelf_list()
        .await
        .unwrap()
        .iter()
        .any(|i| i.id == item.id)); // same person
    assert!(mom.shelf_list().await.unwrap().is_empty());
    assert!(dad.shelf_list().await.unwrap().is_empty());
    assert!(mom.shelf_get(&item.id).await.is_err());
    let saved = phone.shelf_get(&item.id).await.unwrap();
    assert_eq!(std::fs::read(saved).unwrap().len(), 300_000);

    // Only the owner removes it.
    assert!(mom.shelf_delete(&item.id).await.is_err());
    phone.shelf_delete(&item.id).await.unwrap();
    assert!(hub.hub_shelf().unwrap().is_empty());

    // Remote control and status: same person yes, other person no (default policy).
    let mut ev = phone.subscribe();
    me.send_remote(
        &phone.id().to_string(),
        RemoteInput::Media {
            action: "play_pause".into(),
        },
    )
    .await
    .unwrap();
    me.send_url(&phone.id().to_string(), "https://example.org/")
        .await
        .unwrap();
    assert!(me
        .send_url(&phone.id().to_string(), "javascript:alert(1)")
        .await
        .is_err());
    let (mut got_remote, mut got_url) = (false, false);
    for _ in 0..2 {
        match tokio::time::timeout(Duration::from_secs(3), ev.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Event::Remote { .. } => got_remote = true,
            Event::OpenUrl { url, .. } => got_url = url == "https://example.org/",
            _ => {}
        }
    }
    assert!(got_remote && got_url);
    assert!(mom
        .send_remote(
            &phone.id().to_string(),
            RemoteInput::Key {
                key: "enter".into()
            }
        )
        .await
        .is_err());
    assert!(mom
        .send_battery(&phone.id().to_string(), 50, false)
        .await
        .is_err());
    me.send_battery(&phone.id().to_string(), 80, true)
        .await
        .unwrap();
}
