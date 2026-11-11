//! The files of this computer that other devices may browse, and other devices' files for this one.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use base64::Engine;
use futures_util::future::BoxFuture;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use zima_connect_net::webdav::{Entry, Fs};
use zima_connect_net::{Cap, FilesOp};

use crate::state::AppState;

/// What this computer offers: the usual personal folders, nothing else.
pub fn share_roots(app: &AppHandle) -> Vec<(String, PathBuf)> {
    let p = app.path();
    [("Desktop", p.desktop_dir()), ("Documents", p.document_dir()), ("Downloads", p.download_dir()), ("Pictures", p.picture_dir()), ("Music", p.audio_dir()), ("Videos", p.video_dir())]
        .into_iter()
        .filter_map(|(n, d)| d.ok().map(|d| (n.to_string(), d)))
        .filter(|(_, d)| d.is_dir())
        .collect()
}

fn entries(v: &Value) -> Vec<Entry> {
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

/// The other devices as folders for the local WebDAV server.
pub struct DavFs(pub Arc<AppState>);

impl DavFs {
    /// Devices that let this one browse, by the name shown in the file browser.
    fn devices(state: &AppState) -> Vec<(String, String)> {
        let mut seen: Vec<(String, String)> = Vec::new();
        for c in state.node.contacts() {
            if !c.caps.contains(&Cap::Storage) {
                continue;
            }
            let mut name: String = c.name.chars().map(|ch| if "\\/:*?\"<>|".contains(ch) { '_' } else { ch }).collect();
            if seen.iter().any(|(n, _)| *n == name) {
                name = format!("{name} ({})", c.id.chars().take(4).collect::<String>());
            }
            seen.push((name, c.id));
        }
        seen
    }

    fn id_of(state: &AppState, name: &str) -> Result<String> {
        Self::devices(state).into_iter().find(|(n, _)| n == name).map(|(_, id)| id).ok_or_else(|| anyhow!("device not found"))
    }
}

impl Fs for DavFs {
    fn devices(&self) -> BoxFuture<'static, Vec<String>> {
        let s = self.0.clone();
        Box::pin(async move { DavFs::devices(&s).into_iter().map(|(n, _)| n).collect() })
    }

    fn list(&self, device: String, path: String) -> BoxFuture<'static, Result<Vec<Entry>>> {
        let s = self.0.clone();
        Box::pin(async move {
            let id = DavFs::id_of(&s, &device)?;
            Ok(entries(&s.node.files(&id, FilesOp::List { path }).await?))
        })
    }

    fn stat(&self, device: String, path: String) -> BoxFuture<'static, Result<Entry>> {
        let s = self.0.clone();
        Box::pin(async move {
            let id = DavFs::id_of(&s, &device)?;
            let v = s.node.files(&id, FilesOp::Stat { path: path.clone() }).await?;
            Ok(Entry {
                name: path.rsplit('/').next().unwrap_or("").into(),
                dir: v["dir"].as_bool().unwrap_or(false),
                size: v["size"].as_u64().unwrap_or(0),
                mtime: v["mtime"].as_u64().unwrap_or(0),
            })
        })
    }

    fn read(&self, device: String, path: String, offset: u64, len: u64) -> BoxFuture<'static, Result<Vec<u8>>> {
        let s = self.0.clone();
        Box::pin(async move {
            let id = DavFs::id_of(&s, &device)?;
            let v = s.node.files(&id, FilesOp::Read { path, offset, len }).await?;
            let data = v["data"].as_str().ok_or_else(|| anyhow!("no data"))?;
            Ok(base64::engine::general_purpose::STANDARD.decode(data)?)
        })
    }
}

/// Copy a file of another device into `dest_dir`; returns where it landed.
pub async fn download(state: &AppState, peer: &str, path: &str, dest_dir: PathBuf) -> Result<PathBuf> {
    use tokio::io::AsyncWriteExt;
    let name = path.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("file");
    let name: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    tokio::fs::create_dir_all(&dest_dir).await?;
    let mut target = dest_dir.join(&name);
    let mut n = 1;
    while target.exists() {
        let p = std::path::Path::new(&name);
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| name.clone());
        let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        target = dest_dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    let part = target.with_extension("part");
    let mut out = tokio::fs::File::create(&part).await?;
    let mut offset = 0u64;
    loop {
        let v = state.node.files(peer, FilesOp::Read { path: path.into(), offset, len: 768 * 1024 }).await?;
        let data = base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().ok_or_else(|| anyhow!("no data"))?)?;
        out.write_all(&data).await?;
        offset += data.len() as u64;
        if v["eof"].as_bool().unwrap_or(true) || data.is_empty() {
            break;
        }
    }
    out.flush().await?;
    drop(out);
    if offset == 0 && !part.exists() {
        bail!("nothing arrived");
    }
    tokio::fs::rename(&part, &target).await?;
    Ok(target)
}
