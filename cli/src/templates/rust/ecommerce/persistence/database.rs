//! PostgreSQL connection and schema migration for the ecommerce store and its order-event queue.
use crate::{domain::services::PostgresEcommerceStore, error::Result};
use forklaunch_worker_database::postgres::PostgresWorkerQueue;
use sqlx::{PgPool, postgres::PgPoolOptions};
use forklaunch_core::config;

/// Connect with the restricted runtime role; privileged roles are rejected by the stores.
pub async fn connect() -> Result<PgPool> {
    let url: String = config::required("DATABASE_URL")?;
    Ok(PgPoolOptions::new().max_connections(10).connect(&url).await.map_err(|_| "Unable to connect to PostgreSQL")?)
}

/// Apply migrations with a schema-owner `DATABASE_URL`.
pub async fn migrate() -> Result<()> {
    let pool = connect().await?;
    PostgresEcommerceStore::migrate(&pool).await?;
    PostgresWorkerQueue::migrate(&pool).await?;
    pool.close().await;
    Ok(())
}
