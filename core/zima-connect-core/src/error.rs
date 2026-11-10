//! Unified error types for the Zima Connect core library.
//!
//! All fallible operations in the library surface errors through [`ZimaError`],
//! which can be pattern-matched by callers to distinguish between crypto
//! failures, serialization problems, and protocol violations.

use thiserror::Error;

/// Top-level error type returned by every public API in this crate.
#[derive(Debug, Error)]
pub enum ZimaError {
    // ── Cryptography ────────────────────────────────────────────────────
    /// AES-256-GCM encryption failed (e.g. invalid key length).
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    /// AES-256-GCM decryption failed (bad key, corrupted ciphertext, or
    /// authentication tag mismatch).
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    /// Ed25519 signature generation or verification failed.
    #[error("signature error: {0}")]
    SignatureError(String),

    /// HKDF key derivation failed (e.g. invalid output length).
    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),

    /// A supplied key or secret had an unexpected size.
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength {
        /// Byte count the API expected.
        expected: usize,
        /// Byte count that was actually supplied.
        actual: usize,
    },

    // ── Serialization ───────────────────────────────────────────────────
    /// MessagePack encoding failed.
    #[error("msgpack encode error: {0}")]
    EncodeFailed(String),

    /// MessagePack decoding failed.
    #[error("msgpack decode error: {0}")]
    DecodeFailed(String),

    /// Hex decoding of a device ID or key failed.
    #[error("hex decode error: {0}")]
    HexDecodeFailed(String),

    // ── Protocol ────────────────────────────────────────────────────────
    /// An envelope field failed validation (missing, out-of-range, etc.).
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),

    /// The envelope has expired based on its TTL and timestamp.
    #[error("envelope expired")]
    EnvelopeExpired,

    /// The envelope version is not supported by this library build.
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, ZimaError>;

// ── Foreign error conversions ───────────────────────────────────────────

impl From<hex::FromHexError> for ZimaError {
    fn from(e: hex::FromHexError) -> Self {
        ZimaError::HexDecodeFailed(e.to_string())
    }
}

impl From<rmp_serde::encode::Error> for ZimaError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        ZimaError::EncodeFailed(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for ZimaError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        ZimaError::DecodeFailed(e.to_string())
    }
}

impl From<ed25519_dalek::SignatureError> for ZimaError {
    fn from(e: ed25519_dalek::SignatureError) -> Self {
        ZimaError::SignatureError(e.to_string())
    }
}
