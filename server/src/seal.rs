//! Encryption of stored account credentials.
//!
//! Each phone generates a random 256-bit key and keeps it in its own
//! keystore. The server receives it over HTTPS when the phone connects,
//! keeps it only in memory, and uses it to seal (AES-256-GCM) the account
//! settings it stores. A copy of the database alone does not reveal any
//! password.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rand::RngCore;
use zeroize::Zeroizing;

/// A device's key. Wiped from memory when dropped.
#[derive(Clone)]
pub struct DeviceKey(Zeroizing<[u8; 32]>);

impl DeviceKey {
    /// From the base64 text the phone sends.
    pub fn from_base64(text: &str) -> Result<Self> {
        let bytes = Zeroizing::new(STANDARD.decode(text.trim()).context("key is not base64")?);
        let key: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("key must be 32 bytes"))?;
        Ok(DeviceKey(Zeroizing::new(key)))
    }

    #[cfg(test)]
    pub fn random() -> Self {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        DeviceKey(Zeroizing::new(key))
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(self.0.as_slice()).expect("32-byte key")
    }

    /// Encrypts `plain`, bound to `context` (the account id): a sealed blob
    /// cannot be swapped onto another account. Returns (nonce, ciphertext).
    pub fn seal(&self, plain: &[u8], context: &str) -> Result<([u8; 12], Vec<u8>)> {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let sealed = self
            .cipher()
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: context.as_bytes(),
                },
            )
            .map_err(|_| anyhow!("encryption failed"))?;
        Ok((nonce, sealed))
    }

    /// Decrypts; fails with a wrong key or a tampered blob.
    pub fn open(&self, nonce: &[u8], sealed: &[u8], context: &str) -> Result<Zeroizing<Vec<u8>>> {
        if nonce.len() != 12 {
            return Err(anyhow!("bad nonce"));
        }
        self.cipher()
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: context.as_bytes(),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| anyhow!("wrong key"))
    }
}

/// Random token for a device, as URL-safe text.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open() {
        let key = DeviceKey::random();
        let (nonce, sealed) = key.seal(b"secret", "acc-1").unwrap();
        assert_eq!(key.open(&nonce, &sealed, "acc-1").unwrap().as_slice(), b"secret");
        // Wrong account, wrong key, tampered data: all refused.
        assert!(key.open(&nonce, &sealed, "acc-2").is_err());
        assert!(DeviceKey::random().open(&nonce, &sealed, "acc-1").is_err());
        let mut bad = sealed.clone();
        bad[0] ^= 1;
        assert!(key.open(&nonce, &bad, "acc-1").is_err());
    }

    #[test]
    fn key_from_base64() {
        assert!(DeviceKey::from_base64(&STANDARD.encode([7u8; 32])).is_ok());
        assert!(DeviceKey::from_base64(&STANDARD.encode([7u8; 16])).is_err());
        assert!(DeviceKey::from_base64("not base64!").is_err());
    }
}
