//! The core [`Envelope`] message container.
//!
//! Every message exchanged between Zima Connect devices is wrapped in an
//! `Envelope`.  The struct is designed for MessagePack serialization and
//! provides a [`EnvelopeBuilder`] for ergonomic construction.
//!
//! # Wire layout
//!
//! | Field       | Type       | Notes                              |
//! |-------------|------------|------------------------------------|
//! | `version`   | `u8`       | Always `1` for now                 |
//! | `id`        | `String`   | UUID v7 (sortable by time)         |
//! | `from`      | `String`   | 64-char hex device ID              |
//! | `to`        | `String`   | 64-char hex device ID              |
//! | `msg_type`  | `String`   | Dot-delimited message type         |
//! | `timestamp` | `i64`      | Unix milliseconds                  |
//! | `ttl`       | `u32`      | Seconds; 0 = never expires         |
//! | `encrypted` | `bool`     | Whether `payload` is E2E encrypted |
//! | `payload`   | `Vec<u8>`  | Possibly-encrypted body bytes      |
//! | `signature` | `Vec<u8>`  | Ed25519 detached signature         |

use serde::{Deserialize, Serialize};

use crate::error::{Result, ZimaError};
use crate::protocol::serialize;
use crate::protocol::types::{DeviceId, MessageType};

/// Current protocol version constant.
pub const PROTOCOL_VERSION: u8 = 1;

/// A self-contained message envelope used on the wire between devices.
///
/// Construct via [`EnvelopeBuilder`] or deserialize from MessagePack bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// Protocol version (currently `1`).
    pub version: u8,

    /// Unique message identifier (UUID v7, time-sortable).
    pub id: String,

    /// Sender device ID (64-char lowercase hex).
    pub from: String,

    /// Recipient device ID (64-char lowercase hex).
    pub to: String,

    /// Dot-delimited message type string (e.g. `"clipboard.sync"`).
    pub msg_type: String,

    /// Creation timestamp in Unix milliseconds.
    pub timestamp: i64,

    /// Time-to-live in seconds.  `0` means the envelope never expires.
    pub ttl: u32,

    /// `true` when `payload` contains AES-256-GCM ciphertext.
    pub encrypted: bool,

    /// Message body — either plaintext or ciphertext bytes.
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,

    /// Ed25519 detached signature over the canonical envelope fields.
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl Envelope {
    /// Start building a new envelope.
    pub fn builder() -> EnvelopeBuilder {
        EnvelopeBuilder::default()
    }

    /// Serialize this envelope to MessagePack bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serialize::to_msgpack_named(self)
    }

    /// Deserialize an envelope from MessagePack bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let env: Self = serialize::from_msgpack(data)?;
        if env.version != PROTOCOL_VERSION {
            return Err(ZimaError::UnsupportedVersion(env.version));
        }
        Ok(env)
    }

    /// Check whether this envelope has expired relative to `now_ms`
    /// (Unix milliseconds).
    ///
    /// Returns `false` when `ttl == 0` (infinite lifetime).
    pub fn is_expired(&self, now_ms: i64) -> bool {
        if self.ttl == 0 {
            return false;
        }
        let expiry_ms = self.timestamp + (self.ttl as i64) * 1000;
        now_ms > expiry_ms
    }

    /// Return the bytes that should be signed / verified.
    ///
    /// The canonical signing payload is the MessagePack encoding of every
    /// field **except** `signature` itself.
    pub fn signable_bytes(&self) -> Result<Vec<u8>> {
        let signable = SignableEnvelope {
            version: self.version,
            id: &self.id,
            from: &self.from,
            to: &self.to,
            msg_type: &self.msg_type,
            timestamp: self.timestamp,
            ttl: self.ttl,
            encrypted: self.encrypted,
            payload: &self.payload,
        };
        serialize::to_msgpack_named(&signable)
    }
}

/// Borrowed view of the envelope fields that are covered by the signature.
#[derive(Serialize)]
struct SignableEnvelope<'a> {
    version: u8,
    id: &'a str,
    from: &'a str,
    to: &'a str,
    msg_type: &'a str,
    timestamp: i64,
    ttl: u32,
    encrypted: bool,
    #[serde(with = "serde_bytes")]
    payload: &'a [u8],
}

// ── Builder ─────────────────────────────────────────────────────────────

/// Ergonomic builder for constructing [`Envelope`] instances.
///
/// # Example
/// ```ignore
/// let envelope = Envelope::builder()
///     .from_device(&my_id)
///     .to_device(&peer_id)
///     .msg_type(MessageType::ClipboardSync)
///     .payload(b"hello".to_vec())
///     .build()?;
/// ```
#[derive(Debug, Default)]
pub struct EnvelopeBuilder {
    from: Option<DeviceId>,
    to: Option<DeviceId>,
    msg_type: Option<MessageType>,
    payload: Vec<u8>,
    encrypted: bool,
    ttl: u32,
    signature: Vec<u8>,
}

impl EnvelopeBuilder {
    /// Set the sender device.
    pub fn from_device(mut self, id: &DeviceId) -> Self {
        self.from = Some(id.clone());
        self
    }

    /// Set the recipient device.
    pub fn to_device(mut self, id: &DeviceId) -> Self {
        self.to = Some(id.clone());
        self
    }

    /// Set the message type.
    pub fn msg_type(mut self, mt: MessageType) -> Self {
        self.msg_type = Some(mt);
        self
    }

    /// Set the payload bytes (plaintext or ciphertext).
    pub fn payload(mut self, data: Vec<u8>) -> Self {
        self.payload = data;
        self
    }

    /// Mark the payload as encrypted.
    pub fn encrypted(mut self, flag: bool) -> Self {
        self.encrypted = flag;
        self
    }

    /// Set the TTL in seconds.  `0` means never expires.
    pub fn ttl(mut self, seconds: u32) -> Self {
        self.ttl = seconds;
        self
    }

    /// Set a pre-computed Ed25519 signature.
    pub fn signature(mut self, sig: Vec<u8>) -> Self {
        self.signature = sig;
        self
    }

    /// Consume the builder and produce a validated [`Envelope`].
    ///
    /// # Errors
    /// Returns [`ZimaError::InvalidEnvelope`] if required fields are missing.
    pub fn build(self) -> Result<Envelope> {
        let from = self
            .from
            .ok_or_else(|| ZimaError::InvalidEnvelope("missing `from` device".into()))?;
        let to = self
            .to
            .ok_or_else(|| ZimaError::InvalidEnvelope("missing `to` device".into()))?;
        let msg_type = self
            .msg_type
            .ok_or_else(|| ZimaError::InvalidEnvelope("missing message type".into()))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Ok(Envelope {
            version: PROTOCOL_VERSION,
            id: uuid::Uuid::now_v7().to_string(),
            from: from.0,
            to: to.0,
            msg_type: msg_type.as_str().to_string(),
            timestamp: now,
            ttl: self.ttl,
            encrypted: self.encrypted,
            payload: self.payload,
            signature: self.signature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device_id(byte: u8) -> DeviceId {
        DeviceId::from_bytes(&[byte; 32])
    }

    #[test]
    fn build_and_roundtrip() {
        let env = Envelope::builder()
            .from_device(&test_device_id(0xAA))
            .to_device(&test_device_id(0xBB))
            .msg_type(MessageType::Ping)
            .payload(b"hello".to_vec())
            .ttl(60)
            .build()
            .unwrap();

        assert_eq!(env.version, 1);
        assert_eq!(env.msg_type, "ping");
        assert!(!env.is_expired(env.timestamp));

        let bytes = env.to_bytes().unwrap();
        let restored = Envelope::from_bytes(&bytes).unwrap();
        assert_eq!(restored.id, env.id);
        assert_eq!(restored.msg_type, env.msg_type);
        assert_eq!(restored.payload, b"hello");
    }

    #[test]
    fn ttl_expiry() {
        let env = Envelope::builder()
            .from_device(&test_device_id(0x01))
            .to_device(&test_device_id(0x02))
            .msg_type(MessageType::Pong)
            .ttl(10)
            .build()
            .unwrap();

        // Not expired at creation time.
        assert!(!env.is_expired(env.timestamp));
        // Expired 11 seconds later.
        assert!(env.is_expired(env.timestamp + 11_000));
        // Not expired with infinite TTL.
        let env0 = Envelope::builder()
            .from_device(&test_device_id(0x01))
            .to_device(&test_device_id(0x02))
            .msg_type(MessageType::Pong)
            .ttl(0)
            .build()
            .unwrap();
        assert!(!env0.is_expired(i64::MAX));
    }

    #[test]
    fn builder_missing_fields() {
        let res = Envelope::builder().build();
        assert!(res.is_err());
    }

    #[test]
    fn signable_bytes_deterministic() {
        let env = Envelope::builder()
            .from_device(&test_device_id(0xCC))
            .to_device(&test_device_id(0xDD))
            .msg_type(MessageType::ClipboardSync)
            .build()
            .unwrap();

        let a = env.signable_bytes().unwrap();
        let b = env.signable_bytes().unwrap();
        assert_eq!(a, b);
    }
}
