//! Optional authenticated encryption for persistent thingd storage.

#![allow(clippy::redundant_pub_crate)]

use std::path::Path;
use std::sync::Arc;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{ThingdError, ThingdResult};

const MANIFEST: &[u8] = b"THINGD_ENCRYPTED_V1\n";
const CHECK_PLAINTEXT: &[u8] = b"thingd encryption check v1";
const FORMAT_VERSION: u8 = 1;

/// Supplies the key used to open an encrypted database.
pub trait KeyProvider: Send + Sync {
    /// Resolve a 32-byte encryption key.
    ///
    /// # Errors
    ///
    /// Returns an error when the key cannot be resolved or is invalid.
    fn key(&self) -> ThingdResult<[u8; 32]>;
}

/// A key provider backed by a caller-supplied 32-byte key.
#[derive(Clone)]
pub struct StaticKeyProvider {
    key: [u8; 32],
}

impl StaticKeyProvider {
    /// Construct a provider from exactly 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when `key` is not exactly 32 bytes.
    pub fn new(key: &[u8]) -> ThingdResult<Self> {
        let key: [u8; 32] = key.try_into().map_err(|_| {
            ThingdError::InvalidEncryptionKey("encryption key must be exactly 32 bytes".to_string())
        })?;
        Ok(Self { key })
    }
}

impl KeyProvider for StaticKeyProvider {
    fn key(&self) -> ThingdResult<[u8; 32]> {
        Ok(self.key)
    }
}

/// Encryption configuration supplied at database-open time.
#[derive(Clone)]
pub struct EncryptionConfig {
    /// Provider that resolves the database key.
    pub key_provider: Arc<dyn KeyProvider>,
}

impl EncryptionConfig {
    /// Construct a configuration from a raw 32-byte key.
    ///
    /// # Errors
    ///
    /// Returns an error when `key` is not exactly 32 bytes.
    pub fn from_key(key: &[u8]) -> ThingdResult<Self> {
        Ok(Self {
            key_provider: Arc::new(StaticKeyProvider::new(key)?),
        })
    }
}

#[derive(Clone)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct StorageCrypto {
    data_key: [u8; 32],
    index_key: [u8; 32],
}

/// Internal codec boundary shared by persistent storage adapters.
pub(crate) trait StorageCodec: Send + Sync {
    fn encode_value(&self, domain: &str, value: &[u8]) -> ThingdResult<Vec<u8>>;
    fn decode_value(&self, domain: &str, value: &[u8]) -> ThingdResult<Vec<u8>>;
    #[allow(dead_code)]
    fn encode_key(&self, domain: &str, key: &[u8]) -> Vec<u8>;
    fn encrypted(&self) -> bool;
}

pub(crate) struct RawStorageCodec;

impl StorageCodec for RawStorageCodec {
    fn encode_value(&self, _domain: &str, value: &[u8]) -> ThingdResult<Vec<u8>> {
        Ok(value.to_vec())
    }

    fn decode_value(&self, _domain: &str, value: &[u8]) -> ThingdResult<Vec<u8>> {
        Ok(value.to_vec())
    }

    fn encode_key(&self, _domain: &str, key: &[u8]) -> Vec<u8> {
        key.to_vec()
    }

    fn encrypted(&self) -> bool {
        false
    }
}

pub(crate) struct EncryptedStorageCodec {
    crypto: StorageCrypto,
}

impl StorageCodec for EncryptedStorageCodec {
    fn encode_value(&self, domain: &str, value: &[u8]) -> ThingdResult<Vec<u8>> {
        self.crypto.encrypt(value, domain)
    }

    fn decode_value(&self, domain: &str, value: &[u8]) -> ThingdResult<Vec<u8>> {
        self.crypto.decrypt(value, domain)
    }

    fn encode_key(&self, domain: &str, key: &[u8]) -> Vec<u8> {
        self.crypto.hash_key(domain, key)
    }

    fn encrypted(&self) -> bool {
        true
    }
}

pub(crate) fn make_codec(crypto: Option<StorageCrypto>) -> Box<dyn StorageCodec> {
    match crypto {
        Some(crypto) => Box::new(EncryptedStorageCodec { crypto }),
        None => Box::new(RawStorageCodec),
    }
}

impl StorageCrypto {
    pub(crate) fn open(
        path: &Path,
        config: Option<&EncryptionConfig>,
    ) -> ThingdResult<Option<Self>> {
        let marker = path.join(".thingd-encryption");
        let exists = marker.exists();
        if exists && config.is_none() {
            return Err(ThingdError::EncryptionRequired(
                "database requires an encryption key".to_string(),
            ));
        }

        let Some(config) = config else {
            return Ok(None);
        };

        let has_existing_data = path.is_dir()
            && std::fs::read_dir(path)
                .map_err(|e| ThingdError::Storage(e.to_string()))?
                .next()
                .transpose()
                .map_err(|e| ThingdError::Storage(e.to_string()))?
                .is_some();
        if !exists && has_existing_data {
            // A key supplied for an existing unencrypted database must not
            // silently convert it or make plaintext records unreadable.
            return Ok(None);
        }

        let key = config.key_provider.key()?;
        let crypto = Self::from_key(key);

        if exists {
            let found = std::fs::read(&marker).map_err(|e| ThingdError::Storage(e.to_string()))?;
            if found != MANIFEST {
                return Err(ThingdError::UnsupportedEncryptionVersion(
                    "unknown encryption manifest".to_string(),
                ));
            }
            let check_path = path.join(".thingd-encryption-check");
            let check =
                std::fs::read(&check_path).map_err(|e| ThingdError::Storage(e.to_string()))?;
            let decrypted = crypto.decrypt(&check, "manifest")?;
            if decrypted != CHECK_PLAINTEXT {
                return Err(ThingdError::EncryptionAuthentication(
                    "encryption key authentication failed".to_string(),
                ));
            }
            return Ok(Some(crypto));
        }

        std::fs::create_dir_all(path).map_err(|e| ThingdError::Storage(e.to_string()))?;
        std::fs::write(&marker, MANIFEST).map_err(|e| ThingdError::Storage(e.to_string()))?;
        std::fs::write(
            path.join(".thingd-encryption-check"),
            crypto.encrypt(CHECK_PLAINTEXT, "manifest")?,
        )
        .map_err(|e| ThingdError::Storage(e.to_string()))?;
        Ok(Some(crypto))
    }

    fn from_key(key: [u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::new(None, &key);
        let mut data_key = [0; 32];
        let mut index_key = [0; 32];
        hk.expand(b"thingd/data/v1", &mut data_key)
            .expect("fixed HKDF output");
        hk.expand(b"thingd/index/v1", &mut index_key)
            .expect("fixed HKDF output");
        Self {
            data_key,
            index_key,
        }
    }

    pub(crate) fn encrypt(&self, plaintext: &[u8], domain: &str) -> ThingdResult<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&self.data_key));
        let mut nonce = [0_u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| ThingdError::Storage(e.to_string()))?;
        let associated = format!("thingd:{FORMAT_VERSION}:{domain}");
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: associated.as_bytes(),
                },
            )
            .map_err(|_| ThingdError::EncryptionAuthentication("encryption failed".to_string()))?;
        let mut output = Vec::with_capacity(1 + nonce.len() + ciphertext.len());
        output.push(FORMAT_VERSION);
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    pub(crate) fn decrypt(&self, ciphertext: &[u8], domain: &str) -> ThingdResult<Vec<u8>> {
        if ciphertext.len() < 1 + 24 + 16 || ciphertext[0] != FORMAT_VERSION {
            return Err(ThingdError::UnsupportedEncryptionVersion(
                "invalid encrypted value envelope".to_string(),
            ));
        }
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&self.data_key));
        let associated = format!("thingd:{FORMAT_VERSION}:{domain}");
        cipher
            .decrypt(
                XNonce::from_slice(&ciphertext[1..25]),
                Payload {
                    msg: &ciphertext[25..],
                    aad: associated.as_bytes(),
                },
            )
            .map_err(|_| {
                ThingdError::EncryptionAuthentication(
                    "encrypted value authentication failed".to_string(),
                )
            })
    }

    /// Derive a stable opaque key for a logical storage key.
    #[allow(dead_code)]
    pub(crate) fn hash_key(&self, domain: &str, key: &[u8]) -> Vec<u8> {
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(&self.index_key).expect("fixed HMAC key");
        mac.update(domain.as_bytes());
        mac.update(b"\0");
        mac.update(key);
        mac.finalize().into_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_nonce_uniqueness() {
        let crypto = StorageCrypto::from_key([7; 32]);
        let first = crypto.encrypt(b"secret", "test").unwrap();
        let second = crypto.encrypt(b"secret", "test").unwrap();
        assert_ne!(first, second);
        assert_eq!(crypto.decrypt(&first, "test").unwrap(), b"secret");
        assert!(crypto.decrypt(&first, "other").is_err());
    }

    #[test]
    fn tampering_and_wrong_key_fail_authentication() {
        let crypto = StorageCrypto::from_key([1; 32]);
        let mut value = crypto.encrypt(b"secret", "test").unwrap();
        value[30] ^= 1;
        assert!(matches!(
            crypto.decrypt(&value, "test"),
            Err(ThingdError::EncryptionAuthentication(_))
        ));
        let wrong = StorageCrypto::from_key([2; 32]);
        let value = crypto.encrypt(b"secret", "test").unwrap();
        assert!(matches!(
            wrong.decrypt(&value, "test"),
            Err(ThingdError::EncryptionAuthentication(_))
        ));
    }
}
