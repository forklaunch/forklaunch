//! Trusted process configuration, read once at startup. The tenant and every
//! credential come from here, never from request input. Holds secrets, so it
//! deliberately implements neither `Debug` nor `Display`.
use crate::error::Result;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_core::auth::jwt::{RemoteJwks, RemoteJwksConfig};
use forklaunch_worker_database::{http::HttpConfig, runner::RunnerConfig};
use forklaunch_worker_interface::RetryPolicy;
use forklaunch_core::config;
use jsonwebtoken::{Algorithm, Validation};
use std::time::Duration;

/// The tenant and order-event queue policy shared by the HTTP service and the inventory worker.
#[derive(Clone)]
pub struct StoreConfig {
    pub tenant_id: String,
    pub retry: RetryPolicy,
}

impl StoreConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            tenant_id: config::required("TENANT_ID")?,
            retry: RetryPolicy::new(config::required("WORKER_MAX_ATTEMPTS")?, config::required("WORKER_BACKOFF_SECONDS")?),
        })
    }
}

#[derive(Clone)]
pub struct ServiceConfig {
    pub store: StoreConfig,
    pub host: String,
    pub port: u16,
    pub hmac_secret: String,
    pub jwks_url: String,
    pub jwt_issuer: Option<String>,
    pub jwt_audience: Option<String>,
    pub stripe: StripeConfig,
    pub paypal: Option<PaypalConfig>,
}

#[derive(Clone)]
pub struct StripeConfig {
    pub api_key: String,
    /// A trusted HTTPS proxy; unset calls the Stripe API directly.
    pub api_base: Option<String>,
    pub webhook_secret: String,
    /// Direct Connect charges to this account, with an optional platform fee.
    pub connect: Option<(String, u16)>,
}

#[derive(Clone)]
pub struct PaypalConfig {
    pub client_id: String,
    pub client_secret: String,
    pub base_url: String,
    pub webhook_id: String,
}

impl ServiceConfig {
    /// Every setting is required at startup; a missing or malformed variable is
    /// an error naming it. Stripe Connect, PayPal, the Stripe proxy and the JWT
    /// issuer/audience constraints are optional features that stay off while unset.
    pub fn from_env(store: StoreConfig) -> Result<Self> {
        let connected_account: Option<String> = config::optional("STRIPE_CONNECTED_ACCOUNT_ID")?;
        let platform_fee: Option<u16> = config::optional("STRIPE_PLATFORM_FEE_BPS")?;
        let stripe = StripeConfig {
            api_base: config::optional("STRIPE_API_BASE")?,
            api_key: config::required("STRIPE_API_KEY")?,
            webhook_secret: config::required("STRIPE_WEBHOOK_SECRET")?,
            connect: match (connected_account, platform_fee) {
                (Some(account), Some(fee)) => Some((account, fee)),
                (Some(_), None) => return Err("STRIPE_CONNECTED_ACCOUNT_ID requires STRIPE_PLATFORM_FEE_BPS".into()),
                (None, Some(_)) => return Err("STRIPE_PLATFORM_FEE_BPS requires STRIPE_CONNECTED_ACCOUNT_ID".into()),
                (None, None) => None,
            },
        };
        let paypal = match (config::optional::<String>("PAYPAL_CLIENT_ID")?, config::optional::<String>("PAYPAL_CLIENT_SECRET")?) {
            (Some(client_id), Some(client_secret)) => Some(PaypalConfig {
                client_id,
                client_secret,
                base_url: config::required("PAYPAL_BASE_URL")?,
                webhook_id: config::required("PAYPAL_WEBHOOK_ID")?,
            }),
            (None, None) => None,
            _ => return Err("Both PAYPAL_CLIENT_ID and PAYPAL_CLIENT_SECRET are required to enable PayPal".into()),
        };
        Ok(Self {
            store,
            host: config::required("HOST")?,
            port: config::required("PORT")?,
            hmac_secret: config::required("HMAC_SECRET_KEY")?,
            jwks_url: config::required("JWKS_PUBLIC_KEY_URL")?,
            jwt_issuer: config::optional("JWT_ISSUER")?,
            jwt_audience: config::optional("JWT_AUDIENCE")?,
            stripe,
            paypal,
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

/// The inventory worker's queue runner and optional readiness/control listener.
pub struct WorkerConfig {
    pub store: StoreConfig,
    pub runner: RunnerConfig,
    pub http: Option<HttpConfig>,
}

impl WorkerConfig {
    /// The runner settings are required; the readiness/control listener is
    /// optional and stays off without `WORKER_HTTP_ADDR`.
    pub fn from_env(store: StoreConfig) -> Result<Self> {
        let runner = RunnerConfig {
            queue: "order-events".to_owned(),
            concurrency: config::required("WORKER_CONCURRENCY")?,
            batch_size: config::required("WORKER_BATCH_SIZE")?,
            poll_interval: Duration::from_millis(config::required("WORKER_POLL_MS")?),
            lease_duration: Duration::from_millis(config::required("WORKER_LEASE_MS")?),
            renew_interval: Duration::from_millis(config::required("WORKER_RENEW_MS")?),
            drain_timeout: Duration::from_millis(config::required("WORKER_DRAIN_MS")?),
        };
        runner.validate()?;
        Ok(Self { store, runner, http: http_config()? })
    }
}

fn http_config() -> Result<Option<HttpConfig>> {
    let token: Option<String> = config::optional("WORKER_CONTROL_TOKEN")?;
    let Some(address) = config::optional("WORKER_HTTP_ADDR")? else {
        if token.is_some() {
            return Err("WORKER_CONTROL_TOKEN requires WORKER_HTTP_ADDR".into());
        }
        return Ok(None);
    };
    let config = HttpConfig { address, control_token: token };
    config.validate()?;
    Ok(Some(config))
}
