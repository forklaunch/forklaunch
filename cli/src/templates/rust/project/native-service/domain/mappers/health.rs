use std::sync::Arc;
use forklaunch_core::mappers::CheckedMapper;
use crate::{domain::schemas::HealthResponse, error::{AppError, Result}};

/// Projects repository statistics into the public health contract.
/// No protected entity values are read or declassified here.
#[derive(Clone)]
pub struct HealthMapper { response: Arc<CheckedMapper<HealthResponse>> }
impl HealthMapper {
    pub fn new() -> Result<Self> {
        Ok(Self { response: Arc::new(CheckedMapper::new().map_err(|_| AppError::InvalidConfig("invalid health mapping schema".into()))?) })
    }
    pub async fn report(&self, service: &str, record_count: usize) -> Result<HealthResponse> {
        self.response.to_dto((service, record_count), |(service, record_count)| async move {
            Ok(HealthResponse { service: service.into(), status: "ok".into(), record_count, routes: vec!["health".into()] })
        }).await.map_err(|_| AppError::Mapping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn maps_statistics_and_rejects_nonportable_count() {
        let mapper = HealthMapper::new().unwrap();
        let report = mapper.report("records", 3).await.unwrap();
        assert_eq!(report.service, "records");
        assert_eq!(report.record_count, 3);
        assert_eq!(report.routes, ["health"]);
        #[cfg(target_pointer_width = "64")]
        assert!(matches!(mapper.report("records", 9_007_199_254_740_992).await, Err(AppError::Mapping)));
    }
}
