//! Protocol layer for Zima Connect.
//!
//! This module defines the [`Envelope`] message format, shared protocol
//! types, and MessagePack serialization utilities.
//!
//! # Modules
//!
//! - [`envelope`] – The core message container and its builder.
//! - [`types`]    – `DeviceId`, `MessageType`, plugin payload structs.
//! - [`serialize`] – Helpers for encoding/decoding MessagePack.

pub mod envelope;
pub mod serialize;
pub mod types;

// Re-exports for convenience.
pub use envelope::{Envelope, EnvelopeBuilder, PROTOCOL_VERSION};
pub use serialize::{from_msgpack, to_msgpack, to_msgpack_named};
pub use types::{DeviceId, FileReference, MessageType};
