//! Generic retention integration for configured ecommerce Order metadata.
//! No default retention period is imposed. Applications resolve calendar cutoffs
//! and select a policy explicitly through the framework retention API.
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{TenantContext, retention::RetentionRequest},
};

pub use crate::storage_compliance::OrderCompliance;
pub(crate) fn validate(
    request: RetentionRequest<'_>,
    tenant: Option<&TenantContext>,
) -> FrameworkResult<()> {
    if tenant.is_none_or(|tenant| tenant.id() != request.tenant.id())
        || request.policy.entity() != "Order"
        || !request.policy.anonymize_fields().is_empty()
        || !(1..=10_000).contains(&request.batch_size)
        || request.policy.cutoff() > request.now
    {
        return Err(FrameworkError::new(
            "ecommerce_retention_invalid",
            "Invalid ecommerce retention scope or policy",
        ));
    }
    Ok(())
}
pub(crate) fn storage_error<T>(_: T) -> FrameworkError {
    FrameworkError::new(
        "ecommerce_retention_storage_error",
        "Unable to complete ecommerce retention operation",
    )
}
