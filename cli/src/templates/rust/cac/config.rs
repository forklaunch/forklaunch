//! Trusted process configuration, read once at startup. Every credential comes
//! from here, never from request input. Holds secrets, so it deliberately
//! implements neither `Debug` nor `Display`.
use crate::error::Result;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_core::auth::jwt::{RemoteJwks, RemoteJwksConfig};
use forklaunch_core::config;
use jsonwebtoken::{Algorithm, Validation};

#[derive(Clone)]
pub struct ServiceConfig {
    pub host: String,
    pub port: u16,
    pub hmac_secret: String,
    pub jwks_url: String,
    pub jwt_issuer: Option<String>,
    pub jwt_audience: Option<String>,
}

impl ServiceConfig {
    /// Every setting is required at startup except the JWT issuer/audience
    /// constraints; a missing or malformed variable is an error naming it.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: config::required("HOST")?,
            port: config::required("PORT")?,
            hmac_secret: config::required("HMAC_SECRET_KEY")?,
            jwks_url: config::required("JWKS_PUBLIC_KEY_URL")?,
            // Optional constraints: an unset issuer or audience is not checked.
            jwt_issuer: config::optional("JWT_ISSUER")?,
            jwt_audience: config::optional("JWT_AUDIENCE")?,
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
