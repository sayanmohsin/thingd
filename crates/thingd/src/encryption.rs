//! Authenticated encryption primitives for persistent Thingd storage.

use ring::aead::{self, AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

const ENVELOPE_MAGIC: &[u8; 4] = b"TDE1";
const NONCE_LEN: usize = 12;

/// A 256-bit key used to encrypt persistent Thingd values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageKey(pub [u8; 32]);

impl StorageKey {
    /// Construct a storage key from exactly 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parse a 64-character hexadecimal key, suitable for environment-based
    /// sidecar configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the input is not exactly 64 hexadecimal characters.
    pub fn from_hex(value: &str) -> Result<Self, String> {
        if value.len() != 64 {
            return Err("encryption key must contain exactly 64 hexadecimal characters".into());
        }
        let mut bytes = [0u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| "encryption key must be hexadecimal".to_string())?;
        }
        Ok(Self(bytes))
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct StorageCipher {
    key: StorageKey,
}

impl StorageCipher {
    pub(crate) const fn new(key: StorageKey) -> Self {
        Self { key }
    }

    fn key(&self) -> Result<LessSafeKey, String> {
        let unbound = UnboundKey::new(&AES_256_GCM, &self.key.0)
            .map_err(|_| "invalid encryption key".to_string())?;
        Ok(LessSafeKey::new(unbound))
    }

    pub(crate) fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| "failed to generate encryption nonce".to_string())?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = plaintext.to_vec();
        self.key()?
            .seal_in_place_append_tag(nonce, Aad::from(ENVELOPE_MAGIC), &mut ciphertext)
            .map_err(|_| "failed to encrypt persistent value".to_string())?;

        let mut envelope = Vec::with_capacity(ENVELOPE_MAGIC.len() + NONCE_LEN + ciphertext.len());
        envelope.extend_from_slice(ENVELOPE_MAGIC);
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&ciphertext);
        Ok(envelope)
    }

    pub(crate) fn decrypt(&self, envelope: &[u8]) -> Result<Vec<u8>, String> {
        if envelope.len() < ENVELOPE_MAGIC.len() + NONCE_LEN + aead::AES_256_GCM.tag_len()
            || &envelope[..ENVELOPE_MAGIC.len()] != ENVELOPE_MAGIC
        {
            return Err("encrypted value has an invalid format".to_string());
        }

        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes
            .copy_from_slice(&envelope[ENVELOPE_MAGIC.len()..ENVELOPE_MAGIC.len() + NONCE_LEN]);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = envelope[ENVELOPE_MAGIC.len() + NONCE_LEN..].to_vec();
        let plaintext = self
            .key()?
            .open_in_place(nonce, Aad::from(ENVELOPE_MAGIC), &mut ciphertext)
            .map_err(|_| {
                "unable to decrypt persistent value; the key may be wrong or the data corrupted"
                    .to_string()
            })?;
        Ok(plaintext.to_vec())
    }
}
