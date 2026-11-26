use forklaunch_billing_base::services::{
    BillingKind, BillingStore, Scope,
    postgres::{PostgresBillingStore, StripeBindingScope, StripeSubscriptionOwnership},
};
use forklaunch_core::domain::AuditContext;
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub async fn exercise(
    store: &PostgresBillingStore,
    other: &PostgresBillingStore,
    reopened: &PostgresBillingStore,
    owner: &sqlx::PgPool,
    runtime: &sqlx::PgPool,
    role: &str,
    tenant: &str,
) {
    let kind = BillingKind::Subscription;
    let scope = StripeBindingScope::administrative(
        "subscription-deployment".into(),
        "acct_owned".into(),
        false,
    )
    .unwrap();
    let relation = StripeSubscriptionOwnership::administrative(
        "cus_owned".into(),
        "local-party".into(),
        "user".into(),
    )
    .unwrap();
    for (customer, party, party_type) in [
        ("sub_wrong", "local-party", "user"),
        ("cus_", "local-party", "user"),
        ("cus_owned", " ", "user"),
        ("cus_owned", "local-party", " "),
    ] {
        assert!(
            StripeSubscriptionOwnership::administrative(
                customer.into(),
                party.into(),
                party_type.into()
            )
            .is_err()
        );
    }
    let row = json!({"id":"local-subscription", "partyId":"local-party", "partyType":"user", "externalId":"sub_owned", "productId":"local-plan", "active":false, "startDate":"2026-01-01T00:00:00Z", "status":"paused", "providerFields":{"customer":"cus_untrusted","secret":"ciphertext must remain stable"}});
    store
        .insert(Scope::Durable, kind, row.clone(), Duration::from_secs(600))
        .await
        .unwrap();
    let ciphertext:sqlx::types::Json<Value> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND kind='subscription' AND id='local-subscription'").bind(tenant).fetch_one(owner).await.unwrap();
    let public = Uuid::new_v4();
    assert!(
        store
            .provision_stripe_binding(&scope, kind, public, "local-subscription", "sub_owned")
            .await
            .is_err()
    );
    for candidate in [
        other.clone(),
        store
            .with_audit_context(AuditContext {
                actor_id: None,
                correlation_id: "missing-actor".into(),
            })
            .unwrap(),
        store
            .with_audit_context(AuditContext::actor(" ", "blank-actor"))
            .unwrap(),
    ] {
        assert!(
            candidate
                .provision_stripe_subscription_binding(
                    &scope,
                    public,
                    "local-subscription",
                    "sub_owned",
                    &relation
                )
                .await
                .is_err()
        );
    }
    let wrong = StripeSubscriptionOwnership::administrative(
        "cus_owned".into(),
        "another-party".into(),
        "user".into(),
    )
    .unwrap();
    assert!(
        store
            .provision_stripe_subscription_binding(
                &scope,
                public,
                "local-subscription",
                "sub_owned",
                &wrong
            )
            .await
            .is_err()
    );
    assert!(
        store
            .provision_stripe_subscription_binding(
                &scope,
                public,
                "local-subscription",
                "sub_wrong",
                &relation
            )
            .await
            .is_err()
    );
    let mut cached = row.clone();
    cached["id"] = json!("cache-subscription");
    store
        .insert(Scope::Cache, kind, cached, Duration::from_secs(600))
        .await
        .unwrap();
    assert!(
        store
            .provision_stripe_subscription_binding(
                &scope,
                public,
                "cache-subscription",
                "sub_owned",
                &relation
            )
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.provider_binding_audit FROM {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(
        store
            .provision_stripe_subscription_binding(
                &scope,
                public,
                "local-subscription",
                "sub_owned",
                &relation
            )
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.provider_binding_audit TO {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(
        store
            .stripe_binding_by_public_id(&scope, kind, public)
            .await
            .unwrap()
            .is_none()
    );
    let binding = store
        .provision_stripe_subscription_binding(
            &scope,
            public,
            "local-subscription",
            "sub_owned",
            &relation,
        )
        .await
        .unwrap();
    assert_eq!(
        binding
            .subscription_ownership()
            .unwrap()
            .provider_customer_id(),
        "cus_owned"
    );
    assert_eq!(
        binding.subscription_ownership().unwrap().local_party_id(),
        "local-party"
    );
    assert_eq!(
        binding.subscription_ownership().unwrap().local_party_type(),
        "user"
    );
    assert_eq!(store.read_stripe_bound_record(&binding).await.unwrap(), row);
    let current:sqlx::types::Json<Value> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND kind='subscription' AND id='local-subscription'").bind(tenant).fetch_one(owner).await.unwrap();
    assert_eq!(ciphertext.0, current.0);
    assert!(other.read_stripe_bound_record(&binding).await.is_err());
    assert!(
        other
            .stripe_binding_by_provider_id(&scope, kind, "sub_owned")
            .await
            .unwrap()
            .is_none()
    );
    for bad_scope in [
        StripeBindingScope::administrative("different".into(), "acct_owned".into(), false).unwrap(),
        StripeBindingScope::administrative(
            "subscription-deployment".into(),
            "acct_other".into(),
            false,
        )
        .unwrap(),
        StripeBindingScope::administrative(
            "subscription-deployment".into(),
            "acct_owned".into(),
            true,
        )
        .unwrap(),
    ] {
        assert!(
            store
                .stripe_binding_by_public_id(&bad_scope, kind, public)
                .await
                .unwrap()
                .is_none()
        );
    }
    let restored = reopened
        .stripe_binding_by_provider_id(&scope, kind, "sub_owned")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened.read_stripe_bound_record(&restored).await.unwrap(),
        row
    );
    for (field, value) in [
        ("partyId", "another-party"),
        ("partyType", "organization"),
        ("externalId", "sub_replacement"),
    ] {
        sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,ARRAY[$2],to_jsonb($3::text)) WHERE tenant_id=$1 AND kind='subscription' AND id='local-subscription'").bind(tenant).bind(field).bind(value).execute(owner).await.unwrap();
        assert!(store.read_stripe_bound_record(&binding).await.is_err());
        sqlx::query("UPDATE fl_billing.records SET data=$2 WHERE tenant_id=$1 AND kind='subscription' AND id='local-subscription'").bind(tenant).bind(sqlx::types::Json(ciphertext.0.clone())).execute(owner).await.unwrap();
    }
    sqlx::query("UPDATE fl_billing.provider_bindings SET subscription_provider_customer_id='cus_changed' WHERE tenant_id=$1 AND kind='subscription'").bind(tenant).execute(owner).await.unwrap();
    assert!(store.read_stripe_bound_record(&binding).await.is_err());
    assert!(store.revoke_stripe_binding(&binding).await.is_err());
    sqlx::query("UPDATE fl_billing.provider_bindings SET subscription_provider_customer_id='cus_owned' WHERE tenant_id=$1 AND kind='subscription'").bind(tenant).execute(owner).await.unwrap();
    let mut tx = runtime.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(sqlx::query("UPDATE fl_billing.provider_binding_audit SET local_party_id='tampered' WHERE kind='subscription'").execute(&mut *tx).await.unwrap().rows_affected(),0);
    assert_eq!(
        sqlx::query("DELETE FROM fl_billing.provider_binding_audit WHERE kind='subscription'")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.provider_binding_audit FROM {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(store.revoke_stripe_binding(&binding).await.is_err());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.provider_binding_audit TO {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(store.read_stripe_bound_record(&binding).await.is_ok());
    store.revoke_stripe_binding(&binding).await.unwrap();
    let replacement = store
        .provision_stripe_subscription_binding(
            &scope,
            public,
            "local-subscription",
            "sub_owned",
            &relation,
        )
        .await
        .unwrap();
    assert!(store.read_stripe_bound_record(&binding).await.is_err());
    assert!(store.revoke_stripe_binding(&binding).await.is_err());
    assert!(store.read_stripe_bound_record(&replacement).await.is_ok());
    let audits: Vec<(String,String,String)> = sqlx::query_as("SELECT subscription_provider_customer_id,local_party_id,local_party_type FROM fl_billing.provider_binding_audit WHERE tenant_id=$1 AND kind='subscription'").bind(tenant).fetch_all(owner).await.unwrap();
    assert_eq!(audits.len(), 3);
    assert!(
        audits
            .iter()
            .all(|(customer, party, kind)| customer == "cus_owned"
                && party == "local-party"
                && kind == "user")
    );
    migration_compatibility(owner).await;
}

async fn migration_compatibility(owner: &sqlx::PgPool) {
    // A disposable schema inside a rolled-back transaction exercises the actual
    // prior migrations and upgrade without touching the shared billing schema.
    let schema = format!("fl_binding_upgrade_{}", Uuid::new_v4().simple());
    let mut tx = owner.begin().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut *tx)
        .await
        .unwrap();
    for migration in [
        include_str!("../../migrations/1_billing.sql"),
        include_str!("../../migrations/5_provider_bindings.sql"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(
            migration.replace("fl_billing", &schema),
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    let public = Uuid::new_v4();
    let generation = Uuid::new_v4();
    sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO {schema}.records(tenant_id,scope,kind,id,data) VALUES('old','durable','checkout_session','cs_old','{{\"ciphertext\":\"unchanged\"}}')"))).execute(&mut *tx).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO {schema}.provider_bindings(tenant_id,kind,public_id,binding_id,storage_id,deployment_label,account_id,livemode,provider_id) VALUES('old','checkout_session',$1::uuid,$2::uuid,'cs_old','old','acct_old',false,'cs_old')"))).bind(public.to_string()).bind(generation.to_string()).execute(&mut *tx).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO {schema}.provider_binding_audit(id,tenant_id,binding_id,public_id,kind,storage_id,deployment_label,account_id,livemode,provider_id,operation,actor_id,correlation_id) VALUES($1::uuid,'old',$2::uuid,$3::uuid,'checkout_session','cs_old','old','acct_old',false,'cs_old','provision','old-actor','old-correlation')")))
        .bind(Uuid::new_v4().to_string()).bind(generation.to_string()).bind(public.to_string()).execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        include_str!("../../migrations/6_subscription_bindings.sql").replace("fl_billing", &schema),
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let (got_public,got_generation,customer):(String,String,Option<String>)=sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT public_id::text,binding_id::text,subscription_provider_customer_id FROM {schema}.provider_bindings"))).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        (got_public, got_generation, customer),
        (public.to_string(), generation.to_string(), None)
    );
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT data FROM {schema}.records"
    )))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(row.0, json!({"ciphertext":"unchanged"}));
    let history: (String, Option<String>) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT actor_id,subscription_provider_customer_id FROM {schema}.provider_binding_audit"
    )))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(history, ("old-actor".into(), None));
    for change in [
        "subscription_provider_customer_id='cus_wrong'",
        "kind='subscription'",
        "kind='plan'",
    ] {
        sqlx::raw_sql("SAVEPOINT invalid_relation")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE {schema}.provider_bindings SET {change}"
            )))
            .execute(&mut *tx)
            .await
            .is_err()
        );
        sqlx::raw_sql("ROLLBACK TO SAVEPOINT invalid_relation")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.rollback().await.unwrap();
}
