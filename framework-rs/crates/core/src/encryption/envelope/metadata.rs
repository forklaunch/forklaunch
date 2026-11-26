//! Authenticated public metadata, separate from encryption and equality indexes.
use super::{EnvelopeKey, SecurityError};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;
impl EnvelopeKey {
    /// Authenticate intentionally public bytes without decrypting a payload.
    /// Tags bind the wire namespace, key ID, tenant, resource and purpose.
    /// New tags require an active named key; legacy-only keyrings cannot mint them.
    ///
    /// # Errors
    /// Rejects invalid context, oversized metadata or a missing active key.
    pub fn sign_public_metadata(
        &self,
        tenant: Uuid,
        resource: Uuid,
        purpose: &str,
        bytes: &[u8],
    ) -> Result<String, SecurityError> {
        let id = self.active.as_deref().ok_or(SecurityError::InvalidInput)?;
        let mac = self.metadata_mac(id, tenant, resource, purpose, bytes)?;
        Ok(format!(
            "pm1.{id}.{}",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        ))
    }
    /// Constant-time verification accepts configured retained keys during rotation.
    /// Verify a metadata tag against its namespace, key and resource context.
    ///
    /// # Errors
    /// Rejects invalid context, malformed tags, unavailable keys or authentication failure.
    pub fn verify_public_metadata(
        &self,
        tenant: Uuid,
        resource: Uuid,
        purpose: &str,
        bytes: &[u8],
        tag: &str,
    ) -> Result<(), SecurityError> {
        if tag.len() > 256 {
            return Err(SecurityError::InvalidInput);
        }
        let mut parts = tag.split('.');
        if parts.next() != Some("pm1") {
            return Err(SecurityError::InvalidInput);
        }
        let id = parts.next().ok_or(SecurityError::InvalidInput)?;
        let encoded = parts.next().ok_or(SecurityError::InvalidInput)?;
        if parts.next().is_some() {
            return Err(SecurityError::InvalidInput);
        }
        let expected = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| SecurityError::InvalidInput)?;
        self.metadata_mac(id, tenant, resource, purpose, bytes)?
            .verify_slice(&expected)
            .map_err(|_| SecurityError::Encryption)
    }
    fn metadata_mac(
        &self,
        id: &str,
        tenant: Uuid,
        resource: Uuid,
        purpose: &str,
        bytes: &[u8],
    ) -> Result<Hmac<Sha256>, SecurityError> {
        if tenant.is_nil() || resource.is_nil() || bytes.len() > 2_097_152 {
            return Err(SecurityError::InvalidInput);
        }
        let master = self.keys.get(id).ok_or(SecurityError::Encryption)?;
        let aad = self.aad("pm1", Some(id), tenant, resource, purpose)?;
        let mut derived = Zeroizing::new([0u8; 32]);
        hkdf::Hkdf::<Sha256>::new(Some(b"forklaunch-public-metadata"), master.as_ref())
            .expand(&aad, &mut *derived)
            .map_err(|_| SecurityError::Encryption)?;
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(derived.as_ref())
            .map_err(|_| SecurityError::Encryption)?;
        mac.update(bytes);
        Ok(mac)
    }
}
