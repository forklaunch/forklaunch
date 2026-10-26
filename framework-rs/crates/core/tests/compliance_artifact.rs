#![cfg(feature = "compliance-artifact")]
use forklaunch_core::persistence::compliance::{
    ComplianceEntity, ComplianceMetadata, compliance_manifest,
};
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct StoredContact {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "pii", name = "email_ciphertext", nullable)]
    email: Option<String>,
}
#[test]
fn artifact_preserves_storage_mapping_and_reports_unverified_coverage() {
    let artifact = compliance_manifest(&[ComplianceEntity::of::<StoredContact>(
        "contacts", "postgres",
    )]);
    assert_eq!(artifact["coverage"], "declared-storage-members-only");
    assert_eq!(
        artifact["runtimeEnforcement"],
        "requires-independent-verification"
    );
    assert_eq!(artifact["lifecycleCoverage"], "unknown");
    let fields = &artifact["entities"][0]["fields"];
    assert_eq!(fields[1]["column"], "email_ciphertext");
    assert_eq!(fields[1]["classification"], "pii");
    assert_eq!(fields[1]["nullable"], true);
    assert_eq!(fields[0]["classification"], "none");
}
