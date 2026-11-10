//! Ed25519 digital signatures.
//!
//! Each device has an Ed25519 signing key pair (separate from the X25519
//! identity key pair) used to authenticate envelopes.  Signatures are
//! computed over the canonical [`Envelope::signable_bytes`] representation.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

use crate::error::{Result, ZimaError};

/// An Ed25519 signing key pair.
///
/// The secret key is 32 bytes; the public (verifying) key is also 32 bytes.
pub struct SigningKeyPair {
    inner: SigningKey,
}

impl SigningKeyPair {
    /// Generate a fresh random Ed25519 signing key pair.
    pub fn generate() -> Self {
        Self {
            inner: SigningKey::generate(&mut OsRng),
        }
    }

    /// Reconstruct from a raw 32-byte secret seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            inner: SigningKey::from_bytes(&seed),
        }
    }

    /// Return the raw 32-byte secret seed.
    ///
    /// **Security note**: protect this value at rest.
    pub fn seed(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Return the 32-byte public (verifying) key.
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.inner.verifying_key().to_bytes()
    }

    /// Return the [`VerifyingKey`] for this pair.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.inner.verifying_key()
    }

    /// Sign arbitrary `message` bytes, returning the 64-byte Ed25519
    /// signature.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let sig: Signature = self.inner.sign(message);
        sig.to_bytes().to_vec()
    }
}

impl std::fmt::Debug for SigningKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningKeyPair")
            .field("verifying_key", &hex::encode(self.verifying_key_bytes()))
            .finish_non_exhaustive()
    }
}

/// Verify an Ed25519 `signature` over `message` using the supplied public
/// (verifying) key bytes.
///
/// # Errors
/// Returns [`ZimaError::SignatureError`] if the key is malformed or the
/// signature is invalid.
pub fn verify(verifying_key_bytes: &[u8; 32], message: &[u8], signature: &[u8]) -> Result<()> {
    let vk = VerifyingKey::from_bytes(verifying_key_bytes)?;

    let sig =
        Signature::from_slice(signature).map_err(|e| ZimaError::SignatureError(e.to_string()))?;

    vk.verify(message, &sig)
        .map_err(|e| ZimaError::SignatureError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let kp = SigningKeyPair::generate();
        let msg = b"envelope-bytes-here";
        let sig = kp.sign(msg);

        assert_eq!(sig.len(), 64);
        verify(&kp.verifying_key_bytes(), msg, &sig).unwrap();
    }

    #[test]
    fn wrong_message_fails() {
        let kp = SigningKeyPair::generate();
        let sig = kp.sign(b"correct");
        let result = verify(&kp.verifying_key_bytes(), b"wrong", &sig);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let kp1 = SigningKeyPair::generate();
        let kp2 = SigningKeyPair::generate();
        let sig = kp1.sign(b"msg");
        let result = verify(&kp2.verifying_key_bytes(), b"msg", &sig);
        assert!(result.is_err());
    }

    #[test]
    fn seed_roundtrip() {
        let kp = SigningKeyPair::generate();
        let seed = kp.seed();
        let kp2 = SigningKeyPair::from_seed(seed);
        let msg = b"test";
        let sig = kp.sign(msg);
        verify(&kp2.verifying_key_bytes(), msg, &sig).unwrap();
    }
}
