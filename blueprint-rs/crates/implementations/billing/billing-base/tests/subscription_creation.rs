#![cfg(feature = "postgres")]
#[path = "../../billing-stripe/tests/support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{
        PlanCreationClaim, PostgresBillingStore, StripeBindingScope, SubscriptionCreationClaim,
        SubscriptionDependencySnapshot, TrustedCustomerParty,
    },
};
use forklaunch_billing_interface::customers::{CreateCustomerDto, CustomerService};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::json;
use sqlx::{
    PgPool,
    postgres::{PgPoolOptions, PgSslMode},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

struct Fixture {
    admin: PgPool,
    owner: PgPool,
    pool: PgPool,
    store: Arc<PostgresBillingStore>,
    database: String,
    owner_role: String,
    runtime_role: String,
    publisher_role: String,
    scope: StripeBindingScope,
    customer: Uuid,
    plan: Uuid,
    plan_operation: Uuid,
}
impl Fixture {
    async fn store(pool: PgPool, actor: &str) -> PostgresBillingStore {
        PostgresBillingStore::new(
            pool,
            TenantContext::new("subscription-a").unwrap(),
            Arc::new(FieldEncryptor::new("owned-subscription-foundation-key").unwrap()),
            AuditContext::actor(actor, "subscription-foundation"),
        )
        .await
        .unwrap()
    }
    #[allow(clippy::too_many_lines)]
    async fn new() -> Self {
        let admin = PgPoolOptions::new()
            .max_connections(3)
            .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
            .await
            .unwrap();
        let suffix = Uuid::new_v4().simple().to_string();
        let database = format!("fl_sub_claim_{suffix}");
        let owner_role = format!("fl_sub_owner_{suffix}");
        let runtime_role = format!("fl_sub_runtime_{suffix}");
        let publisher_role = format!("fl_sub_publisher_{suffix}");
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {owner_role} LOGIN NOINHERIT NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'; CREATE ROLE {runtime_role} LOGIN NOINHERIT NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'; CREATE ROLE {publisher_role} LOGIN NOINHERIT NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'"))).execute(&admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE {database} OWNER {owner_role}"
        )))
        .execute(&admin)
        .await
        .unwrap();
        let options = (*admin.connect_options())
            .clone()
            .database(&database)
            .password("owned-fixture");
        let owner = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options.clone().username(&owner_role))
            .await
            .unwrap();
        // First install the exact existing migrations, then populate encrypted
        // records/bindings before applying migration 10 under a non-bypass owner.
        sqlx::query("CREATE SCHEMA fl_billing")
            .execute(&owner)
            .await
            .unwrap();
        let mut migrations = sqlx::migrate!("./migrations");
        migrations.migrations = std::borrow::Cow::Owned(
            migrations
                .migrations
                .iter()
                .filter(|m| m.version < 10)
                .cloned()
                .collect(),
        );
        migrations.set_locking(false);
        let mut initial = owner.begin().await.unwrap();
        sqlx::query("SET LOCAL search_path=fl_billing,pg_catalog")
            .execute(&mut *initial)
            .await
            .unwrap();
        migrations
            .run_direct(None, &mut *initial, false)
            .await
            .unwrap();
        initial.commit().await.unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {runtime_role}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {runtime_role}"))).execute(&owner).await.unwrap();
        let pool = PgPoolOptions::new()
            .max_connections(6)
            .connect_with(options.username(&runtime_role))
            .await
            .unwrap();
        let store = Arc::new(Self::store(pool.clone(), "service:subscription").await);
        let scope =
            StripeBindingScope::administrative("owned".into(), "acct_owned".into(), false).unwrap();
        let customer = Uuid::parse_str(
            &BaseBillingService::new(store.clone())
                .create_customer(CreateCustomerDto {
                    id: None,
                    email: "private@example.test".into(),
                    name: None,
                })
                .await
                .unwrap()
                .id,
        )
        .unwrap();
        let association = TrustedCustomerParty::administrative(
            customer,
            "user".into(),
            "external-party-assertion".into(),
        )
        .unwrap();
        let snapshot = store
            .snapshot_customer_registration(&scope, &association, "cus_owned", "fixture-version")
            .await
            .unwrap();
        store
            .complete_customer_registration(snapshot, || Ok(()))
            .await
            .unwrap();
        let plan_operation = Uuid::new_v4();
        let request = json!({"name":"Pro","active":true,"currency":"USD","cadence":"month","unitAmount":1200,"productId":"custom-product"});
        let PlanCreationClaim::Reserved(ticket) = store
            .reserve_plan_creation(
                &scope,
                plan_operation,
                "fixture-version",
                &json!({"request":request}),
            )
            .await
            .unwrap()
        else {
            panic!("new fixture intent")
        };
        let plan = ticket.public_id();
        let product = json!({"id":"custom-product","object":"product","livemode":false});
        let record = json!({"id":plan.to_string(),"name":"Pro","price":1200,"active":true,"currency":"USD","cadence":"month","externalId":"custom-plan","billingProvider":"stripe","providerFields":product,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"});
        store
            .complete_plan_creation(
                ticket,
                record,
                json!({"id":"custom-plan","object":"plan","livemode":false,"product":product}),
                || Ok(()),
            )
            .await
            .unwrap();
        let before = record_bodies(&pool).await;
        PostgresBillingStore::migrate(&owner).await.unwrap();
        assert_eq!(
            before,
            record_bodies(&pool).await,
            "migration changed legacy ciphertext"
        );
        let guards: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM fl_billing.subscription_dependency_guards WHERE claims=0",
        )
        .fetch_one(&owner)
        .await
        .unwrap();
        assert_eq!(
            guards, 4,
            "FORCE RLS migration owner failed to backfill other tenant"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {runtime_role}"
        )))
        .execute(&owner)
        .await
        .unwrap();
        Self {
            admin,
            owner,
            pool,
            store,
            database,
            owner_role,
            runtime_role,
            publisher_role,
            scope,
            customer,
            plan,
            plan_operation,
        }
    }
    async fn snapshot(&self) -> SubscriptionDependencySnapshot {
        self.store
            .snapshot_subscription_dependencies(
                &self.scope,
                self.customer,
                self.plan,
                self.plan_operation,
                "fixture-version",
            )
            .await
            .unwrap()
    }
    async fn count(&self, table: &str) -> i64 {
        let mut tx = self.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM fl_billing.{table}"
        )))
        .fetch_one(&mut *tx)
        .await
        .unwrap()
    }
    async fn denied(&self, sql: &str, tenant: &str) {
        let mut tx = self.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
                .execute(&mut *tx)
                .await
                .is_err(),
            "unexpected success: {sql}"
        );
        tx.rollback().await.unwrap();
    }
    async fn cleanup(self) {
        self.pool.close().await;
        self.owner.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&self.admin)
        .await
        .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP ROLE {}; DROP ROLE {}; DROP ROLE {}",
            self.runtime_role, self.owner_role, self.publisher_role
        )))
        .execute(&self.admin)
        .await
        .unwrap();
        self.admin.close().await;
    }
}

async fn record_bodies(pool: &PgPool) -> Vec<(String, sqlx::types::Json<serde_json::Value>)> {
    let mut tx = tenant_tx(pool).await;
    sqlx::query_as("SELECT kind,data FROM fl_billing.records ORDER BY kind")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "owned isolated PostgreSQL database, no provider calls"]
async fn subscription_reservation_barrier_matrix() {
    let f = Arc::new(Fixture::new().await);
    let test = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move { tokio::task::spawn_local(async move { run(&test).await }).await })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    assert!(
        result.is_ok(),
        "fixture assertion failed; isolated database/roles cleaned"
    );
}
#[allow(clippy::too_many_lines)]
async fn run(f: &Fixture) {
    stale_authority(f).await;
    let mut nested = json!(null);
    for _ in 0..40 {
        nested = json!([nested]);
    }
    for payload in [nested, json!(vec![0; 9000]), json!("x".repeat(262_145))] {
        let snapshot = f.snapshot().await;
        let before = f.count("subscription_creation_audit").await;
        assert!(
            f.store
                .reserve_subscription_creation(snapshot, Uuid::new_v4(), &payload, || panic!(
                    "hostile input reached freshness/DB work"
                ))
                .await
                .is_err()
        );
        assert_eq!(f.count("subscription_creation_intents").await, 0);
        assert_eq!(f.count("subscription_creation_audit").await, before);
    }
    let blank = Fixture::store(f.pool.clone(), " ").await;
    assert!(
        blank
            .inspect_subscription_creation(Uuid::new_v4())
            .await
            .is_err()
    );
    // Store rejects membership in the migration owner even without SUPERUSER/BYPASSRLS.
    let owner_store = Fixture::store(f.owner.clone(), "service:subscription").await;
    assert!(
        owner_store
            .snapshot_subscription_dependencies(
                &f.scope,
                f.customer,
                f.plan,
                f.plan_operation,
                "fixture-version"
            )
            .await
            .is_err()
    );
    let snapshot = f.snapshot().await;
    assert_eq!(snapshot.provider_customer_id(), "cus_owned");
    assert_eq!(snapshot.provider_plan_id(), "custom-plan");
    assert_eq!(snapshot.plan_receipt()["id"], f.plan.to_string());
    // Audit failure rolls back both journal and all four claim counters.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.subscription_creation_audit FROM {}",
        f.runtime_role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        f.store
            .reserve_subscription_creation(snapshot, Uuid::new_v4(), &json!({"quantity":1}), || Ok(
                ()
            ))
            .await
            .is_err()
    );
    assert_eq!(f.count("subscription_creation_intents").await, 0);
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.subscription_creation_audit TO {}",
        f.runtime_role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    // Freshness failure after claims/audit also rolls everything back immediately.
    let calls = AtomicUsize::new(0);
    assert!(
        f.store
            .reserve_subscription_creation(
                f.snapshot().await,
                Uuid::new_v4(),
                &json!({"quantity":1}),
                || {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        Ok(())
                    } else {
                        Err(BlueprintError::new("expired", "Expired"))
                    }
                }
            )
            .await
            .is_err()
    );
    assert_eq!(f.count("subscription_creation_intents").await, 0);
    let operation = Uuid::new_v4();
    let a = f.snapshot().await;
    let b = f.snapshot().await;
    let payload = json!({"quantity":1});
    let (a, b) = tokio::join!(
        f.store
            .reserve_subscription_creation(a, operation, &payload, || Ok(())),
        f.store
            .reserve_subscription_creation(b, operation, &payload, || Ok(()))
    );
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|v| matches!(v, SubscriptionCreationClaim::Reserved(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|v| matches!(v, SubscriptionCreationClaim::Existing(_)))
            .count(),
        1
    );
    assert_eq!(f.count("subscription_dependency_claims").await, 4);
    assert!(
        f.store
            .inspect_subscription_creation(operation)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        f.store
            .reserve_subscription_creation(
                f.snapshot().await,
                operation,
                &json!({"quantity":2}),
                || Ok(())
            )
            .await
            .is_err()
    );
    for table in [
        "subscription_creation_intents",
        "subscription_dependency_claims",
    ] {
        f.denied(&format!("DELETE FROM fl_billing.{table}"), "subscription-a")
            .await;
        f.denied(&format!("TRUNCATE fl_billing.{table} CASCADE"), "")
            .await;
    }
    f.denied("TRUNCATE fl_billing.subscription_dependency_guards", "")
        .await;
    for sql in [
        "UPDATE fl_billing.records SET data=data WHERE kind IN ('customer','plan')",
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "DELETE FROM fl_billing.provider_bindings WHERE kind='plan'",
        "UPDATE fl_billing.provider_bindings SET binding_id=gen_random_uuid() WHERE kind='plan'",
    ] {
        f.denied(sql, "subscription-a").await;
    }
    for table in ["records", "stripe_customer_bindings", "provider_bindings"] {
        f.denied(
            &format!("TRUNCATE fl_billing.{table} CASCADE"),
            "wrong-tenant",
        )
        .await;
    }
    // Broad runtime table grants do not grant private counter authority.
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let visible: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_billing.subscription_dependency_guards")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(visible, 0);
    assert_eq!(
        sqlx::query("UPDATE fl_billing.subscription_dependency_guards SET claims=0")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
    f.denied(
        "SELECT fl_billing.claim_subscription_dependency()",
        "subscription-a",
    )
    .await;
    // Even if an unrelated permissive record policy allows foreign-tenant writes,
    // the trigger uses OLD.tenant_id, not the caller's GUC, and still sees claims.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE POLICY fixture_cross_tenant ON fl_billing.records TO {} USING(true) WITH CHECK(true)",f.runtime_role))).execute(&f.owner).await.unwrap();
    f.denied(
        "UPDATE fl_billing.records SET data=data WHERE kind='customer'",
        "",
    )
    .await;
    f.denied(
        "DELETE FROM fl_billing.records WHERE kind='plan'",
        "wrong-tenant",
    )
    .await;
    sqlx::query("DROP POLICY fixture_cross_tenant ON fl_billing.records")
        .execute(&f.owner)
        .await
        .unwrap();
    let wrong_actor = Fixture::store(f.pool.clone(), "service:other").await;
    assert!(
        wrong_actor
            .inspect_subscription_creation(operation)
            .await
            .is_err()
    );
    let foreign = PostgresBillingStore::new(
        f.pool.clone(),
        TenantContext::new("subscription-b").unwrap(),
        Arc::new(FieldEncryptor::new("owned-subscription-foundation-key").unwrap()),
        AuditContext::actor("service:subscription", "foreign"),
    )
    .await
    .unwrap();
    assert!(
        foreign
            .inspect_subscription_creation(operation)
            .await
            .unwrap()
            .is_none()
    );
    lost_ack(f).await;
}
async fn stale_authority(f: &Fixture) {
    let old = f.snapshot().await;
    let binding = f
        .store
        .stripe_customer_binding(&f.scope, f.customer)
        .await
        .unwrap()
        .unwrap();
    f.store
        .revoke_stripe_customer_binding(&binding)
        .await
        .unwrap();
    let party = TrustedCustomerParty::administrative(
        f.customer,
        "user".into(),
        "external-party-assertion".into(),
    )
    .unwrap();
    let local = f
        .store
        .snapshot_customer_registration(&f.scope, &party, "cus_owned", "fixture-version")
        .await
        .unwrap();
    let replacement = f
        .store
        .complete_customer_registration(local, || Ok(()))
        .await
        .unwrap();
    assert_ne!(binding.generation(), replacement.generation());
    assert!(
        f.store
            .reserve_subscription_creation(old, Uuid::new_v4(), &json!({"quantity":1}), || Ok(()))
            .await
            .is_err()
    );
    assert!(
        f.store
            .revoke_stripe_customer_binding(&binding)
            .await
            .is_err()
    );
    let mut tx = tenant_tx(&f.pool).await;
    let original: sqlx::types::Json<serde_json::Value> =
        sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE kind='plan'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    sqlx::query(
        "UPDATE fl_billing.records SET data=jsonb_set(data,'{active}','false') WHERE kind='plan'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(
        f.store
            .snapshot_subscription_dependencies(
                &f.scope,
                f.customer,
                f.plan,
                f.plan_operation,
                "fixture-version"
            )
            .await
            .is_err()
    );
    let mut tx = tenant_tx(&f.pool).await;
    sqlx::query("UPDATE fl_billing.records SET data=$1 WHERE kind='plan'")
        .bind(original)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let foreign = PostgresBillingStore::new(
        f.pool.clone(),
        TenantContext::new("subscription-b").unwrap(),
        Arc::new(FieldEncryptor::new("owned-subscription-foundation-key").unwrap()),
        AuditContext::actor("service:subscription", "foreign"),
    )
    .await
    .unwrap();
    assert!(
        foreign
            .snapshot_subscription_dependencies(
                &f.scope,
                f.customer,
                f.plan,
                f.plan_operation,
                "fixture-version"
            )
            .await
            .is_err()
    );
    assert_eq!(f.count("subscription_creation_intents").await, 0);
}
async fn lost_ack(f: &Fixture) {
    let options = (*f.pool.connect_options()).clone();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            options
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .unwrap();
    let store = Fixture::store(pool.clone(), "service:subscription").await;
    let operation = Uuid::new_v4();
    let snapshot = f.snapshot().await;
    armed.store(true, Ordering::SeqCst);
    assert!(
        store
            .reserve_subscription_creation(snapshot, operation, &json!({"quantity":1}), || Ok(()))
            .await
            .is_err()
    );
    assert!(!armed.load(Ordering::SeqCst));
    assert!(
        f.store
            .inspect_subscription_creation(operation)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(f.count("subscription_dependency_claims").await, 8);
    assert!(matches!(
        f.store
            .reserve_subscription_creation(
                f.snapshot().await,
                operation,
                &json!({"quantity":1}),
                || Ok(())
            )
            .await
            .unwrap(),
        SubscriptionCreationClaim::Existing(_)
    ));
    pool.close().await;
    proxy.abort();
}

#[tokio::test]
#[ignore = "owned isolated PostgreSQL database, deterministic row/advisory lock races"]
async fn subscription_dependency_lock_races() {
    let f = Arc::new(Fixture::new().await);
    let test = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(
            async move { tokio::task::spawn_local(async move { lock_races(test).await }).await },
        )
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    assert!(
        result.is_ok(),
        "race fixture failed; isolated database/roles cleaned"
    );
}
async fn tenant_tx(pool: &PgPool) -> sqlx::Transaction<'_, sqlx::Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}
async fn wait_for(f: &Fixture, event: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND usename=$2 AND wait_event=$3)")
                .bind(&f.database).bind(&f.runtime_role).bind(event).fetch_one(&f.admin).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
}
#[allow(clippy::too_many_lines)]
async fn lock_races(f: Arc<Fixture>) {
    // Mutation takes the local row first: reservation must reject its old body.
    let snapshot = f.snapshot().await;
    let mut mutation = tenant_tx(&f.pool).await;
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{updatedAt}','\"2026-10-14T01:00:00Z\"') WHERE kind='customer'")
        .execute(&mut *mutation).await.unwrap();
    let store = f.store.clone();
    let reserve = tokio::spawn(async move {
        store
            .reserve_subscription_creation(snapshot, Uuid::new_v4(), &json!({"quantity":1}), || {
                Ok(())
            })
            .await
    });
    wait_for(&f, "transactionid").await;
    mutation.commit().await.unwrap();
    assert!(reserve.await.unwrap().is_err());
    assert_eq!(f.count("subscription_creation_intents").await, 0);

    // Expiry is checked after awaited dependency locks, not just before BEGIN.
    let snapshot = f.snapshot().await;
    let mut lock = tenant_tx(&f.pool).await;
    sqlx::query("SELECT id FROM fl_billing.records WHERE kind='customer' FOR UPDATE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let store = f.store.clone();
    let expired = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let check = expired.clone();
    let reserve = tokio::spawn(async move {
        store
            .reserve_subscription_creation(
                snapshot,
                Uuid::new_v4(),
                &json!({"quantity":1}),
                move || {
                    if check.load(Ordering::SeqCst) {
                        Err(BlueprintError::new("expired", "Expired"))
                    } else {
                        Ok(())
                    }
                },
            )
            .await
    });
    wait_for(&f, "transactionid").await;
    expired.store(true, Ordering::SeqCst);
    lock.rollback().await.unwrap();
    assert!(reserve.await.unwrap().is_err());
    assert_eq!(f.count("subscription_creation_intents").await, 0);

    // Pause reservation after four claims have been inserted but before COMMIT.
    // Both mutation statements start with a snapshot that predates those claims.
    sqlx::raw_sql("CREATE FUNCTION fl_billing.fixture_pause_reservation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(1729101); RETURN NEW; END $$; CREATE TRIGGER fixture_pause BEFORE INSERT ON fl_billing.subscription_creation_audit FOR EACH ROW WHEN(NEW.operation='reserve') EXECUTE FUNCTION fl_billing.fixture_pause_reservation()")
        .execute(&f.owner).await.unwrap();
    let mut blocker = f.owner.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1729101)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let snapshot = f.snapshot().await;
    let store = f.store.clone();
    let reserve = tokio::spawn(async move {
        store
            .reserve_subscription_creation(snapshot, Uuid::new_v4(), &json!({"quantity":1}), || {
                Ok(())
            })
            .await
    });
    wait_for(&f, "advisory").await;
    let pool = f.pool.clone();
    let read_committed = tokio::spawn(async move {
        let mut tx = tenant_tx(&pool).await;
        let result = sqlx::query("UPDATE fl_billing.records SET data=data WHERE kind='customer'")
            .execute(&mut *tx)
            .await;
        tx.rollback().await.unwrap();
        result.is_err()
    });
    let pool = f.pool.clone();
    let repeatable = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        let result = sqlx::query("UPDATE fl_billing.records SET data=data WHERE kind='plan'")
            .execute(&mut *tx)
            .await;
        tx.rollback().await.unwrap();
        result.is_err()
    });
    // There must be two row mutations waiting, not merely a scheduling delay.
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=$1 AND usename=$2 AND wait_event='transactionid'")
                .bind(&f.database).bind(&f.runtime_role).fetch_one(&f.admin).await.unwrap();
            if count==2 {break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    blocker.commit().await.unwrap();
    assert!(matches!(
        reserve.await.unwrap().unwrap(),
        SubscriptionCreationClaim::Reserved(_)
    ));
    assert!(read_committed.await.unwrap());
    assert!(repeatable.await.unwrap());
    assert_eq!(f.count("subscription_dependency_claims").await, 4);
}

const PUBLISH_FUNCTION: &str =
    "fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text)";
#[tokio::test]
#[ignore = "owned isolated PostgreSQL; separate trusted publication role, no provider requests"]
async fn subscription_publication_transaction_matrix() {
    let f = Arc::new(Fixture::new().await);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION {} TO {}",f.publisher_role,f.publisher_role,PUBLISH_FUNCTION,f.publisher_role))).execute(&f.owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    let test = f.clone();
    let publisher_pool = pool.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(
                async move { publication_matrix(&test, &publisher_pool).await },
            )
            .await
        })
        .await;
    pool.close().await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    assert!(
        result.is_ok(),
        "publication test failed; isolated database/roles cleaned"
    );
}
async fn reservation(
    f: &Fixture,
) -> forklaunch_billing_base::services::postgres::SubscriptionCreationReservation {
    let SubscriptionCreationClaim::Reserved(ticket) = f
        .store
        .reserve_subscription_creation(
            f.snapshot().await,
            Uuid::new_v4(),
            &json!({"quantity":1}),
            || Ok(()),
        )
        .await
        .unwrap()
    else {
        panic!("new fixture reservation")
    };
    ticket
}
fn subscription_record(id: Uuid, plan: Uuid, provider: &str) -> serde_json::Value {
    json!({"id":id.to_string(),"partyId":"external-party-assertion","partyType":"user","productId":plan.to_string(),"active":false,"externalId":provider,"billingProvider":"stripe","startDate":"2026-10-14T00:00:00Z","status":"incomplete","providerFields":{"id":provider,"object":"subscription","status":"incomplete"},"createdAt":"2026-10-14T00:00:00Z","updatedAt":"2026-10-14T00:00:00Z"})
}
#[allow(clippy::too_many_lines)]
async fn publication_matrix(f: &Fixture, pool: &PgPool) {
    use forklaunch_billing_base::services::postgres::SubscriptionCreationStatus;
    assert!(
        f.store
            .trusted_subscription_publisher(f.pool.clone())
            .await
            .is_err()
    );
    assert!(
        f.store
            .trusted_subscription_publisher(f.owner.clone())
            .await
            .is_err()
    );
    let publisher = f
        .store
        .trusted_subscription_publisher(pool.clone())
        .await
        .unwrap();
    publisher.validate_for(&f.store).await.unwrap();
    let ticket = reservation(f).await;
    let operation = ticket.operation_id();
    let id = ticket.public_id();
    let record = subscription_record(id, f.plan, "sub_owned");
    // Ordinary runtime cannot turn its readable journal into release authority.
    let mut tx = tenant_tx(&f.pool).await;
    assert_eq!(sqlx::query("UPDATE fl_billing.subscription_creation_intents SET state='completed',receipt_ciphertext='fabricated receipt' WHERE operation_id=$1::uuid").bind(operation.to_string()).execute(&mut *tx).await.unwrap().rows_affected(),0);
    tx.rollback().await.unwrap();
    f.denied("SELECT fl_billing.publish_bound_subscription('subscription-a',gen_random_uuid(),gen_random_uuid(),'x','{}','{}','sub_forged','forged receipt bytes','test')","subscription-a").await;
    f.denied(
        "SELECT fl_billing.release_completed_subscription_claim()",
        "subscription-a",
    )
    .await;
    let mut tx = tenant_tx(pool).await;
    assert!(
        sqlx::query("DELETE FROM fl_billing.subscription_dependency_claims")
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    publisher
        .complete(ticket, record.clone(), || Ok(()))
        .await
        .unwrap();
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    let total: i64 = sqlx::query_scalar(
        "SELECT sum(claims)::bigint FROM fl_billing.subscription_dependency_guards",
    )
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(total, 0);
    let Some(SubscriptionCreationStatus::Completed {
        record: historical, ..
    }) = f
        .store
        .inspect_bound_subscription_creation(operation)
        .await
        .unwrap()
    else {
        panic!("completed receipt missing")
    };
    assert_eq!(historical, record);
    assert!(
        f.store
            .inspect_subscription_creation(operation)
            .await
            .is_err(),
        "pending-only legacy inspection cannot mislabel completion"
    );
    assert!(matches!(
        f.store
            .reserve_subscription_creation(
                f.snapshot().await,
                operation,
                &json!({"quantity":1}),
                || Ok(())
            )
            .await
            .unwrap(),
        SubscriptionCreationClaim::Completed { .. }
    ));
    // Last claim release makes ordinary Customer edits possible again.
    let mut tx = tenant_tx(&f.pool).await;
    sqlx::query("UPDATE fl_billing.records SET data=data WHERE kind='customer'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // A final freshness failure rolls back writes AND completed-state release.
    let ticket = reservation(f).await;
    let expired_operation = ticket.operation_id();
    let calls = AtomicUsize::new(0);
    assert!(
        publisher
            .complete(
                ticket,
                subscription_record(Uuid::new_v4(), f.plan, "sub_wrong_local"),
                || Ok(())
            )
            .await
            .is_err()
    );
    let ticket = reservation(f).await;
    let rollback_operation = ticket.operation_id();
    let record = subscription_record(ticket.public_id(), f.plan, "sub_rollback");
    assert!(
        publisher
            .complete(ticket, record, || {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err(BlueprintError::new("expired", "Expired"))
                }
            })
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    for operation in [expired_operation, rollback_operation] {
        assert!(matches!(
            f.store
                .inspect_bound_subscription_creation(operation)
                .await
                .unwrap(),
            Some(SubscriptionCreationStatus::Pending(_))
        ));
    }
    let mut tx = tenant_tx(&f.pool).await;
    let record_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_billing.records WHERE kind='subscription'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(record_count, 1);
    tx.rollback().await.unwrap();
    assert_eq!(f.count("subscription_dependency_claims").await, 8);
    // Injection in the completion audit rolls back the earlier record and binding.
    sqlx::raw_sql("CREATE FUNCTION fl_billing.fixture_deny_completion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='complete' THEN RAISE EXCEPTION 'owned completion failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_deny_completion BEFORE INSERT ON fl_billing.subscription_creation_audit FOR EACH ROW EXECUTE FUNCTION fl_billing.fixture_deny_completion()").execute(&f.owner).await.unwrap();
    let ticket = reservation(f).await;
    let denied = ticket.operation_id();
    let record = subscription_record(ticket.public_id(), f.plan, "sub_denied");
    assert!(publisher.complete(ticket, record, || Ok(())).await.is_err());
    assert!(matches!(
        f.store
            .inspect_bound_subscription_creation(denied)
            .await
            .unwrap(),
        Some(SubscriptionCreationStatus::Pending(_))
    ));
    assert_eq!(f.count("subscription_dependency_claims").await, 12);
    sqlx::query("DROP TRIGGER fixture_deny_completion ON fl_billing.subscription_creation_audit")
        .execute(&f.owner)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fl_billing.fixture_deny_release() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.claims<OLD.claims THEN RAISE EXCEPTION 'owned release failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_deny_release BEFORE UPDATE ON fl_billing.subscription_dependency_guards FOR EACH ROW EXECUTE FUNCTION fl_billing.fixture_deny_release()").execute(&f.owner).await.unwrap();
    let ticket = reservation(f).await;
    let denied = ticket.operation_id();
    let record = subscription_record(ticket.public_id(), f.plan, "sub_release_denied");
    assert!(publisher.complete(ticket, record, || Ok(())).await.is_err());
    assert!(matches!(
        f.store
            .inspect_bound_subscription_creation(denied)
            .await
            .unwrap(),
        Some(SubscriptionCreationStatus::Pending(_))
    ));
    assert_eq!(f.count("subscription_dependency_claims").await, 16);
    sqlx::query("DROP TRIGGER fixture_deny_release ON fl_billing.subscription_dependency_guards")
        .execute(&f.owner)
        .await
        .unwrap();
    recovery_budget_and_rollback(f, &publisher).await;
    publication_lost_ack(f, pool).await;
}
async fn publication_lost_ack(f: &Fixture, publisher_pool: &PgPool) {
    use forklaunch_billing_base::services::postgres::SubscriptionCreationStatus;
    let options = (*f.pool.connect_options()).clone();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            options
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .unwrap();
    let publisher = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*publisher_pool.connect_options())
                .clone()
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .unwrap();
    let store = Fixture::store(runtime.clone(), "service:subscription").await;
    let trusted = store
        .trusted_subscription_publisher(publisher.clone())
        .await
        .unwrap();
    let ticket = reservation(f).await;
    let operation = ticket.operation_id();
    let record = subscription_record(ticket.public_id(), f.plan, "sub_lost_ack");
    drop(ticket); // Recovery must never recreate dispatch authority.
    let recovery = f
        .store
        .snapshot_subscription_recovery(operation)
        .await
        .unwrap();
    let event = json!({"id":"evt_lost_ack","object":"event","request":{"id":"req_owned"}});
    armed.store(true, Ordering::SeqCst);
    assert!(
        trusted
            .complete_recovered(recovery, record.clone(), event.clone(), || Ok(()))
            .await
            .is_err()
    );
    assert!(!armed.load(Ordering::SeqCst));
    let Some(SubscriptionCreationStatus::Completed {
        record: historical, ..
    }) = f
        .store
        .inspect_bound_subscription_creation(operation)
        .await
        .unwrap()
    else {
        panic!("lost acknowledged COMMIT actually completed")
    };
    assert_eq!(historical, record);
    assert_eq!(
        f.store
            .inspect_subscription_recovery_evidence(operation)
            .await
            .unwrap()
            .unwrap()["event"],
        event
    );
    assert!(
        f.store
            .snapshot_subscription_recovery(operation)
            .await
            .is_err()
    );

    assert_eq!(f.count("subscription_dependency_claims").await, 16);
    publisher.close().await;
    runtime.close().await;
    proxy.abort();
}

// Keep the same pending operation across each rollback fault and final publication.
#[allow(clippy::too_many_lines)]
async fn recovery_budget_and_rollback(
    f: &Fixture,
    publisher: &forklaunch_billing_base::services::postgres::TrustedSubscriptionPublisher,
) {
    let ticket = reservation(f).await;
    let operation = ticket.operation_id();
    let record = subscription_record(ticket.public_id(), f.plan, "sub_recovered");
    let key = ticket.idempotency_key().to_owned();
    drop(ticket);
    let pending = f.count("subscription_dependency_claims").await;
    let recovery = f
        .store
        .snapshot_subscription_recovery(operation)
        .await
        .unwrap();
    assert!(recovery.matches_event_request_key(&key));
    assert!(!recovery.matches_event_request_key("metadata-only"));
    let mut event = json!(null);
    for _ in 0..40 {
        event = json!([event]);
    }
    assert!(
        publisher
            .complete_recovered(recovery, record.clone(), event, || Ok(()))
            .await
            .is_err()
    );
    assert_eq!(f.count("subscription_dependency_claims").await, pending);
    let other = f
        .store
        .with_audit_context(AuditContext::actor("other", "recovery-test"))
        .unwrap();
    assert!(
        other
            .snapshot_subscription_recovery(operation)
            .await
            .is_err()
    );
    let recovery = f
        .store
        .snapshot_subscription_recovery(operation)
        .await
        .unwrap();
    drop(recovery);
    for (table, trigger, when) in [
        (
            "subscription_creation_audit",
            "fixture_deny_completion",
            "INSERT",
        ),
        (
            "subscription_dependency_guards",
            "fixture_deny_release",
            "UPDATE",
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TRIGGER {trigger} BEFORE {when} ON fl_billing.{table} FOR EACH ROW EXECUTE FUNCTION fl_billing.{trigger}()"))).execute(&f.owner).await.unwrap();
        let recovery = f
            .store
            .snapshot_subscription_recovery(operation)
            .await
            .unwrap();
        assert!(
            publisher
                .complete_recovered(recovery, record.clone(), json!({"id":"evt_owned"}), || Ok(
                    ()
                ))
                .await
                .is_err()
        );
        assert_eq!(f.count("subscription_dependency_claims").await, pending);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER {trigger} ON fl_billing.{table}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
    let recovery = f
        .store
        .snapshot_subscription_recovery(operation)
        .await
        .unwrap();
    let calls = AtomicUsize::new(0);
    assert!(
        publisher
            .complete_recovered(recovery, record.clone(), json!({"id":"evt_owned"}), || {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err(BlueprintError::new("owned_expiry", "Expired"))
                }
            })
            .await
            .is_err()
    );
    assert_eq!(f.count("subscription_dependency_claims").await, pending);
    let recovery = f
        .store
        .snapshot_subscription_recovery(operation)
        .await
        .unwrap();
    publisher
        .complete_recovered(recovery, record, json!({"id":"evt_owned"}), || Ok(()))
        .await
        .unwrap();
    assert_eq!(f.count("subscription_dependency_claims").await, pending - 4);
    recovery_receipt_tamper(f, operation).await;
}
// Deliberately exercise corruption by the trusted migration owner only inside
// this disposable database. Ordinary runtime cannot disable these triggers.
async fn recovery_receipt_tamper(f: &Fixture, operation: Uuid) {
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let original:String=sqlx::query_scalar("SELECT receipt_ciphertext FROM fl_billing.subscription_creation_intents WHERE operation_id=$1::uuid").bind(operation.to_string()).fetch_one(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    let cipher = FieldEncryptor::new("owned-subscription-foundation-key").unwrap();
    let good: serde_json::Value =
        serde_json::from_str(&cipher.decrypt(&original, "subscription-a").unwrap()).unwrap();
    for field in [
        "/operationId",
        "/publicId",
        "/payloadHash",
        "/publicationEvidence/eventHash",
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(field).unwrap() = json!("different");
        let encrypted = cipher
            .seal(&bad.to_string(), "subscription-a", "owned tamper fixture")
            .unwrap();
        overwrite_recovery_receipt(f, operation, &encrypted).await;
        assert!(
            f.store
                .inspect_subscription_recovery_evidence(operation)
                .await
                .is_err(),
            "{field}"
        );
    }
    overwrite_recovery_receipt(f, operation, &original).await;
    assert!(
        f.store
            .inspect_subscription_recovery_evidence(operation)
            .await
            .unwrap()
            .is_some()
    );
}
async fn overwrite_recovery_receipt(f: &Fixture, operation: Uuid, value: &str) {
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE fl_billing.subscription_creation_intents DISABLE TRIGGER subscription_intent_completion").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE fl_billing.subscription_creation_intents SET receipt_ciphertext=$1 WHERE operation_id=$2::uuid").bind(value).bind(operation.to_string()).execute(&mut *tx).await.unwrap();
    sqlx::query("ALTER TABLE fl_billing.subscription_creation_intents ENABLE TRIGGER subscription_intent_completion").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
