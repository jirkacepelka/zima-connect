use std::time::Duration;
use zima_connect_net::{AdminOp, Cap, Event, Node, NodeConfig};

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

#[tokio::test(flavor = "multi_thread")]
async fn household_policy() {
    let tmp = std::env::temp_dir().join(format!("zc-house-{}", std::process::id()));
    let hub = Node::start(cfg(&tmp, "hub", true)).await.unwrap();
    let me = Node::start(cfg(&tmp, "jiri-pc", false)).await.unwrap();
    let mom = Node::start(cfg(&tmp, "mama-phone", false)).await.unwrap();

    // The hub prints a setup code until an owner has joined.
    let dash = hub.hub_dashboard().unwrap();
    assert_eq!(dash.invites.len(), 1);
    let info = me
        .join(&dash.invites[0].code, Some("Jiri".into()))
        .await
        .unwrap();
    assert!(info.can_admin);
    // The setup code is single-use.
    assert!(mom.join(&dash.invites[0].code, None).await.is_err());

    // Owner invites a new person.
    let code = me
        .admin(AdminOp::CreateInvite {
            user_id: None,
            new_user_name: Some("Mom".into()),
        })
        .await
        .unwrap()
        .unwrap();
    let mom_info = mom.join(&code, None).await.unwrap();
    assert_eq!(mom_info.me.name, "Mom");
    assert!(!mom_info.can_admin);
    // A member cannot administer.
    assert!(mom
        .admin(AdminOp::CreateInvite {
            user_id: None,
            new_user_name: None
        })
        .await
        .is_err());

    // Owner learns about Mom's phone.
    me.sync().await.unwrap();
    let mom_contact = me
        .contacts()
        .into_iter()
        .find(|c| c.name == "mama-phone")
        .unwrap();
    // Default policy between people: only files.
    assert_eq!(mom_contact.caps, vec![Cap::Files]);
    assert!(me.ring(&mom_contact.id).await.is_err());
    assert!(me
        .send_clipboard(&mom_contact.id, "x".into())
        .await
        .is_err());

    // Photos to Mom work.
    let mut mom_events = mom.subscribe();
    let photo = tmp.join("holiday.jpg");
    std::fs::write(&photo, vec![1u8; 500_000]).unwrap();
    let mut ok = false;
    for _ in 0..20 {
        if me.send_file(&mom_contact.id, &photo).await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(ok, "photo not delivered");
    let ev = tokio::time::timeout(Duration::from_secs(5), mom_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, Event::FileReceived { size: 500_000, .. }));

    // Mom's phone cannot ring or notify the owner (default policy, receiver enforces it).
    let me_contact = mom
        .contacts()
        .into_iter()
        .find(|c| c.name == "jiri-pc")
        .unwrap();
    assert!(mom.ring(&me_contact.id).await.is_err());

    // Even if a sender ignored its own policy, the receiving side refuses.
    // Owner explicitly forbids Mom -> owner photos; Mom's stale copy must be refreshed on the receiver.
    let users = me.household_info().unwrap().users;
    let owner_id = users.iter().find(|u| u.name == "Jiri").unwrap().id.clone();
    let mom_id = users.iter().find(|u| u.name == "Mom").unwrap().id.clone();
    me.admin(AdminOp::SetRule {
        from_user: mom_id.clone(),
        to_user: owner_id.clone(),
        cap: Cap::Files,
        allow: false,
    })
    .await
    .unwrap();
    // Mom has NOT synced yet: her copy still allows it, but the receiver enforces the new rule.
    let err = mom
        .send_file(&me_contact.id, &photo)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("policy does not allow"), "{err}");

    // Allow ring for the owner -> Mom only.
    me.admin(AdminOp::SetRule {
        from_user: owner_id.clone(),
        to_user: mom_id.clone(),
        cap: Cap::Ring,
        allow: true,
    })
    .await
    .unwrap();
    me.ring(&mom_contact.id).await.unwrap();

    // Removing a device cuts it off.
    me.admin(AdminOp::RemoveDevice {
        id: mom_contact.id.clone(),
    })
    .await
    .unwrap();
    assert!(mom.sync().await.is_err());
    assert!(!mom.is_joined());
}
