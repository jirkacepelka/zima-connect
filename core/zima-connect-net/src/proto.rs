use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::household::{AdminOp, SignedSnapshot};
use crate::mailbox::{Entry, Item};

const MAX_MSG: u32 = 4 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Request {
    // -- direct 1:1 pairing (works without any server) --
    Pair {
        secret: String,
        name: String,
        platform: String,
    },
    // -- device to device --
    Ping,
    Clipboard {
        text: String,
    },
    Ring,
    Notify {
        app: String,
        title: String,
        body: String,
    },
    /// Followed by exactly `size` raw bytes on the same stream.
    File {
        name: String,
        size: u64,
    },
    // -- device to hub --
    Join {
        code: String,
        name: String,
        platform: String,
        user_name: Option<String>,
    },
    Sync {
        have: u64,
    },
    Admin {
        op: AdminOp,
    },
    /// This device leaves the household (removes itself from the server).
    Leave,
    // -- offline queue (hub) --
    /// Leave an item for an offline device. Files are followed by raw bytes.
    Deposit {
        to: String,
        item: Item,
    },
    Inbox,
    /// Followed (in the reply) by the raw bytes when the item is a file.
    Fetch {
        id: String,
    },
    Ack {
        id: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Response {
    Ok,
    PairOk { name: String, platform: String },
    Joined { snapshot: SignedSnapshot },
    Snapshot { snapshot: SignedSnapshot },
    NotModified,
    InviteCode { code: String },
    Queued,
    Inbox { entries: Vec<Entry> },
    Entry { entry: Entry },
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
