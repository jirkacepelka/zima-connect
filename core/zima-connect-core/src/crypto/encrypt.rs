//! AES-256-GCM authenticated encryption.
//!
//! Provides a simple encrypt / decrypt API around AES-256-GCM.  A random
//! 96-bit nonce is generated for each encryption and **prepended** to the
//! ciphertext so the decryptor can extract it without out-of-band state.
//!
//! # Wire format
//!
//! ```text
//! [ 12-byte nonce | ciphertext + 16-byte auth tag ]
//! ```

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;

use crate::error::{Result, ZimaError};

/// Nonce length in bytes (96 bits as required by AES-GCM).
pub const NONCE_LEN: usize = 12;

/// AES-256 key length in bytes.
pub const KEY_LEN: usize = 32;

/// Encrypt `plaintext` with a 256-bit `key` using AES-256-GCM.
///
/// A fresh random nonce is generated for every call and prepended to the
/// returned ciphertext.
///
/// # Errors
/// Returns [`ZimaError::InvalidKeyLength`] if the key is not 32 bytes, or
/// [`ZimaError::EncryptionFailed`] on cipher failure.
pub fn encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    if key.len() != KEY_LEN {
        return Err(ZimaError::InvalidKeyLength {
            expected: KEY_LEN,
            actual: key.len(),
        });
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| ZimaError::EncryptionFailed(e.to_string()))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| ZimaError::EncryptionFailed(e.to_string()))?;

    // Prepend nonce to ciphertext.
    let mut output = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt `nonce_and_ciphertext` with a 256-bit `key` using AES-256-GCM.
///
/// Expects the input format produced by [`encrypt`]: a 12-byte nonce
/// followed by ciphertext + auth tag.
///
/// # Errors
/// Returns [`ZimaError::InvalidKeyLength`] if the key is not 32 bytes,
/// [`ZimaError::DecryptionFailed`] if the data is too short or
/// authentication fails.
pub fn decrypt(key: &[u8], nonce_and_ciphertext: &[u8]) -> Result<Vec<u8>> {
    if key.len() != KEY_LEN {
        return Err(ZimaError::InvalidKeyLength {
            expected: KEY_LEN,
            actual: key.len(),
        });
    }

    if nonce_and_ciphertext.len() < NONCE_LEN {
        return Err(ZimaError::DecryptionFailed(
            "input too short to contain nonce".into(),
        ));
    }

    let (nonce_bytes, ciphertext) = nonce_and_ciphertext.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| ZimaError::DecryptionFailed(e.to_string()))?;

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| ZimaError::DecryptionFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let plaintext = b"hello, Zima Connect!";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails() {
        let key1 = [0x01u8; 32];
        let key2 = [0x02u8; 32];
        let encrypted = encrypt(&key1, b"secret").unwrap();
        let result = decrypt(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn bad_key_length() {
        assert!(encrypt(&[0u8; 16], b"data").is_err());
        assert!(decrypt(&[0u8; 16], &[0u8; 28]).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [0xAA; 32];
        let mut encrypted = encrypt(&key, b"payload").unwrap();
        // Flip a byte in the ciphertext portion.
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;
        assert!(decrypt(&key, &encrypted).is_err());
    }

    #[test]
    fn unique_nonces() {
        let key = [0xBB; 32];
        let a = encrypt(&key, b"same").unwrap();
        let b = encrypt(&key, b"same").unwrap();
        // The nonce prefix should differ.
        assert_ne!(&a[..NONCE_LEN], &b[..NONCE_LEN]);
    }
}
