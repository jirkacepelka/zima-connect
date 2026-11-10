//! X25519 key-pair generation and device ID derivation.
//!
//! Each Zima Connect device maintains a long-lived X25519 key pair:
//!
//! * The **private key** is kept in secure storage and used for ECDH during
//!   pairing.
//! * The **public key** is freely shared with peers and serves as the basis
//!   for the device's unique [`DeviceId`].
//!
//! A [`DeviceId`] is the SHA-256 hash of the raw 32-byte public key, encoded
//! as a 64-character lowercase hex string.

use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

use crate::protocol::types::DeviceId;

/// A long-lived X25519 key pair used for device identity and ECDH.
///
/// The private key is [`Zeroize`]-on-drop to reduce the window of exposure
/// in memory.
pub struct DeviceKeyPair {
    /// The X25519 static secret (private key).
    secret: StaticSecret,
    /// The corresponding public key.
    public: PublicKey,
}

impl DeviceKeyPair {
    /// Generate a fresh random key pair using the OS CSPRNG.
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Reconstruct a key pair from a raw 32-byte private key.
    ///
    /// This is used when loading a previously persisted key from storage.
    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Return a reference to the static secret for ECDH operations.
    pub fn secret(&self) -> &StaticSecret {
        &self.secret
    }

    /// Return a reference to the public key.
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Export the raw 32-byte public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Export the raw 32-byte private key.
    ///
    /// **Security note**: handle the return value with care — it is the
    /// device's long-term secret.
    pub fn secret_key_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// Derive the [`DeviceId`] from the public key.
    ///
    /// `DeviceId = lowercase_hex(SHA-256(public_key_bytes))`
    pub fn device_id(&self) -> DeviceId {
        let hash = Sha256::digest(self.public.as_bytes());
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash);
        DeviceId::from_bytes(&arr)
    }
}

impl Drop for DeviceKeyPair {
    fn drop(&mut self) {
        // StaticSecret already implements Zeroize, but we ensure the public
        // key bytes are also wiped.
        let mut pub_bytes = self.public.to_bytes();
        pub_bytes.zeroize();
    }
}

impl std::fmt::Debug for DeviceKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeyPair")
            .field("device_id", &self.device_id())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_device_id() {
        let kp = DeviceKeyPair::generate();
        let id = kp.device_id();
        // 32 bytes → 64 hex chars
        assert_eq!(id.as_hex().len(), 64);
    }

    #[test]
    fn roundtrip_from_bytes() {
        let kp1 = DeviceKeyPair::generate();
        let secret_bytes = kp1.secret_key_bytes();
        let kp2 = DeviceKeyPair::from_secret_bytes(secret_bytes);
        assert_eq!(kp1.public_key_bytes(), kp2.public_key_bytes());
        assert_eq!(kp1.device_id(), kp2.device_id());
    }

    #[test]
    fn different_keys_different_ids() {
        let a = DeviceKeyPair::generate();
        let b = DeviceKeyPair::generate();
        assert_ne!(a.device_id(), b.device_id());
    }
}
