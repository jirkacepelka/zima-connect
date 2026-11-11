use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::household::{AdminOp, SignedSnapshot};
use crate::mailbox::{Entry, Item};
use crate::shelf::{Audience, ShelfItem};

/// What another device asks of a phone's text messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SmsOp {
    /// The conversations, newest first.
    Threads,
    /// The messages of one conversation.
    Thread {
        id: String,
    },
    Send {
        to: String,
        body: String,
    },
}

/// What another device asks of a phone's line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PhoneOp {
    /// Place a call to this number.
    Dial { number: String },
    /// Pick up the ringing call.
    Answer,
    /// Turn the ringing call down.
    Decline,
    /// End the current call.
    HangUp,
    /// Mute or unmute the phone's microphone.
    Mute { on: bool },
    /// Ask what the line is doing right now.
    State,
}

/// What another device asks of a phone's screen. Positions are fractions of the screen (0 to 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ScreenOp {
    /// What is shared right now: { sharing, control, width, height }.
    Status,
    /// The latest picture as { jpg (base64), width, height }; `max_width` shrinks it.
    Frame {
        max_width: Option<u32>,
    },
    Tap {
        x: f32,
        y: f32,
    },
    /// Press and hold.
    Hold {
        x: f32,
        y: f32,
    },
    Swipe {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        ms: u32,
    },
    /// "back" | "home" | "recents" | "notifications"
    Key {
        name: String,
    },
}

/// Things one device can do on another by remote control.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum RemoteInput {
    Move {
        dx: i32,
        dy: i32,
    },
    /// "left" | "right" | "middle" | "double"
    Click {
        button: String,
    },
    Scroll {
        dy: i32,
    },
    Text {
        text: String,
    },
    /// "enter" | "backspace" | "esc" | "tab" | "left" | "right" | "up" | "down" | "space" | …
    Key {
        key: String,
    },
    /// "play_pause" | "next" | "prev" | "vol_up" | "vol_down" | "mute"
    Media {
        action: String,
    },
}

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
    RingStop,
    /// Ask a phone for its text messages (the phone answers through the app).
    Sms {
        op: SmsOp,
    },
    /// A phone tells its owner's other devices that a text message arrived.
    SmsPush {
        address: String,
        name: Option<String>,
        body: String,
    },
    /// Use a phone's line (dial, answer, hang up); the phone's app answers.
    Phone {
        op: PhoneOp,
    },
    /// A phone tells its owner's other devices that its call state changed.
    PhonePush {
        state: String,
        number: String,
        name: Option<String>,
    },
    /// Look at and control a phone's screen; the phone's app answers.
    Screen {
        op: ScreenOp,
    },
    /// Open a link on the other device.
    OpenUrl {
        url: String,
    },
    /// Battery level push.
    Battery {
        percent: u8,
        charging: bool,
    },
    Remote {
        input: RemoteInput,
    },
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
    /// This device got a new name.
    Rename {
        name: String,
    },
    /// The person using this device picked a colour for themselves.
    Color {
        hue: u16,
    },
    // -- shelf (hub) --
    /// Followed by exactly `size` raw bytes.
    ShelfPut {
        name: String,
        size: u64,
        #[serde(default)]
        audience: Audience,
    },
    ShelfList,
    /// Followed (in the reply) by the raw bytes.
    ShelfGet {
        id: String,
    },
    ShelfDelete {
        id: String,
    },
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
    Json { value: serde_json::Value },
    PairOk { name: String, platform: String },
    Joined { snapshot: SignedSnapshot },
    Snapshot { snapshot: SignedSnapshot },
    NotModified,
    InviteCode { code: String },
    Created { id: String },
    Queued,
    Shelf { items: Vec<ShelfItem> },
    ShelfItem { item: ShelfItem },
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
