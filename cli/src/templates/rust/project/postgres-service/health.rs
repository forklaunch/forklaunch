use crate::{domain::schemas::HealthResponse, persistence::PostgresRepository, error::Result};
#[derive(Clone)]
pub struct HealthService { name: String, repository: PostgresRepository }
impl HealthService {
    pub fn new(name: String, repository: PostgresRepository) -> Self { Self { name, repository } }
    pub async fn report(&self) -> Result<HealthResponse> {
        Ok(HealthResponse { service: self.name.clone(), status: "ok".into(),
            record_count: self.repository.count().await?, routes: vec!["health".into()] })
    }
}
