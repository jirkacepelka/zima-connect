use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_MSG: u32 = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Request {
    Pair {
        secret: String,
        name: String,
        platform: String,
    },
    Ping,
    Clipboard {
        text: String,
    },
    Ring,
    /// Followed by exactly `size` raw bytes on the same stream.
    File {
        name: String,
        size: u64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Response {
    Ok,
    PairOk { name: String, platform: String },
    Err { message: String },
}

pub async fn write_msg<W: AsyncWrite + Unpin, T: Serialize>(w: &mut W, msg: &T) -> Result<()> {
    let data = serde_json::to_vec(msg)?;
    w.write_all(&(data.len() as u32).to_be_bytes()).await?;
    w.write_all(&data).await?;
    Ok(())
}

pub async fn read_msg<R: AsyncRead + Unpin, T: DeserializeOwned>(r: &mut R) -> Result<T> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len);
    if len > MAX_MSG {
        bail!("message too large");
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf)?)
}
