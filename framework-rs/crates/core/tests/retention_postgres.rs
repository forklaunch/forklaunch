#![cfg(any(feature = "postgres", feature = "postgres09"))]
#[cfg(feature = "postgres09")]
use forklaunch_core::persistence::retention::postgres09::*;
#[cfg(not(feature = "postgres09"))]
use forklaunch_core::persistence::retention::{
    PostgresRetentionStore, RetentionAudit, RetentionColumns,
};
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{
        TenantContext,
        compliance::ComplianceMetadata,
        retention::{RetentionAction, RetentionPolicy, RetentionRequest, RetentionStore},
    },
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct Contact {
    #[compliance(level = "pii", nullable)]
    email: Option<String>,
    #[compliance(level = "phi")]
    required: String,
    #[compliance(level = "none", nullable)]
    public: Option<String>,
}
#[derive(Default)]
struct Audit(AtomicBool);
impl RetentionAudit for Audit {
    fn intent<'a>(
        &'a self,
        _: RetentionRequest<'a>,
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
#[ignore = "requires owned disposable retention_proof PostgreSQL database"]
fn postgres_retention_is_bounded_audited_tenant_scoped_and_retryable() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
 let url=std::env::var("RETENTION_TEST_DATABASE_URL").unwrap();let options:sqlx::postgres::PgConnectOptions=url.parse().unwrap();assert_eq!(options.get_database(),Some("retention_proof"));assert_eq!(options.get_host(),"127.0.0.1");
 let owner=sqlx::PgPool::connect(&url).await.unwrap();sqlx::raw_sql("CREATE ROLE retention_actor NOLOGIN NOSUPERUSER NOBYPASSRLS; CREATE TABLE contacts(tenant text,id int,created_at timestamptz NOT NULL,anonymized_at timestamptz,email text,required text NOT NULL,public text,PRIMARY KEY(tenant,id));ALTER TABLE contacts ENABLE ROW LEVEL SECURITY;ALTER TABLE contacts FORCE ROW LEVEL SECURITY;CREATE POLICY tenant_scope ON contacts TO retention_actor USING(tenant=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant=current_setting('forklaunch.tenant_id',true));GRANT SELECT,UPDATE,DELETE ON contacts TO retention_actor;INSERT INTO contacts VALUES('a',1,to_timestamp(1),NULL,'secret','retain','public'),('a',2,to_timestamp(2),NULL,'secret2','retain','public'),('a',3,to_timestamp(10),NULL,'boundary','retain','public'),('b',1,to_timestamp(1),NULL,'other-tenant','retain','public');").execute(&owner).await.unwrap();
 let pool=sqlx::postgres::PgPoolOptions::new().connect_with(options.options([("role","retention_actor")])).await.unwrap();let audit=Arc::new(Audit::default());
 let store=PostgresRetentionStore::new::<Contact>(pool.clone(),RetentionColumns{table:"contacts".into(),tenant:"tenant".into(),id:"id".into(),created_at:"created_at".into(),anonymized_at:"anonymized_at".into()},audit.clone()).unwrap();
 let tenant=TenantContext::new("a").unwrap();let cutoff=UNIX_EPOCH+Duration::from_secs(10);let now=UNIX_EPOCH+Duration::from_secs(20);let policy=RetentionPolicy::for_entity::<Contact>(cutoff,RetentionAction::Anonymize).unwrap();let request=RetentionRequest{tenant:&tenant,policy:&policy,now,batch_size:1};
 assert_eq!(store.count(request).await.unwrap(),2);audit.0.store(true,Ordering::SeqCst);assert!(store.apply_batch(request).await.is_err());assert_eq!(store.count(request).await.unwrap(),2);audit.0.store(false,Ordering::SeqCst);
 sqlx::query("ALTER TABLE contacts ADD CONSTRAINT reject_clear CHECK(email IS NOT NULL)").execute(&owner).await.unwrap();assert!(store.apply_batch(request).await.is_err());assert_eq!(store.count(request).await.unwrap(),2);sqlx::query("ALTER TABLE contacts DROP CONSTRAINT reject_clear").execute(&owner).await.unwrap();
 assert_eq!(store.apply_batch(request).await.unwrap(),1);assert_eq!(store.count(request).await.unwrap(),1);assert_eq!(store.apply_batch(request).await.unwrap(),1);assert_eq!(store.apply_batch(request).await.unwrap(),0);
 let preserved:i64=sqlx::query_scalar("SELECT count(*) FROM contacts WHERE required='retain' AND public='public'").fetch_one(&owner).await.unwrap();assert_eq!(preserved,4);
 let other:String=sqlx::query_scalar("SELECT email FROM contacts WHERE tenant='b'").fetch_one(&owner).await.unwrap();assert_eq!(other,"other-tenant");
 let delete=RetentionPolicy::for_entity::<Contact>(cutoff,RetentionAction::Delete).unwrap();let request=RetentionRequest{policy:&delete,..request};assert_eq!(store.apply_batch(request).await.unwrap(),1);assert_eq!(store.apply_batch(request).await.unwrap(),1);assert_eq!(store.apply_batch(request).await.unwrap(),0);
 let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM contacts").fetch_one(&owner).await.unwrap();assert_eq!(remaining,2);pool.close().await;owner.close().await;
 });
}
