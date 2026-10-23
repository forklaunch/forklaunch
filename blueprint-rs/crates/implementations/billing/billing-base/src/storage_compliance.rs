//! Canonical declared logical storage members copied from the explicit TS blueprint policies.
//! Source: blueprint/billing-base/persistence/entities/*.entity.ts.
//! These declarations do not classify inherited fields, relations, unmapped credential stores,
//! index/lease/provider-control/audit tables or arbitrary extension members.
//! The encrypted-container location describes ownership, not encryption proof.
//! Explicit TS `none` remains `none`; changing it is an application policy decision.
#![allow(dead_code)]
use forklaunch_core::persistence::compliance::{
    ComplianceEntity, ComplianceField, ComplianceLevel, ComplianceMetadata,
};
pub struct PlanStorage;
impl ComplianceMetadata for PlanStorage {
    const ENTITY: &'static str = "Plan";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "active",
            storage_name: "active",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "name",
            storage_name: "name",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "description",
            storage_name: "description",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "price",
            storage_name: "price",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "currency",
            storage_name: "currency",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "cadence",
            storage_name: "cadence",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "features",
            storage_name: "features",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "providerFields",
            storage_name: "providerFields",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "externalId",
            storage_name: "externalId",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "billingProvider",
            storage_name: "billingProvider",
            level: ComplianceLevel::None,
            nullable: true,
        },
    ];
}
pub struct SubscriptionStorage;
impl ComplianceMetadata for SubscriptionStorage {
    const ENTITY: &'static str = "Subscription";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "partyId",
            storage_name: "partyId",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "partyType",
            storage_name: "partyType",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "description",
            storage_name: "description",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "active",
            storage_name: "active",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "productId",
            storage_name: "productId",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "providerFields",
            storage_name: "providerFields",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "externalId",
            storage_name: "externalId",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "billingProvider",
            storage_name: "billingProvider",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "startDate",
            storage_name: "startDate",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "endDate",
            storage_name: "endDate",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "status",
            storage_name: "status",
            level: ComplianceLevel::None,
            nullable: false,
        },
    ];
}
pub struct CheckoutSessionStorage;
impl ComplianceMetadata for CheckoutSessionStorage {
    const ENTITY: &'static str = "CheckoutSession";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "customerId",
            storage_name: "customerId",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "paymentMethods",
            storage_name: "paymentMethods",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "currency",
            storage_name: "currency",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "uri",
            storage_name: "uri",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "successRedirectUri",
            storage_name: "successRedirectUri",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "cancelRedirectUri",
            storage_name: "cancelRedirectUri",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "expiresAt",
            storage_name: "expiresAt",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "status",
            storage_name: "status",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "providerFields",
            storage_name: "providerFields",
            level: ComplianceLevel::None,
            nullable: true,
        },
    ];
}
pub struct PaymentLinkStorage;
impl ComplianceMetadata for PaymentLinkStorage {
    const ENTITY: &'static str = "PaymentLink";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "amount",
            storage_name: "amount",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "paymentMethods",
            storage_name: "paymentMethods",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "currency",
            storage_name: "currency",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "description",
            storage_name: "description",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "status",
            storage_name: "status",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "providerFields",
            storage_name: "providerFields",
            level: ComplianceLevel::None,
            nullable: true,
        },
    ];
}
pub struct BillingPortalStorage;
impl ComplianceMetadata for BillingPortalStorage {
    const ENTITY: &'static str = "BillingPortal";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "customerId",
            storage_name: "customerId",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "uri",
            storage_name: "uri",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "expiresAt",
            storage_name: "expiresAt",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "providerFields",
            storage_name: "providerFields",
            level: ComplianceLevel::None,
            nullable: true,
        },
    ];
}
pub fn declarations() -> Vec<ComplianceEntity> {
    vec![
        ComplianceEntity::of::<PlanStorage>(
            "fl_billing.records.data[kind=plan]",
            "json-record-members",
        ),
        ComplianceEntity::of::<SubscriptionStorage>(
            "fl_billing.records.data[kind=subscription]",
            "json-record-members",
        ),
        ComplianceEntity::of::<CheckoutSessionStorage>(
            "fl_billing.records.data[kind=checkout_session]",
            "json-record-members",
        ),
        ComplianceEntity::of::<PaymentLinkStorage>(
            "fl_billing.records.data[kind=payment_link]",
            "json-record-members",
        ),
        ComplianceEntity::of::<BillingPortalStorage>(
            "fl_billing.records.data[kind=billing_portal]",
            "json-record-members",
        ),
    ]
}
