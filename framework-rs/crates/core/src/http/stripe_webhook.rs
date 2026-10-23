//! Raw-body Stripe signature verification with the SDK's 300-second replay tolerance.
use crate::domain::BlueprintError;
use crate::domain::BlueprintResult;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
#[derive(Clone)]
pub struct StripeWebhookVerifier {
    secret: Arc<Zeroizing<String>>,
}
impl std::fmt::Debug for StripeWebhookVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeWebhookVerifier")
            .finish_non_exhaustive()
    }
}
impl StripeWebhookVerifier {
    /// # Errors
    /// Rejects missing or control-containing configuration.
    pub fn new(secret: String) -> BlueprintResult<Self> {
        if secret.is_empty() || secret.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(BlueprintError::new(
                "stripe_invalid_config",
                "Invalid Stripe webhook configuration",
            ));
        }
        Ok(Self {
            secret: Arc::new(Zeroizing::new(secret)),
        })
    }
    #[must_use]
    pub fn verify(&self, raw_body: &[u8], header: &str) -> bool {
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        self.verify_at(raw_body, header, now.as_secs())
    }
    /// Verifies all `v1` signatures, accepting rotated secrets when any signature
    /// matches. Like Stripe's SDK, the age check rejects old timestamps and
    /// allows future timestamps. Callers still validate the authenticated JSON.
    #[must_use]
    pub fn verify_at(&self, raw_body: &[u8], header: &str, now: u64) -> bool {
        if raw_body.is_empty() || header.len() > 8192 || header.split(',').count() > 256 {
            return false;
        }
        let mut timestamp = None;
        let mut signatures = Vec::new();
        for part in header.split(',') {
            let mut fields = part.split('=');
            let key = fields.next();
            let value = fields.next();
            match (key, value) {
                (Some("t"), Some(value)) => timestamp = value.parse::<u64>().ok(),
                (Some("v1"), Some(value)) => signatures.push(value),
                _ => {}
            }
        }
        let Some(timestamp) = timestamp else {
            return false;
        };
        if now.saturating_sub(timestamp) > 300 {
            return false;
        }
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes()) else {
            return false;
        };
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(raw_body);
        signatures.into_iter().any(|signature| {
            let Some(bytes) = decode_signature(signature) else {
                return false;
            };
            mac.clone().verify_slice(&bytes).is_ok()
        })
    }
}
fn decode_signature(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }

    let mut decoded = [0; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        decoded[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(decoded)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
