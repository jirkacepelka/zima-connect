//! Self-update of the server from GitHub Releases.
//!
//! The container entrypoint (`server/entrypoint.sh`) starts the newest of "the binary in the image"
//! and "the binary in /data/bin". An update downloads the release binary into /data/bin, checks its
//! checksum and exits with code 42, which makes the entrypoint start the new version.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version of this build (set by CI from the release tag).
pub const VERSION: &str = match option_env!("ZC_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

pub const EXIT_FOR_UPDATE: i32 = 42;
const LATEST: &str = "https://api.github.com/repos/jirkacepelka/zima-connect/releases/latest";

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Serialize, Clone)]
pub struct Info {
    pub latest: String,
    /// What is new (release notes).
    pub notes: String,
    pub size: u64,
    pub url: String,
    pub sha256: Option<String>,
    /// The server can replace itself (running in the official container with a matching binary).
    pub can_install: bool,
    pub page: String,
}

fn parse(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

fn asset_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "zima-connect-hub-linux-x86_64",
        _ => "zima-connect-hub-linux-unsupported",
    }
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("zima-connect-hub-updater")
        .timeout(Duration::from_secs(30))
        .build()?)
}

/// Latest release if it is newer than this build.
pub async fn check() -> anyhow::Result<Option<Info>> {
    let rel: Release = client()?
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if !newer(&rel.tag_name, VERSION) {
        return Ok(None);
    }
    let asset = rel.assets.iter().find(|a| a.name == asset_name());
    let in_container = std::env::var("ZC_SELF_UPDATE").is_ok_and(|v| v == "1");
    Ok(Some(Info {
        latest: rel.tag_name.trim_start_matches('v').to_string(),
        notes: rel.body.unwrap_or_default(),
        size: asset.map(|a| a.size).unwrap_or(0),
        url: asset
            .map(|a| a.browser_download_url.clone())
            .unwrap_or_default(),
        sha256: asset
            .and_then(|a| a.digest.as_ref())
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(|s| s.to_string()),
        can_install: in_container && asset.is_some(),
        page: rel.html_url,
    }))
}

/// Download the new binary next to the data, verify it and put it in place.
pub async fn install(data_dir: &Path, info: &Info) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    if !info.can_install {
        anyhow::bail!("this server cannot update itself");
    }
    if !info
        .url
        .starts_with("https://github.com/jirkacepelka/zima-connect/")
    {
        anyhow::bail!("unexpected download address");
    }
    let Some(want) = info.sha256.as_deref() else {
        anyhow::bail!("the release has no checksum, refusing to install");
    };
    let dir: PathBuf = data_dir.join("bin");
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("zima-connect-hub.download");
    let resp = client()?
        .get(&info.url)
        .timeout(Duration::from_secs(900))
        .send()
        .await?
        .error_for_status()?;
    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk)?;
    }
    file.flush()?;
    drop(file);
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !got.eq_ignore_ascii_case(want) {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!("the download does not match its checksum, update cancelled");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, dir.join("zima-connect-hub"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order() {
        assert!(newer("v0.3.1", "0.3.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("v0.3.0", "0.3.0"));
        assert!(!newer("0.2.9", "0.3.0"));
    }
}
