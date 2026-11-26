#![cfg(feature = "postgres-identity")]
#[path = "support/bound_plan_creation_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::{Fixture, plan, request};
use forklaunch_billing_base::services::{BaseBillingService, BillingKind, BillingStore, Scope};
use forklaunch_billing_stripe::{
    bound_plan_creation::{BoundPlanCreationOutcome, BoundPlanCreationProfile},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn created_plan_read_provenance_matrix() {
    let f = Fixture::new().await;
    let BoundPlanCreationOutcome::Completed { record, .. } = f
        .service
        .create_bound_plan(&f.profile, Uuid::new_v4(), request())
        .await
        .unwrap()
    else {
        panic!("expected created Plan")
    };
    let id = Uuid::parse_str(&record.id).unwrap();
    let posts = f.posts();
    let mut fresh = plan(&record.external_id);
    fresh["product"]["name"] = json!("Fresh product presentation");
    *f.provider.readback.lock().unwrap() = Some(fresh.clone());
    let observed = f
        .service
        .get_bound_created_plan(&f.profile, id)
        .await
        .unwrap();
    let mut expected = serde_json::to_value(&record).unwrap();
    expected["providerFields"] = fresh["product"].clone();
    assert_eq!(serde_json::to_value(observed).unwrap(), expected);
    assert_eq!(f.posts(), posts);
    invalid_provider(&f, id, &fresh).await;
    denied_provenance(&f, id).await;
    snapshot_changes(&f, id).await;
    audit_waits(&f, id).await;
    // Mutable DTO values cannot replace sealed receipt/target authority.
    f.store
        .patch(
            Scope::Durable,
            BillingKind::Plan,
            &id.to_string(),
            json!({"externalId":"attacker","providerFields":{"id":"attacker"}}),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let before = f.provider.requests.load(Ordering::SeqCst);
    assert!(
        f.service
            .get_bound_created_plan(&f.profile, id)
            .await
            .is_err()
    );
    assert_eq!(before, f.provider.requests.load(Ordering::SeqCst));
    assert_eq!(f.posts(), posts);
    f.cleanup().await;
}
async fn invalid_provider(f: &Fixture, id: Uuid, good: &Value) {
    for (key, bad) in [
        ("id", json!("wrong")),
        ("object", json!("price")),
        ("livemode", json!(true)),
        ("amount", json!(1201)),
        ("amount", json!(1200.5)),
        ("currency", json!("eur")),
        ("interval", json!("year")),
        ("interval_count", json!(2)),
        ("active", json!(false)),
        ("usage_type", json!("metered")),
        ("billing_scheme", json!("tiered")),
        ("product", json!("custom-product")),
        ("product", Value::Null),
    ] {
        let mut value = good.clone();
        value[key] = bad;
        *f.provider.readback.lock().unwrap() = Some(value);
        assert!(
            f.service
                .get_bound_created_plan(&f.profile, id)
                .await
                .is_err(),
            "{key}"
        );
    }
    for (key, bad) in [
        ("id", json!("wrong")),
        ("object", json!("plan")),
        ("livemode", json!(true)),
        ("deleted", json!(true)),
        ("deleted", Value::Null),
    ] {
        let mut value = good.clone();
        value["product"][key] = bad;
        *f.provider.readback.lock().unwrap() = Some(value);
        assert!(
            f.service
                .get_bound_created_plan(&f.profile, id)
                .await
                .is_err(),
            "product {key}"
        );
    }
    *f.provider.readback.lock().unwrap() = Some(good.clone());
}
async fn denied_provenance(f: &Fixture, id: Uuid) {
    let changed = BoundPlanCreationProfile::trusted(
        f.scope.clone(),
        vec!["custom-product".into(), "other".into()],
    )
    .unwrap();
    let before = f.provider.requests.load(Ordering::SeqCst);
    assert!(
        f.service
            .get_bound_created_plan(&changed, id)
            .await
            .is_err()
    );
    let actor = StripeBillingService::new(
        BaseBillingService::new(std::sync::Arc::new(
            f.store
                .with_audit_context(AuditContext::actor("other", "read"))
                .unwrap(),
        )),
        f.client.clone(),
    );
    assert!(actor.get_bound_created_plan(&f.profile, id).await.is_err());
    assert!(
        f.service
            .get_bound_created_plan(&f.profile, Uuid::new_v4())
            .await
            .is_err()
    );
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::Plan, id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .plan_creation_read_snapshot(&binding, "wrong-version")
            .await
            .is_err()
    );
    missing_provenance(f, id).await;
    let managed = StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        forklaunch_billing_stripe::client::StripeBillingClient::managed(
            "http://127.0.0.1:9",
            Uuid::new_v4().to_string(),
            "key".into(),
        )
        .unwrap(),
    );
    assert!(
        managed
            .get_bound_created_plan(&f.profile, id)
            .await
            .is_err()
    );
    let foreign_store = forklaunch_billing_base::services::postgres::PostgresBillingStore::new(
        f.pool.clone(),
        forklaunch_core::persistence::TenantContext::new(format!("{}-foreign", f.role)).unwrap(),
        std::sync::Arc::new(
            forklaunch_core::persistence::encryption::FieldEncryptor::new(
                "owned-bound-create-encryption-key",
            )
            .unwrap(),
        ),
        AuditContext::actor("service:bound-create", "read"),
    )
    .await
    .unwrap();
    let foreign = StripeBillingService::new(
        BaseBillingService::new(std::sync::Arc::new(foreign_store)),
        f.client.clone(),
    );
    assert!(
        foreign
            .get_bound_created_plan(&f.profile, id)
            .await
            .is_err()
    );
    assert_eq!(before, f.provider.requests.load(Ordering::SeqCst));
}
async fn missing_provenance(f: &Fixture, id: Uuid) {
    for pending in [false, true] {
        let public_id = if pending {
            let forklaunch_billing_base::services::postgres::PlanCreationClaim::Reserved(ticket) =
                f.store
                    .reserve_plan_creation(
                        &f.scope,
                        Uuid::new_v4(),
                        forklaunch_billing_stripe::client::STRIPE_API_VERSION,
                        &json!({}),
                    )
                    .await
                    .unwrap()
            else {
                panic!("fresh reservation")
            };
            ticket.public_id()
        } else {
            Uuid::new_v4()
        };
        let provider_id = format!("custom-administrative-{public_id}");
        let mut local = f
            .store
            .get(Scope::Durable, BillingKind::Plan, &id.to_string())
            .await
            .unwrap()
            .unwrap();
        local["id"] = json!(public_id.to_string());
        local["externalId"] = json!(provider_id);
        f.store
            .insert(
                Scope::Durable,
                BillingKind::Plan,
                local,
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        f.store
            .provision_stripe_binding(
                &f.scope,
                BillingKind::Plan,
                public_id,
                &public_id.to_string(),
                &provider_id,
            )
            .await
            .unwrap();
        assert!(
            f.service
                .get_bound_created_plan(&f.profile, public_id)
                .await
                .is_err()
        );
    }
}
async fn snapshot_changes(f: &Fixture, id: Uuid) {
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::Plan, id)
        .await
        .unwrap()
        .unwrap();
    for sql in [
        "UPDATE fl_billing.records SET data=data WHERE tenant_id=$1 AND kind='plan'",
        "UPDATE fl_billing.provider_bindings SET provider_id=provider_id WHERE tenant_id=$1 AND kind='plan'",
    ] {
        let snapshot = f
            .store
            .plan_creation_read_snapshot(
                &binding,
                forklaunch_billing_stripe::client::STRIPE_API_VERSION,
            )
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind(f.tenant.id())
            .execute(&f.owner)
            .await
            .unwrap();
        assert!(
            f.store
                .read_plan_creation_snapshot(&snapshot)
                .await
                .is_err()
        );
    }
}
async fn waiting(f: &Fixture) {
    for _ in 0..100 {
        let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%INSERT INTO fl_billing.sensitive_read_audit%')")
            .fetch_one(&f.owner).await.unwrap();
        if found {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("audit did not wait");
}
async fn audit_waits(f: &Fixture, id: Uuid) {
    for expired in [false, true] {
        f.provider.pause_read.store(true, Ordering::SeqCst);
        let call = f.service.get_bound_created_plan(&f.profile, id);
        tokio::pin!(call);
        tokio::select! {()=f.provider.entered.notified()=>{}, result=&mut call=>panic!("must reach GET: {}",result.is_ok())}
        let mut lock = f.owner.begin().await.unwrap();
        sqlx::query("LOCK TABLE fl_billing.sensitive_read_audit IN ACCESS EXCLUSIVE MODE")
            .execute(&mut *lock)
            .await
            .unwrap();
        f.provider.resume.notify_one();
        tokio::select! {()=waiting(f)=>{}, result=&mut call=>panic!("must wait: {}",result.is_ok())}
        if expired {
            tokio::time::sleep(Duration::from_millis(5100)).await;
        } else {
            sqlx::query(
                "UPDATE fl_billing.records SET data=data WHERE tenant_id=$1 AND kind='plan'",
            )
            .bind(f.tenant.id())
            .execute(&f.owner)
            .await
            .unwrap();
        }
        lock.commit().await.unwrap();
        assert!(call.await.is_err());
        f.provider.pause_read.store(false, Ordering::SeqCst);
    }
}
