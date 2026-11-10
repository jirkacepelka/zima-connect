//! End-to-end sealing for items the server keeps for offline devices.
//!
//! The sender encrypts to the recipient device's public key (its iroh Ed25519 identity,
//! converted to X25519), so the server only ever stores ciphertext it cannot read.
//! Large bodies are sealed in 64 KiB chunks (AES-256-GCM, counter nonces, the last chunk
//! is marked in the associated data so truncation is detected).

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, bail, Result};
use hkdf::Hkdf;
use iroh::{PublicKey, SecretKey};
use sha2::{Digest, Sha256};
use x25519_dalek::StaticSecret;

pub const CHUNK: usize = 64 * 1024;
/// Ephemeral X25519 public key + 4-byte nonce prefix.
pub const HEADER: usize = 36;
const TAG: usize = 16;
const INFO: &[u8] = b"zima-connect-seal-v1";

fn x_public(pk: &PublicKey) -> Result<x25519_dalek::PublicKey> {
    let vk = ed25519_dalek::VerifyingKey::from_bytes(pk.as_bytes())
        .map_err(|_| anyhow!("invalid device key"))?;
    Ok(x25519_dalek::PublicKey::from(vk.to_montgomery().to_bytes()))
}

fn x_secret(sk: &SecretKey) -> StaticSecret {
    let s = ed25519_dalek::SigningKey::from_bytes(&sk.to_bytes());
    StaticSecret::from(s.to_scalar_bytes())
}

fn cipher(shared: &[u8; 32], eph: &[u8; 32], rcpt: &[u8; 32]) -> Result<Aes256Gcm> {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(eph);
    salt[32..].copy_from_slice(rcpt);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut key = [0u8; 32];
    hk.expand(INFO, &mut key)
        .map_err(|_| anyhow!("key derivation failed"))?;
    Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow!("bad key"))
}

fn nonce(prefix: &[u8; 4], counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..4].copy_from_slice(prefix);
    n[4..].copy_from_slice(&counter.to_be_bytes());
    n
}

/// Size on the wire of a sealed body with `plain` bytes of content.
pub fn sealed_size(plain: u64) -> u64 {
    let chunks = plain.div_ceil(CHUNK as u64).max(1);
    HEADER as u64 + chunks * (4 + TAG as u64) + plain
}

pub struct Sealer {
    cipher: Aes256Gcm,
    prefix: [u8; 4],
    counter: u64,
    pub header: [u8; HEADER],
}

impl Sealer {
    pub fn new(recipient: &PublicKey) -> Result<Self> {
        let rpub = x_public(recipient)?;
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
        let eph = StaticSecret::from(seed);
        let eph_pub = x25519_dalek::PublicKey::from(&eph);
        let shared = eph.diffie_hellman(&rpub);
        let mut prefix = [0u8; 4];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut prefix);
        let mut header = [0u8; HEADER];
        header[..32].copy_from_slice(eph_pub.as_bytes());
        header[32..].copy_from_slice(&prefix);
        Ok(Self {
            cipher: cipher(shared.as_bytes(), eph_pub.as_bytes(), rpub.as_bytes())?,
            prefix,
            counter: 0,
            header,
        })
    }

    /// Encrypt one chunk; returns `len(u32 BE) || ciphertext`.
    pub fn seal(&mut self, chunk: &[u8], last: bool) -> Result<Vec<u8>> {
        let n = nonce(&self.prefix, self.counter);
        self.counter += 1;
        let ct = self
            .cipher
            .encrypt(
                Nonce::from_slice(&n),
                Payload {
                    msg: chunk,
                    aad: &[last as u8],
                },
            )
            .map_err(|_| anyhow!("encryption failed"))?;
        let mut out = Vec::with_capacity(4 + ct.len());
        out.extend_from_slice(&(ct.len() as u32).to_be_bytes());
        out.extend_from_slice(&ct);
        Ok(out)
    }
}

pub struct Opener {
    cipher: Aes256Gcm,
    prefix: [u8; 4],
    counter: u64,
}

impl Opener {
    pub fn new(me: &SecretKey, header: &[u8]) -> Result<Self> {
        if header.len() != HEADER {
            bail!("corrupted item");
        }
        let mut eph = [0u8; 32];
        eph.copy_from_slice(&header[..32]);
        let mut prefix = [0u8; 4];
        prefix.copy_from_slice(&header[32..]);
        let secret = x_secret(me);
        let my_pub = x25519_dalek::PublicKey::from(&secret);
        let shared = secret.diffie_hellman(&x25519_dalek::PublicKey::from(eph));
        Ok(Self {
            cipher: cipher(shared.as_bytes(), &eph, my_pub.as_bytes())?,
            prefix,
            counter: 0,
        })
    }

    /// Largest ciphertext chunk accepted.
    pub const MAX_CT: usize = CHUNK + TAG;

    pub fn open(&mut self, ct: &[u8], last: bool) -> Result<Vec<u8>> {
        let n = nonce(&self.prefix, self.counter);
        self.counter += 1;
        self.cipher
            .decrypt(
                Nonce::from_slice(&n),
                Payload {
                    msg: ct,
                    aad: &[last as u8],
                },
            )
            .map_err(|_| anyhow!("item could not be decrypted (corrupted or not for this device)"))
    }
}

/// Seal a small blob in one piece: `header || len || ciphertext`.
pub fn seal_bytes(recipient: &PublicKey, data: &[u8]) -> Result<Vec<u8>> {
    let mut s = Sealer::new(recipient)?;
    let mut out = s.header.to_vec();
    out.extend(s.seal(data, true)?);
    Ok(out)
}

pub fn open_bytes(me: &SecretKey, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < HEADER + 4 {
        bail!("corrupted item");
    }
    let mut o = Opener::new(me, &blob[..HEADER])?;
    let len = u32::from_be_bytes(blob[HEADER..HEADER + 4].try_into().unwrap()) as usize;
    let ct = blob
        .get(HEADER + 4..HEADER + 4 + len)
        .ok_or_else(|| anyhow!("corrupted item"))?;
    o.open(ct, true)
}

pub fn header_hash(header: &[u8]) -> String {
    Sha256::digest(header)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_key() {
        let alice = SecretKey::generate();
        let bob = SecretKey::generate();
        let blob = seal_bytes(&bob.public(), b"hello mom").unwrap();
        assert_eq!(open_bytes(&bob, &blob).unwrap(), b"hello mom");
        assert!(open_bytes(&alice, &blob).is_err());
        assert!(!blob.windows(9).any(|w| w == b"hello mom"));
    }

    #[test]
    fn chunks_detect_truncation() {
        let bob = SecretKey::generate();
        let mut s = Sealer::new(&bob.public()).unwrap();
        let c1 = s.seal(&[1u8; CHUNK], false).unwrap();
        let c2 = s.seal(&[2u8; 10], true).unwrap();
        let mut o = Opener::new(&bob, &s.header).unwrap();
        assert_eq!(o.open(&c1[4..], false).unwrap(), vec![1u8; CHUNK]);
        assert_eq!(o.open(&c2[4..], true).unwrap(), vec![2u8; 10]);
        // A stream cut after the first chunk cannot pass as complete.
        let mut o = Opener::new(&bob, &s.header).unwrap();
        assert!(o.open(&c1[4..], true).is_err());
        assert_eq!(
            sealed_size(CHUNK as u64 + 10),
            (HEADER + 2 * 20 + CHUNK + 10) as u64
        );
    }
}
