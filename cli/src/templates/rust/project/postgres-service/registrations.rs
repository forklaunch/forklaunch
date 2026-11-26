use crate::{config::ServiceConfig, domain::services::HealthService, error::{AppError, Result},
    persistence::{PostgresRepository, database::DatabaseConfig}};
use forklaunch_axum::native::{SessionPolicy, optional_jwt_policy_from_env};
use forklaunch_core::telemetry::OpenTelemetryCollector;
#[derive(Clone)]
pub struct AppState {
    pub config: ServiceConfig,
    pub health: HealthService,
    pub repository: PostgresRepository,
    pub telemetry: OpenTelemetryCollector,
    /// IAM session JWTs verified against `JWKS_PUBLIC_KEY_URL` (see `api::policy`).
    /// Without that setting every protected route answers 401.
    pub auth: SessionPolicy,
}
pub async fn register(config: ServiceConfig) -> Result<AppState> {
    config.validate()?;
    let repository = PostgresRepository::connect(DatabaseConfig::from_env()?).await?;
    let telemetry = OpenTelemetryCollector::new(config.name.clone());
    let auth = optional_jwt_policy_from_env().map_err(|error| AppError::InvalidConfig(error.to_string()))?;
    Ok(AppState { auth, telemetry, health: HealthService::new(config.name.clone(), repository.clone()), repository, config })
}
