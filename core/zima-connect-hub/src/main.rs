//! Zima Connect household server.
//!
//! * Speaks the Zima Connect P2P protocol (devices join, sync people / policy).
//! * Serves a small read-only status page for the home network.
//!
//! All changes (people, devices, policy) are made from the owner's app; the
//! web page only shows the one-time setup code until an owner has joined.

use std::path::PathBuf;

use axum::{response::Html, routing::get, Json, Router};
use zima_connect_net::{Node, NodeConfig, Role};

const DASHBOARD: &str = include_str!("dashboard.html");

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = PathBuf::from(env("ZC_DATA_DIR", "./data"));
    let port: u16 = env("ZC_DASHBOARD_PORT", "9121").parse().unwrap_or(9121);
    let household = env("ZC_HOUSEHOLD", "Home");

    let node = Node::start(NodeConfig {
        download_dir: data_dir.join("files"),
        data_dir,
        device_name: env("ZC_SERVER_NAME", "Zima Connect Hub"),
        platform: "server".into(),
        internet: env("ZC_OFFLINE", "") != "1",
        hub: Some(household),
    })
    .await?;

    println!("Zima Connect hub started. Server id: {}", node.id());
    if let Some(d) = node.hub_dashboard() {
        if !d.users.iter().any(|u| u.role == Role::Owner) {
            if let Some(i) = d.invites.first() {
                println!(
                    "\nFirst-time setup code (also shown on the web page):\n{}\n",
                    i.code
                );
            }
        }
    }

    let n = node.clone();
    let app = Router::new()
        .route("/", get(|| async { Html(DASHBOARD) }))
        .route("/health", get(|| async { "ok" }))
        .route(
            "/api/state",
            get(move || {
                let n = n.clone();
                async move {
                    let mut d = n.hub_dashboard();
                    if let Some(d) = d.as_mut() {
                        // After the owner has joined, invite codes are only visible in the owner's app.
                        if d.users.iter().any(|u| u.role == Role::Owner) {
                            d.invites.clear();
                        }
                    }
                    Json(d)
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("Status page: http://0.0.0.0:{port}/");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    node.shutdown().await;
    Ok(())
}
