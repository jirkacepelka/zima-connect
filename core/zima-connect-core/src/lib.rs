//! # Zima Connect Core
//!
//! Portable protocol and cryptography library for [Zima Connect](https://github.com/user/zima-connect),
//! a peer-to-server-to-peer device connectivity platform.
//!
//! ## Features
//!
//! * **End-to-end encryption** — AES-256-GCM with per-message random nonces.
//! * **Device identity** — X25519 key pairs, SHA-256 derived device IDs.
//! * **Mutual authentication** — Ed25519 envelope signatures.
//! * **Secure pairing** — ECDH key exchange, HKDF-SHA256 session keys, 6-digit SAS codes.
//! * **Compact wire format** — MessagePack-serialized [`Envelope`](protocol::Envelope).
//! * **Cross-language** — UniFFI bindings for Kotlin/Swift/Python (feature `uniffi-bindings`).
//!
//! ## Quick start
//!
//! ```rust
//! use zima_connect_core::crypto::{DeviceKeyPair, SigningKeyPair, encrypt, decrypt, perform_pairing};
//! use zima_connect_core::protocol::{Envelope, MessageType, DeviceId};
//!
//! // 1. Generate device identity
//! let device_keys = DeviceKeyPair::generate();
//! let signing_keys = SigningKeyPair::generate();
//! let my_id = device_keys.device_id();
//!
//! // 2. Pair with another device (given their public key bytes)
//! // let (session_key, sas) = perform_pairing(&device_keys, &peer_pub_bytes)?;
//!
//! // 3. Build, sign, and encrypt an envelope
//! // let payload = encrypt(&session_key, b"clipboard text")?;
//! // let env = Envelope::builder()
//! //     .from_device(&my_id)
//! //     .to_device(&peer_id)
//! //     .msg_type(MessageType::ClipboardSync)
//! //     .payload(payload)
//! //     .encrypted(true)
//! //     .build()?;
//! // let signable = env.signable_bytes()?;
//! // let sig = signing_keys.sign(&signable);
//! ```
//!
//! ## Crate feature flags
//!
//! | Flag             | Default | Description                      |
//! |------------------|---------|----------------------------------|
//! | `uniffi-bindings`| off     | Enable UniFFI scaffold generation|
//!
//! ## Module map
//!
//! - [`crypto`] — all cryptographic primitives
//! - [`protocol`] — envelope format, types, serialization
//! - [`error`] — unified `ZimaError` type
//!
//! ---
//!
//! > **Note on UniFFI**: The `uniffi` dependency is included behind the
//! > `uniffi-bindings` feature flag.  Actual UDL definitions and procedural
//! > macro annotations will be wired up in a follow-up phase once the API
//! > surface is stable.

pub mod crypto;
pub mod error;
pub mod protocol;

// Top-level re-exports for ergonomic use.
pub use error::{Result, ZimaError};
