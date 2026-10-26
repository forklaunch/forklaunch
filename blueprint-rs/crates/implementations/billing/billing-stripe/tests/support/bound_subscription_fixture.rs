use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{PlanCreationClaim, PostgresBillingStore, StripeBindingScope, TrustedCustomerParty},
};
use forklaunch_billing_interface::customers::{CreateCustomerDto, CustomerService};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;
use uuid::Uuid;

pub struct Fixture {
    admin: PgPool,
    pub owner: PgPool,
    pub pool: PgPool,
    pub store: Arc<PostgresBillingStore>,
    database: String,
    owner_role: String,
    runtime_role: String,
    pub publisher_role: String,
    pub scope: StripeBindingScope,
    pub customer: Uuid,
    pub plan: Uuid,
    pub plan_operation: Uuid,
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
    pub async fn new() -> Self {
        Self::new_with_plan_actor("service:subscription").await
    }
    #[allow(clippy::too_many_lines)]
    pub async fn new_with_plan_actor(plan_actor: &str) -> Self {
        let admin = PgPoolOptions::new()
            .max_connections(3)
            .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
            .await
            .unwrap();
        let suffix = Uuid::new_v4().simple().to_string();
        let database = format!("fl_sub_wire_{suffix}");
        let owner_role = format!("fl_sub_wire_owner_{suffix}");
        let runtime_role = format!("fl_sub_wire_runtime_{suffix}");
        let publisher_role = format!("fl_sub_wire_publisher_{suffix}");
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
        PostgresBillingStore::migrate(&owner).await.unwrap();
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
            .snapshot_customer_registration(
                &scope,
                &association,
                "cus_owned",
                forklaunch_billing_stripe::client::STRIPE_API_VERSION,
            )
            .await
            .unwrap();
        store
            .complete_customer_registration(snapshot, || Ok(()))
            .await
            .unwrap();
        let plan_store = store
            .with_audit_context(AuditContext::actor(plan_actor, "catalog-fixture"))
            .unwrap();
        let plan_operation = Uuid::new_v4();
        let request = json!({"name":"Pro","active":true,"currency":"USD","cadence":"month","unitAmount":1200,"productId":"custom-product"});
        let PlanCreationClaim::Reserved(ticket) = plan_store
            .reserve_plan_creation(
                &scope,
                plan_operation,
                forklaunch_billing_stripe::client::STRIPE_API_VERSION,
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
        plan_store
            .complete_plan_creation(
                ticket,
                record,
                json!({"id":"custom-plan","object":"plan","livemode":false,"product":product}),
                || Ok(()),
            )
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
    pub async fn count(&self, table: &str) -> i64 {
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
    pub async fn denied(&self, sql: &str, tenant: &str) {
        let mut tx = self.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
        let error = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(
            error
                .as_database_error()
                .and_then(sqlx::error::DatabaseError::code)
                .as_deref(),
            Some("P0001"),
            "wrong denial: {error}"
        );
        tx.rollback().await.unwrap();
    }
    pub async fn cleanup(self) {
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
