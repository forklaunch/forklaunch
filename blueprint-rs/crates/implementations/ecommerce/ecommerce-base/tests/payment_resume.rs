#![cfg(feature = "postgres")]
use forklaunch_core::domain::AuditContext;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    payment_resume::PaymentResumeStore, postgres::PostgresEcommerceStore,
};
use forklaunch_ecommerce_interface::PaymentDto;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn resume_guard_selects_no_order_pii_and_scopes_tenant() {
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("ecom_resume_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT ON fl_ecommerce.records TO {role}"))).execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password("owned-fixture"),
        )
        .await
        .unwrap();
    let tenant = TenantContext::new(role.clone()).unwrap();
    let keys = Arc::new(FieldEncryptor::new("owned-resume-key").unwrap());
    let store = PostgresEcommerceStore::new(
        pool.clone(),
        tenant.clone(),
        keys.clone(),
        AuditContext::actor("operator", "resume-fixture"),
    )
    .await
    .unwrap();
    let other = PostgresEcommerceStore::new(
        pool.clone(),
        TenantContext::new(format!("other-{role}")).unwrap(),
        keys,
        AuditContext::actor("operator", "resume-fixture"),
    )
    .await
    .unwrap();
    let payment:PaymentDto=serde_json::from_value(json!({"id":"payment","orderId":"order","providerRef":"pi_owned","amountCents":100,"currency":"usd","status":"pending"})).unwrap();
    let order = json!({"id":"order","status":"pending","shippingAddress":{"ciphertext":"invalid-if-decrypted"}});
    for (kind, id, data) in [
        (
            "payment",
            "payment",
            serde_json::to_value(&payment).unwrap(),
        ),
        ("order", "order", order.clone()),
    ] {
        sqlx::query("INSERT INTO fl_ecommerce.records(tenant_id,kind,id,data) VALUES($1,$2,$3,$4)")
            .bind(tenant.id())
            .bind(kind)
            .bind(id)
            .bind(data)
            .execute(&owner)
            .await
            .unwrap();
    }
    store.validate_payment_resume(&payment).await.unwrap();
    assert!(other.validate_payment_resume(&payment).await.is_err());
    for status in [
        "paid",
        "fulfilled",
        "shipped",
        "delivered",
        "cancelled",
        "invalid",
    ] {
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}',$2) WHERE tenant_id=$1 AND kind='order'").bind(tenant.id()).bind(json!(status)).execute(&owner).await.unwrap();
        assert_eq!(
            store.validate_payment_resume(&payment).await.is_ok(),
            !matches!(status, "cancelled" | "invalid")
        );
    }
    sqlx::query("UPDATE fl_ecommerce.records SET data=$2 WHERE tenant_id=$1 AND kind='order'")
        .bind(tenant.id())
        .bind(&order)
        .execute(&owner)
        .await
        .unwrap();
    for (key, value) in [("providerRef", "pi_changed"), ("orderId", "other-order")] {
        let mut wrong = payment.clone();
        if key == "providerRef" {
            wrong.provider_ref = Some(value.into());
        } else {
            wrong.order_id = value.into();
        }
        assert!(store.validate_payment_resume(&wrong).await.is_err());
    }
    for kind in ["payment", "order"] {
        sqlx::query(
            "UPDATE fl_ecommerce.records SET data=data-'id' WHERE tenant_id=$1 AND kind=$2",
        )
        .bind(tenant.id())
        .bind(kind)
        .execute(&owner)
        .await
        .unwrap();
        assert!(store.validate_payment_resume(&payment).await.is_err());
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{id}',to_jsonb(id)) WHERE tenant_id=$1 AND kind=$2").bind(tenant.id()).bind(kind).execute(&owner).await.unwrap();
    }
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order'",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(after, order);
    sqlx::query("DELETE FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order'")
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
    assert!(store.validate_payment_resume(&payment).await.is_err());
    drop(store);
    drop(other);
    pool.close().await;
    sqlx::query("DELETE FROM fl_ecommerce.records WHERE tenant_id=$1")
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
