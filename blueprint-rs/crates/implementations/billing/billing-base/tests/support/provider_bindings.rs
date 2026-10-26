use forklaunch_billing_base::services::{
    BillingKind, BillingStore, Scope,
    postgres::{PostgresBillingStore, StripeBindingScope},
};
use forklaunch_core::domain::AuditContext;
use serde_json::Value;
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
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT SELECT,INSERT,DELETE ON fl_billing.provider_bindings TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.provider_binding_audit TO {role}")))
        .execute(owner).await.unwrap();
    let kind = BillingKind::CheckoutSession;
    let scope =
        StripeBindingScope::administrative("owned-deployment".into(), "acct_owned".into(), false)
            .unwrap();
    assert!(
        StripeBindingScope::administrative("bad\nlabel".into(), "acct_owned".into(), false)
            .is_err()
    );
    assert!(
        StripeBindingScope::administrative("owned".into(), "cus_not_account".into(), false)
            .is_err()
    );
    let public = Uuid::new_v4();
    for (id, row_scope) in [
        ("cs_legacy_binding", Scope::Durable),
        ("cs_binding_other", Scope::Durable),
        ("cs_cache_only", Scope::Cache),
    ] {
        store
            .insert(
                row_scope,
                kind,
                super::checkout(id),
                Duration::from_secs(600),
            )
            .await
            .unwrap();
    }
    let raw:sqlx::types::Json<Value> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='checkout_session' AND id='cs_legacy_binding'")
        .bind(tenant).fetch_one(owner).await.unwrap();
    let anonymous = store
        .with_audit_context(AuditContext {
            actor_id: None,
            correlation_id: "binding-anonymous".into(),
        })
        .unwrap();
    assert!(
        anonymous
            .provision_stripe_binding(
                &scope,
                kind,
                public,
                "cs_legacy_binding",
                "cs_provider_owned"
            )
            .await
            .is_err()
    );
    assert!(
        store
            .provision_stripe_binding(&scope, kind, public, "cs_cache_only", "cs_provider_owned")
            .await
            .is_err()
    );
    assert!(
        other
            .provision_stripe_binding(
                &scope,
                kind,
                public,
                "cs_legacy_binding",
                "cs_provider_owned"
            )
            .await
            .is_err()
    );
    assert!(
        store
            .provision_stripe_binding(
                &scope,
                BillingKind::Plan,
                public,
                "cs_legacy_binding",
                "cs_provider_owned"
            )
            .await
            .is_err()
    );
    // No binding may survive a failed audit write.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.provider_binding_audit FROM {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(
        store
            .provision_stripe_binding(
                &scope,
                kind,
                public,
                "cs_legacy_binding",
                "cs_provider_owned"
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
    let (a, b) = tokio::join!(
        store.provision_stripe_binding(
            &scope,
            kind,
            public,
            "cs_legacy_binding",
            "cs_provider_owned"
        ),
        store.provision_stripe_binding(
            &scope,
            kind,
            public,
            "cs_legacy_binding",
            "cs_provider_owned"
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let binding = a.or(b).unwrap();
    assert_eq!(binding.public_id(), public);
    assert_eq!(binding.storage_id(), "cs_legacy_binding");
    assert_eq!(binding.provider_id(), "cs_provider_owned");
    // Both public and provider lookup require exact tenant, deployment, account,
    // mode and kind, including after reconstructing the store.
    assert_eq!(
        reopened
            .stripe_binding_by_provider_id(&scope, kind, "cs_provider_owned")
            .await
            .unwrap()
            .unwrap()
            .public_id(),
        public
    );
    assert!(
        other
            .stripe_binding_by_public_id(&scope, kind, public)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        other
            .stripe_binding_by_provider_id(&scope, kind, "cs_provider_owned")
            .await
            .unwrap()
            .is_none()
    );
    for wrong in [
        StripeBindingScope::administrative("other-deployment".into(), "acct_owned".into(), false)
            .unwrap(),
        StripeBindingScope::administrative("owned-deployment".into(), "acct_other".into(), false)
            .unwrap(),
        StripeBindingScope::administrative("owned-deployment".into(), "acct_owned".into(), true)
            .unwrap(),
    ] {
        assert!(
            store
                .stripe_binding_by_public_id(&wrong, kind, public)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .stripe_binding_by_provider_id(&wrong, kind, "cs_provider_owned")
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        store
            .stripe_binding_by_public_id(&scope, BillingKind::PaymentLink, public)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .provision_stripe_binding(
                &scope,
                kind,
                Uuid::new_v4(),
                "cs_binding_other",
                "cs_provider_owned"
            )
            .await
            .is_err()
    );
    assert!(
        store
            .provision_stripe_binding(&scope, kind, public, "cs_binding_other", "cs_other")
            .await
            .is_err()
    );
    assert!(
        store
            .provision_stripe_binding(
                &scope,
                kind,
                Uuid::new_v4(),
                "cs_legacy_binding",
                "cs_other"
            )
            .await
            .is_err()
    );
    assert!(
        store
            .delete(Scope::Durable, kind, "cs_legacy_binding")
            .await
            .is_err()
    );
    assert!(other.revoke_stripe_binding(&binding).await.is_err());
    assert!(anonymous.revoke_stripe_binding(&binding).await.is_err());
    let blank = store
        .with_audit_context(AuditContext {
            actor_id: Some("   ".into()),
            correlation_id: "binding-blank".into(),
        })
        .unwrap();
    assert!(
        blank
            .provision_stripe_binding(
                &scope,
                kind,
                Uuid::new_v4(),
                "cs_binding_other",
                "cs_blank_actor"
            )
            .await
            .is_err()
    );
    assert!(blank.revoke_stripe_binding(&binding).await.is_err());
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
    assert!(
        store
            .stripe_binding_by_public_id(&scope, kind, public)
            .await
            .unwrap()
            .is_some()
    );
    store.revoke_stripe_binding(&binding).await.unwrap();
    let replacement = store
        .provision_stripe_binding(
            &scope,
            kind,
            public,
            "cs_legacy_binding",
            "cs_provider_owned",
        )
        .await
        .unwrap();
    assert!(store.revoke_stripe_binding(&binding).await.is_err());
    assert!(
        store
            .stripe_binding_by_public_id(&scope, kind, public)
            .await
            .unwrap()
            .is_some()
    );
    store.revoke_stripe_binding(&replacement).await.unwrap();
    let after:sqlx::types::Json<Value> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='checkout_session' AND id='cs_legacy_binding'")
        .bind(tenant).fetch_one(owner).await.unwrap();
    assert_eq!(
        raw, after,
        "administrative aliases must not change storage keys/ciphertext"
    );
    let audited:i64 = sqlx::query_scalar("SELECT count(*) FROM fl_billing.provider_binding_audit WHERE tenant_id=$1 AND actor_id='service:billing-fixture' AND correlation_id='billing-fixture-correlation'")
        .bind(tenant).fetch_one(owner).await.unwrap();
    assert_eq!(audited, 4);
    // RLS forbids audit UPDATE/DELETE even with broad fixture-role table grants.
    let mut tx = runtime.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query(
            "UPDATE fl_billing.provider_binding_audit SET actor_id='tampered' WHERE tenant_id=$1"
        )
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected(),
        0
    );
    assert_eq!(
        sqlx::query("DELETE FROM fl_billing.provider_binding_audit WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.commit().await.unwrap();
    assert!(
        store
            .delete(Scope::Durable, kind, "cs_legacy_binding")
            .await
            .unwrap()
    );
}
