//! Trusted process configuration, read once at startup. The tenant and every
//! credential come from here, never from request input. Holds secrets, so it
//! deliberately implements neither `Debug` nor `Display`.
use crate::error::Result;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_stripe::http::{BillingRequestDateProfile, profiles};
use forklaunch_core::auth::jwt::{RemoteJwks, RemoteJwksConfig};
use forklaunch_core::config;
use jsonwebtoken::{Algorithm, Validation};

#[derive(Clone)]
pub struct ServiceConfig {
    pub host: String,
    pub port: u16,
    pub tenant_id: String,
    pub hmac_secret: String,
    pub jwks_url: String,
    pub jwt_issuer: Option<String>,
    pub jwt_audience: Option<String>,
{{#stripe}}    pub stripe: StripeConfig,
{{/stripe}}    pub database_backup: bool,
    pub request_dates: BillingRequestDateProfile,
}
{{#stripe}}
/// Your own Stripe key takes precedence; the managed gateway is the fallback
/// when no key is configured.
#[derive(Clone)]
pub struct StripeConfig {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub webhook_secret: Option<String>,
    pub gateway_url: Option<String>,
    pub instance_id: Option<String>,
    pub instance_key: Option<String>,
}
{{/stripe}}
impl ServiceConfig {
    /// Every setting is required at startup; a missing or malformed variable is
    /// an error naming it. The JWT issuer/audience constraints are optional{{#stripe}},
    /// and Stripe needs either a key or the complete managed gateway settings{{/stripe}}.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: config::required("HOST")?,
            port: config::required("PORT")?,
            tenant_id: config::required("TENANT_ID")?,
            hmac_secret: config::required("HMAC_SECRET_KEY")?,
            jwks_url: config::required("JWKS_PUBLIC_KEY_URL")?,
            jwt_issuer: config::optional("JWT_ISSUER")?,
            jwt_audience: config::optional("JWT_AUDIENCE")?,
{{#stripe}}            stripe: StripeConfig {
                api_key: match config::optional("STRIPE_API_KEY")? {
                    Some(key) => Some(key),
                    None => config::optional("STRIPE_SECRET_KEY")?,
                },
                api_base: config::optional("STRIPE_API_BASE")?,
                webhook_secret: config::optional("STRIPE_WEBHOOK_SECRET")?,
                gateway_url: config::optional("PLATFORM_GATEWAY_URL")?,
                instance_id: config::optional("INSTANCE_ID")?,
                instance_key: config::optional("INSTANCE_HMAC_KEY")?,
            },
{{/stripe}}            database_backup: config::required("BILLING_DATABASE_BACKUP")?,
            request_dates: profiles::request_date_profile_from_env()?,
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
