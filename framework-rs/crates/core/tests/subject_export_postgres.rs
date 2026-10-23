#![cfg(all(
    any(feature = "postgres", feature = "postgres09"),
    feature = "encryption"
))]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
#[cfg(not(feature = "postgres09"))]
use forklaunch_core::persistence::subject::export::*;
#[cfg(feature = "postgres09")]
use forklaunch_core::persistence::subject::export09::*;
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
    time::{Duration, SystemTime},
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
struct Authorization {
    deny: AtomicBool,
    deny_delivery: AtomicBool,
}
impl ExportAuthorization for Authorization {
    fn authorize<'a>(
        &'a self,
        _: &'a SubjectRequest<'_>,
        _: Option<&'a str>,
        action: ExportAction,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if self.deny.load(Ordering::SeqCst)
                || (matches!(action, ExportAction::Deliver)
                    && self.deny_delivery.load(Ordering::SeqCst))
            {
                Err(FrameworkError::new("authorization", "denied"))
            } else {
                Ok(())
            }
        })
    }
}
#[test]
#[ignore = "requires owned disposable subject_export_proof PostgreSQL database"]
fn snapshots_and_mailbox_resume_after_delivery_ack_failure_without_duplicates_or_plaintext_storage()
{
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
    let url=std::env::var("SUBJECT_EXPORT_TEST_DATABASE_URL").unwrap();let options:sqlx::postgres::PgConnectOptions=url.parse().unwrap();assert_eq!(options.get_host(),"127.0.0.1");assert_eq!(options.get_database(),Some("subject_export_proof"));
    let owner=sqlx::PgPool::connect(&url).await.unwrap();sqlx::query("CREATE ROLE export_actor NOLOGIN NOSUPERUSER NOBYPASSRLS").execute(&owner).await.unwrap();
    for ddl in [PostgresExportSnapshot::migration_sql(),PostgresExportMailbox::migration_sql()]{sqlx::raw_sql(ddl).execute(&owner).await.unwrap();}
    sqlx::raw_sql("GRANT SELECT,INSERT,UPDATE ON forklaunch_export_snapshots TO export_actor;GRANT SELECT,INSERT ON forklaunch_export_records,forklaunch_export_deliveries TO export_actor;").execute(&owner).await.unwrap();
    let pool=sqlx::postgres::PgPoolOptions::new().max_connections(4).connect_with(options.options([("role","export_actor")])).await.unwrap();
    let key=Arc::new(EnvelopeKey::with_rotation(EnvelopeProfile::new("export-proof").unwrap(),"active".into(),[6;32],vec![],None).unwrap());let authorization=Arc::new(Authorization::default());
    let store=PostgresExportSnapshot::new(pool.clone(),key.clone(),authorization.clone());let tenant=TenantContext::new(Uuid::new_v4().to_string()).unwrap();let entity=SubjectEntity::for_entity::<Contact>("subject").unwrap();let operation=Uuid::new_v4().to_string();
    let request=SubjectRequest{tenant:&tenant,subject:"private-subject",entity:&entity,operation_id:&operation,batch_size:2};
    let records=vec![serde_json::json!({"email":"private-export-canary-a","rank":1}),serde_json::json!({"rank":2,"email":"private-export-canary-b"}),serde_json::json!({"email":"private-export-canary-c","rank":3})];
    let snapshot_expiry=SystemTime::now()+Duration::from_secs(3600);
    authorization.deny.store(true,Ordering::SeqCst);assert!(store.prepare(&request,&records,snapshot_expiry).await.is_err());authorization.deny.store(false,Ordering::SeqCst);
    store.prepare(&request,&records,snapshot_expiry).await.unwrap();store.prepare(&request,&records,snapshot_expiry).await.unwrap();
    let changed=vec![serde_json::json!({"email":"changed-after-capture"})];assert!(store.prepare(&request,&changed,snapshot_expiry).await.is_err());
    let first=store.next_batch(&request).await.unwrap().unwrap();assert_eq!(first.batch_id,"0-2");assert_eq!(first.records[0].decode::<serde_json::Value>().unwrap(),records[0]);assert_eq!(format!("{:?}",first.records[0]),"ExportDocument([redacted])");
    let original_cipher:String=sqlx::query_scalar("SELECT ciphertext FROM forklaunch_export_records WHERE position=0").fetch_one(&owner).await.unwrap();
    sqlx::query("UPDATE forklaunch_export_records SET ciphertext=(SELECT ciphertext FROM forklaunch_export_records WHERE position=1) WHERE position=0").execute(&owner).await.unwrap();
    assert!(store.next_batch(&request).await.is_err());
    sqlx::query("UPDATE forklaunch_export_records SET ciphertext=$1 WHERE position=0").bind(&original_cipher).execute(&owner).await.unwrap();
    let expires=SystemTime::now()+Duration::from_secs(3600);let mailbox=PostgresExportMailbox::new(pool.clone(),key.clone(),authorization.clone(),"recipient-private".into(),expires).unwrap();
    authorization.deny_delivery.store(true,Ordering::SeqCst);let failed=export_subject(&store,&mailbox,&request,10).await.unwrap();assert!(!failed.complete);assert_eq!(failed.incomplete,Some(IncompleteReason::Destination));authorization.deny_delivery.store(false,Ordering::SeqCst);
    mailbox.deliver(&request,&first).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture rollback'; END $$;CREATE TRIGGER reject_ack BEFORE UPDATE ON forklaunch_export_snapshots FOR EACH ROW EXECUTE FUNCTION reject_ack();").execute(&owner).await.unwrap();
    assert!(store.acknowledge(&request,&first.batch_id).await.is_err());sqlx::query("DROP TRIGGER reject_ack ON forklaunch_export_snapshots").execute(&owner).await.unwrap();
    drop(store);drop(mailbox);
    let store=PostgresExportSnapshot::new(pool.clone(),key.clone(),authorization.clone());let mailbox=PostgresExportMailbox::new(pool.clone(),key.clone(),authorization.clone(),"recipient-private".into(),expires).unwrap();
    let replay=store.next_batch(&request).await.unwrap().unwrap();assert_eq!(replay.batch_id,first.batch_id);assert_eq!(replay.records[0].json_bytes(),first.records[0].json_bytes());
    let done=export_subject(&store,&mailbox,&request,10).await.unwrap();assert!(done.complete);assert_eq!(done.records,3);
    assert!(!store.acknowledge(&request,"0-2").await.unwrap());assert!(store.next_batch(&request).await.unwrap().is_none());
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM forklaunch_export_deliveries").fetch_one(&owner).await.unwrap();assert_eq!(count,2);
    let delivered=mailbox.read(&request,"0-2").await.unwrap().decode::<Vec<serde_json::Value>>().unwrap();assert_eq!(delivered,&records[..2]);
    authorization.deny.store(true,Ordering::SeqCst);assert!(mailbox.read(&request,"0-2").await.is_err());authorization.deny.store(false,Ordering::SeqCst);
    let other=PostgresExportMailbox::new(pool.clone(),key.clone(),authorization.clone(),"another-recipient".into(),expires).unwrap();assert!(other.read(&request,"0-2").await.is_err());
    let other_tenant=TenantContext::new(Uuid::new_v4().to_string()).unwrap();
    let cross_tenant=SubjectRequest{tenant:&other_tenant,..request};assert!(store.next_batch(&cross_tenant).await.is_err());assert!(mailbox.read(&cross_tenant,"0-2").await.is_err());
    let changed=SubjectRequest{subject:"another-subject",..request};assert!(store.next_batch(&changed).await.is_err());assert!(mailbox.read(&changed,"0-2").await.is_err());
    let resized=SubjectRequest{subject:"private-subject",batch_size:1,..changed};assert!(store.next_batch(&resized).await.is_err());
    for sql in ["SELECT to_jsonb(t)::text FROM forklaunch_export_snapshots t","SELECT to_jsonb(t)::text FROM forklaunch_export_records t","SELECT to_jsonb(t)::text FROM forklaunch_export_deliveries t"]{let stored:Vec<String>=sqlx::query_scalar(sql).fetch_all(&owner).await.unwrap();assert!(stored.iter().all(|text|!text.contains("private-export-canary")&&!text.contains("private-subject")&&!text.contains("recipient-private")));}
    for sql in ["UPDATE forklaunch_export_records SET ciphertext='changed'","UPDATE forklaunch_export_deliveries SET ciphertext='changed'"]{assert!(sqlx::query(sql).execute(&pool).await.is_err());}
    sqlx::query("UPDATE forklaunch_export_deliveries SET expires_epoch=expires_epoch+1000 WHERE batch_id='0-2'").execute(&owner).await.unwrap();assert!(mailbox.read(&request,"0-2").await.is_err());
    sqlx::query("UPDATE forklaunch_export_deliveries SET expires_epoch=0 WHERE batch_id='0-2'").execute(&owner).await.unwrap();assert!(mailbox.read(&request,"0-2").await.is_err());
    // Separate cleanup authority; normal runtime never receives DELETE privileges.
    let cleanup_operation=Uuid::new_v4().to_string();let cleanup_request=SubjectRequest{operation_id:&cleanup_operation,..request};
    let short_expiry=SystemTime::now()+Duration::from_secs(2);
    store.prepare(&cleanup_request,&records,short_expiry).await.unwrap();
    let snapshot_cleanup=PostgresExportSnapshot::new(owner.clone(),key.clone(),authorization.clone());
    assert!(snapshot_cleanup.purge_expired(&cleanup_request).await.is_err());

    let short_mailbox=PostgresExportMailbox::new(pool.clone(),key.clone(),authorization.clone(),"recipient-private".into(),short_expiry).unwrap();
    short_mailbox.deliver(&cleanup_request,&first).await.unwrap();
    let cleanup_mailbox=PostgresExportMailbox::new(owner.clone(),key.clone(),authorization.clone(),"recipient-private".into(),short_expiry).unwrap();
    assert_eq!(cleanup_mailbox.purge_expired(&cleanup_request).await.unwrap(),0);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(short_mailbox.read(&cleanup_request,"0-2").await.is_err());
    assert!(store.next_batch(&cleanup_request).await.is_err());
    assert!(store.acknowledge(&cleanup_request,"0-2").await.is_err());
    assert!(store.purge_expired(&cleanup_request).await.is_err());
    authorization.deny.store(true,Ordering::SeqCst);assert!(snapshot_cleanup.purge_expired(&cleanup_request).await.is_err());authorization.deny.store(false,Ordering::SeqCst);
    sqlx::query("UPDATE forklaunch_export_snapshots SET expires_epoch=expires_epoch+1000 WHERE operation_id=$1::uuid").bind(&cleanup_operation).execute(&owner).await.unwrap();assert!(snapshot_cleanup.purge_expired(&cleanup_request).await.is_err());
    sqlx::query("UPDATE forklaunch_export_snapshots SET expires_epoch=expires_epoch-1000 WHERE operation_id=$1::uuid").bind(&cleanup_operation).execute(&owner).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_snapshot_cleanup() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture rollback'; END $$;CREATE TRIGGER reject_snapshot_cleanup BEFORE DELETE ON forklaunch_export_records FOR EACH ROW EXECUTE FUNCTION reject_snapshot_cleanup();").execute(&owner).await.unwrap();assert!(snapshot_cleanup.purge_expired(&cleanup_request).await.is_err());
    let retired:bool=sqlx::query_scalar("SELECT retired FROM forklaunch_export_snapshots WHERE operation_id=$1::uuid").bind(&cleanup_operation).fetch_one(&owner).await.unwrap();assert!(!retired);
    sqlx::query("DROP TRIGGER reject_snapshot_cleanup ON forklaunch_export_records").execute(&owner).await.unwrap();
    assert_eq!(snapshot_cleanup.purge_expired(&cleanup_request).await.unwrap(),SnapshotCleanup{deleted:2,complete:false});
    assert!(store.prepare(&cleanup_request,&records,SystemTime::now()+Duration::from_secs(3600)).await.is_err());
    assert_eq!(snapshot_cleanup.purge_expired(&cleanup_request).await.unwrap(),SnapshotCleanup{deleted:1,complete:true});
    assert_eq!(snapshot_cleanup.purge_expired(&cleanup_request).await.unwrap(),SnapshotCleanup{deleted:0,complete:true});
    let retained:i64=sqlx::query_scalar("SELECT count(*) FROM forklaunch_export_records WHERE operation_id=$1::uuid").bind(&cleanup_operation).fetch_one(&owner).await.unwrap();assert_eq!(retained,0);
    let tombstones:i64=sqlx::query_scalar("SELECT count(*) FROM forklaunch_export_snapshots WHERE operation_id=$1::uuid AND retired").bind(&cleanup_operation).fetch_one(&owner).await.unwrap();assert_eq!(tombstones,1);

    assert!(short_mailbox.purge_expired(&cleanup_request).await.is_err());
    authorization.deny.store(true,Ordering::SeqCst);assert!(cleanup_mailbox.purge_expired(&cleanup_request).await.is_err());authorization.deny.store(false,Ordering::SeqCst);
    assert_eq!(cleanup_mailbox.purge_expired(&cleanup_request).await.unwrap(),1);
    assert_eq!(cleanup_mailbox.purge_expired(&cleanup_request).await.unwrap(),0);
    assert!(short_mailbox.deliver(&cleanup_request,&first).await.is_err());
    let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM forklaunch_export_deliveries WHERE operation_id=$1::uuid").bind(&cleanup_operation).fetch_one(&owner).await.unwrap();assert_eq!(remaining,0);
    pool.close().await;owner.close().await;
 });
}
