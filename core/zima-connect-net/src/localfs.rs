//! Answers "list this folder" and "read this piece of that file" for a set of shared folders.
//! Read-only. A request can never leave the shared folders: every path is checked part by part,
//! and the final location must still lie inside the folder after links are resolved.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{json, Value};

use crate::proto::FilesOp;

/// The most one read returns.
pub const MAX_READ: u64 = 768 * 1024;

/// `roots` are shown as the top-level folders: `("Documents", /home/me/Documents)`.
/// With one root the folder itself is the top.
pub fn answer(roots: &[(String, PathBuf)], op: &FilesOp) -> Value {
    match run(roots, op) {
        Ok(v) => v,
        Err(m) => json!({ "error": m }),
    }
}

fn parts(path: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    for p in path.split(['/', '\\']) {
        match p {
            "" | "." => {}
            ".." => return Err("that location is not available".into()),
            p if p.contains(':') || p.contains('\0') => {
                return Err("that location is not available".into())
            }
            p => out.push(p),
        }
    }
    Ok(out)
}

/// The real path for a shared path, or `None` for the virtual top level.
fn resolve(roots: &[(String, PathBuf)], path: &str) -> Result<Option<PathBuf>, String> {
    let mut parts = parts(path)?;
    let (base, rest): (&Path, Vec<&str>) = if roots.len() == 1 {
        (&roots[0].1, parts)
    } else {
        if parts.is_empty() {
            return Ok(None);
        }
        let first = parts.remove(0);
        let Some((_, dir)) = roots.iter().find(|(n, _)| n == first) else {
            return Err("that folder is not shared".into());
        };
        (dir, parts)
    };
    let mut full = base.to_path_buf();
    for p in rest {
        full.push(p);
    }
    // Links must not lead out of the shared folder.
    let real = full.canonicalize().map_err(|_| "not found".to_string())?;
    let base_real = base
        .canonicalize()
        .map_err(|_| "the shared folder is missing".to_string())?;
    if !real.starts_with(&base_real) {
        return Err("that location is not available".into());
    }
    Ok(Some(real))
}

fn mtime(m: &std::fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn run(roots: &[(String, PathBuf)], op: &FilesOp) -> Result<Value, String> {
    match op {
        FilesOp::List { path } => {
            let Some(dir) = resolve(roots, path)? else {
                let entries: Vec<Value> = roots
                    .iter()
                    .filter(|(_, p)| p.is_dir())
                    .map(|(n, _)| json!({ "name": n, "dir": true, "size": 0, "mtime": 0 }))
                    .collect();
                return Ok(json!({ "entries": entries }));
            };
            let rd = std::fs::read_dir(&dir).map_err(|e| format!("cannot open the folder: {e}"))?;
            let mut entries: Vec<Value> = Vec::new();
            for e in rd.flatten().take(5000) {
                let Ok(m) = e.metadata() else { continue };
                let name = e.file_name().to_string_lossy().into_owned();
                entries.push(json!({ "name": name, "dir": m.is_dir(), "size": if m.is_dir() { 0 } else { m.len() }, "mtime": mtime(&m) }));
            }
            Ok(json!({ "entries": entries }))
        }
        FilesOp::Stat { path } => {
            let Some(p) = resolve(roots, path)? else {
                return Ok(json!({ "dir": true, "size": 0, "mtime": 0 }));
            };
            let m = std::fs::metadata(&p).map_err(|_| "not found".to_string())?;
            Ok(
                json!({ "dir": m.is_dir(), "size": if m.is_dir() { 0 } else { m.len() }, "mtime": mtime(&m) }),
            )
        }
        FilesOp::Read { path, offset, len } => {
            use std::io::{Read, Seek, SeekFrom};
            let Some(p) = resolve(roots, path)? else {
                return Err("that is a folder".into());
            };
            let mut f =
                std::fs::File::open(&p).map_err(|e| format!("cannot open the file: {e}"))?;
            let m = f.metadata().map_err(|e| e.to_string())?;
            if m.is_dir() {
                return Err("that is a folder".into());
            }
            let want = (*len).min(MAX_READ);
            f.seek(SeekFrom::Start(*offset))
                .map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; want as usize];
            let mut got = 0;
            while got < buf.len() {
                let n = f.read(&mut buf[got..]).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                got += n;
            }
            buf.truncate(got);
            use base64::Engine;
            Ok(json!({
                "data": base64::engine::general_purpose::STANDARD.encode(&buf),
                "size": m.len(),
                "eof": offset + got as u64 >= m.len(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zc-localfs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.txt"), "hello world").unwrap();
        std::fs::write(d.join("sub").join("b.txt"), "b").unwrap();
        d
    }

    #[test]
    fn lists_reads_and_refuses_escapes() {
        let d = tmp("one");
        let roots = vec![("Docs".to_string(), d.clone())];
        let l = answer(&roots, &FilesOp::List { path: "/".into() });
        assert_eq!(l["entries"].as_array().unwrap().len(), 2);
        let r = answer(
            &roots,
            &FilesOp::Read {
                path: "/a.txt".into(),
                offset: 6,
                len: 100,
            },
        );
        assert_eq!(r["eof"], true);
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(r["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(bytes, b"world");
        for bad in [
            "/../x",
            "/sub/../../x",
            "C:/Windows",
            "/sub/../..",
            "\\..\\x",
        ] {
            assert!(
                answer(&roots, &FilesOp::List { path: bad.into() })
                    .get("error")
                    .is_some(),
                "{bad}"
            );
        }
        assert!(answer(
            &roots,
            &FilesOp::Read {
                path: "/sub".into(),
                offset: 0,
                len: 1
            }
        )
        .get("error")
        .is_some());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn several_roots_show_a_top_level() {
        let a = tmp("a");
        let b = tmp("b");
        let roots = vec![("A".to_string(), a.clone()), ("B".to_string(), b.clone())];
        let top = answer(&roots, &FilesOp::List { path: "".into() });
        assert_eq!(top["entries"].as_array().unwrap().len(), 2);
        let s = answer(
            &roots,
            &FilesOp::List {
                path: "/B/sub".into(),
            },
        );
        assert_eq!(s["entries"][0]["name"], "b.txt");
        assert!(answer(&roots, &FilesOp::List { path: "/C".into() })
            .get("error")
            .is_some());
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
    }
}
