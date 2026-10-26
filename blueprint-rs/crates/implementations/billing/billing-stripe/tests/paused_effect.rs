#![cfg(feature = "postgres-reconciliation")]
use axum::{
    Json, Router,
    extract::{Request, State},
    routing::any,
};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope,
    postgres::{
        PausedEffectAcknowledgement, PendingWebhook, PostgresBillingStore, StripeBindingScope,
        StripeSubscriptionOwnership,
    },
};
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone)]
struct Evidence {
    event: Arc<Mutex<Value>>,
    subscription: Arc<Mutex<Value>>,
    paths: Arc<Mutex<Vec<String>>>,
    managed: bool,
    scope: Arc<Mutex<Value>>,
}
async fn pending(store: &PostgresBillingStore, id: &str) -> PendingWebhook {
    assert!(
        store
            .with_webhook_receipt("stripe", id, "customer.subscription.paused", || async {
                Err(BlueprintError::new("fixture_interruption", "interrupted"))
            })
            .await
            .is_err()
    );
    store
        .pending_webhooks(1000)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.event_id == id)
        .unwrap()
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn paused_effect_evidence_and_atomic_bookkeeping() {
    exercise(false).await;
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn managed_paused_effect_evidence_and_atomic_bookkeeping() {
    exercise(true).await;
}
#[allow(clippy::too_many_lines)]
async fn exercise(managed: bool) {
    let owner = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresBillingStore::migrate(&owner).await.unwrap();
    let role = format!("fl_paused_ack_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'; GRANT USAGE ON SCHEMA fl_billing TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.records,fl_billing.provider_bindings TO {role}; GRANT INSERT ON fl_billing.provider_binding_audit,fl_billing.sensitive_read_audit,fl_billing.webhook_reconciliation_audit TO {role}; GRANT SELECT,INSERT ON fl_billing.webhook_receipts TO {role}; GRANT SELECT,INSERT,DELETE ON fl_billing.webhook_attempts TO {role}"))).execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password("owned-fixture"),
        )
        .await
        .unwrap();
    let tenant = TenantContext::new(format!("{role}-a")).unwrap();
    let keys = Arc::new(FieldEncryptor::new("paused-ack-owned-key").unwrap());
    let store = Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            tenant.clone(),
            keys.clone(),
            AuditContext::actor("operator:fixture", "paused-ack-fixture"),
        )
        .await
        .unwrap(),
    );
    let other = Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            TenantContext::new(format!("{role}-b")).unwrap(),
            keys,
            AuditContext::actor("operator:fixture", "paused-ack-fixture"),
        )
        .await
        .unwrap(),
    );
    let subscription = json!({"id":"sub_owned","object":"subscription","status":"paused","livemode":false,"customer":"cus_owned"});
    let event = json!({"object":"event","type":"customer.subscription.paused","created":chrono::Utc::now().timestamp()-1,"livemode":false,"data":{"object":subscription}});
    let evidence = Evidence {
        event: Arc::new(Mutex::new(event.clone())),
        subscription: Arc::new(Mutex::new(subscription.clone())),
        paths: Arc::new(Mutex::new(Vec::new())),
        managed,
        scope: Arc::new(Mutex::new(Value::Null)),
    };
    let app = Router::new()
        .fallback(any(
            |State(state): State<Evidence>, request: Request| async move {
                if state.managed {
                    use forklaunch_core::auth::request_hmac::{HmacAuthorization, SignedBody};
                    assert_eq!(request.method(), "POST");
                    assert!(request.uri().query().is_none());
                    let path = request.uri().path().to_owned();
                    let authorization = request.headers()["authorization"].to_str().unwrap().to_owned();
                    let auth = HmacAuthorization::parse(&authorization).unwrap();
                    let body = axum::body::to_bytes(request.into_body(), 8192).await.unwrap();
                    assert_eq!(auth.key_id(), "fixture-instance");
                    assert!(auth.verify_at(b"fixture-key", "POST", path.strip_prefix("/instance-gateway").unwrap(), SignedBody::Bytes(&body), chrono::Utc::now().timestamp_millis(), Duration::from_secs(5)));
                    let body: Value = serde_json::from_slice(&body).unwrap();
                    state.paths.lock().unwrap().push(path.clone());
                    if path == "/instance-gateway/payments/context" {
                        assert_eq!(body["accountId"], "acct_owned");
                        let now = chrono::Utc::now().timestamp_millis();
                        let scope = json!({"version":1,"instanceId":"fixture-instance","organizationId":"org-owned","accountId":"acct_owned","livemode":false,"bindingGeneration":"00000000-0000-4000-8000-000000000001","apiVersion":body["apiVersion"],"issuedAt":now,"expiresAt":now+5000});
                        *state.scope.lock().unwrap() = scope.clone();
                        return Json(json!({"scope":scope,"contextToken":"fixture-token"}));
                    }
                    assert_eq!(path, "/instance-gateway/payments/context/paused-subscription-read");
                    assert_eq!(body["contextToken"], "fixture-token");
                    assert_eq!(body["subscriptionId"], "sub_owned");
                    let mut event = state.event.lock().unwrap().clone();
                    if event.get("id").is_none() { event["id"] = body["eventId"].clone(); }
                    return Json(json!({"scope":state.scope.lock().unwrap().clone(),"event":event,"subscription":state.subscription.lock().unwrap().clone()}));
                }
                assert_eq!(request.method(), "GET");
                assert_eq!(
                    request.headers()["authorization"],
                    "Bearer opaque-owned-credential"
                );
                assert!(!request.headers().contains_key("stripe-account"));
                let path = request.uri().path().to_string();
                state.paths.lock().unwrap().push(path.clone());
                Json(match path.as_str() {
                    "/v1/account" => json!({"id":"acct_owned","object":"account"}),
                    "/v1/balance" => json!({"object":"balance","livemode":false}),
                    "/v1/subscriptions/sub_owned" => state.subscription.lock().unwrap().clone(),
                    _ => {
                        assert!(path.starts_with("/v1/events/evt_"));
                        let mut value = state.event.lock().unwrap().clone();
                        if value.get("id").is_none() {
                            value["id"] = json!(path.trim_start_matches("/v1/events/"));
                        }
                        value
                    }
                })
            },
        ))
        .with_state(evidence.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let http = ProviderHttpClient::new(
        &format!("http://{address}"),
        Duration::from_secs(10),
        1024 * 1024,
    )
    .unwrap();
    let client = if managed {
        StripeBillingClient::managed(
            &format!("http://{address}"),
            "fixture-instance".into(),
            "fixture-key".into(),
        )
        .unwrap()
    } else {
        StripeBillingClient::with_transport("opaque-owned-credential".into(), http).unwrap()
    };
    let service = StripeBillingService::new(BaseBillingService::new(store.clone()), client.clone());
    let scope =
        StripeBindingScope::administrative("paused-deployment".into(), "acct_owned".into(), false)
            .unwrap();
    let public = Uuid::new_v4();
    let row = json!({"id":"local-sub","externalId":"sub_owned","partyId":"local-user","partyType":"user","productId":"local-plan","active":false,"status":"paused","startDate":"2026-01-01T00:00:00Z","providerFields":{"customer":"cus_untrusted","private":"unchanged"}});
    store
        .insert(
            Scope::Durable,
            BillingKind::Subscription,
            row,
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    let relation = StripeSubscriptionOwnership::administrative(
        "cus_owned".into(),
        "local-user".into(),
        "user".into(),
    )
    .unwrap();
    let mut binding = store
        .provision_stripe_subscription_binding(&scope, public, "local-sub", "sub_owned", &relation)
        .await
        .unwrap();
    let original: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND id='local-sub'",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    let pending = pending(&store, "evt_main").await;
    for (index, change) in [
        json!({"id":"evt_other"}),
        json!({"type":"customer.subscription.resumed"}),
        json!({"livemode":true}),
        json!({"account":"acct_other"}),
        json!({"account":123}),
        json!({"account":{}}),
        json!({"context":"acct_other"}),
        json!({"created":chrono::Utc::now().timestamp()+60}),
        json!({"created":0}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut altered = event.clone();
        for (key, value) in change.as_object().unwrap() {
            altered[key] = value.clone();
        }
        *evidence.event.lock().unwrap() = altered;
        assert!(
            service
                .acknowledge_already_paused_effect(&pending, &scope, public)
                .await
                .is_err(),
            "event {index}"
        );
        assert!(
            !store
                .paused_subscription_effect_receipt_recorded(&pending)
                .await
                .unwrap()
        );
    }
    *evidence.event.lock().unwrap() = event.clone();
    for change in [
        json!({"status":"active","pause_collection":{"behavior":"void"}}),
        json!({"id":"sub_other"}),
        json!({"livemode":true}),
        json!({"customer":"cus_other"}),
        json!({"customer":null}),
        json!({"customer":{"id":"cus_owned","object":"customer","livemode":true}}),
        json!({"customer":{"id":"cus_owned","object":"customer","deleted":true}}),
        json!({"customer":{"id":"cus_owned","object":"customer","deleted":"false"}}),
    ] {
        let mut altered = subscription.clone();
        for (key, value) in change.as_object().unwrap() {
            altered[key] = value.clone();
        }
        *evidence.subscription.lock().unwrap() = subscription.clone();
        let mut event_changed = event.clone();
        event_changed["data"]["object"] = altered.clone();
        *evidence.event.lock().unwrap() = event_changed;
        assert!(
            service
                .acknowledge_already_paused_effect(&pending, &scope, public)
                .await
                .is_err()
        );
        *evidence.event.lock().unwrap() = event.clone();
        *evidence.subscription.lock().unwrap() = altered;
        assert!(
            service
                .acknowledge_already_paused_effect(&pending, &scope, public)
                .await
                .is_err()
        );
    }
    *evidence.subscription.lock().unwrap() = subscription;
    let foreign = StripeBillingService::new(BaseBillingService::new(other.clone()), client);
    assert!(
        foreign
            .acknowledge_already_paused_effect(&pending, &scope, public)
            .await
            .is_err()
    );
    let mut stale = pending.clone();
    stale.attempt_id = Uuid::new_v4().to_string();
    assert!(
        service
            .acknowledge_already_paused_effect(&stale, &scope, public)
            .await
            .is_err()
    );
    let mut changed_start = pending.clone();
    changed_start.started_at = "2099-01-01T00:00:00Z".into();
    assert!(
        service
            .acknowledge_already_paused_effect(&changed_start, &scope, public)
            .await
            .is_err()
    );
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let running_store = store.clone();
    let running = tokio::spawn(async move {
        running_store
            .with_webhook_receipt(
                "stripe",
                "evt_running",
                "customer.subscription.paused",
                || async {
                    entered.send(()).unwrap();
                    resume.await.unwrap();
                    Err(BlueprintError::new("interrupted", "interrupted"))
                },
            )
            .await
    });
    ready.await.unwrap();
    let busy = store
        .pending_webhooks(1000)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.event_id == "evt_running")
        .unwrap();
    assert!(
        service
            .acknowledge_already_paused_effect(&busy, &scope, public)
            .await
            .is_err()
    );
    release.send(()).unwrap();
    assert!(running.await.unwrap().is_err());
    // Both the local row and binding xmin defend against change-and-restore ABA.
    for binding_change in [false, true] {
        let snapshot = store
            .inspect_paused_subscription_effect(&pending, &binding)
            .await
            .unwrap();
        let table = if binding_change {
            "provider_bindings"
        } else {
            "records"
        };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE fl_billing.{table} SET tenant_id=tenant_id WHERE tenant_id=$1"
        )))
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
        assert!(
            store
                .acknowledge_paused_subscription_effect(snapshot, || Ok(()))
                .await
                .is_err()
        );
    }
    let revoked_snapshot = store
        .inspect_paused_subscription_effect(&pending, &binding)
        .await
        .unwrap();
    store.revoke_stripe_binding(&binding).await.unwrap();
    binding = store
        .provision_stripe_subscription_binding(&scope, public, "local-sub", "sub_owned", &relation)
        .await
        .unwrap();
    assert!(
        store
            .acknowledge_paused_subscription_effect(revoked_snapshot, || Ok(()))
            .await
            .is_err()
    );
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{active}','true') WHERE tenant_id=$1 AND id='local-sub'").bind(tenant.id()).execute(&owner).await.unwrap();
    assert!(
        service
            .acknowledge_already_paused_effect(&pending, &scope, public)
            .await
            .is_err()
    );
    sqlx::query("UPDATE fl_billing.records SET data=$2 WHERE tenant_id=$1 AND id='local-sub'")
        .bind(tenant.id())
        .bind(sqlx::types::Json(original.0.clone()))
        .execute(&owner)
        .await
        .unwrap();
    // Failure after local bookkeeping writes must roll them all back.
    let snapshot = store
        .inspect_paused_subscription_effect(&pending, &binding)
        .await
        .unwrap();
    let checks = AtomicUsize::new(0);
    assert!(
        store
            .acknowledge_paused_subscription_effect(snapshot, || {
                if checks.fetch_add(1, Ordering::SeqCst) >= 3 {
                    Err(BlueprintError::new("expired", "expired"))
                } else {
                    Ok(())
                }
            })
            .await
            .is_err()
    );
    assert!(
        !store
            .paused_subscription_effect_receipt_recorded(&pending)
            .await
            .unwrap()
    );
    for permission in [
        "INSERT ON fl_billing.webhook_reconciliation_audit",
        "INSERT ON fl_billing.webhook_receipts",
        "DELETE ON fl_billing.webhook_attempts",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {permission} FROM {role}"
        )))
        .execute(&owner)
        .await
        .unwrap();
        assert!(
            service
                .acknowledge_already_paused_effect(&pending, &scope, public)
                .await
                .is_err()
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT {permission} TO {role}")))
            .execute(&owner)
            .await
            .unwrap();
        assert!(
            !store
                .paused_subscription_effect_receipt_recorded(&pending)
                .await
                .unwrap()
        );
    }
    // Actual binding-row wait consumes the evidence window; no late success.
    let mut blocked = owner.begin().await.unwrap();
    sqlx::query("SELECT public_id FROM fl_billing.provider_bindings WHERE tenant_id=$1 FOR UPDATE")
        .bind(tenant.id())
        .execute(&mut *blocked)
        .await
        .unwrap();
    let held = async {
        tokio::time::sleep(Duration::from_millis(5200)).await;
        blocked.rollback().await.unwrap();
    };
    let (result, ()) = tokio::join!(
        service.acknowledge_already_paused_effect(&pending, &scope, public),
        held
    );
    assert!(result.is_err());
    assert!(
        !store
            .paused_subscription_effect_receipt_recorded(&pending)
            .await
            .unwrap()
    );
    assert_eq!(
        service
            .acknowledge_already_paused_effect(&pending, &scope, public)
            .await
            .unwrap(),
        PausedEffectAcknowledgement::Recorded
    );
    assert!(
        store
            .paused_subscription_effect_receipt_recorded(&pending)
            .await
            .unwrap()
    );
    assert!(
        service
            .acknowledge_already_paused_effect(&pending, &scope, public)
            .await
            .is_err()
    );
    let after: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND id='local-sub'",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(after.0, original.0);
    // A failed post-COMMIT evidence check reports a written receipt, not rollback.
    let late = self::pending(&store, "evt_late").await;
    let snapshot = store
        .inspect_paused_subscription_effect(&late, &binding)
        .await
        .unwrap();
    let checks = AtomicUsize::new(0);
    assert_eq!(
        store
            .acknowledge_paused_subscription_effect(snapshot, || {
                if checks.fetch_add(1, Ordering::SeqCst) >= 4 {
                    Err(BlueprintError::new("expired", "expired"))
                } else {
                    Ok(())
                }
            })
            .await
            .unwrap(),
        PausedEffectAcknowledgement::RecordedEvidenceNoLongerCurrent
    );
    assert!(
        store
            .paused_subscription_effect_receipt_recorded(&late)
            .await
            .unwrap()
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_billing.webhook_reconciliation_audit WHERE tenant_id=$1",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(audits, 2);
    assert!(evidence.paths.lock().unwrap().iter().all(|path| path
        == "/instance-gateway/payments/context"
        || path == "/instance-gateway/payments/context/paused-subscription-read"
        || path == "/v1/account"
        || path == "/v1/balance"
        || path == "/v1/subscriptions/sub_owned"
        || path.starts_with("/v1/events/evt_")));
    server.abort();
    drop(service);
    drop(foreign);
    drop(store);
    drop(other);
    pool.close().await;
    for table in [
        "provider_bindings",
        "provider_binding_audit",
        "records",
        "sensitive_read_audit",
        "webhook_receipts",
        "webhook_attempts",
        "webhook_reconciliation_audit",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_billing.{table} WHERE tenant_id=$1"
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
    owner.close().await;
}
