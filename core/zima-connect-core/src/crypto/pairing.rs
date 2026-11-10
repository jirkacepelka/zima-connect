//! ECDH pairing, session key derivation, and SAS verification codes.
//!
//! # Pairing flow overview
//!
//! 1. Both devices exchange their X25519 **public keys** out-of-band (e.g.
//!    via discovery broadcast).
//! 2. Each side computes the ECDH **shared secret** from its own private key
//!    and the peer's public key.
//! 3. The shared secret is fed through HKDF-SHA256 to derive a 256-bit
//!    **session key** bound to both parties.
//! 4. A short numeric **SAS code** is derived from the session key so the
//!    user can visually compare the two devices and confirm no MITM.
//! 5. Once confirmed, the session key is used as the AES-256-GCM symmetric
//!    key for subsequent envelope encryption.

use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::crypto::keys::DeviceKeyPair;
use crate::error::{Result, ZimaError};

/// Application-specific HKDF info string for session key derivation.
const HKDF_INFO: &[u8] = b"zima-connect-session-key-v1";

/// Default SAS code length (6 decimal digits → 0..999_999).
const SAS_DIGITS: usize = 6;

/// Perform an X25519 Diffie-Hellman exchange and return the 32-byte raw
/// shared secret.
///
/// Both sides must call this with **their own** secret and **the peer's**
/// public key; they will arrive at the same value.
pub fn ecdh_shared_secret(our_secret: &StaticSecret, peer_public: &PublicKey) -> [u8; 32] {
    our_secret.diffie_hellman(peer_public).to_bytes()
}

/// Derive a 256-bit session key from the raw ECDH shared secret using
/// HKDF-SHA256.
///
/// `salt` is formed by sorting and concatenating the two device IDs (hex
/// strings), ensuring both sides produce the same key regardless of who
/// initiated.
///
/// # Errors
/// Returns [`ZimaError::KeyDerivationFailed`] if HKDF expansion fails (this
/// should never happen for a 32-byte output).
pub fn derive_session_key(
    shared_secret: &[u8; 32],
    our_device_id_hex: &str,
    peer_device_id_hex: &str,
) -> Result<[u8; 32]> {
    // Canonical salt: sorted concatenation of both IDs.
    let salt = if our_device_id_hex <= peer_device_id_hex {
        format!("{}{}", our_device_id_hex, peer_device_id_hex)
    } else {
        format!("{}{}", peer_device_id_hex, our_device_id_hex)
    };

    let hk = Hkdf::<Sha256>::new(Some(salt.as_bytes()), shared_secret);

    let mut okm = [0u8; 32];
    hk.expand(HKDF_INFO, &mut okm)
        .map_err(|e| ZimaError::KeyDerivationFailed(e.to_string()))?;

    Ok(okm)
}

/// Derive a human-readable decimal SAS verification code from a session key.
///
/// The code is [`SAS_DIGITS`] digits long (zero-padded), e.g. `"042519"`.
/// Both devices display this code and the user confirms they match.
pub fn sas_code(session_key: &[u8; 32]) -> String {
    // Use the first 4 bytes of SHA-256(session_key) as a uint32, then
    // truncate to SAS_DIGITS decimal digits.
    use sha2::Digest;
    let hash = Sha256::digest(session_key);
    let n = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
    let modulus = 10u32.pow(SAS_DIGITS as u32);
    format!("{:0>width$}", n % modulus, width = SAS_DIGITS)
}

/// Convenience: run the full pairing key derivation from two [`DeviceKeyPair`]s.
///
/// Returns `(session_key, sas_code)`.
pub fn perform_pairing(
    our_keys: &DeviceKeyPair,
    peer_public_bytes: &[u8; 32],
) -> Result<([u8; 32], String)> {
    let peer_pub = PublicKey::from(*peer_public_bytes);
    let shared = ecdh_shared_secret(our_keys.secret(), &peer_pub);

    let our_id = our_keys.device_id();
    let peer_kp_tmp = DeviceKeyPair::from_secret_bytes([0u8; 32]); // dummy, unused
    // We need the peer's DeviceId. Derive it the same way.
    let peer_id = {
        use sha2::Digest;
        let hash = Sha256::digest(peer_public_bytes);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash);
        crate::protocol::types::DeviceId::from_bytes(&arr)
    };
    let _ = peer_kp_tmp; // drop the dummy

    let session_key = derive_session_key(&shared, our_id.as_hex(), peer_id.as_hex())?;
    let code = sas_code(&session_key);

    Ok((session_key, code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keys::DeviceKeyPair;

    #[test]
    fn ecdh_symmetric() {
        let alice = DeviceKeyPair::generate();
        let bob = DeviceKeyPair::generate();

        let secret_ab = ecdh_shared_secret(alice.secret(), bob.public_key());
        let secret_ba = ecdh_shared_secret(bob.secret(), alice.public_key());
        assert_eq!(secret_ab, secret_ba);
    }

    #[test]
    fn session_key_is_symmetric() {
        let alice = DeviceKeyPair::generate();
        let bob = DeviceKeyPair::generate();

        let shared = ecdh_shared_secret(alice.secret(), bob.public_key());

        let key_a = derive_session_key(
            &shared,
            alice.device_id().as_hex(),
            bob.device_id().as_hex(),
        )
        .unwrap();
        let key_b = derive_session_key(
            &shared,
            bob.device_id().as_hex(),
            alice.device_id().as_hex(),
        )
        .unwrap();

        assert_eq!(key_a, key_b);
    }

    #[test]
    fn sas_code_length_and_determinism() {
        let key = [0xAB; 32];
        let code = sas_code(&key);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        // Deterministic.
        assert_eq!(code, sas_code(&key));
    }

    #[test]
    fn perform_pairing_symmetric() {
        let alice = DeviceKeyPair::generate();
        let bob = DeviceKeyPair::generate();

        let (key_a, code_a) = perform_pairing(&alice, &bob.public_key_bytes()).unwrap();
        let (key_b, code_b) = perform_pairing(&bob, &alice.public_key_bytes()).unwrap();

        assert_eq!(key_a, key_b);
        assert_eq!(code_a, code_b);
    }
}
