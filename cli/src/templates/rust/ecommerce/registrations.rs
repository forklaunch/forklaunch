//! Builds the ecommerce store, order-event queue, payment providers and HTTP
//! handlers from trusted configuration.
use crate::{
    config::{ServiceConfig, StoreConfig},
    domain::services::PostgresEcommerceStore,
    error::Result,
    persistence::database,
    service_info,
};
use forklaunch_core::{
    domain::AuditContext,
    http::provider_http::ProviderHttpClient,
    persistence::{TenantContext, encryption::FieldEncryptor},
};
use forklaunch_ecommerce_paypal::PaypalClient;
use forklaunch_ecommerce_stripe::{
    StripeClient, StripeConnect,
    http::{EcommerceBackend, EcommerceHttp, EcommerceRoutes},
    webhook::StripeWebhookVerifier,
};
use forklaunch_worker_database::postgres::PostgresWorkerQueue;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};

/// The module's routes, collected before mounting.
pub type Routes = EcommerceRoutes;

pub struct AppState {
    pub config: ServiceConfig,
    pub pool: PgPool,
    pub ecommerce: EcommerceHttp,
}

impl AppState {
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// The tenant-bound store and the order-event queue it produces into.
pub struct Stores {
    pub pool: PgPool,
    pub store: PostgresEcommerceStore,
    pub queue: PostgresWorkerQueue,
}

pub async fn stores(config: &StoreConfig) -> Result<Stores> {
    let pool = database::connect().await?;
    let tenant = TenantContext::new(config.tenant_id.clone())?;
    let store = PostgresEcommerceStore::new(
        pool.clone(),
        tenant.clone(),
        Arc::new(FieldEncryptor::from_env()?),
        AuditContext::system("ecommerce-startup"),
    )
    .await?;
    let queue = PostgresWorkerQueue::new(pool.clone(), tenant, config.retry).await?;
    Ok(Stores { pool, store, queue })
}

pub async fn register(config: ServiceConfig, stores: Stores) -> Result<AppState> {
    let auth = config.module_auth()?;
    let stripe = &config.stripe;
    let client = match &stripe.api_base {
        Some(base) => StripeClient::with_transport(stripe.api_key.clone(), ProviderHttpClient::new(base, Duration::from_secs(15), 1024 * 1024)?)?,
        None => StripeClient::new(stripe.api_key.clone())?,
    };
    let mut backend = EcommerceBackend::new(stores.store, stores.queue, client)
        .with_stripe_webhook(StripeWebhookVerifier::new(stripe.webhook_secret.clone())?);
    if let Some((account, fee)) = stripe.connect.clone() {
        backend = backend.with_connect(StripeConnect::new(account, fee)?);
    }
    if let Some(paypal) = &config.paypal {
        backend = backend
            .with_paypal(PaypalClient::new(paypal.client_id.clone(), paypal.client_secret.clone(), &paypal.base_url)?)
            .with_paypal_webhook_id(paypal.webhook_id.clone())?;
    }
    let ecommerce = EcommerceHttp::new(backend, auth, service_info());
    Ok(AppState { config, pool: stores.pool, ecommerce })
}
