use super::{CoreIdTokenClaims, CoreTokenResponse, OidcProvider, failed};
use forklaunch_common::FrameworkResult;
use oauth2::{RefreshToken, TokenResponse as _};
use openidconnect::{Nonce, TokenResponse as _};
use subtle::ConstantTimeEq;

/// A refresh may omit an ID token. `claims` contains only a newly verified ID
/// token's claims; it never relabels the original login's claims as refreshed.
/// An omitted replacement refresh token retains the supplied refresh token.
pub struct OidcRefresh {
    pub tokens: CoreTokenResponse,
    pub claims: Option<CoreIdTokenClaims>,
}
impl std::fmt::Debug for OidcRefresh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcRefresh").finish_non_exhaustive()
    }
}
impl OidcProvider {
    /// Refresh using a server-owned token and the original, already verified
    /// login claims. Retrieve both from trusted encrypted account storage;
    /// never accept `initial_claims` from a request. Keep the original claims
    /// across refreshes, including their original nonce and authentication time.
    /// The application owns rotation persistence and concurrency control.
    /// # Errors
    /// Rejects a mismatched original issuer/client before network access,
    /// provider failures, invalid tokens and changes to the authenticated identity.
    pub async fn refresh(
        &self,
        refresh_token: &RefreshToken,
        initial_claims: &CoreIdTokenClaims,
    ) -> FrameworkResult<OidcRefresh> {
        if refresh_token.secret().is_empty()
            || refresh_token.secret().len() > 16 * 1024
            || initial_claims.issuer() != self.metadata.issuer()
            || initial_claims.subject().as_str().is_empty()
            || initial_claims.audiences().is_empty()
            || initial_claims
                .audiences()
                .iter()
                .any(|audience| audience.as_str() != self.client_id.as_str())
        {
            return Err(failed());
        }
        let client = self.client();
        let mut tokens = client
            .exchange_refresh_token(refresh_token)
            .map_err(|_| failed())?
            .request_async(&self.transport)
            .await
            .map_err(|_| failed())?;
        super::validate_tokens(&tokens)?;
        let claims = if tokens.id_token().is_some() {
            let verifier = self.verification_client(&tokens).await?;
            let claims =
                self.verify_claims(&verifier, &tokens, |nonce: Option<&Nonce>| {
                    match (nonce, initial_claims.nonce()) {
                        (None, _) => Ok(()),
                        (Some(actual), Some(expected))
                            if bool::from(
                                actual
                                    .secret()
                                    .as_bytes()
                                    .ct_eq(expected.secret().as_bytes()),
                            ) =>
                        {
                            Ok(())
                        }
                        _ => Err("invalid refresh nonce".into()),
                    }
                })?;
            if claims.subject() != initial_claims.subject()
                || claims.audiences() != initial_claims.audiences()
                || claims.issue_time() < initial_claims.issue_time()
                || (claims.auth_time().is_some()
                    && claims.auth_time() != initial_claims.auth_time())
            {
                return Err(failed());
            }
            Some(claims)
        } else {
            None
        };
        if tokens.refresh_token().is_none() {
            tokens.set_refresh_token(Some(refresh_token.clone()));
        }
        Ok(OidcRefresh { tokens, claims })
    }
}
