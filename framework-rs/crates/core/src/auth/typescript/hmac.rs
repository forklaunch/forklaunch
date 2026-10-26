//! Opt-in TypeScript HMAC header parsing and trusted verifier dispatch.
use crate::http::{ErrorKind, ErrorResponse};
use std::{collections::BTreeMap, fmt, future::Future};

/// Unverified wire components passed only to a trusted credential verifier.
/// Timestamp text is intentionally raw: the verifier owns date interpretation,
/// signature verification, freshness and atomic nonce/replay policy. No session
/// or principal is created by this adapter. Debug omits all credential material.
pub struct HmacCredential<'a> {
    pub key_id: &'a str,
    pub timestamp: &'a str,
    pub nonce: &'a str,
    pub signature: &'a str,
    pub secret_key: Option<&'a str>,
}
impl fmt::Debug for HmacCredential<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HmacCredential")
            .finish_non_exhaustive()
    }
}

/// Verify through a trusted async callback using TypeScript header/error order.
/// The first four space-delimited components are used; additional components are
/// ignored, and values retain embedded `=` characters. Key lookup uses the
/// operator-owned dictionary; a missing key is passed as `None` and must be
/// rejected by the verifier. Request method/path/raw body belong to the callback.
/// Native strict [`crate::auth::request_hmac::HmacAuthorization`] remains unchanged.
///
/// # Errors
/// Returns missing/format/configuration errors without invoking the verifier,
/// a TypeScript 403 when verification returns false, or the callback's explicit
/// public error. Dropping the future drops the verifier without retrying it.
pub async fn verify_hmac<'a, Fut>(
    header: Option<&'a str>,
    token_prefix: Option<&str>,
    secret_keys: Option<&'a BTreeMap<String, String>>,
    verify: impl FnOnce(HmacCredential<'a>) -> Fut,
) -> Result<(), ErrorResponse>
where
    Fut: Future<Output = Result<bool, ErrorResponse>>,
{
    let header =
        header.ok_or_else(|| error(ErrorKind::Unauthorized, "Authorization token required."))?;
    let mut parts = header.split(' ');
    let prefix = parts.next().unwrap_or_default();
    let key_id = parts.next().ok_or_else(format_error)?;
    if prefix.is_empty() {
        return Err(format_error());
    }
    let timestamp = parts.next();
    let nonce = parts.next();
    let signature = parts.next();
    let (Some(timestamp), Some(nonce), Some(signature)) = (timestamp, nonce, signature) else {
        return Err(error(ErrorKind::Forbidden, "Invalid Authorization token."));
    };
    if prefix != token_prefix.unwrap_or("HMAC") {
        return Err(error(ErrorKind::Forbidden, "Invalid Authorization token."));
    }
    let keys = secret_keys.ok_or_else(|| {
        error(
            ErrorKind::Internal,
            "Invalid instantiation of authorization method.",
        )
    })?;
    let key_id = part(key_id, "keyId")?;
    let credential = HmacCredential {
        key_id,
        timestamp: part(timestamp, "ts")?,
        nonce: part(nonce, "nonce")?,
        signature: part(signature, "signature")?,
        secret_key: keys.get(key_id).map(String::as_str),
    };
    if !verify(credential).await? {
        return Err(error(
            ErrorKind::Forbidden,
            "Invalid Authorization signature.",
        ));
    }
    Ok(())
}

fn part<'a>(value: &'a str, name: &str) -> Result<&'a str, ErrorResponse> {
    value
        .split_once('=')
        .filter(|(key, value)| *key == name && !value.is_empty())
        .map(|(_, value)| value)
        .ok_or_else(format_error)
}
fn error(kind: ErrorKind, message: &str) -> ErrorResponse {
    ErrorResponse::new(kind, message)
}
fn format_error() -> ErrorResponse {
    error(
        ErrorKind::Unauthorized,
        "Invalid Authorization token format.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn verification_is_required_and_credentials_never_appear_in_debug() {
        let keys = BTreeMap::from([("peer".into(), "private-key".into())]);
        let error = verify_hmac(
            Some("HMAC keyId=peer ts=raw-date nonce=n=one signature=private-signature= ignored"),
            None,
            Some(&keys),
            |input| async move {
                assert_eq!(input.timestamp, "raw-date");
                assert_eq!(input.nonce, "n=one");
                assert_eq!(input.signature, "private-signature=");
                assert_eq!(input.secret_key, Some("private-key"));
                assert!(!format!("{input:?}").contains("private"));
                tokio::task::yield_now().await;
                Ok(false)
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.message, "Invalid Authorization signature.");
    }
    #[tokio::test]
    async fn malformed_credentials_and_missing_configuration_never_reach_verification() {
        let keys = BTreeMap::new();
        for (header, configured) in [
            (None, true),
            (Some("HMAC keyId=peer"), true),
            (Some("HMAC keyId= ts=x nonce=n signature=s"), true),
            (Some("HMAC keyId=peer ts=x nonce=n signature=s"), false),
        ] {
            assert!(
                verify_hmac(header, None, configured.then_some(&keys), |_| async {
                    panic!("invalid input must not invoke verifier");
                })
                .await
                .is_err()
            );
        }
    }
}
