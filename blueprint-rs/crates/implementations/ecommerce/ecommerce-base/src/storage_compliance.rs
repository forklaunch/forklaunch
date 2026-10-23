//! Existing Order policy, shared by retention and build-time storage metadata.
//! Covers only these declared Order JSON members, not every ecommerce table.
use forklaunch_core::persistence::compliance::{
    ComplianceEntity, ComplianceField, ComplianceLevel, ComplianceMetadata,
};
/// The configured shipping address is classified PII but non-nullable. Therefore
/// anonymization records its completion without clearing that required address,
/// matching TypeScript. A delete policy removes the entire Order instead.
pub struct OrderCompliance;
impl ComplianceMetadata for OrderCompliance {
    const ENTITY: &'static str = "Order";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "id",
            storage_name: "id",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "customer_id",
            storage_name: "customerId",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "shipping_address",
            storage_name: "shippingAddress",
            level: ComplianceLevel::Pii,
            nullable: false,
        },
        ComplianceField {
            rust_name: "created_at",
            storage_name: "createdAt",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "updated_at",
            storage_name: "updatedAt",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "retention_anonymized_at",
            storage_name: "retentionAnonymizedAt",
            level: ComplianceLevel::None,
            nullable: true,
        },
    ];
}
#[must_use]
pub fn declarations() -> Vec<ComplianceEntity> {
    vec![ComplianceEntity::of::<OrderCompliance>(
        "fl_ecommerce.records.data[kind=order]",
        "json-record-members",
    )]
}
