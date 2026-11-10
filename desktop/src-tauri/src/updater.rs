//! In-app updates from GitHub Releases: check, show what is new, download, verify, install, restart.

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

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

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    /// What is new (release notes, markdown).
    pub notes: String,
    pub size: u64,
    pub name: String,
    pub url: String,
    pub sha256: Option<String>,
    /// `false`: this install type cannot update itself (e.g. .deb), open `page` instead.
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

fn newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

/// The installer that fits how this copy was installed.
fn pick<'a>(assets: &'a [Asset]) -> Option<&'a Asset> {
    #[cfg(target_os = "windows")]
    {
        assets.iter().find(|a| a.name.ends_with("_x64-setup.exe"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        if std::env::var_os("APPIMAGE").is_some() {
            assets.iter().find(|a| a.name.ends_with(".AppImage"))
        } else {
            None
        }
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("zima-connect-updater")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

pub async fn check(current: &str) -> Result<Option<UpdateInfo>, String> {
    let rel: Release = client()?
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !newer(&rel.tag_name, current) {
        return Ok(None);
    }
    let asset = pick(&rel.assets);
    Ok(Some(UpdateInfo {
        current: current.to_string(),
        latest: rel.tag_name.trim_start_matches('v').to_string(),
        notes: rel.body.unwrap_or_default(),
        size: asset.map(|a| a.size).unwrap_or(0),
        name: asset.map(|a| a.name.clone()).unwrap_or_default(),
        url: asset.map(|a| a.browser_download_url.clone()).unwrap_or_default(),
        sha256: asset.and_then(|a| a.digest.as_ref()).and_then(|d| d.strip_prefix("sha256:")).map(|s| s.to_string()),
        can_install: asset.is_some(),
        page: rel.html_url,
    }))
}

/// Download `info.url` into a temp file, reporting progress, and verify its checksum.
async fn download(app: &AppHandle, info: &UpdateInfo) -> Result<PathBuf, String> {
    use futures_util::StreamExt;
    if !info.url.starts_with("https://github.com/jirkacepelka/zima-connect/") {
        return Err("unexpected download address".into());
    }
    let dir = std::env::temp_dir().join("zima-connect-update");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(&info.name);
    let resp = client()?
        .get(&info.url)
        .timeout(std::time::Duration::from_secs(1800))
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let total = resp.content_length().unwrap_or(info.size);
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download failed: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if done - last > 256 * 1024 || done == total {
            last = done;
            let _ = app.emit("update-progress", (done, total));
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    match &info.sha256 {
        Some(want) if !want.eq_ignore_ascii_case(&got) => {
            let _ = std::fs::remove_file(&path);
            Err("the downloaded file does not match its checksum, update cancelled".into())
        }
        _ => Ok(path),
    }
}

/// Download, install and restart into the new version.
pub async fn install(app: AppHandle, info: UpdateInfo) -> Result<(), String> {
    let path = download(&app, &info).await?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "Start-Sleep -Seconds 2; Start-Process -FilePath '{}' -ArgumentList '/S' -Wait; Start-Process -FilePath '{}'",
            path.display().to_string().replace('\'', "''"),
            exe.display().to_string().replace('\'', "''"),
        );
        // CREATE_NO_WINDOW | DETACHED_PROCESS: outlives this app, shows no console.
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(0x0800_0000 | 0x0000_0008)
            .spawn()
            .map_err(|e| format!("could not start the installer: {e}"))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let target = std::env::var_os("APPIMAGE").map(PathBuf::from).ok_or("this install cannot update itself")?;
        let staged = target.with_extension("new");
        std::fs::copy(&path, &staged).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        std::fs::rename(&staged, &target).map_err(|e| e.to_string())?;
        std::process::Command::new(&target).spawn().map_err(|e| e.to_string())?;
        let _ = exe;
    }
    let _ = std::fs::remove_file(&path);
    app.exit(0);
    Ok(())
}
