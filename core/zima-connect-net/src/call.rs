//! Voice calls: one long-lived QUIC stream per call carrying small audio frames both ways.
//!
//! Audio is raw 16-bit little-endian PCM, mono, 16 000 Hz, in 20 ms frames (640 bytes). QUIC already
//! encrypts it end to end between the two devices, and nothing goes through the server.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};

/// Largest audio frame we accept (limits memory for a misbehaving peer).
pub const MAX_FRAME: usize = 4096;
const HANGUP: u16 = 0xFFFF;
/// How long a call rings before it is given up.
pub const RING_TIMEOUT: Duration = Duration::from_secs(45);

pub enum Frame {
    Audio(Vec<u8>),
    Hangup,
}

pub struct CallEntry {
    pub peer: String,
    pub peer_name: String,
    pub outgoing: bool,
    pub accept: std::sync::Mutex<Option<oneshot::Sender<bool>>>,
    pub out_tx: mpsc::Sender<Frame>,
    pub in_rx: Mutex<mpsc::Receiver<Vec<u8>>>,
}

pub struct Channels {
    pub out_rx: mpsc::Receiver<Frame>,
    pub in_tx: mpsc::Sender<Vec<u8>>,
}

pub fn new_entry(
    peer: String,
    peer_name: String,
    outgoing: bool,
    accept: Option<oneshot::Sender<bool>>,
) -> (CallEntry, Channels) {
    let (out_tx, out_rx) = mpsc::channel(32);
    let (in_tx, in_rx) = mpsc::channel(64);
    (
        CallEntry {
            peer,
            peer_name,
            outgoing,
            accept: std::sync::Mutex::new(accept),
            out_tx,
            in_rx: Mutex::new(in_rx),
        },
        Channels { out_rx, in_tx },
    )
}

/// Move audio between the stream and the channels until either side hangs up.
pub async fn pump<W, R>(mut send: W, mut recv: R, ch: Channels)
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let Channels { mut out_rx, in_tx } = ch;
    let mut writer = tokio::spawn(async move {
        while let Some(f) = out_rx.recv().await {
            match f {
                Frame::Audio(d) => {
                    if d.is_empty() || d.len() > MAX_FRAME {
                        continue;
                    }
                    if send
                        .write_all(&(d.len() as u16).to_be_bytes())
                        .await
                        .is_err()
                        || send.write_all(&d).await.is_err()
                    {
                        return;
                    }
                }
                Frame::Hangup => break,
            }
        }
        let _ = send.write_all(&HANGUP.to_be_bytes()).await;
        let _ = send.flush().await;
        let _ = send.shutdown().await;
    });
    let mut reader = tokio::spawn(async move {
        let mut buf = vec![0u8; MAX_FRAME];
        loop {
            let mut len = [0u8; 2];
            if recv.read_exact(&mut len).await.is_err() {
                break;
            }
            let n = u16::from_be_bytes(len);
            if n == HANGUP || n as usize > MAX_FRAME {
                break;
            }
            if recv.read_exact(&mut buf[..n as usize]).await.is_err() {
                break;
            }
            // A slow listener drops audio instead of building up delay.
            let _ = in_tx.try_send(buf[..n as usize].to_vec());
        }
    });
    tokio::select! {
        _ = &mut writer => {}
        _ = &mut reader => {}
    }
    writer.abort();
    reader.abort();
}
