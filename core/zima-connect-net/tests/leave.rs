use zima_connect_net::{AdminOp, Node, NodeConfig};

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

/// Leaving tells the server; the administrator can remove any device and revoke invitations.
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
async fn leave_and_admin() {
    let tmp = std::env::temp_dir().join(format!("zc-leave-{}", std::process::id()));
    let hub = Node::start(cfg(&tmp, "hub", true)).await.unwrap();
    let pc = Node::start(cfg(&tmp, "pc", false)).await.unwrap();
    let phone = Node::start(cfg(&tmp, "phone", false)).await.unwrap();

    pc.join(&first_code(&hub).await, Some("Jiri".into()))
        .await
        .unwrap();
    let owner = pc.household_info().unwrap().me.id;
    let code = pc
        .admin(AdminOp::CreateInvite {
            user_id: Some(owner.clone()),
            new_user_name: None,
        })
        .await
        .unwrap()
        .unwrap();
    phone.join(&code, None).await.unwrap();
    assert_eq!(hub.hub_dashboard().unwrap().devices.len(), 2);

    // The phone leaves: it disappears from the server too.
    phone.leave_household_async().await;
    assert!(!phone.is_joined());
    assert_eq!(hub.hub_dashboard().unwrap().devices.len(), 1);

    // An invitation can be revoked before it is used.
    let code2 = hub
        .hub_admin(AdminOp::CreateInvite {
            user_id: Some(owner.clone()),
            new_user_name: None,
        })
        .await
        .unwrap()
        .unwrap();
    let secret = hub
        .hub_dashboard()
        .unwrap()
        .invites
        .iter()
        .find(|i| i.code == code2)
        .unwrap()
        .secret
        .clone();
    hub.hub_admin(AdminOp::RevokeInvite { secret })
        .await
        .unwrap();
    assert!(phone.join(&code2, None).await.is_err());

    // The owner cannot remove their own last device through the app, but the server administrator can.
    let pc_id = pc.id().to_string();
    assert!(pc
        .admin(AdminOp::RemoveDevice { id: pc_id.clone() })
        .await
        .is_err());
    hub.hub_admin(AdminOp::RemoveDevice { id: pc_id })
        .await
        .unwrap();
    assert_eq!(hub.hub_dashboard().unwrap().devices.len(), 0);
    // ...and the next sync tells the device it was removed.
    assert!(pc.sync().await.is_err());
    assert!(!pc.is_joined());
}
