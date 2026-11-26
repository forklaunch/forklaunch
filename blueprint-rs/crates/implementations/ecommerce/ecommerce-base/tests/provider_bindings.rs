#![cfg(feature = "postgres")]
use forklaunch_core::domain::AuditContext;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    postgres::{PaymentProviderScope, PostgresEcommerceStore},
};
use forklaunch_ecommerce_interface::CreatePaymentDto;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn administrative_scope_tenants_audit_and_generation() {
    let owner = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("ecom_binding_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'fixture-only'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit TO {role}"))).execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password("fixture-only"),
        )
        .await
        .unwrap();
    let tenant = TenantContext::new(role.clone()).unwrap();
    let keys = Arc::new(FieldEncryptor::new("provider-binding-owned-key").unwrap());
    let store = Arc::new(
        PostgresEcommerceStore::new(
            pool.clone(),
            tenant.clone(),
            keys.clone(),
            AuditContext::actor("operator", "binding-fixture"),
        )
        .await
        .unwrap(),
    );
    let other = PostgresEcommerceStore::new(
        pool.clone(),
        TenantContext::new(format!("other-{role}")).unwrap(),
        keys,
        AuditContext::actor("operator", "binding-fixture"),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO fl_ecommerce.records(tenant_id,kind,id,data) VALUES($1,'order','owned-order','{\"id\":\"owned-order\",\"status\":\"cancelled\",\"opaqueFixture\":\"unchanged\"}')").bind(tenant.id()).execute(&owner).await.unwrap();
    let service = BaseEcommerceService::new(store.clone());
    let payment = service
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("owned-payment".into()),
                order_id: "owned-order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("pi_owned".into()),
        )
        .await
        .unwrap();
    let scope = PaymentProviderScope::administrative(
        "stripe".into(),
        "owned-deployment".into(),
        "acct_owned".into(),
        false,
    )
    .unwrap();
    for (provider, label, merchant) in [
        ("other", "owned", "acct_owned"),
        ("stripe", " ", "acct_owned"),
        ("stripe", "owned", "acct_"),
        ("paypal", "owned", "secret/token"),
    ] {
        assert!(
            PaymentProviderScope::administrative(
                provider.into(),
                label.into(),
                merchant.into(),
                false
            )
            .is_err()
        );
    }
    assert!(
        PaymentProviderScope::administrative(
            "paypal".into(),
            "owned".into(),
            "merchant_owned".into(),
            true
        )
        .is_ok()
    );
    for patch in [
        serde_json::json!({"providerRef":"not-an-intent"}),
        serde_json::json!({"amountCents":0}),
        serde_json::json!({"currency":"US"}),
    ] {
        let mut value = serde_json::to_value(&payment).unwrap();
        for (key, value_patch) in patch.as_object().unwrap() {
            value[key] = value_patch.clone();
        }
        let invalid = serde_json::from_value(value).unwrap();
        assert!(
            store
                .provision_payment_provider_binding(&scope, &invalid)
                .await
                .is_err()
        );
    }
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2",
    )
    .bind(tenant.id())
    .bind(&payment.id)
    .fetch_one(&owner)
    .await
    .unwrap();
    let mut changed = payment.clone();
    changed.amount_cents += 1;
    assert!(
        store
            .provision_payment_provider_binding(&scope, &changed)
            .await
            .is_err()
    );
    assert!(
        other
            .provision_payment_provider_binding(&scope, &payment)
            .await
            .is_err()
    );
    for audit in [
        AuditContext::system("anonymous"),
        AuditContext::actor(" ", "blank"),
    ] {
        let unowned = store.with_audit_context(audit).unwrap();
        assert!(
            unowned
                .provision_payment_provider_binding(&scope, &payment)
                .await
                .is_err()
        );
    }
    let first = store
        .provision_payment_provider_binding(&scope, &payment)
        .await
        .unwrap();
    assert_eq!(first.payment_id(), payment.id);
    assert_eq!(first.provider_ref(), "pi_owned");
    assert_eq!(first.scope().merchant_id(), "acct_owned");
    assert_eq!(first.scope().provider(), "stripe");
    assert!(!first.scope().livemode());
    service
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("other-payment".into()),
                order_id: "owned-order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("pi_duplicate_fixture".into()),
        )
        .await
        .unwrap();
    // Even a broadly granted tenant role cannot alias one scoped reference twice.
    let mut duplicate = forklaunch_core::persistence::begin_tenant_transaction(&pool, &tenant)
        .await
        .unwrap();
    assert!(sqlx::query("INSERT INTO fl_ecommerce.payment_provider_bindings(tenant_id,payment_id,provider,deployment_label,merchant_id,livemode,generation,provider_ref,order_id,amount_cents,currency) SELECT tenant_id,'other-payment',provider,deployment_label,merchant_id,livemode,$1::uuid,provider_ref,order_id,amount_cents,currency FROM fl_ecommerce.payment_provider_bindings WHERE payment_id='owned-payment'").bind(uuid::Uuid::new_v4().to_string()).execute(&mut *duplicate).await.is_err());
    duplicate.rollback().await.unwrap();
    assert!(
        store
            .provision_payment_provider_binding(&scope, &payment)
            .await
            .is_err()
    );
    assert!(
        other
            .payment_provider_binding(&scope, &payment.id)
            .await
            .is_err()
    );
    assert!(other.revoke_payment_provider_binding(&first).await.is_err());
    for audit in [
        AuditContext::system("anonymous"),
        AuditContext::actor(" ", "blank"),
    ] {
        assert!(
            store
                .with_audit_context(audit)
                .unwrap()
                .revoke_payment_provider_binding(&first)
                .await
                .is_err()
        );
    }
    for alternate in [
        PaymentProviderScope::administrative(
            "stripe".into(),
            "other".into(),
            "acct_owned".into(),
            false,
        )
        .unwrap(),
        PaymentProviderScope::administrative(
            "stripe".into(),
            "owned-deployment".into(),
            "acct_other".into(),
            false,
        )
        .unwrap(),
        PaymentProviderScope::administrative(
            "stripe".into(),
            "owned-deployment".into(),
            "acct_owned".into(),
            true,
        )
        .unwrap(),
    ] {
        assert!(
            store
                .payment_provider_binding(&alternate, &payment.id)
                .await
                .is_err()
        );
    }
    // The records CHECK permits absent JSON id through SQL NULL semantics.
    // Bindings still require explicit key/body identity for both related records.
    for (kind, id) in [("payment", payment.id.as_str()), ("order", "owned-order")] {
        sqlx::query("UPDATE fl_ecommerce.records SET data=data-'id' WHERE tenant_id=$1 AND kind=$2 AND id=$3").bind(tenant.id()).bind(kind).bind(id).execute(&owner).await.unwrap();
        assert!(
            store
                .payment_provider_binding(&scope, &payment.id)
                .await
                .is_err()
        );
        assert!(
            store
                .provision_payment_provider_binding(&scope, &payment)
                .await
                .is_err()
        );
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{id}',to_jsonb(id)) WHERE tenant_id=$1 AND kind=$2 AND id=$3").bind(tenant.id()).bind(kind).bind(id).execute(&owner).await.unwrap();
    }
    // Immutable relation edits deny reads; status/time changes retain the binding.
    for (field, value) in [
        ("orderId", serde_json::json!("other-order")),
        ("providerRef", serde_json::json!("pi_other")),
        ("amountCents", serde_json::json!(106)),
        ("currency", serde_json::json!("EUR")),
    ] {
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,ARRAY[$3],$4) WHERE tenant_id=$1 AND kind='payment' AND id=$2").bind(tenant.id()).bind(&payment.id).bind(field).bind(value).execute(&owner).await.unwrap();
        assert!(
            store
                .payment_provider_binding(&scope, &payment.id)
                .await
                .is_err()
        );
        sqlx::query("UPDATE fl_ecommerce.records SET data=$3 WHERE tenant_id=$1 AND kind='payment' AND id=$2").bind(tenant.id()).bind(&payment.id).bind(&before).execute(&owner).await.unwrap();
    }
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"failed\"') WHERE tenant_id=$1 AND kind='payment' AND id=$2").bind(tenant.id()).bind(&payment.id).execute(&owner).await.unwrap();
    assert_eq!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .unwrap()
            .generation(),
        first.generation()
    );
    sqlx::query(
        "UPDATE fl_ecommerce.records SET data=$3 WHERE tenant_id=$1 AND kind='payment' AND id=$2",
    )
    .bind(tenant.id())
    .bind(&payment.id)
    .bind(&before)
    .execute(&owner)
    .await
    .unwrap();
    // Audit failure must roll back revocation, and RLS forbids audit alteration
    // even though this test role intentionally has broad table privileges.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.payment_provider_binding_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(store.revoke_payment_provider_binding(&first).await.is_err());
    assert_eq!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .unwrap()
            .generation(),
        first.generation()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.payment_provider_binding_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let mut tx = forklaunch_core::persistence::begin_tenant_transaction(&pool, &tenant)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query("DELETE FROM fl_ecommerce.payment_provider_binding_audit")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    assert_eq!(
        sqlx::query("UPDATE fl_ecommerce.payment_provider_binding_audit SET operation='revoke'")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.commit().await.unwrap();
    store.revoke_payment_provider_binding(&first).await.unwrap();
    assert!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .is_err()
    );
    // Failed provision leaves no binding or audit; replacement gets a new generation.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.payment_provider_binding_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        store
            .provision_payment_provider_binding(&scope, &payment)
            .await
            .is_err()
    );
    assert!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.payment_provider_binding_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let replacement = store
        .provision_payment_provider_binding(&scope, &payment)
        .await
        .unwrap();
    assert_ne!(first.generation(), replacement.generation());
    assert!(store.revoke_payment_provider_binding(&first).await.is_err());
    assert_eq!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .unwrap()
            .generation(),
        replacement.generation()
    );
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2",
    )
    .bind(tenant.id())
    .bind(&payment.id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(before, after);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_ecommerce.payment_provider_binding_audit WHERE tenant_id=$1",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(count, 3);
    // Deleting and recreating an identical record cannot revive a binding.
    sqlx::query("DELETE FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2")
        .bind(tenant.id())
        .bind(&payment.id)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO fl_ecommerce.records(tenant_id,kind,id,data) VALUES($1,'payment',$2,$3)",
    )
    .bind(tenant.id())
    .bind(&payment.id)
    .bind(&before)
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .is_err()
    );
    assert!(
        store
            .revoke_payment_provider_binding(&replacement)
            .await
            .is_err()
    );
    let recreated = store
        .provision_payment_provider_binding(&scope, &payment)
        .await
        .unwrap();
    assert_ne!(recreated.generation(), replacement.generation());
    // Order deletion also invalidates the association without deleting audit history.
    sqlx::query(
        "DELETE FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND id='owned-order'",
    )
    .bind(tenant.id())
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        store
            .payment_provider_binding(&scope, &payment.id)
            .await
            .is_err()
    );
    assert!(
        store
            .provision_payment_provider_binding(&scope, &payment)
            .await
            .is_err()
    );
    drop(service);
    drop(store);
    drop(other);
    pool.close().await;
    for table in [
        "payment_provider_bindings",
        "payment_provider_binding_audit",
        "records",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_ecommerce.{table} WHERE tenant_id=$1"
        )))
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    old_schema_upgrade(&owner).await;
    owner.close().await;
}

// Run the exact old SQL in an isolated schema, then the additive migration.
// Production migrations still use the normal schema-private checksum history.
async fn old_schema_upgrade(owner: &sqlx::PgPool) {
    let schema = format!("ecom_binding_upgrade_{}", uuid::Uuid::new_v4().simple());
    let mut tx = owner.begin().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut *tx)
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/1_ecommerce.sql"),
        include_str!("../migrations/2_catalog.sql"),
        include_str!("../migrations/3_order_events.sql"),
        include_str!("../migrations/4_webhooks.sql"),
        include_str!("../migrations/5_order_retention.sql"),
        include_str!("../migrations/6_payment_reconciliation.sql"),
        include_str!("../migrations/7_payment_cancellation.sql"),
        include_str!("../migrations/8_optional_cancellation_time.sql"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(
            sql.replace("fl_ecommerce.", &format!("{schema}.")),
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    let data = serde_json::json!({"id":"legacy","status":"cancelled","shippingAddress":{"ciphertext":"owned-opaque-sentinel"}});
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {schema}.records(tenant_id,kind,id,data) VALUES('legacy','order','legacy',$1)"
    )))
    .bind(&data)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("INSERT INTO {schema}.payment_cancellation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,canceled_at,amount_cents,currency,actor_id,correlation_id) VALUES('00000000-0000-4000-8000-000000000009','legacy','payment','pending','paypal','legacy-order',NULL,105,'USD','operator','legacy')"))).execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        include_str!("../migrations/9_payment_provider_bindings.sql")
            .replace("fl_ecommerce.", &format!("{schema}.")),
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        include_str!("../migrations/10_scoped_cancellation_receipts.sql")
            .replace("fl_ecommerce.", &format!("{schema}.")),
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let receipt_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {schema}.scoped_payment_cancellation_receipts"
    )))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(receipt_count, 0);
    let after: serde_json::Value = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT data FROM {schema}.records WHERE id='legacy'"
    )))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(after, data);
    let old_count:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {schema}.payment_cancellation_audit WHERE provider_ref='legacy-order' AND canceled_at IS NULL"))).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(old_count, 1);
    let new_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {schema}.payment_provider_bindings"
    )))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(new_count, 0);
    tx.rollback().await.unwrap();
}
