#![cfg(feature = "encryption")]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{
        compliance::ComplianceLevel,
        protected::{migration::*, *},
    },
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};
use uuid::Uuid;
#[derive(Default)]
struct Audit {
    events: Mutex<Vec<String>>,
    fail_access: AtomicBool,
    fail_migration: AtomicBool,
}
impl AsyncComplianceAudit for Audit {
    fn record<'a>(
        &'a self,
        event: ComplianceAccess<'a>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if self.fail_access.load(Ordering::SeqCst) {
                return Err(FrameworkError::new("audit_failed", "unavailable"));
            }
            self.events.lock().unwrap().push(format!("{event:?}"));
            Ok(())
        })
    }
}
impl MigrationAudit for Audit {
    fn prepared<'a>(
        &'a self,
        event: MigrationPrepared<'a>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if self.fail_migration.load(Ordering::SeqCst) {
                return Err(FrameworkError::new("audit_failed", "unavailable"));
            }
            self.events.lock().unwrap().push(format!("{event:?}"));
            Ok(())
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test sinks acknowledge immediately"),
    }
}
fn key(id: &str, byte: u8) -> EnvelopeKey {
    EnvelopeKey::with_rotation(
        EnvelopeProfile::new("migration-proof").unwrap(),
        id.into(),
        [byte; 32],
        vec![],
        None,
    )
    .unwrap()
}
fn context(tenant: u128, id: u128) -> ProtectedContext {
    ProtectedContext::new(Uuid::from_u128(tenant), Uuid::from_u128(id)).unwrap()
}
fn member(level: ComplianceLevel, purpose: &'static str) -> ProtectedMember {
    ProtectedMember::new("Contact", "email", purpose, level)
        .unwrap()
        .with_equality_index(IndexNormalization::TrimLowercase)
}
fn request<'a>(
    old: &'a EnvelopeKey,
    new: &'a EnvelopeKey,
    target: ProtectedMember,
    kind: MigrationKind,
) -> MigrationRequest<'a> {
    MigrationRequest {
        source_key: old,
        source_context: context(1, 2),
        target_key: new,
        target_context: context(1, 2),
        target_member: target,
        operation_id: Uuid::from_u128(44),
        kind,
        actor: "service:migration",
        reason: "approved-key-transition",
    }
}
#[test]
fn rotation_and_reclassification_reseal_reindex_and_audit_without_plaintext_logs() {
    let old = key("old", 7);
    let new = key("new", 8);
    let metadata = member(ComplianceLevel::Pii, "email");
    let field = ProtectedField::seal(
        &old,
        context(1, 2),
        metadata.clone(),
        b" Private@Example.test ",
    )
    .unwrap();
    let audit = Audit::default();
    let rotation = ready(field.prepare_migration(
        request(&old, &new, metadata.clone(), MigrationKind::Rotate),
        &audit,
        &audit,
    ))
    .unwrap();
    assert_eq!(rotation.expected_ciphertext(), field.ciphertext());
    assert_ne!(rotation.replacement().ciphertext(), field.ciphertext());
    assert_eq!(
        rotation.replacement().blind_index(),
        Some(
            metadata
                .query(
                    &new,
                    Uuid::from_u128(1),
                    ProtectedQuery::Equal("private@example.test")
                )
                .unwrap()
                .values()[0]
                .as_str()
        )
    );
    assert_eq!(
        new.open(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            "email",
            rotation.replacement().ciphertext()
        )
        .unwrap()
        .as_slice(),
        b" Private@Example.test "
    );
    let reclassified = member(ComplianceLevel::Phi, "medical-contact");
    assert!(
        ready(field.prepare_migration(
            request(&old, &new, reclassified.clone(), MigrationKind::Rotate),
            &audit,
            &audit
        ))
        .is_err()
    );
    let changed = ready(field.prepare_migration(
        request(&old, &new, reclassified, MigrationKind::Reclassify),
        &audit,
        &audit,
    ))
    .unwrap();
    assert!(
        new.open(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            "email",
            changed.replacement().ciphertext()
        )
        .is_err()
    );
    assert!(
        new.open(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            "medical-contact",
            changed.replacement().ciphertext()
        )
        .is_ok()
    );
    let events = audit.events.lock().unwrap();
    assert_eq!(events.len(), 4);
    assert!(
        events
            .iter()
            .all(|event| !event.contains("Private@Example"))
    );
    assert!(events[1].contains("source_key_id: Some(\"old\")"));
    assert!(events[1].contains("target_key_id: Some(\"new\")"));
    assert_eq!(format!("{rotation:?}"), "PreparedMigration([redacted])");
}
#[test]
fn audit_failures_and_context_moves_never_release_a_migration_plan() {
    let old = key("old", 7);
    let new = key("new", 8);
    let metadata = member(ComplianceLevel::Pii, "email");
    let field = ProtectedField::seal(&old, context(1, 2), metadata.clone(), b"private").unwrap();
    let audit = Audit::default();
    audit.fail_access.store(true, Ordering::SeqCst);
    assert!(
        ready(field.prepare_migration(
            request(&old, &new, metadata.clone(), MigrationKind::Rotate),
            &audit,
            &audit
        ))
        .is_err()
    );
    audit.fail_access.store(false, Ordering::SeqCst);
    audit.fail_migration.store(true, Ordering::SeqCst);
    assert!(
        ready(field.prepare_migration(
            request(&old, &new, metadata.clone(), MigrationKind::Rotate),
            &audit,
            &audit
        ))
        .is_err()
    );
    audit.fail_migration.store(false, Ordering::SeqCst);
    let mut moved = request(&old, &new, metadata.clone(), MigrationKind::Reclassify);
    moved.target_context = context(9, 2);
    assert!(ready(field.prepare_migration(moved, &audit, &audit)).is_err());
    let mut moved = request(&old, &new, metadata, MigrationKind::Reclassify);
    moved.target_context = context(1, 9);
    assert!(ready(field.prepare_migration(moved, &audit, &audit)).is_err());
}

#[cfg(feature = "postgres")]
#[test]
#[ignore = "requires owned disposable protected_migration_proof PostgreSQL database"]
fn postgres_cas_is_retryable_atomic_tenant_scoped_and_preserves_competing_writes() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        use sqlx::Row;
        let url=std::env::var("PROTECTED_MIGRATION_TEST_DATABASE_URL").expect("owned local database");
        let options:sqlx::postgres::PgConnectOptions=url.parse().unwrap();assert!(matches!(options.get_host(),"127.0.0.1"|"localhost"));assert_eq!(options.get_database(),Some("protected_migration_proof"));
        let owner=sqlx::PgPool::connect(&url).await.unwrap();
        sqlx::raw_sql("CREATE ROLE protected_migrator NOLOGIN NOSUPERUSER NOBYPASSRLS; CREATE TABLE protected_records(tenant_id uuid NOT NULL,id uuid NOT NULL,ciphertext text NOT NULL,lookup_index text,migration_id uuid,PRIMARY KEY(tenant_id,id));ALTER TABLE protected_records ENABLE ROW LEVEL SECURITY;ALTER TABLE protected_records FORCE ROW LEVEL SECURITY;CREATE POLICY tenant_scope ON protected_records TO protected_migrator USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true));GRANT SELECT,UPDATE ON protected_records TO protected_migrator;").execute(&owner).await.unwrap();
        let pool=sqlx::postgres::PgPoolOptions::new().max_connections(2).connect_with(options.options([("role","protected_migrator")])).await.unwrap();
        let store=PostgresMigrationStore::new(pool.clone(),MigrationColumns{table:"protected_records".into(),tenant:"tenant_id".into(),resource:"id".into(),ciphertext:"ciphertext".into(),blind_index:Some("lookup_index".into()),operation:"migration_id".into()}).unwrap();
        let old=key("old",7);let new=key("new",8);let metadata=member(ComplianceLevel::Pii,"email");let audit=Audit::default();
        let source=ProtectedField::seal(&old,context(1,2),metadata.clone(),b"first@example.test").unwrap();
        let plan=source.prepare_migration(request(&old,&new,metadata.clone(),MigrationKind::Rotate),&audit,&audit).await.unwrap();
        for tenant in [1u128,9] {sqlx::query("INSERT INTO protected_records VALUES($1::uuid,$2::uuid,$3,$4,NULL)").bind(Uuid::from_u128(tenant).to_string()).bind(Uuid::from_u128(2).to_string()).bind(source.ciphertext()).bind(source.blind_index()).execute(&owner).await.unwrap();}
        // A storage failure must roll back ciphertext, index and checkpoint together.
        sqlx::query("ALTER TABLE protected_records ADD CONSTRAINT reject_migration CHECK(migration_id IS NULL)").execute(&owner).await.unwrap();
        assert!(store.apply(&plan).await.is_err());
        let failed=sqlx::query("SELECT ciphertext,lookup_index,migration_id::text AS migration_id FROM protected_records WHERE tenant_id=$1::uuid").bind(Uuid::from_u128(1).to_string()).fetch_one(&owner).await.unwrap();
        assert_eq!(failed.get::<String,_>("ciphertext"),source.ciphertext());
        assert_eq!(failed.get::<String,_>("lookup_index"),source.blind_index().unwrap());
        assert!(failed.get::<Option<String>,_>("migration_id").is_none());
        sqlx::query("ALTER TABLE protected_records DROP CONSTRAINT reject_migration").execute(&owner).await.unwrap();
        assert_eq!(store.apply(&plan).await.unwrap(),ApplyOutcome::Applied);
        assert_eq!(store.apply(&plan).await.unwrap(),ApplyOutcome::AlreadyApplied);
        // A restarted runner regenerates random ciphertext but reuses the operation checkpoint.
        let resumed=source.prepare_migration(request(&old,&new,metadata.clone(),MigrationKind::Rotate),&audit,&audit).await.unwrap();
        assert_eq!(store.apply(&resumed).await.unwrap(),ApplyOutcome::AlreadyApplied);

        let row=sqlx::query("SELECT ciphertext,lookup_index,migration_id::text AS migration_id FROM protected_records WHERE tenant_id=$1::uuid").bind(Uuid::from_u128(1).to_string()).fetch_one(&owner).await.unwrap();
        assert_eq!(row.get::<String,_>("ciphertext"),plan.replacement().ciphertext());assert_eq!(row.get::<String,_>("lookup_index"),plan.replacement().blind_index().unwrap());assert_eq!(row.get::<String,_>("migration_id"),plan.operation_id().to_string());
        let other:String=sqlx::query_scalar("SELECT ciphertext FROM protected_records WHERE tenant_id=$1::uuid").bind(Uuid::from_u128(9).to_string()).fetch_one(&owner).await.unwrap();assert_eq!(other,source.ciphertext());
        // Reset only the owned fixture, then let an application write race the old snapshot.
        let competing=ProtectedField::seal(&new,context(1,2),metadata,b"newer@example.test").unwrap();
        sqlx::query("UPDATE protected_records SET ciphertext=$1,lookup_index=$2,migration_id=NULL WHERE tenant_id=$3::uuid").bind(competing.ciphertext()).bind(competing.blind_index()).bind(Uuid::from_u128(1).to_string()).execute(&owner).await.unwrap();
        assert_eq!(store.apply(&plan).await.unwrap(),ApplyOutcome::Conflict);
        let stored:String=sqlx::query_scalar("SELECT ciphertext FROM protected_records WHERE tenant_id=$1::uuid").bind(Uuid::from_u128(1).to_string()).fetch_one(&owner).await.unwrap();assert_eq!(stored,competing.ciphertext());
        pool.close().await;owner.close().await;
    });
}

#[test]
#[allow(clippy::too_many_lines)] // One end-to-end rotation scenario.
fn disclosure_rotation_preserves_token_and_reclassification_requires_audits() {
    let old = key("old", 7);
    let new = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("migration-proof").unwrap(),
        "new".into(),
        [8; 32],
        vec![("old".into(), [7; 32])],
        None,
    )
    .unwrap();
    let metadata =
        member(ComplianceLevel::Pii, "email-v1").with_disclosure(DisclosurePolicy::RandomToken);
    let field = ProtectedField::seal(
        &old,
        context(1, 2),
        metadata.clone(),
        b"private@example.test",
    )
    .unwrap();
    let token = field.disclosed(&old, context(1, 2)).unwrap();
    assert_eq!(
        field.disclosed(&new, context(1, 2)).unwrap().as_str(),
        token.as_str()
    );
    let audit = Audit::default();
    let plan = ready(field.prepare_migration(
        request(&old, &new, metadata, MigrationKind::Rotate),
        &audit,
        &audit,
    ))
    .unwrap();
    assert_eq!(
        plan.replacement()
            .disclosed(&new, context(1, 2))
            .unwrap()
            .as_str(),
        token.as_str()
    );
    assert_ne!(plan.replacement().ciphertext(), field.ciphertext());
    let retired = key("new", 8);
    assert!(field.disclosed(&retired, context(1, 2)).is_err());
    assert_eq!(
        plan.replacement()
            .disclosed(&retired, context(1, 2))
            .unwrap()
            .as_str(),
        token.as_str()
    );
    assert!(
        !audit
            .events
            .lock()
            .unwrap()
            .join(" ")
            .contains(token.as_str())
    );
    assert!(
        audit
            .events
            .lock()
            .unwrap()
            .join(" ")
            .contains("source_key_id: Some(\"old\")")
    );
    assert!(
        audit
            .events
            .lock()
            .unwrap()
            .join(" ")
            .contains("target_key_id: Some(\"new\")")
    );
    let plain_policy = member(ComplianceLevel::Pii, "email-v1");
    let legacy = ProtectedField::seal(
        &old,
        context(1, 2),
        plain_policy.clone(),
        b"private@example.test",
    )
    .unwrap();
    audit.fail_access.store(true, Ordering::SeqCst);
    let target = plain_policy.with_disclosure(DisclosurePolicy::RandomToken);
    assert!(
        ready(legacy.prepare_migration(
            request(&old, &new, target.clone(), MigrationKind::Reclassify),
            &audit,
            &audit
        ))
        .is_err()
    );
    audit.fail_access.store(false, Ordering::SeqCst);
    let migrated = ready(legacy.prepare_migration(
        request(&old, &new, target, MigrationKind::Reclassify),
        &audit,
        &audit,
    ))
    .unwrap();
    assert!(
        migrated
            .replacement()
            .disclosed(&new, context(1, 2))
            .unwrap()
            .as_str()
            .starts_with("tok_")
    );
}
