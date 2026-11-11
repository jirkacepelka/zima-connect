//! Your other devices as folders in the computer's own file browser.
//!
//! A small read-only WebDAV server that listens on this computer only (127.0.0.1). Windows
//! Explorer can open it as a network location (`\\127.0.0.1@PORT\DavWWWRoot`), Linux file
//! managers as `dav://127.0.0.1:PORT/`. The top level lists the devices, below them the folders
//! each device shares. What a device shows is decided by that device; this only passes it on.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::future::BoxFuture;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

type Body = UnsyncBoxBody<Bytes, std::io::Error>;

/// Largest piece asked of a device at once.
const PIECE: u64 = 700 * 1024;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    /// Unix seconds
    pub mtime: u64,
}

/// What the server needs: the devices and their files. Paths start with `/` and are relative to
/// the device's shared top.
pub trait Fs: Send + Sync + 'static {
    fn devices(&self) -> BoxFuture<'static, Vec<String>>;
    fn list(&self, device: String, path: String) -> BoxFuture<'static, Result<Vec<Entry>>>;
    fn stat(&self, device: String, path: String) -> BoxFuture<'static, Result<Entry>>;
    fn read(
        &self,
        device: String,
        path: String,
        offset: u64,
        len: u64,
    ) -> BoxFuture<'static, Result<Vec<u8>>>;
}

/// Start listening on this computer. `port` 0 picks a free one; returns the port in use.
pub async fn serve(fs: Arc<dyn Fs>, port: u16) -> Result<u16> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .context("webdav listen")?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                continue;
            };
            let fs = fs.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req| {
                    let fs = fs.clone();
                    async move { Ok::<_, Infallible>(route(fs, req).await) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(sock), svc)
                    .await;
            });
        }
    });
    Ok(port)
}

fn full(status: StatusCode, b: impl Into<Bytes>) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Full::new(b.into()).map_err(|e| match e {}).boxed_unsync())
        .unwrap()
}

fn decode(p: &str) -> String {
    let b = p.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(p: &str) -> String {
    let mut out = String::new();
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// "Sun, 06 Nov 1994 08:49:37 GMT"
fn http_date(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let wd = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days.rem_euclid(7)) as usize];
    let mon = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(m - 1) as usize];
    format!(
        "{wd}, {d:02} {mon} {y} {:02}:{:02}:{:02} GMT",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn response_xml(href: &str, e: &Entry) -> String {
    let h = xml(&encode(href));
    let date = http_date(e.mtime);
    if e.dir {
        format!(
            "<D:response><D:href>{h}</D:href><D:propstat><D:prop><D:displayname>{}</D:displayname><D:resourcetype><D:collection/></D:resourcetype><D:getlastmodified>{date}</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",
            xml(&e.name)
        )
    } else {
        format!(
            "<D:response><D:href>{h}</D:href><D:propstat><D:prop><D:displayname>{}</D:displayname><D:resourcetype/><D:getcontentlength>{}</D:getcontentlength><D:getlastmodified>{date}</D:getlastmodified><D:getcontenttype>application/octet-stream</D:getcontenttype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",
            xml(&e.name),
            e.size
        )
    }
}

/// `/Device/a/b` → (`Device`, `/a/b`); the top level is `None`.
fn split(path: &str) -> Option<(String, String)> {
    let p = decode(path);
    let p = p.trim_matches('/');
    if p.is_empty() {
        return None;
    }
    match p.split_once('/') {
        Some((d, rest)) => Some((d.to_string(), format!("/{rest}"))),
        None => Some((p.to_string(), "/".to_string())),
    }
}

async fn route(fs: Arc<dyn Fs>, req: Request<Incoming>) -> Response<Body> {
    let path = req.uri().path().to_string();
    let method = req.method().as_str().to_string();
    match method.as_str() {
        "OPTIONS" => Response::builder()
            .status(StatusCode::OK)
            .header("DAV", "1")
            .header("MS-Author-Via", "DAV")
            .header("Allow", "OPTIONS, PROPFIND, GET, HEAD")
            .header("Content-Length", "0")
            .body(
                Full::new(Bytes::new())
                    .map_err(|e| match e {})
                    .boxed_unsync(),
            )
            .unwrap(),
        "PROPFIND" => {
            let depth = req
                .headers()
                .get("Depth")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("1")
                .to_string();
            propfind(fs, &path, depth != "0").await
        }
        "GET" | "HEAD" => {
            let range = req
                .headers()
                .get("Range")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            get(fs, &path, range, method == "HEAD").await
        }
        _ => {
            let mut r = full(StatusCode::METHOD_NOT_ALLOWED, "read only");
            r.headers_mut()
                .insert("Allow", "OPTIONS, PROPFIND, GET, HEAD".parse().unwrap());
            r
        }
    }
}

async fn propfind(fs: Arc<dyn Fs>, path: &str, children: bool) -> Response<Body> {
    let mut out =
        String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\">");
    let base = format!("/{}", decode(path).trim_matches('/'));
    let slash = |s: &str| {
        if s == "/" {
            s.to_string()
        } else {
            format!("{s}/")
        }
    };
    match split(path) {
        None => {
            out.push_str(&response_xml(
                "/",
                &Entry {
                    name: "Zima Connect".into(),
                    dir: true,
                    size: 0,
                    mtime: 0,
                },
            ));
            if children {
                for d in fs.devices().await {
                    out.push_str(&response_xml(
                        &format!("/{d}/"),
                        &Entry {
                            name: d,
                            dir: true,
                            size: 0,
                            mtime: 0,
                        },
                    ));
                }
            }
        }
        Some((device, rest)) => {
            let st = match fs.stat(device.clone(), rest.clone()).await {
                Ok(s) => s,
                Err(_) => return full(StatusCode::NOT_FOUND, "not found"),
            };
            let st = Entry {
                name: if rest == "/" { device.clone() } else { st.name },
                ..st
            };
            let me = if st.dir { slash(&base) } else { base.clone() };
            out.push_str(&response_xml(&me, &st));
            if st.dir && children {
                match fs.list(device, rest).await {
                    Ok(list) => {
                        for e in list {
                            let href = format!(
                                "{}{}{}",
                                slash(&base),
                                e.name,
                                if e.dir { "/" } else { "" }
                            );
                            out.push_str(&response_xml(&href, &e));
                        }
                    }
                    Err(e) => return full(StatusCode::BAD_GATEWAY, e.to_string()),
                }
            }
        }
    }
    out.push_str("</D:multistatus>");
    let mut r = full(StatusCode::MULTI_STATUS, out);
    r.headers_mut().insert(
        "Content-Type",
        "application/xml; charset=utf-8".parse().unwrap(),
    );
    r
}

async fn get(fs: Arc<dyn Fs>, path: &str, range: Option<String>, head: bool) -> Response<Body> {
    let Some((device, rest)) = split(path) else {
        return full(StatusCode::FORBIDDEN, "choose a device");
    };
    let st = match fs.stat(device.clone(), rest.clone()).await {
        Ok(s) => s,
        Err(_) => return full(StatusCode::NOT_FOUND, "not found"),
    };
    if st.dir {
        return full(StatusCode::FORBIDDEN, "that is a folder");
    }
    let size = st.size;
    let (start, end) = match range.as_deref().and_then(|r| r.strip_prefix("bytes=")) {
        Some(r) => {
            let (a, b) = r.split_once('-').unwrap_or((r, ""));
            if a.is_empty() {
                // the last N bytes
                let n: u64 = b.parse().unwrap_or(0);
                (size.saturating_sub(n), size.saturating_sub(1))
            } else {
                let s: u64 = a.parse().unwrap_or(0);
                let e: u64 = b.parse().unwrap_or(size.saturating_sub(1));
                (s, e.min(size.saturating_sub(1)))
            }
        }
        None => (0, size.saturating_sub(1)),
    };
    let partial = range.is_some();
    if size == 0 || start > end || start >= size {
        if partial && size > 0 {
            let mut r = full(StatusCode::RANGE_NOT_SATISFIABLE, "");
            r.headers_mut()
                .insert("Content-Range", format!("bytes */{size}").parse().unwrap());
            return r;
        }
        let mut r = full(StatusCode::OK, "");
        r.headers_mut()
            .insert("Content-Length", "0".parse().unwrap());
        return r;
    }
    let len = end - start + 1;
    let mut b = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("Content-Type", "application/octet-stream")
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", len.to_string())
        .header("Last-Modified", http_date(st.mtime));
    if partial {
        b = b.header("Content-Range", format!("bytes {start}-{end}/{size}"));
    }
    if head {
        return b
            .body(
                Full::new(Bytes::new())
                    .map_err(|e| match e {})
                    .boxed_unsync(),
            )
            .unwrap();
    }
    // Pieces are fetched from the device one after the other as the client reads.
    let stream = futures_util::stream::unfold((start, len), move |(pos, left)| {
        let fs = fs.clone();
        let device = device.clone();
        let rest = rest.clone();
        async move {
            if left == 0 {
                return None;
            }
            let want = left.min(PIECE);
            match fs.read(device, rest, pos, want).await {
                Ok(buf) if !buf.is_empty() => {
                    let n = buf.len() as u64;
                    Some((
                        Ok(Frame::data(Bytes::from(buf))),
                        (pos + n, left - n.min(left)),
                    ))
                }
                Ok(_) => Some((Err(std::io::Error::other("the file ended early")), (pos, 0))),
                Err(e) => Some((Err(std::io::Error::other(e.to_string())), (pos, 0))),
            }
        }
    });
    b.body(StreamBody::new(stream).boxed_unsync()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Mock;
    impl Fs for Mock {
        fn devices(&self) -> BoxFuture<'static, Vec<String>> {
            Box::pin(async { vec!["My phone".to_string()] })
        }
        fn list(&self, _d: String, path: String) -> BoxFuture<'static, Result<Vec<Entry>>> {
            Box::pin(async move {
                Ok(if path == "/" {
                    vec![
                        Entry {
                            name: "DCIM".into(),
                            dir: true,
                            size: 0,
                            mtime: 1_700_000_000,
                        },
                        Entry {
                            name: "a b&c.txt".into(),
                            dir: false,
                            size: 11,
                            mtime: 1_700_000_000,
                        },
                    ]
                } else {
                    vec![]
                })
            })
        }
        fn stat(&self, _d: String, path: String) -> BoxFuture<'static, Result<Entry>> {
            Box::pin(async move {
                match path.as_str() {
                    "/" | "/DCIM" => Ok(Entry {
                        name: "DCIM".into(),
                        dir: true,
                        size: 0,
                        mtime: 0,
                    }),
                    "/a b&c.txt" => Ok(Entry {
                        name: "a b&c.txt".into(),
                        dir: false,
                        size: 11,
                        mtime: 1_700_000_000,
                    }),
                    _ => anyhow::bail!("not found"),
                }
            })
        }
        fn read(
            &self,
            _d: String,
            _p: String,
            offset: u64,
            len: u64,
        ) -> BoxFuture<'static, Result<Vec<u8>>> {
            Box::pin(async move {
                let all = b"hello world";
                let a = (offset as usize).min(all.len());
                let b = ((offset + len) as usize).min(all.len());
                Ok(all[a..b].to_vec())
            })
        }
    }

    async fn http(port: u16, req: &str) -> String {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn dates_and_paths() {
        assert_eq!(http_date(784111777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(encode("/a b&c"), "/a%20b%26c");
        assert_eq!(decode("/a%20b%26c"), "/a b&c");
        assert_eq!(
            split("/My%20phone/DCIM/x"),
            Some(("My phone".into(), "/DCIM/x".into()))
        );
        assert_eq!(split("/"), None);
    }

    #[tokio::test]
    async fn explorer_style_requests() {
        let port = serve(Arc::new(Mock), 0).await.unwrap();
        let o = http(
            port,
            "OPTIONS / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(o.contains("DAV: 1") || o.contains("dav: 1"), "{o}");
        let top = http(
            port,
            "PROPFIND / HTTP/1.1\r\nHost: x\r\nDepth: 1\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(top.starts_with("HTTP/1.1 207"), "{top}");
        assert!(top.contains("<D:href>/My%20phone/</D:href>"), "{top}");
        let dev = http(
            port,
            "PROPFIND /My%20phone/ HTTP/1.1\r\nHost: x\r\nDepth: 1\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(dev.contains("/My%20phone/DCIM/"), "{dev}");
        assert!(dev.contains("a%20b%26c.txt"), "{dev}");
        assert!(
            dev.contains("<D:getcontentlength>11</D:getcontentlength>"),
            "{dev}"
        );
        let get = http(
            port,
            "GET /My%20phone/a%20b%26c.txt HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(get.ends_with("hello world"), "{get}");
        let part = http(port, "GET /My%20phone/a%20b%26c.txt HTTP/1.1\r\nHost: x\r\nRange: bytes=6-8\r\nConnection: close\r\n\r\n").await;
        assert!(
            part.starts_with("HTTP/1.1 206")
                && part.ends_with("wor")
                && part.contains("bytes 6-8/11"),
            "{part}"
        );
        let none = http(
            port,
            "GET /My%20phone/nope HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(none.starts_with("HTTP/1.1 404"), "{none}");
        let put = http(port, "PUT /My%20phone/x HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        assert!(put.starts_with("HTTP/1.1 405"), "{put}");
    }
}
