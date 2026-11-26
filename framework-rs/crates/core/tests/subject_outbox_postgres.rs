#![cfg(all(
    any(feature = "postgres", feature = "postgres09"),
    feature = "encryption"
))]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
#[cfg(not(feature = "postgres09"))]
use forklaunch_core::persistence::subject::outbox::*;
#[cfg(feature = "postgres09")]
use forklaunch_core::persistence::subject::outbox09::*;
#[cfg(not(feature = "postgres09"))]
use forklaunch_core::persistence::{
    begin_tenant_transaction, subject::postgres::SubjectEraseAudit,
};
#[cfg(feature = "postgres09")]
use forklaunch_core::persistence::{
    postgres09::begin_tenant_transaction, subject::postgres09::SubjectEraseAudit,
};
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{
        TenantContext,
        compliance::ComplianceMetadata,
        subject::{SubjectEntity, SubjectRequest},
    },
};
use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use uuid::Uuid;
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct Owned {
    #[compliance(level = "pii")]
    subject: String,
}
#[derive(Default)]
struct Audit(AtomicBool);
impl SubjectEraseAudit for Audit {
    fn intent<'a>(
        &'a self,
        _: &'a SubjectRequest<'_>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if self.0.load(Ordering::SeqCst) {
                Err(FrameworkError::new("denied", "denied"))
            } else {
                Ok(())
            }
        })
    }
}
#[derive(Default)]
struct Destination {
    deny: AtomicBool,
    calls: AtomicUsize,
    deleted: Mutex<HashSet<Uuid>>,
}
impl ExternalErasureDestination for Destination {
    async fn erase(&self, _: &SubjectRequest<'_>, target: &ErasureTarget) -> FrameworkResult<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.deny.load(Ordering::SeqCst) {
            return Err(FrameworkError::new("failed", "failed"));
        }
        assert!(target.locator.starts_with(b"private-versioned-object"));
        self.deleted.lock().unwrap().insert(target.id);
        Ok(())
    }
}
#[test]
#[ignore = "requires owned disposable subject_export_proof PostgreSQL database"]
fn external_erasure_retries_receipt_failure_and_never_completes_pending_targets() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
 let url=std::env::var("SUBJECT_EXPORT_TEST_DATABASE_URL").unwrap();let options:sqlx::postgres::PgConnectOptions=url.parse().unwrap();assert_eq!(options.get_host(),"127.0.0.1");assert_eq!(options.get_database(),Some("subject_export_proof"));
 let owner=sqlx::PgPool::connect(&url).await.unwrap();sqlx::query("CREATE ROLE outbox_actor NOLOGIN NOSUPERUSER NOBYPASSRLS").execute(&owner).await.unwrap();sqlx::raw_sql(PostgresErasureOutbox::migration_sql()).execute(&owner).await.unwrap();sqlx::raw_sql("GRANT SELECT,INSERT ON forklaunch_erasure_sets TO outbox_actor;GRANT SELECT,INSERT,UPDATE ON forklaunch_erasure_tasks TO outbox_actor;CREATE TABLE source_marker(id integer PRIMARY KEY);INSERT INTO source_marker VALUES(1);GRANT SELECT,DELETE ON source_marker TO outbox_actor;").execute(&owner).await.unwrap();
 let pool=sqlx::postgres::PgPoolOptions::new().connect_with(options.options([("role","outbox_actor")])).await.unwrap();let audit=Arc::new(Audit::default());let key=Arc::new(EnvelopeKey::with_rotation(EnvelopeProfile::new("outbox-proof").unwrap(),"current".into(),[7;32],vec![],None).unwrap());let outbox=PostgresErasureOutbox::new(pool.clone(),key.clone(),audit.clone());let tenant=TenantContext::new(Uuid::new_v4().to_string()).unwrap();let operation=Uuid::new_v4().to_string();let entity=SubjectEntity::for_entity::<Owned>("subject").unwrap();let request=SubjectRequest{tenant:&tenant,subject:"private-subject",entity:&entity,operation_id:&operation,batch_size:1};let targets=vec![ErasureTarget{id:Uuid::new_v4(),locator:b"private-versioned-object-a".to_vec().into()},ErasureTarget{id:Uuid::new_v4(),locator:b"private-versioned-object-b".to_vec().into()}];let destination=Destination::default();
 assert!(outbox.reconcile(&request,&destination).await.is_err());
 let mut wrong=pool.begin().await.unwrap();assert!(outbox.enqueue(&mut wrong,&request,&targets).await.is_err());wrong.rollback().await.unwrap();
 let mut tx=begin_tenant_transaction(&pool,&tenant).await.unwrap();audit.0.store(true,Ordering::SeqCst);assert!(outbox.enqueue(&mut tx,&request,&targets).await.is_err());tx.rollback().await.unwrap();audit.0.store(false,Ordering::SeqCst);
 let mut tx=begin_tenant_transaction(&pool,&tenant).await.unwrap();outbox.enqueue(&mut tx,&request,&targets).await.unwrap();sqlx::query("DELETE FROM source_marker").execute(&mut *tx).await.unwrap();tx.rollback().await.unwrap();assert!(outbox.reconcile(&request,&destination).await.is_err());let source:i64=sqlx::query_scalar("SELECT count(*) FROM source_marker").fetch_one(&owner).await.unwrap();assert_eq!(source,1);
 let mut tx=begin_tenant_transaction(&pool,&tenant).await.unwrap();outbox.enqueue(&mut tx,&request,&targets).await.unwrap();sqlx::query("DELETE FROM source_marker").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
 let mut tx=begin_tenant_transaction(&pool,&tenant).await.unwrap();outbox.enqueue(&mut tx,&request,&targets).await.unwrap();tx.commit().await.unwrap();let mut tx=begin_tenant_transaction(&pool,&tenant).await.unwrap();assert!(outbox.enqueue(&mut tx,&request,&targets[..1]).await.is_err());tx.rollback().await.unwrap();
 destination.deny.store(true,Ordering::SeqCst);assert!(outbox.reconcile(&request,&destination).await.is_err());destination.deny.store(false,Ordering::SeqCst);
 sqlx::raw_sql("CREATE FUNCTION reject_erasure_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture rollback'; END $$;CREATE TRIGGER reject_erasure_receipt BEFORE UPDATE ON forklaunch_erasure_tasks FOR EACH ROW EXECUTE FUNCTION reject_erasure_receipt();").execute(&owner).await.unwrap();assert!(outbox.reconcile(&request,&destination).await.is_err());assert_eq!(destination.deleted.lock().unwrap().len(),1);sqlx::query("DROP TRIGGER reject_erasure_receipt ON forklaunch_erasure_tasks").execute(&owner).await.unwrap();
 drop(outbox);let outbox=PostgresErasureOutbox::new(pool.clone(),key,audit.clone());let progress=outbox.reconcile(&request,&destination).await.unwrap();assert_eq!(progress.acknowledged,1);assert!(!progress.complete);assert_eq!(destination.deleted.lock().unwrap().len(),1);
 let changed=SubjectRequest{subject:"another-subject",..request};assert!(outbox.reconcile(&changed,&destination).await.is_err());let other=TenantContext::new(Uuid::new_v4().to_string()).unwrap();let crossed=SubjectRequest{tenant:&other,..request};assert!(outbox.reconcile(&crossed,&destination).await.is_err());
 audit.0.store(true,Ordering::SeqCst);assert!(outbox.reconcile(&request,&destination).await.is_err());audit.0.store(false,Ordering::SeqCst);
 let progress=outbox.reconcile(&request,&destination).await.unwrap();assert!(progress.complete);assert_eq!(destination.deleted.lock().unwrap().len(),2);let calls=destination.calls.load(Ordering::SeqCst);assert_eq!(outbox.reconcile(&request,&destination).await.unwrap(),ErasureReconciliation{acknowledged:0,complete:true});assert_eq!(calls,destination.calls.load(Ordering::SeqCst));
 for sql in ["SELECT to_jsonb(t)::text FROM forklaunch_erasure_sets t","SELECT to_jsonb(t)::text FROM forklaunch_erasure_tasks t"]{let rows:Vec<String>=sqlx::query_scalar(sql).fetch_all(&owner).await.unwrap();assert!(rows.iter().all(|row|!row.contains("private-")));}pool.close().await;owner.close().await;
 });
}
