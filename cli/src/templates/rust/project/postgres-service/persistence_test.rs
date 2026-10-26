use std::{sync::atomic::{AtomicUsize, Ordering}, env};
use forklaunch_core::persistence::{TenantContext, begin_tenant_transaction, connect_runtime_postgres,
    protected::{ComplianceAccess, ComplianceAudit, ProtectedContext}};
use forklaunch_core::{encryption::{EnvelopeKey, EnvelopeProfile}, common::{FrameworkError, FrameworkResult}};
use {{crate_module}}::persistence::{PostgresRepository, database::DatabaseConfig};
use uuid::Uuid;

struct Audit(AtomicUsize);
impl ComplianceAudit for Audit {
    fn record(&self, event: &ComplianceAccess<'_>) -> FrameworkResult<()> {
        assert_eq!(event.reason, "fixture verification");
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[tokio::test]
#[ignore = "requires migrated isolated PostgreSQL and a restricted runtime role"]
async fn encrypted_records_are_tenant_bound_and_explicitly_audited() {
    let config=DatabaseConfig::from_env().unwrap();
    let tenant=config.tenant;
    let pool=connect_runtime_postgres(&config.url, config.policy).await.unwrap();
    let repository=PostgresRepository::connect(DatabaseConfig::from_env().unwrap()).await.unwrap();
    let id=Uuid::new_v4();
    repository.insert(id, "sensitive fixture name").await.unwrap();
    let protected=repository.protected_name(id).await.unwrap().unwrap();
    assert!(!protected.ciphertext().contains("sensitive fixture name"));
    let audit=Audit(AtomicUsize::new(0));
    let key=EnvelopeKey::from_env(EnvelopeProfile::new(&env::var("STORAGE_ENCRYPTION_NAMESPACE").unwrap()).unwrap()).unwrap();
    let value=protected.reveal(&key, ProtectedContext::new(tenant,id).unwrap(), "fixture", "fixture verification", &audit).unwrap();
    assert_eq!(&*value, b"sensitive fixture name");
    assert_eq!(audit.0.load(Ordering::SeqCst),1);
    assert!(protected.reveal(&key, ProtectedContext::new(Uuid::new_v4(),id).unwrap(), "fixture", "fixture verification", &audit).is_err());
    assert_eq!(audit.0.load(Ordering::SeqCst),1);
    let mut other=DatabaseConfig::from_env().unwrap();
    other.tenant=Uuid::new_v4();
    let other=PostgresRepository::connect(other).await.unwrap();
    assert!(other.protected_name(id).await.unwrap().is_none());
    assert_eq!(other.count().await.unwrap(),0);
    let mut transaction=begin_tenant_transaction(&pool,&TenantContext::new(tenant.to_string()).unwrap()).await.unwrap();
    sqlx::query("UPDATE records SET name_ciphertext='legacy plaintext' WHERE tenant_id=$1 AND id=$2").bind(tenant.to_string()).bind(id).execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
    assert!(repository.protected_name(id).await.is_err());
    let mut transaction=begin_tenant_transaction(&pool,&TenantContext::new(tenant.to_string()).unwrap()).await.unwrap();
    sqlx::query("DELETE FROM records WHERE tenant_id=$1 AND id=$2").bind(tenant.to_string()).bind(id).execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
    other.close().await;
    repository.close().await;
    pool.close().await;
}


#[test]
fn actual_persisted_model_enforces_binding_and_acknowledged_disclosure() {
    use {{crate_module}}::persistence::entities::StoredRecord;
    use forklaunch_core::persistence::protected::{ProtectedField, ProtectedMember};
    struct RejectAudit;
    impl ComplianceAudit for RejectAudit {
        fn record(&self, _: &ComplianceAccess<'_>) -> FrameworkResult<()> {
            Err(FrameworkError::new("audit_unavailable", "Audit unavailable"))
        }
    }
    let tenant = Uuid::new_v4();
    let id = Uuid::new_v4();
    let context = ProtectedContext::new(tenant, id).unwrap();
    let key = EnvelopeKey::new(EnvelopeProfile::new("record-fixture").unwrap(), [41; 32]);
    let row = StoredRecord {
        id, tenant_id: tenant.to_string(),
        name_ciphertext: ProtectedField::seal(&key, context, StoredRecord::protected_name_ciphertext().unwrap(), b"protected-record-canary").unwrap(),
    };
    row.validate().unwrap();
    assert!(!format!("{row:?}").contains("protected-record-canary"));
    assert!(!row.name_ciphertext.ciphertext().contains("protected-record-canary"));
    assert!(row.name_ciphertext.reveal(&key, context, "fixture", "fixture verification", &RejectAudit).is_err());
    let audit = Audit(AtomicUsize::new(0));
    let plaintext = row.name_ciphertext.reveal(&key, context, "fixture", "fixture verification", &audit).unwrap();
    assert_eq!(&*plaintext, b"protected-record-canary");
    assert_eq!(audit.0.load(Ordering::SeqCst), 1);
    assert!(row.name_ciphertext.reveal(&key, ProtectedContext::new(Uuid::new_v4(), id).unwrap(), "fixture", "fixture verification", &audit).is_err());
    assert_eq!(audit.0.load(Ordering::SeqCst), 1);
    let wrong = ProtectedMember::from_entity::<StoredRecord>("name_ciphertext", "different-purpose").unwrap();
    let mut wrong_row = row;
    wrong_row.name_ciphertext = ProtectedField::seal(&key, context, wrong, b"wrong-binding").unwrap();
    assert!(wrong_row.validate().is_err());
}
