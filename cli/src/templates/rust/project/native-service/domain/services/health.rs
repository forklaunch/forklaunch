use crate::{domain::schemas::HealthResponse, persistence::InMemoryRepository};
#[derive(Clone)]
pub struct HealthService { name: String, repository: InMemoryRepository }
impl HealthService {
    pub fn new(name: String, repository: InMemoryRepository) -> Self { Self { name, repository } }
    pub fn report(&self) -> HealthResponse {
        HealthResponse { service: self.name.clone(), status: "ok".into(),
            record_count: self.repository.count().unwrap_or_default(), routes: vec!["health".into()] }
    }
}
