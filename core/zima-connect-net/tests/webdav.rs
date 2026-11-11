//! Serves a folder as the device "Test device" so a real file browser can be pointed at it by hand:
//!   cargo test -p zima-connect-net --test webdav -- --ignored --nocapture
//! then open  \127.0.0.1@21999\DavWWWRoot\  (Windows)  or  dav://127.0.0.1:21999/  (Linux).
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use zima_connect_net::localfs;
use zima_connect_net::webdav::{serve, Entry, Fs};
use zima_connect_net::FilesOp;

struct Folder(PathBuf);

fn entries(v: &serde_json::Value) -> Vec<Entry> {
    v["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| Entry {
                    name: e["name"].as_str().unwrap_or("").into(),
                    dir: e["dir"].as_bool().unwrap_or(false),
                    size: e["size"].as_u64().unwrap_or(0),
                    mtime: e["mtime"].as_u64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Fs for Folder {
    fn devices(&self) -> BoxFuture<'static, Vec<String>> {
        Box::pin(async { vec!["Test device".to_string()] })
    }
    fn list(&self, _d: String, path: String) -> BoxFuture<'static, anyhow::Result<Vec<Entry>>> {
        let root = ("Files".to_string(), self.0.clone());
        Box::pin(async move {
            let v = localfs::answer(&[root], &FilesOp::List { path });
            if let Some(e) = v.get("error") {
                anyhow::bail!("{e}");
            }
            Ok(entries(&v))
        })
    }
    fn stat(&self, _d: String, path: String) -> BoxFuture<'static, anyhow::Result<Entry>> {
        let root = self.0.clone();
        Box::pin(async move {
            let v = localfs::answer(
                &[("Files".to_string(), root)],
                &FilesOp::Stat { path: path.clone() },
            );
            if let Some(e) = v.get("error") {
                anyhow::bail!("{e}");
            }
            Ok(Entry {
                name: path.rsplit('/').next().unwrap_or("").into(),
                dir: v["dir"].as_bool().unwrap_or(false),
                size: v["size"].as_u64().unwrap_or(0),
                mtime: v["mtime"].as_u64().unwrap_or(0),
            })
        })
    }
    fn read(
        &self,
        _d: String,
        path: String,
        offset: u64,
        len: u64,
    ) -> BoxFuture<'static, anyhow::Result<Vec<u8>>> {
        let root = self.0.clone();
        Box::pin(async move {
            use base64::Engine;
            let v = localfs::answer(
                &[("Files".to_string(), root)],
                &FilesOp::Read { path, offset, len },
            );
            if let Some(e) = v.get("error") {
                anyhow::bail!("{e}");
            }
            Ok(base64::engine::general_purpose::STANDARD
                .decode(v["data"].as_str().unwrap_or(""))?)
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn serve_a_folder_by_hand() {
    let dir = std::env::temp_dir().join("zc-webdav-demo");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Photos")).unwrap();
    std::fs::write(dir.join("hello.txt"), "hello from another device\n").unwrap();
    std::fs::write(dir.join("Photos").join("big.bin"), vec![7u8; 3_000_000]).unwrap();
    let port = serve(Arc::new(Folder(dir)), 21999).await.unwrap();
    println!("SERVING on {port}");
    tokio::time::sleep(Duration::from_secs(150)).await;
}
