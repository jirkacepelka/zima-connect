//! MessagePack serialization helpers.
//!
//! Thin wrappers around [`rmp_serde`] that normalise encoding options and
//! provide a uniform [`ZimaError`]-based error surface.

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Result, ZimaError};

/// Encode `value` into a MessagePack byte vector.
///
/// Uses the compact (unnamed-field) format to minimise wire size.
///
/// # Errors
/// Returns [`ZimaError::EncodeFailed`] if serialization fails.
pub fn to_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    rmp_serde::to_vec(value).map_err(|e| ZimaError::EncodeFailed(e.to_string()))
}

/// Encode `value` into MessagePack using the human-readable named-field
/// format.  Slightly larger on the wire but easier to debug.
///
/// # Errors
/// Returns [`ZimaError::EncodeFailed`] if serialization fails.
pub fn to_msgpack_named<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(|e| ZimaError::EncodeFailed(e.to_string()))
}

/// Decode a MessagePack byte slice into `T`.
///
/// # Errors
/// Returns [`ZimaError::DecodeFailed`] if the bytes are not valid MessagePack
/// or do not match the target type.
pub fn from_msgpack<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    rmp_serde::from_slice(bytes).map_err(|e| ZimaError::DecodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Dummy {
        a: u32,
        b: String,
    }

    #[test]
    fn roundtrip_compact() {
        let original = Dummy { a: 42, b: "hello".into() };
        let packed = to_msgpack(&original).unwrap();
        let back: Dummy = from_msgpack(&packed).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn roundtrip_named() {
        let original = Dummy { a: 99, b: "world".into() };
        let packed = to_msgpack_named(&original).unwrap();
        let back: Dummy = from_msgpack(&packed).unwrap();
        assert_eq!(original, back);
    }
}
