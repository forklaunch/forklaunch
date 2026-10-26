//! Builds the billing store, provider client and HTTP handlers from trusted configuration.
use crate::{STRIPE, config::ServiceConfig, domain::services::PostgresBillingStore, error::Result, persistence::database, service_info};
use forklaunch_billing_stripe::http::{BillingBackend, BillingHttp, BillingRoutes, BoundBillingV1, BoundCheckoutV1, profiles};
{{#stripe}}use forklaunch_billing_stripe::client::StripeBillingClient;
use forklaunch_core::http::{instance_gateway::InstanceGatewayClient, provider_http::ProviderHttpClient};
use std::time::Duration;
{{/stripe}}use forklaunch_core::{
    domain::AuditContext,
    persistence::{TenantContext, encryption::FieldEncryptor},
};
use sqlx::PgPool;
use std::sync::Arc;

/// The module's routes, collected before mounting.
pub type Routes = BillingRoutes;

pub struct AppState {
    pub config: ServiceConfig,
    pub pool: PgPool,
    /// Present only when a selected bound capability publishes provider effects.
    pub publisher_pool: Option<PgPool>,
    pub billing: BillingHttp,
}

impl AppState {
    pub async fn close(&self) {
        if let Some(pool) = &self.publisher_pool {
            pool.close().await;
        }
        self.pool.close().await;
    }
}

pub async fn register(config: ServiceConfig) -> Result<AppState> {
    let pool = database::connect().await?;
    let bound_checkout = profiles::bound_checkout_config_from_env(STRIPE)?;
    let bound_billing = profiles::bound_billing_config_from_env(STRIPE)?;
    let store = PostgresBillingStore::new(
        pool.clone(),
        TenantContext::new(config.tenant_id.clone())?,
        Arc::new(FieldEncryptor::from_env()?),
        AuditContext::system("billing-startup"),
    )
    .await?;
    let auth = config.module_auth()?;
    let publisher_pool = if bound_checkout.is_some() || bound_billing.as_ref().is_some_and(|c| c.requires_publisher()) {
        Some(database::connect_publisher().await?)
    } else {
        None
    };
    let publisher = |selected: bool| -> Result<Option<PgPool>> {
        if !selected {
            return Ok(None);
        }
        Ok(Some(publisher_pool.clone().ok_or("Missing publisher")?))
    };
    let startup_store = store.with_audit_context(AuditContext::actor("service:bound-billing-startup", "billing-startup"))?;
    let checkout_publisher = match publisher(bound_checkout.is_some() || bound_billing.as_ref().is_some_and(|c| c.has_checkout()))? {
        Some(pool) => Some(startup_store.trusted_checkout_publisher(pool).await?),
        None => None,
    };
    let portal_publisher = match publisher(bound_billing.as_ref().is_some_and(|c| c.has_portal()))? {
        Some(pool) => Some(startup_store.trusted_portal_publisher(pool).await?),
        None => None,
    };
    let subscription_publisher =
        match publisher(bound_billing.as_ref().is_some_and(|c| c.has_subscription() || c.has_subscription_recovery()))? {
            Some(pool) => Some(startup_store.trusted_subscription_publisher(pool).await?),
            None => None,
        };
    let expiration_publisher = match publisher(
        bound_billing.as_ref().is_some_and(|c| c.has_payment_link_expiration() || c.has_payment_link_expiration_recovery()),
    )? {
        Some(pool) => Some(startup_store.trusted_payment_link_expiration_publisher(pool).await?),
        None => None,
    };
    let backend = backend(&config, store)?
        .with_database_backup(config.database_backup)
        .with_request_dates(config.request_dates);
    let backend = if let Some(configuration) = bound_billing {
        backend
            .with_bound_billing(BoundBillingV1::trusted_with_payment_link_expiration(
                configuration,
                checkout_publisher,
                portal_publisher,
                subscription_publisher,
                expiration_publisher,
            )?)
            .await?
    } else {
        match (bound_checkout, checkout_publisher) {
            (Some(configuration), Some(publisher)) => backend.with_bound_checkout(BoundCheckoutV1::trusted(configuration, publisher)).await?,
            (None, None) => backend,
            _ => return Err("Incomplete bound billing configuration".into()),
        }
    };
    let billing = BillingHttp::new(backend, auth, service_info());
    Ok(AppState { config, pool, publisher_pool, billing })
}
{{#stripe}}
/// Stripe with the configured key, or the managed gateway when no key is set.
fn backend(config: &ServiceConfig, store: PostgresBillingStore) -> Result<BillingBackend> {
    let stripe = &config.stripe;
    let client = if let Some(secret) = stripe.api_key.clone() {
        match &stripe.api_base {
            Some(base) => StripeBillingClient::with_transport(secret, ProviderHttpClient::new(base, Duration::from_secs(15), 1024 * 1024)?)?,
            None => StripeBillingClient::new(secret)?,
        }
    } else if stripe.gateway_url.is_some() && stripe.instance_id.is_some() && stripe.instance_key.is_some() {
        StripeBillingClient::with_gateway(InstanceGatewayClient::from_env()?)
    } else if stripe.gateway_url.is_some() || stripe.instance_id.is_some() {
        return Err("Managed Stripe requires PLATFORM_GATEWAY_URL, INSTANCE_ID and INSTANCE_HMAC_KEY together".into());
    } else {
        return Err("Configure STRIPE_API_KEY (or STRIPE_SECRET_KEY), or the managed gateway with PLATFORM_GATEWAY_URL, INSTANCE_ID and INSTANCE_HMAC_KEY".into());
    };
    let mut backend = BillingBackend::stripe(store, client)
        .with_configured_hmac(true)
        .with_configured_subscription_updates(true)
        .with_configured_cached_reads(true)
        .with_configured_payment_link_updates(true);
    if let Some(secret) = stripe.webhook_secret.clone() {
        backend = backend.with_webhook_secret(secret)?;
    }
    if let Some(secret) = stripe.instance_key.clone() {
        backend = backend.with_platform_event_key(secret)?;
    }
    Ok(backend)
}
{{/stripe}}{{^stripe}}
fn backend(_: &ServiceConfig, store: PostgresBillingStore) -> Result<BillingBackend> {
    Ok(BillingBackend::base(store))
}
{{/stripe}}