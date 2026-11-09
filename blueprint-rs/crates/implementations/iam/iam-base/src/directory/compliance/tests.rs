use super::*;
use crate::directory::memory::MemoryDirectoryStore;
use serde_json::json;
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
#[compliance(entity = "User")]
struct UserMetadata {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "pii")]
    email: String,
    #[compliance(level = "phi", name = "firstName")]
    first_name: String,
    #[compliance(level = "none")]
    unclassified: String,
}
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
#[compliance(entity = "Role")]
struct RoleMetadata {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "none")]
    owner: String,
    #[compliance(level = "pci")]
    label: String,
}
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct NoneMetadata {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "none")]
    email: String,
}
fn config() -> DirectoryCompliance {
    DirectoryCompliance::default()
        .with_entity::<UserMetadata>(DirectoryKind::User, "id")
        .unwrap()
        .with_entity::<RoleMetadata>(DirectoryKind::Role, "owner")
        .unwrap()
}
/// Shared against the memory and actual encrypted `PostgreSQL` stores.
pub(crate) async fn exercise<S: DirectoryStore>(store: Arc<S>, other: Arc<S>) {
    for target in [&store, &other] {
        let mut tx = target.begin(true).await.unwrap();
        tx.insert(DirectoryKind::User,json!({"id":"compliance-subject","email":"compliance-subject@fixture.test","firstName":"Classified","unclassified":"keep-private","roles":[]})).await.unwrap();
        tx.insert(DirectoryKind::User,json!({"id":"compliance-foreign","email":"compliance-foreign@fixture.test","firstName":"Foreign","roles":[]})).await.unwrap();
        tx.insert(DirectoryKind::Role,json!({"id":"compliance-role","owner":"compliance-subject","label":"Classified role","permissions":[]})).await.unwrap();
        tx.commit().await.unwrap();
    }
    let unclassified = DirectoryComplianceService::new(
        store.clone(),
        DirectoryCompliance::default()
            .with_entity::<NoneMetadata>(DirectoryKind::User, "id")
            .unwrap(),
    );
    assert!(
        unclassified
            .export("compliance-subject")
            .await
            .unwrap()
            .entities
            .is_empty()
    );
    assert_eq!(
        unclassified
            .erase("compliance-subject")
            .await
            .unwrap()
            .records_deleted,
        0
    );
    let service = DirectoryComplianceService::new(store.clone(), config());
    let exported = service.export("compliance-subject").await.unwrap();
    assert!(exported.failures.is_empty());
    assert_eq!(
        exported.entities["User"],
        vec![
            json!({"id":"compliance-subject","email":"compliance-subject@fixture.test","firstName":"Classified"})
        ]
    );
    assert_eq!(
        exported.entities["Role"],
        vec![json!({"id":"compliance-role","label":"Classified role"})]
    );
    let erased = service.erase("compliance-subject").await.unwrap();
    assert_eq!(erased.records_deleted, 2);
    assert!(erased.failures.is_empty());
    assert_eq!(erased.entities_affected, vec!["User", "Role"]);
    assert_eq!(
        service
            .erase("compliance-subject")
            .await
            .unwrap()
            .records_deleted,
        0
    );
    assert!(
        service
            .export("compliance-subject")
            .await
            .unwrap()
            .entities
            .is_empty()
    );
    let mut tx = store.begin(false).await.unwrap();
    assert!(
        tx.get(DirectoryKind::User, "compliance-foreign")
            .await
            .unwrap()
            .is_some()
    );
    tx.commit().await.unwrap();
    let foreign = DirectoryComplianceService::new(other.clone(), config());
    assert_eq!(
        foreign
            .export("compliance-subject")
            .await
            .unwrap()
            .entities
            .len(),
        2
    );
    assert!(service.erase("").await.is_err());
    assert!(service.export("subject\nother").await.is_err());
    // Remove only fixture rows, allowing the shared PG suite to continue.
    for target in [&store, &other] {
        let mut tx = target.begin(true).await.unwrap();
        for (kind, id) in [
            (DirectoryKind::User, "compliance-subject"),
            (DirectoryKind::User, "compliance-foreign"),
            (DirectoryKind::Role, "compliance-role"),
        ] {
            tx.delete(kind, id).await.unwrap();
        }
        tx.commit().await.unwrap();
    }
}
#[tokio::test]
async fn classifications_are_explicit_and_tenant_bound() {
    exercise(
        Arc::new(MemoryDirectoryStore::default()),
        Arc::new(MemoryDirectoryStore::default()),
    )
    .await;
}
#[test]
fn invalid_and_duplicate_entity_mappings_fail() {
    assert!(
        DirectoryCompliance::default()
            .with_entity::<UserMetadata>(DirectoryKind::User, "missing")
            .is_err()
    );
    assert!(
        config()
            .with_entity::<UserMetadata>(DirectoryKind::Organization, "id")
            .is_err()
    );
    assert!(
        config()
            .with_entity::<NoneMetadata>(DirectoryKind::User, "id")
            .is_err()
    );
}

struct FailedCommitStore(MemoryDirectoryStore);
struct FailedCommitTransaction(crate::directory::memory::MemoryDirectoryTransaction);
impl DirectoryStore for FailedCommitStore {
    type Transaction<'a> = FailedCommitTransaction;
    async fn begin(&self, write: bool) -> BlueprintResult<Self::Transaction<'_>> {
        Ok(FailedCommitTransaction(self.0.begin(write).await?))
    }
}
impl DirectoryTransaction for FailedCommitTransaction {
    async fn get(&mut self, kind: DirectoryKind, id: &str) -> BlueprintResult<Option<Value>> {
        self.0.get(kind, id).await
    }
    async fn list(
        &mut self,
        kind: DirectoryKind,
        ids: Option<&[String]>,
        organization: Option<&str>,
    ) -> BlueprintResult<Vec<Value>> {
        self.0.list(kind, ids, organization).await
    }
    async fn insert(&mut self, kind: DirectoryKind, value: Value) -> BlueprintResult<()> {
        self.0.insert(kind, value).await
    }
    async fn replace(&mut self, kind: DirectoryKind, value: Value) -> BlueprintResult<()> {
        self.0.replace(kind, value).await
    }
    async fn delete(&mut self, kind: DirectoryKind, id: &str) -> BlueprintResult<()> {
        self.0.delete(kind, id).await
    }
    async fn commit(self) -> BlueprintResult<()> {
        Err(BlueprintError::new(
            "private_database_details",
            "private row contents",
        ))
    }
}
#[tokio::test]
async fn failed_commit_rolls_back_and_never_reports_a_complete_export_or_erase() {
    let store = MemoryDirectoryStore::default();
    let mut tx = store.begin(true).await.unwrap();
    tx.insert(
        DirectoryKind::User,
        json!({"id":"subject","email":"private@fixture.test","roles":[]}),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let service = DirectoryComplianceService::new(
        Arc::new(FailedCommitStore(store.clone())),
        DirectoryCompliance::default()
            .with_entity::<UserMetadata>(DirectoryKind::User, "id")
            .unwrap(),
    );
    let exported = service.export("subject").await.unwrap();
    assert!(exported.entities.is_empty());
    assert_eq!(exported.failures.len(), 1);
    let erased = service.erase("subject").await.unwrap();
    assert_eq!(erased.records_deleted, 0);
    assert!(erased.entities_affected.is_empty());
    assert_eq!(erased.failures.len(), 1);
    assert!(!serde_json::to_string(&erased).unwrap().contains("private"));
    let mut tx = store.begin(false).await.unwrap();
    assert!(
        tx.get(DirectoryKind::User, "subject")
            .await
            .unwrap()
            .is_some()
    );
}
