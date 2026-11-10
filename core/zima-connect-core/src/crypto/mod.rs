//! Cryptography primitives for Zima Connect.
//!
//! # Modules
//!
//! | Module      | Purpose                                        |
//! |-------------|------------------------------------------------|
//! | [`keys`]    | X25519 key pair generation, device ID derivation |
//! | [`encrypt`] | AES-256-GCM authenticated encryption/decryption |
//! | [`sign`]    | Ed25519 signing and verification               |
//! | [`pairing`] | ECDH + HKDF session key derivation, SAS codes  |

pub mod encrypt;
pub mod keys;
pub mod pairing;
pub mod sign;

// Handy re-exports.
pub use encrypt::{decrypt, encrypt};
pub use keys::DeviceKeyPair;
pub use pairing::{derive_session_key, ecdh_shared_secret, perform_pairing, sas_code};
pub use sign::{verify, SigningKeyPair};
