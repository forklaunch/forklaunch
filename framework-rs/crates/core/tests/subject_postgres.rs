#![cfg(all(
    any(feature = "postgres", feature = "postgres09"),
    feature = "encryption"
))]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
#[cfg(not(feature = "postgres09"))]
use forklaunch_core::persistence::subject::postgres::*;
#[cfg(feature = "postgres09")]
use forklaunch_core::persistence::subject::postgres09::*;
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{TenantContext, compliance::ComplianceMetadata, subject::*},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct Contact {
    #[compliance(level = "none")]
    subject: String,
    #[compliance(level = "pii")]
    email: String,
}
#[derive(Default)]
struct Audit(AtomicBool);
impl SubjectEraseAudit for Audit {
    fn intent<'a>(
        &'a self,
        _: &'a SubjectRequest<'_>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>> {
        Box::pin(async {
            if self.0.load(Ordering::SeqCst) {
                Err(FrameworkError::new("audit", "unavailable"))
            } else {
                Ok(())
            }
        })
    }
}
#[test]
#[ignore = "requires owned disposable subject_erasure_proof PostgreSQL database"]
fn bounded_erasure_keeps_cutoff_and_receipts_across_failures_holds_retries_and_concurrency() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let url=std::env::var("SUBJECT_ERASURE_TEST_DATABASE_URL").unwrap();
        let options:sqlx::postgres::PgConnectOptions=url.parse().unwrap();
        assert_eq!(options.get_host(),"127.0.0.1");assert_eq!(options.get_database(),Some("subject_erasure_proof"));
        let owner=sqlx::PgPool::connect(&url).await.unwrap();
        sqlx::raw_sql("CREATE ROLE subject_actor NOLOGIN NOSUPERUSER NOBYPASSRLS; CREATE TABLE contacts(tenant uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),subject text NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),hold boolean NOT NULL DEFAULT false,email text NOT NULL,PRIMARY KEY(tenant,id));ALTER TABLE contacts ENABLE ROW LEVEL SECURITY;ALTER TABLE contacts FORCE ROW LEVEL SECURITY;CREATE POLICY scope ON contacts TO subject_actor USING(tenant::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant::text=current_setting('forklaunch.tenant_id',true));GRANT SELECT,DELETE ON contacts TO subject_actor;").execute(&owner).await.unwrap();
        sqlx::raw_sql(PostgresSubjectEraseStore::migration_sql()).execute(&owner).await.unwrap();
        sqlx::query("GRANT SELECT,INSERT,UPDATE ON forklaunch_subject_erasure TO subject_actor").execute(&owner).await.unwrap();
        let tenant_id=Uuid::new_v4();let other_tenant=Uuid::new_v4();
        for (tenant,subject,hold) in [(tenant_id,"private-subject",false),(tenant_id,"private-subject",false),(tenant_id,"private-subject",true),(tenant_id,"other-subject",false),(other_tenant,"private-subject",false)] {
            sqlx::query("INSERT INTO contacts(tenant,subject,hold,email) VALUES($1::uuid,$2,$3,'encrypted-placeholder')").bind(tenant.to_string()).bind(subject).bind(hold).execute(&owner).await.unwrap();
        }
        let pool=sqlx::postgres::PgPoolOptions::new().max_connections(4).connect_with(options.options([("role","subject_actor")])).await.unwrap();
        let audit=Arc::new(Audit::default());
        let key=Arc::new(EnvelopeKey::with_rotation(EnvelopeProfile::new("subject-proof").unwrap(),"active".into(),[5;32],vec![],None).unwrap());
        let store=PostgresSubjectEraseStore::database_only::<Contact>(pool.clone(),key,SubjectEraseColumns {table:"contacts".into(),tenant:"tenant".into(),id:"id".into(),subject:"subject".into(),created_at:"created_at".into(),hold:"hold".into()},audit.clone()).unwrap();
        let tenant=TenantContext::new(tenant_id.to_string()).unwrap();let entity=SubjectEntity::for_entity::<Contact>("subject").unwrap();let operation=Uuid::new_v4().to_string();
        let request=SubjectRequest {tenant:&tenant,subject:"private-subject",entity:&entity,operation_id:&operation,batch_size:1};
        audit.0.store(true,Ordering::SeqCst);assert!(store.erase_batch(&request).await.is_err());
        let before:i64=sqlx::query_scalar("SELECT count(*) FROM forklaunch_subject_erasure").fetch_one(&owner).await.unwrap();assert_eq!(before,0);audit.0.store(false,Ordering::SeqCst);
        sqlx::raw_sql("CREATE FUNCTION reject_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture rollback'; END $$; CREATE TRIGGER reject_delete BEFORE DELETE ON contacts FOR EACH ROW EXECUTE FUNCTION reject_delete();").execute(&owner).await.unwrap();
        assert!(store.erase_batch(&request).await.is_err());
        let (count,done):(i64,bool)=sqlx::query_as("SELECT deleted_count,completed FROM forklaunch_subject_erasure").fetch_one(&owner).await.unwrap();assert_eq!((count,done),(0,false));
        sqlx::query("INSERT INTO contacts(tenant,subject,email) VALUES($1::uuid,'private-subject','late-record')").bind(tenant_id.to_string()).execute(&owner).await.unwrap();
        sqlx::query("DROP TRIGGER reject_delete ON contacts").execute(&owner).await.unwrap();
        for _ in 0..2 {let result=store.erase_batch(&request).await.unwrap();assert_eq!(result.deleted,1);assert!(!result.complete);}
        assert!(store.erase_batch(&request).await.is_err());
        sqlx::query("UPDATE contacts SET hold=false WHERE tenant=$1::uuid").bind(tenant_id.to_string()).execute(&owner).await.unwrap();
        let result=store.erase_batch(&request).await.unwrap();assert_eq!(result.deleted,1);assert!(result.complete);
        let repeated=store.erase_batch(&request).await.unwrap();assert_eq!(repeated.deleted,0);assert!(repeated.complete);
        let changed=SubjectRequest {subject:"other-subject",..request};assert!(store.erase_batch(&changed).await.is_err());
        let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM contacts").fetch_one(&owner).await.unwrap();assert_eq!(remaining,3);
        let ledger:String=sqlx::query_scalar("SELECT to_jsonb(t)::text FROM forklaunch_subject_erasure t").fetch_one(&owner).await.unwrap();assert!(!ledger.contains("private-subject"));assert!(!ledger.contains("encrypted-placeholder"));
        let next_operation=Uuid::new_v4().to_string();let next=SubjectRequest{operation_id:&next_operation,subject:"private-subject",..changed};
        let mut first=Box::pin(store.erase_batch(&next));let mut second=Box::pin(store.erase_batch(&next));
        let mut first_result=None;let mut second_result=None;
        let (a,b)=std::future::poll_fn(|cx|{
            if first_result.is_none()&& let std::task::Poll::Ready(value)=first.as_mut().poll(cx) {first_result=Some(value);}
            if second_result.is_none()&& let std::task::Poll::Ready(value)=second.as_mut().poll(cx) {second_result=Some(value);}
            if first_result.is_some() && second_result.is_some() {std::task::Poll::Ready((first_result.take().unwrap(),second_result.take().unwrap()))} else {std::task::Poll::Pending}
        }).await;let(a,b)=(a.unwrap(),b.unwrap());assert_eq!(a.deleted+b.deleted,1);assert!(a.complete&&b.complete);
        let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM contacts").fetch_one(&owner).await.unwrap();assert_eq!(remaining,2);
        pool.close().await;owner.close().await;
    });
}
