//! Actual serialized encrypted queue envelope, independent of broker backend.
//! Describes only the application-owned payload, not adapter scheduler/lease/DLQ
//! columns. Public context is authenticated; only ciphertext holds protected data.
#![allow(dead_code)]
use forklaunch_core::persistence::compliance::{ComplianceEntity, ComplianceMetadata};
use uuid::Uuid;
#[derive(ComplianceMetadata)]
#[compliance(entity = "SealedEvent")]
struct QueueEnvelope {
    #[compliance(level = "none")]
    channel: String,
    #[compliance(level = "none")]
    context: QueueContext,
    #[compliance(level = "pii")]
    ciphertext: String,
}
#[derive(ComplianceMetadata)]
#[compliance(entity = "JobContext")]
struct QueueContext {
    #[compliance(level = "none", name = "tenantId")]
    tenant_id: Uuid,
    #[compliance(level = "none", name = "resourceId")]
    resource_id: Uuid,
    #[compliance(level = "none", name = "operationId")]
    operation_id: Uuid,
    #[compliance(level = "none")]
    revision: i64,
}
pub fn declarations() -> Vec<ComplianceEntity> {
    vec![
        ComplianceEntity::of::<QueueEnvelope>("job.payload_json", "serialized-encrypted-event"),
        ComplianceEntity::of::<QueueContext>("job.payload_json.context", "serialized-routing-context"),
    ]
}
