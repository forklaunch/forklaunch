//! Randomized authenticated envelopes with bounded explicit key rotation.
mod metadata;
use super::{EnvelopeError as SecurityError, EnvelopeProfile};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::BTreeMap;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Key material is private and zeroized on drop. No Debug/Serialize implementation.
pub struct EnvelopeKey {
    profile: EnvelopeProfile,
    active: Option<String>,
    keys: BTreeMap<String, Zeroizing<[u8; 32]>>,
    legacy: Option<Zeroizing<[u8; 32]>>,
}
impl EnvelopeKey {
    /// Equality indexes for the active key followed by retained rotation keys.
    /// The caller supplies the canonical normalized value and member purpose.
    /// Indexes never include the resource ID: equal values in the same tenant
    /// and field intentionally match. They reveal equality/frequency, not order.
    /// Retaining keys supports queries during an explicit reindex migration.
    ///
    /// # Errors
    /// Rejects invalid tenant/purpose/value bounds or an unavailable active rotation key.
    pub fn blind_indexes(
        &self,
        tenant: Uuid,
        purpose: &str,
        value: &[u8],
    ) -> Result<Vec<String>, SecurityError> {
        use hmac::Mac;
        if tenant.is_nil() || value.len() > 1_048_576 {
            return Err(SecurityError::InvalidInput);
        }
        let active = self.active.as_ref().ok_or(SecurityError::InvalidInput)?;
        // Reuse bounded, unambiguous member-purpose validation.
        self.aad("v3", Some(active), tenant, tenant, purpose)?;
        let mut indexes = Vec::new();
        for id in std::iter::once(active).chain(self.keys.keys().filter(|id| *id != active)) {
            let master = self.keys.get(id).ok_or(SecurityError::Encryption)?;
            let mut derived = Zeroizing::new([0u8; 32]);
            let info = format!("{}:bi1:{tenant}:{purpose}", self.profile.namespace);
            hkdf::Hkdf::<sha2::Sha256>::new(Some(b"forklaunch-blind-index"), master.as_ref())
                .expand(info.as_bytes(), &mut *derived)
                .map_err(|_| SecurityError::Encryption)?;
            let mut mac =
                <hmac::Hmac<sha2::Sha256> as hmac::KeyInit>::new_from_slice(derived.as_ref())
                    .map_err(|_| SecurityError::Encryption)?;
            mac.update(value);
            indexes.push(format!(
                "bi1.{id}.{}",
                URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
            ));
        }
        Ok(indexes)
    }
    /// Compatibility constructor: writes and reads the original v1 envelope.
    #[must_use]
    pub fn new(profile: EnvelopeProfile, key: [u8; 32]) -> Self {
        Self {
            profile,
            active: None,
            keys: BTreeMap::new(),
            legacy: Some(Zeroizing::new(key)),
        }
    }
    /// New writes use v3 with an HKDF-SHA256 tenant key and the active key ID.
    /// Previous keys are read-only; unknown IDs never
    /// fall back to another key. The optional legacy key alone opens unkeyed v1 data.
    ///
    /// # Errors
    /// Rejects invalid, duplicate or excessive key identifiers.
    pub fn with_rotation(
        profile: EnvelopeProfile,
        active_id: String,
        active_key: [u8; 32],
        previous: Vec<(String, [u8; 32])>,
        legacy: Option<[u8; 32]>,
    ) -> Result<Self, SecurityError> {
        if !valid_id(&active_id) || previous.len() > 4 {
            return Err(SecurityError::InvalidInput);
        }
        let mut keys = BTreeMap::new();
        keys.insert(active_id.clone(), Zeroizing::new(active_key));
        for (id, key) in previous {
            if !valid_id(&id) || keys.contains_key(&id) {
                return Err(SecurityError::InvalidInput);
            }
            keys.insert(id, Zeroizing::new(key));
        }
        Ok(Self {
            profile,
            active: Some(active_id),
            keys,
            legacy: legacy.map(Zeroizing::new),
        })
    }
    fn aad(
        &self,
        version: &str,
        id: Option<&str>,
        tenant: Uuid,
        resource: Uuid,
        purpose: &str,
    ) -> Result<Vec<u8>, SecurityError> {
        if purpose.is_empty()
            || purpose.len() > 64
            || !purpose
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(SecurityError::InvalidInput);
        }
        Ok(match id {
            Some(id) => format!(
                "{}:{version}:{id}:{tenant}:{resource}:{purpose}",
                self.profile.namespace
            ),
            None => format!(
                "{}:{version}:{tenant}:{resource}:{purpose}",
                self.profile.namespace
            ),
        }
        .into_bytes())
    }

    fn tenant_key(
        &self,
        master: &[u8],
        tenant: Uuid,
    ) -> Result<Zeroizing<[u8; 32]>, SecurityError> {
        let mut key = Zeroizing::new([0u8; 32]);
        let info = format!("{}:v3:{tenant}", self.profile.namespace);
        hkdf::Hkdf::<sha2::Sha256>::new(Some(b"forklaunch-envelope"), master)
            .expand(info.as_bytes(), &mut *key)
            .map_err(|_| SecurityError::Encryption)?;
        Ok(key)
    }
    /// Seal plaintext with the configured authenticated context.
    ///
    /// # Errors
    /// Rejects invalid context, oversized plaintext or encryption failure.
    pub fn seal(
        &self,
        tenant: Uuid,
        resource: Uuid,
        purpose: &str,
        plaintext: &[u8],
    ) -> Result<String, SecurityError> {
        if plaintext.len() > 1_048_576 {
            return Err(SecurityError::InvalidInput);
        }
        let (version, key, id) = match &self.active {
            Some(id) => (
                "v3",
                self.keys.get(id).ok_or(SecurityError::Encryption)?,
                Some(id.as_str()),
            ),
            None => (
                "v1",
                self.legacy.as_ref().ok_or(SecurityError::Encryption)?,
                None,
            ),
        };
        let aad = self.aad(version, id, tenant, resource, purpose)?;
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|_| SecurityError::Encryption)?;
        let derived = if version == "v3" {
            Some(self.tenant_key(key.as_ref(), tenant)?)
        } else {
            None
        };
        let cipher_key = derived.as_ref().map_or(key.as_ref(), |key| key.as_ref());
        let cipher =
            Aes256Gcm::new_from_slice(cipher_key).map_err(|_| SecurityError::Encryption)?;
        let encrypted = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| SecurityError::Encryption)?;
        let prefix = match id {
            Some(id) => format!("{version}.{id}"),
            None => "v1".into(),
        };
        Ok(format!(
            "{prefix}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(encrypted)
        ))
    }
    /// Open a bounded envelope only after authenticating its context and ciphertext.
    ///
    /// # Errors
    /// Rejects malformed envelopes, unknown keys, context mismatch or authentication failure.
    pub fn open(
        &self,
        tenant: Uuid,
        resource: Uuid,
        purpose: &str,
        envelope: &str,
    ) -> Result<Zeroizing<Vec<u8>>, SecurityError> {
        if envelope.len() > 1_400_000 {
            return Err(SecurityError::Encryption);
        }
        let mut parts = envelope.split('.');
        let version = parts.next().ok_or(SecurityError::Encryption)?;
        let (id, key) = match version {
            "v1" => (None, self.legacy.as_ref().ok_or(SecurityError::Encryption)?),
            "v2" | "v3" => {
                let id = parts.next().ok_or(SecurityError::Encryption)?;
                if !valid_id(id) {
                    return Err(SecurityError::Encryption);
                }
                (
                    Some(id),
                    self.keys.get(id).ok_or(SecurityError::Encryption)?,
                )
            }
            _ => return Err(SecurityError::Encryption),
        };
        let aad = self.aad(version, id, tenant, resource, purpose)?;
        let nonce = URL_SAFE_NO_PAD
            .decode(parts.next().ok_or(SecurityError::Encryption)?)
            .map_err(|_| SecurityError::Encryption)?;
        let ciphertext = URL_SAFE_NO_PAD
            .decode(parts.next().ok_or(SecurityError::Encryption)?)
            .map_err(|_| SecurityError::Encryption)?;
        if parts.next().is_some() {
            return Err(SecurityError::Encryption);
        }
        let nonce: [u8; 12] = nonce.try_into().map_err(|_| SecurityError::Encryption)?;
        let derived = if version == "v3" {
            Some(self.tenant_key(key.as_ref(), tenant)?)
        } else {
            None
        };
        let cipher_key = derived.as_ref().map_or(key.as_ref(), |key| key.as_ref());
        let cipher =
            Aes256Gcm::new_from_slice(cipher_key).map_err(|_| SecurityError::Encryption)?;
        cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| SecurityError::Encryption)
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_nonce_lengths_are_rejected_without_panicking() {
        let tenant = Uuid::new_v4();
        let resource = Uuid::new_v4();
        let legacy = EnvelopeKey::new(EnvelopeProfile::new("fieldnotes").unwrap(), [7; 32]);
        let rotated = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "primary".into(),
            [7; 32],
            vec![],
            Some([7; 32]),
        )
        .unwrap();
        for key in [&legacy, &rotated] {
            let envelope = key.seal(tenant, resource, "note-body", b"private").unwrap();
            let mut parts: Vec<_> = envelope.split('.').map(str::to_owned).collect();
            let nonce_index = parts.len() - 2;
            for length in [0, 1, 11, 13, 24] {
                parts[nonce_index] = URL_SAFE_NO_PAD.encode(vec![9; length]);
                assert!(
                    rotated
                        .open(tenant, resource, "note-body", &parts.join("."))
                        .is_err(),
                    "nonce length {length} must be rejected"
                );
            }
        }
    }
    #[test]
    fn blind_indexes_separate_tenants_fields_and_rotation_keys() {
        let tenant = Uuid::new_v4();
        let old = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("test").unwrap(),
            "old".into(),
            [7; 32],
            vec![],
            None,
        )
        .unwrap();
        let rotated = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("test").unwrap(),
            "new".into(),
            [8; 32],
            vec![("old".into(), [7; 32])],
            None,
        )
        .unwrap();
        let value = b"private@example.test";
        let first = old.blind_indexes(tenant, "email", value).unwrap();
        let indexes = rotated.blind_indexes(tenant, "email", value).unwrap();
        assert_eq!(indexes.len(), 2);
        assert!(indexes[0].starts_with("bi1.new."));
        assert_eq!(indexes[1], first[0]);
        assert_ne!(
            indexes,
            rotated
                .blind_indexes(Uuid::new_v4(), "email", value)
                .unwrap()
        );
        assert_ne!(
            indexes,
            rotated.blind_indexes(tenant, "other-field", value).unwrap()
        );
        assert_ne!(
            indexes,
            rotated
                .blind_indexes(tenant, "email", b"other@example.test")
                .unwrap()
        );
        assert!(!indexes.iter().any(|index| index.contains("private")));
        assert!(old.blind_indexes(Uuid::nil(), "email", value).is_err());
    }
    #[test]
    fn node_v3_hkdf_vector_and_version_downgrade_rejection() {
        let key = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "primary".into(),
            [7; 32],
            vec![],
            None,
        )
        .unwrap();
        let tenant = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let resource = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        // Produced independently with Node crypto.hkdfSync/createCipheriv.
        let envelope = "v3.primary.CQkJCQkJCQkJCQkJ.BhYWbHvMs2p_fZ3stSe379HTKbCO50Dgh24";
        assert_eq!(
            key.open(tenant, resource, "note-body", envelope)
                .unwrap()
                .as_slice(),
            "hello 📝".as_bytes()
        );
        assert!(
            key.open(
                tenant,
                resource,
                "note-body",
                &envelope.replacen("v3.", "v2.", 1)
            )
            .is_err()
        );
        let other = Uuid::new_v4();
        assert_ne!(
            *key.tenant_key(&[7; 32], tenant).unwrap(),
            *key.tenant_key(&[7; 32], other).unwrap()
        );
        assert!(key.open(other, resource, "note-body", envelope).is_err());
        assert!(
            key.open(tenant, Uuid::new_v4(), "note-body", envelope)
                .is_err()
        );
        assert!(
            key.open(tenant, resource, "other-purpose", envelope)
                .is_err()
        );
    }
    #[test]
    fn node_aes_gcm_vector() {
        let key = EnvelopeKey::new(EnvelopeProfile::new("fieldnotes").unwrap(), [7; 32]);
        let tenant = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let resource = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let envelope = "v1.CQkJCQkJCQkJCQkJ.T-Do-NHQMf4z_5yjLcoHQRB8zcRSOhIXIrI";
        assert_eq!(
            key.open(tenant, resource, "note-body", envelope)
                .unwrap()
                .as_slice(),
            "hello 📝".as_bytes()
        );
    }
    #[test]
    fn rotation_retains_old_data_and_authenticates_key_id() {
        let tenant = Uuid::new_v4();
        let resource = Uuid::new_v4();
        let legacy = EnvelopeKey::new(EnvelopeProfile::new("fieldnotes").unwrap(), [7; 32])
            .seal(tenant, resource, "note-body", b"old")
            .unwrap();
        let first = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "old".into(),
            [7; 32],
            vec![],
            Some([7; 32]),
        )
        .unwrap();
        let old = first.seal(tenant, resource, "note-body", b"old").unwrap();
        let rotated = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "new".into(),
            [8; 32],
            vec![("old".into(), [7; 32])],
            Some([7; 32]),
        )
        .unwrap();
        for envelope in [&legacy, &old] {
            assert_eq!(
                rotated
                    .open(tenant, resource, "note-body", envelope)
                    .unwrap()
                    .as_slice(),
                b"old"
            );
        }
        let fresh = rotated.seal(tenant, resource, "note-body", b"new").unwrap();
        assert!(fresh.starts_with("v3.new."));
        assert!(first.open(tenant, resource, "note-body", &fresh).is_err());
        let removed = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "new".into(),
            [8; 32],
            vec![],
            None,
        )
        .unwrap();
        assert!(removed.open(tenant, resource, "note-body", &old).is_err());
        assert!(
            removed
                .open(tenant, resource, "note-body", &legacy)
                .is_err()
        );
        let same = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "alias".into(),
            [7; 32],
            vec![],
            None,
        )
        .unwrap();
        assert!(
            same.open(
                tenant,
                resource,
                "note-body",
                &old.replacen("v3.old.", "v3.alias.", 1)
            )
            .is_err()
        );
        assert!(
            EnvelopeKey::with_rotation(
                EnvelopeProfile::new("fieldnotes").unwrap(),
                "bad.id".into(),
                [7; 32],
                vec![],
                None
            )
            .is_err()
        );
        assert!(
            EnvelopeKey::with_rotation(
                EnvelopeProfile::new("fieldnotes").unwrap(),
                "old".into(),
                [7; 32],
                vec![("old".into(), [8; 32])],
                None
            )
            .is_err()
        );
    }
    #[test]
    fn node_v2_vector() {
        let key = EnvelopeKey::with_rotation(
            EnvelopeProfile::new("fieldnotes").unwrap(),
            "primary".into(),
            [7; 32],
            vec![],
            None,
        )
        .unwrap();
        let tenant = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let resource = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        assert_eq!(
            key.open(
                tenant,
                resource,
                "note-body",
                "v2.primary.CQkJCQkJCQkJCQkJ.T-Do-NHQMf4z_6GNtEvLtyNSJoYTWZ8XNfA"
            )
            .unwrap()
            .as_slice(),
            "hello 📝".as_bytes()
        );
    }
    #[test]
    fn binding_randomness_and_tamper() {
        let key = EnvelopeKey::new(EnvelopeProfile::new("fieldnotes").unwrap(), [7; 32]);
        let tenant = Uuid::new_v4();
        let note = Uuid::new_v4();
        let a = key.seal(tenant, note, "note-body", b"secret").unwrap();
        let b = key.seal(tenant, note, "note-body", b"secret").unwrap();
        assert_ne!(a, b);
        assert_eq!(
            key.open(tenant, note, "note-body", &a).unwrap().as_slice(),
            b"secret"
        );
        assert!(key.open(Uuid::new_v4(), note, "note-body", &a).is_err());
        assert!(key.open(tenant, Uuid::new_v4(), "note-body", &a).is_err());
        assert!(key.open(tenant, note, "note-title", &a).is_err());
        let (prefix, encoded) = a.rsplit_once('.').unwrap();
        let mut modified = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        modified[0] ^= 1;
        let tampered = format!("{prefix}.{}", URL_SAFE_NO_PAD.encode(modified));
        assert!(key.open(tenant, note, "note-body", &tampered).is_err());
        assert!(
            EnvelopeKey::new(EnvelopeProfile::new("fieldnotes").unwrap(), [8; 32])
                .open(tenant, note, "note-body", &a)
                .is_err()
        );
    }
}
