//! Shared protocol types used across Zima Connect messages.
//!
//! This module defines the core value types that appear in [`Envelope`] fields
//! and plugin payloads.  All types derive `Serialize` / `Deserialize` for
//! MessagePack round-tripping and `Clone` + `Debug` for ergonomics.

use serde::{Deserialize, Serialize};

// ── DeviceID ────────────────────────────────────────────────────────────

/// A 32-byte device identifier encoded as a lowercase hex string (64 chars).
///
/// The ID is derived from the device's X25519 public key via SHA-256, which
/// provides a stable, collision-resistant handle independent of the actual
/// key material.
///
/// # Wire format
/// Serialized as a plain UTF-8 string (`"a1b2c3…"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl DeviceId {
    /// Create a [`DeviceId`] from raw 32-byte hash.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self(hex::encode(bytes))
    }

    /// Parse a hex-encoded device ID, validating length.
    pub fn from_hex(s: &str) -> crate::error::Result<Self> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(crate::error::ZimaError::InvalidKeyLength {
                expected: 32,
                actual: bytes.len(),
            });
        }
        Ok(Self(s.to_lowercase()))
    }

    /// Return the hex string representation.
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Decode back into raw 32-byte form.
    pub fn to_bytes(&self) -> crate::error::Result<[u8; 32]> {
        let v = hex::decode(&self.0)?;
        v.try_into()
            .map_err(|_| crate::error::ZimaError::InvalidKeyLength {
                expected: 32,
                actual: 0,
            })
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── MessageType ─────────────────────────────────────────────────────────

/// Well-known message types carried inside an [`Envelope`].
///
/// The wire representation is a dot-delimited string such as
/// `"clipboard.sync"`.  Unknown types received from peers are captured as
/// [`Custom`](MessageType::Custom).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    // ── Pairing ─────────────────────────────────────────────────────
    /// Initial pairing request from one device to another.
    #[serde(rename = "pair.request")]
    PairRequest,

    /// Positive pairing response.
    #[serde(rename = "pair.accept")]
    PairAccept,

    /// Negative pairing response / revocation.
    #[serde(rename = "pair.reject")]
    PairReject,

    // ── Clipboard ───────────────────────────────────────────────────
    /// Push clipboard content to paired device(s).
    #[serde(rename = "clipboard.sync")]
    ClipboardSync,

    // ── Notifications ───────────────────────────────────────────────
    /// Mirror a notification from the source device.
    #[serde(rename = "notification.mirror")]
    NotificationMirror,

    /// Dismiss a previously mirrored notification.
    #[serde(rename = "notification.dismiss")]
    NotificationDismiss,

    // ── File transfer ───────────────────────────────────────────────
    /// Offer to send one or more files.
    #[serde(rename = "file.offer")]
    FileOffer,

    /// Accept a file transfer offer (triggers data stream).
    #[serde(rename = "file.accept")]
    FileAccept,

    /// Cancel / abort a file transfer.
    #[serde(rename = "file.cancel")]
    FileCancel,

    // ── Media control ───────────────────────────────────────────────
    /// Remote media transport command (play/pause/next/prev).
    #[serde(rename = "media.control")]
    MediaControl,

    // ── Ping ────────────────────────────────────────────────────────
    /// Lightweight heartbeat / keep-alive.
    #[serde(rename = "ping")]
    Ping,

    /// Reply to a ping.
    #[serde(rename = "pong")]
    Pong,

    // ── Catch-all ───────────────────────────────────────────────────
    /// Any message type not covered above.
    #[serde(untagged)]
    Custom(String),
}

impl MessageType {
    /// Return the canonical dot-delimited string for this type.
    pub fn as_str(&self) -> &str {
        match self {
            Self::PairRequest => "pair.request",
            Self::PairAccept => "pair.accept",
            Self::PairReject => "pair.reject",
            Self::ClipboardSync => "clipboard.sync",
            Self::NotificationMirror => "notification.mirror",
            Self::NotificationDismiss => "notification.dismiss",
            Self::FileOffer => "file.offer",
            Self::FileAccept => "file.accept",
            Self::FileCancel => "file.cancel",
            Self::MediaControl => "media.control",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Plugin payload types ────────────────────────────────────────────────

/// Describes a file being offered in a [`FileOffer`](MessageType::FileOffer)
/// message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReference {
    /// Human-readable file name (e.g. `"photo.jpg"`).
    pub name: String,

    /// MIME type (e.g. `"image/jpeg"`).
    pub mime_type: String,

    /// Size in bytes.  `None` if unknown (streaming).
    pub size: Option<u64>,

    /// SHA-256 hash of the file content, hex-encoded.
    /// Present when the sender has pre-computed it.
    pub sha256: Option<String>,
}

/// Payload for [`ClipboardSync`](MessageType::ClipboardSync) messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPayload {
    /// The clipboard text content.
    pub text: String,
}

/// Payload for [`MediaControl`](MessageType::MediaControl) messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaControlPayload {
    /// The control action (e.g. `"play"`, `"pause"`, `"next"`, `"prev"`).
    pub action: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_roundtrip() {
        let bytes = [0xABu8; 32];
        let id = DeviceId::from_bytes(&bytes);
        assert_eq!(id.as_hex().len(), 64);
        let decoded = id.to_bytes().unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn message_type_serde_roundtrip() {
        let mt = MessageType::ClipboardSync;
        let packed = rmp_serde::to_vec(&mt).unwrap();
        let back: MessageType = rmp_serde::from_slice(&packed).unwrap();
        assert_eq!(back, mt);
    }
}
