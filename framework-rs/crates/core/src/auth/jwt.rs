//! JWT verification against explicitly configured keys and claim policies.
//!
//! Algorithms come from application policy. Token-provided `jku`, `x5u`, and
//! embedded keys never determine the verification key or remote endpoint.

use jsonwebtoken::jwk::{Jwk, JwkSet, KeyOperations, PublicKeyUse};
use jsonwebtoken::{DecodingKey, decode, decode_header};
use serde::de::DeserializeOwned;

use crate::auth::AuthError;
pub use jsonwebtoken::{Algorithm, Validation};

// jsonwebtoken 9 ignores extension fields in its typed Header. Reject critical
// extensions and unencoded payloads explicitly instead of silently accepting them.
pub(crate) fn check_header_policy(token: &str) -> Result<(), AuthError> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let encoded = token
        .split('.')
        .next()
        .ok_or(AuthError::InvalidCredentials)?;
    if encoded.len() > 16 * 1024 {
        return Err(AuthError::InvalidCredentials);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AuthError::InvalidCredentials)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| AuthError::InvalidCredentials)?;
    let object = value.as_object().ok_or(AuthError::InvalidCredentials)?;
    if object.contains_key("crit") || object.contains_key("b64") {
        return Err(AuthError::InvalidCredentials);
    }
    Ok(())
}

/// Verifies a token with a configured JWK and returns typed, verified claims.
///
/// # Errors
/// Returns invalid credentials for a mismatched key ID, key usage, algorithm,
/// signature, or claim policy. `validation` must explicitly allow algorithms.
pub fn verify_jwk<T: DeserializeOwned>(
    token: &str,
    jwk: &Jwk,
    validation: &Validation,
) -> Result<T, AuthError> {
    check_header_policy(token)?;
    let header = decode_header(token).map_err(|_| AuthError::InvalidCredentials)?;
    if !validation.algorithms.contains(&header.alg)
        || (header.kid.is_some() && header.kid != jwk.common.key_id)
        || jwk
            .common
            .public_key_use
            .as_ref()
            .is_some_and(|usage| *usage != PublicKeyUse::Signature)
        || jwk
            .common
            .key_operations
            .as_ref()
            .is_some_and(|ops| !ops.contains(&KeyOperations::Verify))
        || jwk
            .common
            .key_algorithm
            .is_some_and(|algorithm| algorithm.to_string() != format!("{:?}", header.alg))
    {
        return Err(AuthError::InvalidCredentials);
    }
    let key = DecodingKey::from_jwk(jwk).map_err(|_| AuthError::InvalidCredentials)?;
    // jsonwebtoken rejects a validation list containing any other key family.
    // The application allowlist and JWK policy were checked above; narrow only
    // this decode operation to the selected, allowed algorithm. This preserves
    // rotation across RSA/EC/EdDSA keys without accepting a token-chosen policy.
    let mut selected = validation.clone();
    selected.algorithms = vec![header.alg];
    decode::<T>(token, &key, &selected)
        .map(|data| data.claims)
        .map_err(|_| AuthError::InvalidCredentials)
}

/// Selects exactly one matching key; ambiguous and unknown keys fail closed.
fn select_key<'a>(set: &'a JwkSet, kid: Option<&str>) -> Result<&'a Jwk, AuthError> {
    let mut keys = set
        .keys
        .iter()
        .filter(|key| kid.is_none_or(|kid| key.common.key_id.as_deref() == Some(kid)));
    let key = keys.next().ok_or(AuthError::InvalidCredentials)?;
    if keys.next().is_some() {
        return Err(AuthError::InvalidCredentials);
    }
    Ok(key)
}

/// Verifies against a configured key set, requiring unambiguous key selection.
///
/// # Errors
/// Returns invalid credentials for missing/ambiguous keys or failed verification.
pub fn verify_jwks<T: DeserializeOwned>(
    token: &str,
    set: &JwkSet,
    validation: &Validation,
) -> Result<T, AuthError> {
    let header = decode_header(token).map_err(|_| AuthError::InvalidCredentials)?;
    verify_jwk(token, select_key(set, header.kid.as_deref())?, validation)
}

#[cfg(feature = "remote-jwks")]
pub use remote::{RemoteJwks, RemoteJwksConfig};

#[cfg(feature = "remote-jwks")]
mod remote {
    use super::{
        Algorithm, AuthError, DeserializeOwned, JwkSet, Validation, decode_header, select_key,
        verify_jwks,
    };
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use tokio::sync::Mutex;

    /// Bounds JWKS cache lifetime, response size, and refresh frequency.
    #[derive(Debug, Clone)]
    pub struct RemoteJwksConfig {
        pub url: String,
        pub default_ttl: Duration,
        pub max_ttl: Duration,
        pub refresh_cooldown: Duration,
        pub timeout: Duration,
        pub max_response_bytes: usize,
    }

    impl RemoteJwksConfig {
        #[must_use]
        pub fn new(url: impl Into<String>) -> Self {
            Self {
                url: url.into(),
                default_ttl: Duration::from_secs(300),
                max_ttl: Duration::from_secs(3600),
                refresh_cooldown: Duration::from_secs(30),
                timeout: Duration::from_secs(5),
                max_response_bytes: 1024 * 1024,
            }
        }
    }

    struct CachedKeys {
        keys: Option<JwkSet>,
        expires_at: Instant,
        last_attempt: Option<Instant>,
        refresh_failed: bool,
    }

    /// A per-endpoint cache shared by clones. Serializes refreshes, including on
    /// unknown key IDs, to bound requests during rotations or forged-token floods.
    #[derive(Clone)]
    pub struct RemoteJwks {
        config: RemoteJwksConfig,
        client: reqwest::Client,
        cache: Arc<Mutex<CachedKeys>>,
    }

    impl RemoteJwks {
        /// # Errors
        /// Accepts HTTP(S) endpoints (internal networks need not use TLS); rejects credentials,
        /// fragments, zero timeouts, and invalid cache bounds.
        pub fn new(config: RemoteJwksConfig) -> Result<Self, AuthError> {
            let url =
                reqwest::Url::parse(&config.url).map_err(|_| AuthError::InvalidCredentials)?;
            if !matches!(url.scheme(), "https" | "http")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || config.timeout.is_zero()
                || config.max_ttl.is_zero()
                || config.max_response_bytes == 0
            {
                return Err(AuthError::InvalidCredentials);
            }
            let client = reqwest::Client::builder()
                .timeout(config.timeout)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| AuthError::InvalidCredentials)?;
            Ok(Self {
                config,
                client,
                cache: Arc::new(Mutex::new(CachedKeys {
                    keys: None,
                    expires_at: Instant::now(),
                    last_attempt: None,
                    refresh_failed: false,
                })),
            })
        }

        /// Verifies a token, refreshing expired or rotated keys as needed.
        /// Remote key sets accept only asymmetric algorithms; caller policy still
        /// determines which of those algorithms and claims are valid.
        ///
        /// # Errors
        /// Fails closed on fetch/parse failures, unknown keys, cooldown, or invalid tokens.
        pub async fn verify<T: DeserializeOwned>(
            &self,
            token: &str,
            validation: &Validation,
        ) -> Result<T, AuthError> {
            super::check_header_policy(token)?;
            let header = decode_header(token).map_err(|_| AuthError::InvalidCredentials)?;
            if !validation.algorithms.contains(&header.alg)
                || matches!(
                    header.alg,
                    Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
                )
            {
                return Err(AuthError::InvalidCredentials);
            }
            let mut cache = self.cache.lock().await;
            let now = Instant::now();
            let known = cache
                .keys
                .as_ref()
                .is_some_and(|keys| select_key(keys, header.kid.as_deref()).is_ok());
            if cache.expires_at <= now || !known {
                if (!known || cache.refresh_failed)
                    && cache
                        .last_attempt
                        .is_some_and(|last| now.duration_since(last) < self.config.refresh_cooldown)
                {
                    return Err(AuthError::InvalidCredentials);
                }
                cache.last_attempt = Some(now);
                cache.refresh_failed = true;
                let (keys, ttl) = self.fetch().await?;
                cache.refresh_failed = false;
                cache.keys = Some(keys);
                cache.expires_at = Instant::now() + ttl;
            }
            verify_jwks(
                token,
                cache.keys.as_ref().ok_or(AuthError::InvalidCredentials)?,
                validation,
            )
        }

        async fn fetch(&self) -> Result<(JwkSet, Duration), AuthError> {
            let mut response = self
                .client
                .get(&self.config.url)
                .send()
                .await
                .map_err(|_| AuthError::InvalidCredentials)?
                .error_for_status()
                .map_err(|_| AuthError::InvalidCredentials)?;
            let ttl = cache_ttl(response.headers(), &self.config);
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| AuthError::InvalidCredentials)?
            {
                if chunk.len() > self.config.max_response_bytes.saturating_sub(bytes.len()) {
                    return Err(AuthError::InvalidCredentials);
                }
                bytes.extend_from_slice(&chunk);
            }
            let set: JwkSet =
                serde_json::from_slice(&bytes).map_err(|_| AuthError::InvalidCredentials)?;
            if set.keys.is_empty() {
                return Err(AuthError::InvalidCredentials);
            }
            Ok((set, ttl))
        }
    }

    fn cache_ttl(headers: &reqwest::header::HeaderMap, config: &RemoteJwksConfig) -> Duration {
        let directives: Vec<_> = headers
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .collect();
        if directives.iter().any(|value| {
            value.eq_ignore_ascii_case("no-store") || value.eq_ignore_ascii_case("no-cache")
        }) {
            return Duration::ZERO;
        }
        let ttl = directives
            .iter()
            .find_map(|directive| {
                let (name, value) = directive.split_once('=')?;
                if !name.trim().eq_ignore_ascii_case("max-age") {
                    return None;
                }
                value
                    .trim()
                    .trim_matches('"')
                    .parse::<u64>()
                    .ok()
                    .map(Duration::from_secs)
            })
            .unwrap_or(config.default_ttl)
            .min(config.max_ttl);
        let age = headers
            .get(reqwest::header::AGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_default();
        ttl.saturating_sub(age)
    }
}

#[cfg(test)]
#[path = "jwt_tests.rs"]
pub(crate) mod tests;
