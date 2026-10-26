//! Canonical declared logical storage members copied from the explicit TS blueprint policies.
//! Source: blueprint/iam-base/persistence/entities/*.entity.ts.
//! These declarations do not classify inherited fields, relations, unmapped credential stores,
//! index/lease/provider-control/audit tables or arbitrary extension members.
//! The encrypted-container location describes ownership, not encryption proof.
//! Explicit TS `none` remains `none`; changing it is an application policy decision.
#![allow(dead_code)]
use forklaunch_core::persistence::compliance::{
    ComplianceEntity, ComplianceField, ComplianceLevel, ComplianceMetadata,
};
pub struct UserStorage;
impl ComplianceMetadata for UserStorage {
    const ENTITY: &'static str = "User";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "email",
            storage_name: "email",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "firstName",
            storage_name: "firstName",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "lastName",
            storage_name: "lastName",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "phoneNumber",
            storage_name: "phoneNumber",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "subscription",
            storage_name: "subscription",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "providerFields",
            storage_name: "providerFields",
            level: ComplianceLevel::None,
            nullable: true,
        },
    ];
}
pub struct OrganizationStorage;
impl ComplianceMetadata for OrganizationStorage {
    const ENTITY: &'static str = "Organization";
    const FIELDS: &'static [ComplianceField] = &[
        ComplianceField {
            rust_name: "name",
            storage_name: "name",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "domain",
            storage_name: "domain",
            level: ComplianceLevel::None,
            nullable: false,
        },
        ComplianceField {
            rust_name: "logoUrl",
            storage_name: "logoUrl",
            level: ComplianceLevel::None,
            nullable: true,
        },
        ComplianceField {
            rust_name: "subscription",
            storage_name: "subscription",
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
            rust_name: "status",
            storage_name: "status",
            level: ComplianceLevel::None,
            nullable: false,
        },
    ];
}
pub struct RoleStorage;
impl ComplianceMetadata for RoleStorage {
    const ENTITY: &'static str = "Role";
    const FIELDS: &'static [ComplianceField] = &[ComplianceField {
        rust_name: "name",
        storage_name: "name",
        level: ComplianceLevel::None,
        nullable: false,
    }];
}
pub struct PermissionStorage;
impl ComplianceMetadata for PermissionStorage {
    const ENTITY: &'static str = "Permission";
    const FIELDS: &'static [ComplianceField] = &[ComplianceField {
        rust_name: "slug",
        storage_name: "slug",
        level: ComplianceLevel::None,
        nullable: false,
    }];
}
pub fn declarations() -> Vec<ComplianceEntity> {
    vec![
        ComplianceEntity::of::<UserStorage>(
            "fl_iam_directory.data[kind=user]",
            "json-record-members",
        ),
        ComplianceEntity::of::<OrganizationStorage>(
            "fl_iam_directory.data[kind=organization]",
            "json-record-members",
        ),
        ComplianceEntity::of::<RoleStorage>(
            "fl_iam_directory.data[kind=role]",
            "json-record-members",
        ),
        ComplianceEntity::of::<PermissionStorage>(
            "fl_iam_directory.data[kind=permission]",
            "json-record-members",
        ),
    ]
}
