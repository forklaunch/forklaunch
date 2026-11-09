//! TypeScript-compatible field encryption, key rotation, and blind indexes.
//!
//! `encrypt` preserves deterministic v2/v3 rows for equality-query compatibility.
//! Prefer random-nonce `seal` and separate blind indexes for new compliant fields.
//! Master keys must contain high-entropy secret material and remain application-owned.
use crate::common::{FrameworkError, FrameworkResult};
use aes_gcm::{
    Aes256Gcm, Nonce, Tag,
    aead::{AeadInOut, KeyInit},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiphertextFormat {
    V2,
    V3,
}

#[derive(Clone)]
struct KeyMaterial {
    id: String,
    master: Zeroizing<String>,
}

/// The returned anonymous value is clear metadata in the existing TS v4 format;
/// it must never be used as an authenticated authorization/session claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedCiphertext {
    pub plaintext: String,
    pub key_id: String,
    pub current: bool,
    pub version: String,
    pub anon: Option<String>,
    pub stale: bool,
}

/// Does not implement Debug so key material cannot be printed accidentally.
#[derive(Clone)]
pub struct FieldEncryptor {
    ring: Vec<KeyMaterial>,
    format: CiphertextFormat,
}

fn crypto_error() -> FrameworkError {
    FrameworkError::new(
        "decryption_failed",
        "Ciphertext could not be authenticated or decoded",
    )
}

#[must_use]
pub fn encryption_key_id(master: &str) -> String {
    Sha256::digest(master.as_bytes())
        .iter()
        .take(6)
        .fold(String::new(), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[must_use]
pub fn parse_encryption_key_list(sources: &[&str]) -> Vec<String> {
    let mut keys = Vec::new();
    for source in sources {
        for key in source
            .split(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
        {
            let key = key.trim_matches(['\'', '"']);
            if !key.is_empty() && !keys.iter().any(|old| old == key) {
                keys.push(key.to_owned());
            }
        }
    }
    keys
}

/// Detects recognized envelope prefixes without claiming the ciphertext is valid.
#[must_use]
pub fn is_encrypted_ciphertext(value: &str) -> bool {
    ["v1:", "v2:", "v3:", "v4:"]
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

#[must_use]
pub fn stamped_key_id(value: &str) -> Option<&str> {
    let parts: Vec<_> = value.split(':').collect();
    match parts.as_slice() {
        ["v3", id, _, _, _] | ["v4", id, _, _, _, _] | ["bi1", id, _] => Some(id),
        _ => None,
    }
}

/// Reads clear v4 metadata; this does not authenticate or decrypt the envelope.
#[must_use]
pub fn sealed_anon(value: &str) -> Option<String> {
    let parts: Vec<_> = value.split(':').collect();
    match parts.as_slice() {
        ["v4", _, anon, _, _, _] => String::from_utf8(URL_SAFE_NO_PAD.decode(anon).ok()?).ok(),
        _ => None,
    }
}

#[must_use]
pub fn stamped_prefix(key_id: &str) -> String {
    format!("v3:{key_id}:")
}

impl FieldEncryptor {
    /// # Errors
    /// Rejects empty master keys.
    pub fn new(master: impl Into<String>) -> FrameworkResult<Self> {
        let master = master.into();
        if master.is_empty() {
            return Err(FrameworkError::new(
                "missing_encryption_key",
                "Master encryption key is required",
            ));
        }
        Ok(Self {
            ring: vec![KeyMaterial {
                id: encryption_key_id(&master),
                master: Zeroizing::new(master),
            }],
            format: CiphertextFormat::V2,
        })
    }

    /// # Errors
    /// Rejects distinct keys with colliding public fingerprints.
    pub fn with_previous_keys(mut self, previous: &[String]) -> FrameworkResult<Self> {
        self.ring.truncate(1);
        for key in previous {
            if key.is_empty()
                || self
                    .ring
                    .iter()
                    .any(|material| material.master.as_str() == key)
            {
                continue;
            }
            let id = encryption_key_id(key);
            if self.ring.iter().any(|material| material.id == id) {
                return Err(crypto_error());
            }
            self.ring.push(KeyMaterial {
                id,
                master: Zeroizing::new(key.clone()),
            });
        }
        Ok(self)
    }

    #[must_use]
    pub const fn with_format(mut self, format: CiphertextFormat) -> Self {
        self.format = format;
        self
    }

    /// Reads the same key-ring environment settings as the TypeScript framework.
    ///
    /// # Errors
    /// Rejects a missing master key or invalid write format.
    pub fn from_env() -> FrameworkResult<Self> {
        let master = std::env::var("ENCRYPTION_KEY").map_err(|_| {
            FrameworkError::new("missing_encryption_key", "ENCRYPTION_KEY is required")
        })?;
        let format = match std::env::var("ENCRYPTION_FORMAT")
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .as_str()
        {
            "" | "v2" => CiphertextFormat::V2,
            "v3" => CiphertextFormat::V3,
            _ => {
                return Err(FrameworkError::new(
                    "invalid_encryption_format",
                    "ENCRYPTION_FORMAT must be v2 or v3",
                ));
            }
        };
        let previous = std::env::var("LEGACY_ENCRYPTION_KEYS").unwrap_or_default();
        let legacy = std::env::var("LEGACY_ENCRYPTION_KEY").unwrap_or_default();
        Self::new(master)?
            .with_previous_keys(&parse_encryption_key_list(&[&previous, &legacy]))
            .map(|encryptor| encryptor.with_format(format))
    }

    #[must_use]
    pub fn key_ids(&self) -> Vec<&str> {
        self.ring.iter().map(|key| key.id.as_str()).collect()
    }

    fn derive_key(material: &KeyMaterial, context: &str) -> FrameworkResult<Zeroizing<[u8; 32]>> {
        let mut key = Zeroizing::new([0_u8; 32]);
        Hkdf::<Sha256>::new(Some(&[]), material.master.as_bytes())
            .expand(context.as_bytes(), key.as_mut())
            .map_err(|_| crypto_error())?;
        Ok(key)
    }

    /// Writes deterministic, TS-compatible v2/v3 envelopes. New compliant fields
    /// should use `seal` to avoid exposing equality through their ciphertext.
    ///
    /// # Errors
    /// Returns a safe error if key derivation or encryption fails.
    pub fn encrypt(&self, plaintext: &str, tenant: &str) -> FrameworkResult<String> {
        let key = Self::derive_key(&self.ring[0], tenant)?;
        let mut mac =
            <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_ref()).map_err(|_| crypto_error())?;
        mac.update(plaintext.as_bytes());
        // The IV is the leading 96 bits of the HMAC (TS-compatible deterministic envelope).
        let digest = mac.finalize().into_bytes();
        let iv: [u8; 12] = digest[..12].try_into().map_err(|_| crypto_error())?;
        let body = Self::encrypt_body(plaintext, &key, iv)?;
        Ok(match self.format {
            CiphertextFormat::V2 => format!("v2:{body}"),
            CiphertextFormat::V3 => format!("v3:{}:{body}", self.ring[0].id),
        })
    }

    fn encrypt_body(plaintext: &str, key: &[u8; 32], iv: [u8; 12]) -> FrameworkResult<String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| crypto_error())?;
        let mut data = plaintext.as_bytes().to_vec();
        let tag = cipher
            .encrypt_inout_detached(&Nonce::from(iv), b"", data.as_mut_slice().into())
            .map_err(|_| crypto_error())?;
        Ok(format!(
            "{}:{}:{}",
            STANDARD.encode(iv),
            STANDARD.encode(tag),
            STANDARD.encode(data)
        ))
    }

    /// Writes random-nonce v4 envelopes matching TypeScript compliant fields.
    ///
    /// # Errors
    /// Returns a safe error if randomness, derivation, or encryption fails.
    pub fn seal(&self, plaintext: &str, tenant: &str, anon: &str) -> FrameworkResult<String> {
        let key = Self::derive_key(&self.ring[0], tenant)?;
        let mut iv = [0_u8; 12];
        getrandom::fill(&mut iv).map_err(|_| crypto_error())?;
        Ok(format!(
            "v4:{}:{}:{}",
            self.ring[0].id,
            URL_SAFE_NO_PAD.encode(anon),
            Self::encrypt_body(plaintext, &key, iv)?
        ))
    }

    /// # Errors
    /// Returns a safe error on malformed data, absent keys, wrong tenant, or tampering.
    pub fn decrypt(&self, ciphertext: &str, tenant: &str) -> FrameworkResult<String> {
        self.open(ciphertext, tenant).map(|opened| opened.plaintext)
    }

    /// Opens v1/v2 legacy and stamped v3/v4 values and reports rotation status.
    ///
    /// # Errors
    /// Fails closed on malformed envelopes, missing keys, wrong tenants, or invalid tags.
    pub fn open(&self, ciphertext: &str, tenant: &str) -> FrameworkResult<OpenedCiphertext> {
        let parts: Vec<_> = ciphertext.split(':').collect();
        let (version, candidates, body, anon) = match parts.as_slice() {
            [version @ ("v1" | "v2"), iv, tag, data] => (
                *version,
                self.ring.iter().enumerate().collect::<Vec<_>>(),
                [*iv, *tag, *data],
                None,
            ),
            [version @ "v3", id, iv, tag, data] => (
                *version,
                self.ring
                    .iter()
                    .enumerate()
                    .filter(|(_, key)| key.id == *id)
                    .collect(),
                [*iv, *tag, *data],
                None,
            ),
            [version @ "v4", id, anon, iv, tag, data] => (
                *version,
                self.ring
                    .iter()
                    .enumerate()
                    .filter(|(_, key)| key.id == *id)
                    .collect(),
                [*iv, *tag, *data],
                Some(
                    String::from_utf8(URL_SAFE_NO_PAD.decode(anon).map_err(|_| crypto_error())?)
                        .map_err(|_| crypto_error())?,
                ),
            ),
            _ => return Err(crypto_error()),
        };
        let iv: [u8; 12] = STANDARD
            .decode(body[0])
            .map_err(|_| crypto_error())?
            .try_into()
            .map_err(|_| crypto_error())?;
        let tag: [u8; 16] = STANDARD
            .decode(body[1])
            .map_err(|_| crypto_error())?
            .try_into()
            .map_err(|_| crypto_error())?;
        let data = STANDARD.decode(body[2]).map_err(|_| crypto_error())?;
        for (index, material) in candidates {
            let key = Self::derive_key(material, tenant)?;
            let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| crypto_error())?;
            let mut plaintext = data.clone();
            if cipher
                .decrypt_inout_detached(
                    &Nonce::from(iv),
                    b"",
                    plaintext.as_mut_slice().into(),
                    &Tag::from(tag),
                )
                .is_ok()
            {
                let current = index == 0;
                return Ok(OpenedCiphertext {
                    plaintext: String::from_utf8(plaintext).map_err(|_| crypto_error())?,
                    key_id: material.id.clone(),
                    current,
                    version: version.to_owned(),
                    anon,
                    stale: !current
                        || (version != "v4"
                            && self.format == CiphertextFormat::V3
                            && version != "v3"),
                });
            }
        }
        Err(crypto_error())
    }

    /// # Errors
    /// Returns a safe error when the input cannot be authenticated.
    pub fn needs_rotation(&self, ciphertext: &str, tenant: &str) -> FrameworkResult<bool> {
        self.open(ciphertext, tenant).map(|opened| opened.stale)
    }

    /// Rewrites stale values under the current key; preserves v4 random-nonce
    /// format and its anonymous metadata. Current values are returned unchanged.
    ///
    /// # Errors
    /// Returns a safe error on authentication or encryption failure.
    pub fn rotate(&self, ciphertext: &str, tenant: &str) -> FrameworkResult<String> {
        let opened = self.open(ciphertext, tenant)?;
        if !opened.stale {
            return Ok(ciphertext.to_owned());
        }
        if opened.version == "v4" {
            self.seal(
                &opened.plaintext,
                tenant,
                opened.anon.as_deref().unwrap_or(""),
            )
        } else {
            self.encrypt(&opened.plaintext, tenant)
        }
    }

    /// # Errors
    /// Returns a safe error on derivation failure.
    pub fn blind_index(&self, plaintext: &str, tenant: &str) -> FrameworkResult<String> {
        Self::blind_index_for(&self.ring[0], plaintext, tenant)
    }

    /// # Errors
    /// Returns a safe error on derivation failure.
    pub fn blind_indexes(&self, plaintext: &str, tenant: &str) -> FrameworkResult<Vec<String>> {
        self.ring
            .iter()
            .map(|material| Self::blind_index_for(material, plaintext, tenant))
            .collect()
    }

    #[must_use]
    pub fn is_stale_blind_index(&self, index: &str) -> bool {
        stamped_key_id(index) != Some(self.ring[0].id.as_str())
    }

    fn blind_index_for(
        material: &KeyMaterial,
        plaintext: &str,
        tenant: &str,
    ) -> FrameworkResult<String> {
        let key = Self::derive_key(material, &format!("forklaunch:blind-index\0{tenant}"))?;
        let mut mac =
            <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_ref()).map_err(|_| crypto_error())?;
        mac.update(plaintext.as_bytes());
        Ok(format!(
            "bi1:{}:{}",
            material.id,
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        ))
    }
}

// Loaded through persistence.rs's #[path]; name the child file explicitly.
#[cfg(test)]
#[path = "field_encryptor/tests.rs"]
mod tests;
