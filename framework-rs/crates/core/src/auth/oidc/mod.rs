//! Bounded, operator-configured `OpenID` Connect authorization-code clients.
mod durable;
mod refresh;
mod transport;
use forklaunch_common::{FrameworkError, FrameworkResult};
use oauth2::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse as _,
    url::Url,
};
use openidconnect::{
    AccessTokenHash, IssuerUrl, Nonce, TokenResponse as _,
    core::{
        CoreAuthenticationFlow, CoreClient, CoreClientAuthMethod, CoreIdTokenClaims,
        CoreJsonWebKeySet, CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreResponseType,
        CoreTokenResponse, CoreTokenType,
    },
};
pub use refresh::OidcRefresh;
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

type Client = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// A discovered provider pinned to operator-owned issuer/client/callback config.
/// This adapter supports authorization code + S256 PKCE and RS256 ID tokens.
/// Discovery pins endpoints. Each ID-token verification fetches fresh keys from
/// the pinned JWKS endpoint, with the same transport limits and no redirects.
pub struct OidcProvider {
    metadata: CoreProviderMetadata,
    client_id: ClientId,
    secret: Option<ClientSecret>,
    redirect: RedirectUrl,
    transport: transport::Transport,
}
impl std::fmt::Debug for OidcProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcProvider").finish_non_exhaustive()
    }
}
/// Server-owned, single-use state. Store it against a browser-bound session;
/// send only `authorization_url()` to the browser. The callback consumes it.
/// Not cloneable; durable serialization is explicitly secret-bearing and must
/// enter authenticated encrypted storage. Expires after ten minutes.
pub struct PendingAuthorization {
    url: Url,
    state: CsrfToken,
    nonce: Nonce,
    verifier: PkceCodeVerifier,
    issuer: String,
    client_id: String,
    redirect: String,
    created: Instant,
    expires_at: i64,
}
impl std::fmt::Debug for PendingAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingAuthorization")
            .finish_non_exhaustive()
    }
}
impl PendingAuthorization {
    #[must_use]
    pub const fn authorization_url(&self) -> &Url {
        &self.url
    }
    /// State index for the application's browser-bound, single-use pending store.
    #[must_use]
    pub fn state(&self) -> &str {
        self.state.secret()
    }
}
/// Claims have passed signature, issuer, audience, expiry, nonce and access-token
/// hash checks when the hash is present. Tokens remain secrets and should only enter encrypted storage.
pub struct OidcLogin {
    pub claims: CoreIdTokenClaims,
    pub tokens: CoreTokenResponse,
}
impl std::fmt::Debug for OidcLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcLogin").finish_non_exhaustive()
    }
}
impl OidcProvider {
    /// Operator-configured callback URI; never selected by a login request.
    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        self.redirect.as_str()
    }

    /// Discover metadata and keys from a trusted issuer. Requests are bounded to
    /// 1 MiB/15 seconds and never follow redirects. HTTPS is required except for
    /// loopback fixtures. Client authentication uses HTTP Basic when secretful.
    /// # Errors
    /// Rejects invalid configuration, discovery/protocol errors and missing
    /// authorization-code token endpoints. Provider response details are hidden.
    pub async fn discover(
        issuer: &str,
        client_id: String,
        secret: Option<String>,
        redirect_uri: &str,
    ) -> FrameworkResult<Self> {
        let issuer_url = transport::endpoint(issuer)?;
        if issuer_url.query().is_some()
            || client_id.is_empty()
            || client_id.len() > 1024
            || client_id.chars().any(char::is_control)
            || secret.as_ref().is_some_and(|secret| {
                secret.is_empty()
                    || secret.len() > 16 * 1024
                    || secret.chars().any(char::is_control)
            })
        {
            return Err(failed());
        }
        transport::endpoint(redirect_uri)?;
        let transport = transport::Transport::new()?;
        let metadata = CoreProviderMetadata::discover_async(
            IssuerUrl::new(issuer.into()).map_err(|_| failed())?,
            &transport,
        )
        .await
        .map_err(|_| failed())?;
        transport::endpoint(metadata.authorization_endpoint().as_str())?;
        transport::endpoint(metadata.token_endpoint().ok_or_else(failed)?.as_str())?;
        if !metadata
            .id_token_signing_alg_values_supported()
            .contains(&CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256)
        {
            return Err(failed());
        }
        if !metadata
            .response_types_supported()
            .iter()
            .any(|types| types.as_slice() == [CoreResponseType::Code])
            || metadata
                .token_endpoint_auth_methods_supported()
                .is_some_and(|methods| {
                    !methods.contains(&if secret.is_some() {
                        CoreClientAuthMethod::ClientSecretBasic
                    } else {
                        CoreClientAuthMethod::None
                    })
                })
        {
            return Err(failed());
        }
        Ok(Self {
            metadata,
            client_id: ClientId::new(client_id),
            secret: secret.map(ClientSecret::new),
            redirect: RedirectUrl::new(redirect_uri.into()).map_err(|_| failed())?,
            transport,
        })
    }
    fn client(&self) -> Client {
        self.client_with_metadata(self.metadata.clone())
    }
    fn client_with_metadata(&self, metadata: CoreProviderMetadata) -> Client {
        CoreClient::from_provider_metadata(metadata, self.client_id.clone(), self.secret.clone())
            .set_redirect_uri(self.redirect.clone())
    }
    async fn verification_client(&self, tokens: &CoreTokenResponse) -> FrameworkResult<Client> {
        validate_tokens(tokens)?;
        tokens.id_token().ok_or_else(failed)?;
        // The token never selects this URL. Fetch once per verification without
        // credential headers; do not fall back to discovery-time keys on failure.
        let keys = CoreJsonWebKeySet::fetch_async(self.metadata.jwks_uri(), &self.transport)
            .await
            .map_err(|_| failed())?;
        Ok(self.client_with_metadata(self.metadata.clone().set_jwks(keys)))
    }
    /// Create unpredictable CSRF state, nonce and an S256 PKCE verifier.
    /// # Errors
    /// Rejects oversized, empty or whitespace-containing scope entries.
    pub fn begin(&self, scopes: &[String]) -> FrameworkResult<PendingAuthorization> {
        if scopes.len() > 64
            || scopes.iter().any(|scope| {
                scope.is_empty()
                    || scope.len() > 256
                    || scope.chars().any(|c| c.is_whitespace() || c.is_control())
            })
        {
            return Err(failed());
        }
        let client = self.client();
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(challenge);
        for scope in scopes {
            if scope != "openid" {
                request = request.add_scope(Scope::new(scope.clone()));
            }
        }
        let (url, state, nonce) = request.url();
        Ok(PendingAuthorization {
            url,
            state,
            nonce,
            verifier,
            issuer: self.metadata.issuer().as_str().into(),
            client_id: self.client_id.as_str().into(),
            redirect: self.redirect.as_str().into(),
            created: Instant::now(),
            expires_at: chrono::Utc::now().timestamp() + 600,
        })
    }
    /// Consume browser-bound pending state before exchanging a callback code.
    /// The application must retrieve pending state from the initiating browser's
    /// session, not solely from the untrusted callback's state string.
    /// # Errors
    /// Rejects expired/mismatched state/configuration before network access, and
    /// invalid provider responses or ID tokens without exposing provider details.
    pub async fn exchange(
        &self,
        pending: PendingAuthorization,
        returned_state: &str,
        code: &str,
    ) -> FrameworkResult<OidcLogin> {
        if pending.created.elapsed() >= Duration::from_secs(600)
            || !durable::valid_expiry(pending.expires_at)
            || !bool::from(
                pending
                    .state
                    .secret()
                    .as_bytes()
                    .ct_eq(returned_state.as_bytes()),
            )
            || pending.issuer != self.metadata.issuer().as_str()
            || pending.client_id != self.client_id.as_str()
            || pending.redirect != self.redirect.as_str()
            || code.is_empty()
            || code.len() > 16 * 1024
        {
            return Err(failed());
        }
        let client = self.client();
        let tokens = client
            .exchange_code(AuthorizationCode::new(code.into()))
            .map_err(|_| failed())?
            .set_pkce_verifier(pending.verifier)
            .request_async(&self.transport)
            .await
            .map_err(|_| failed())?;
        let verifier = self.verification_client(&tokens).await?;
        let claims = self.verify_claims(&verifier, &tokens, &pending.nonce)?;
        Ok(OidcLogin { claims, tokens })
    }
    fn verify_claims(
        &self,
        client: &Client,
        tokens: &CoreTokenResponse,
        nonce: impl openidconnect::NonceVerifier,
    ) -> FrameworkResult<CoreIdTokenClaims> {
        validate_tokens(tokens)?;
        let id_token = tokens.id_token().ok_or_else(failed)?;
        let verifier = client
            .id_token_verifier()
            .set_allowed_algs([CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256])
            .set_issue_time_verifier_fn(|issued| {
                if issued > chrono::Utc::now() + chrono::Duration::seconds(60) {
                    Err("invalid issue time".into())
                } else {
                    Ok(())
                }
            });
        let claims = id_token.claims(&verifier, nonce).map_err(|_| failed())?;
        if claims.subject().as_str().is_empty()
            || claims
                .authorized_party()
                .is_some_and(|party| party != &self.client_id)
        {
            return Err(failed());
        }
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                tokens.access_token(),
                id_token.signing_alg().map_err(|_| failed())?,
                id_token.signing_key(&verifier).map_err(|_| failed())?,
            )
            .map_err(|_| failed())?;
            if actual != *expected {
                return Err(failed());
            }
        }
        Ok(claims.clone())
    }
}
fn validate_tokens(tokens: &CoreTokenResponse) -> FrameworkResult<()> {
    if tokens.access_token().secret().is_empty()
        || tokens
            .refresh_token()
            .is_some_and(|token| token.secret().is_empty())
        || !matches!(tokens.token_type(), CoreTokenType::Bearer)
    {
        return Err(failed());
    }
    Ok(())
}
fn failed() -> FrameworkError {
    FrameworkError::new("oidc_authentication_failed", "OIDC authentication failed")
}

#[cfg(test)]
mod tests;
