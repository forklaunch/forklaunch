//! Trusted process configuration, read once at startup. The tenant and every
//! credential come from here, never from request input. Holds secrets, so it
//! deliberately implements neither `Debug` nor `Display`.
use crate::error::Result;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_core::auth::jwt::{RemoteJwks, RemoteJwksConfig};
use forklaunch_iam_base::http::DirectoryAuthorizationMode;
use forklaunch_core::config;
use jsonwebtoken::{Algorithm, Validation};

#[derive(Clone)]
pub struct ServiceConfig {
    pub host: String,
    pub port: u16,
    pub tenant_id: String,
    pub directory_mode: DirectoryAuthorizationMode,
    /// Exact HTTPS origin for browser sessions; `None` leaves them disabled.
    pub browser_origin: Option<String>,
    pub oidc: Option<OidcConfig>,
    pub hmac_secret: String,
    pub jwks_url: String,
    pub jwt_issuer: Option<String>,
    pub jwt_audience: Option<String>,
    /// Exact browser origins allowed to call cross-origin (`CORS_ORIGINS`,
    /// comma-separated); empty sends no CORS headers.
    pub cors_origins: Vec<String>,
}

/// The single operator-configured OIDC provider for browser login.
#[derive(Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

impl ServiceConfig {
    /// Every setting is required at startup; a missing or malformed variable is
    /// an error naming it. Browser sessions (`SESSION_COOKIE_ORIGIN`), OIDC
    /// login (`SESSION_OIDC_*`) and the JWT issuer/audience constraints are
    /// optional features that stay off while unset.
    pub fn from_env() -> Result<Self> {
        let directory_mode: String = config::required("IAM_DIRECTORY_AUTHORIZATION")?;
        let directory_mode = match directory_mode.as_str() {
            "jwt-claims" => DirectoryAuthorizationMode::JwtClaims,
            "stored-session" => DirectoryAuthorizationMode::StoredSessions,
            _ => return Err("IAM_DIRECTORY_AUTHORIZATION must be jwt-claims or stored-session".into()),
        };
        let issuer: Option<String> = config::optional("SESSION_OIDC_ISSUER")?;
        let client_id: Option<String> = config::optional("SESSION_OIDC_CLIENT_ID")?;
        let client_secret: Option<String> = config::optional("SESSION_OIDC_CLIENT_SECRET")?;
        let browser_origin: Option<String> = config::optional("SESSION_COOKIE_ORIGIN")?;
        let oidc_enabled = issuer.is_some() || client_id.is_some() || client_secret.is_some();
        if oidc_enabled && browser_origin.is_none() {
            return Err("OIDC requires SESSION_COOKIE_ORIGIN".into());
        }
        let oidc = if oidc_enabled {
            Some(OidcConfig {
                issuer: issuer.ok_or("SESSION_OIDC_ISSUER is required with OIDC")?,
                client_id: client_id.ok_or("SESSION_OIDC_CLIENT_ID is required with OIDC")?,
                // Public clients have no secret.
                client_secret,
            })
        } else {
            None
        };
        Ok(Self {
            host: config::required("HOST")?,
            port: config::required("PORT")?,
            tenant_id: config::required("TENANT_ID")?,
            directory_mode,
            browser_origin,
            oidc,
            hmac_secret: config::required("HMAC_SECRET_KEY")?,
            jwks_url: config::required("JWKS_PUBLIC_KEY_URL")?,
            jwt_issuer: config::optional("JWT_ISSUER")?,
            jwt_audience: config::optional("JWT_AUDIENCE")?,
            cors_origins: config::optional::<String>("CORS_ORIGINS")?
                .map(|value| value.split(',').map(str::trim).filter(|o| !o.is_empty()).map(str::to_owned).collect())
                .unwrap_or_default(),
        })
    }

    #[must_use]
    pub fn bind_addr(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    /// Internal routes verify request HMAC signatures; protected routes verify
    /// asymmetric JWTs from the trusted JWKS endpoint.
    pub fn module_auth(&self) -> Result<HmacJwksAuth> {
        let jwks = RemoteJwks::new(RemoteJwksConfig::new(self.jwks_url.clone()))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.algorithms = vec![
            Algorithm::RS256, Algorithm::RS384, Algorithm::RS512,
            Algorithm::PS256, Algorithm::PS384, Algorithm::PS512,
            Algorithm::ES256, Algorithm::ES384, Algorithm::EdDSA,
        ];
        validation.validate_nbf = true;
        validation.leeway = 0;
        validation.validate_aud = false;
        if let Some(issuer) = &self.jwt_issuer {
            validation.set_issuer(&[issuer]);
        }
        if let Some(audience) = &self.jwt_audience {
            validation.validate_aud = true;
            validation.set_audience(&[audience]);
        }
        Ok(HmacJwksAuth::new(self.hmac_secret.clone().into_bytes(), jwks, validation)?)
    }
}
