//! PostgreSQL connections and schema migration for the billing store.
use crate::{domain::services::PostgresBillingStore, error::Result};
use sqlx::{PgPool, postgres::PgPoolOptions};
use forklaunch_core::config;

/// Connect with the restricted runtime role; privileged roles are rejected by the store.
pub async fn connect() -> Result<PgPool> {
    let url: String = config::required("DATABASE_URL")?;
    Ok(PgPoolOptions::new().max_connections(10).connect(&url).await.map_err(|_| "Unable to connect to PostgreSQL")?)
}

/// The separate role that publishes bound provider effects.
pub async fn connect_publisher() -> Result<PgPool> {
    let url: String = config::required("BILLING_PUBLISHER_DATABASE_URL")?;
    Ok(PgPoolOptions::new().max_connections(4).connect(&url).await.map_err(|_| "Unable to connect to billing publisher database")?)
}

/// Apply migrations with a schema-owner `DATABASE_URL`.
pub async fn migrate() -> Result<()> {
    let pool = connect().await?;
    PostgresBillingStore::migrate(&pool).await?;
    pool.close().await;
    Ok(())
}
