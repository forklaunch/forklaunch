use super::*;
use forklaunch_billing_base::services::{BillingKind, postgres::PostgresBillingStore};
use forklaunch_core::domain::AuditContext;

pub async fn matrix(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundSubscriptionCreationProfile,
    publisher: &TrustedSubscriptionPublisher,
) {
    let Outcome::Completed { record, .. } = service
        .create_bound_subscription(profile, publisher, Uuid::new_v4(), request(f))
        .await
        .unwrap()
    else {
        panic!("created")
    };
    let id = Uuid::parse_str(&record.id).unwrap();
    let mut current = subscription(1);
    current["status"] = json!("active");
    current["metadata"] = json!({"presentation":"fresh"});
    *p.readback.lock().unwrap() = Some(current.clone());
    let fresh = service
        .get_bound_created_subscription(profile, id)
        .await
        .unwrap();
    let mut expected = serde_json::to_value(&record).unwrap();
    expected["providerFields"] = current.clone();
    assert_eq!(serde_json::to_value(fresh).unwrap(), expected);
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    provider_mismatches(f, p, service, profile, id, &current).await;
    active_regression(f, p, service, profile, publisher).await;
    *p.readback.lock().unwrap() = Some(current.clone());
    local_denials(f, p, service, profile, id).await;
    let role: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT,UPDATE,DELETE ON fl_billing.records,fl_billing.provider_bindings,fl_billing.stripe_customer_bindings,fl_billing.subscription_creation_intents,fl_billing.subscription_creation_audit,fl_billing.subscription_dependency_claims,fl_billing.plan_creation_intents FROM {role}"))).execute(&f.owner).await.unwrap();
    // Ordinary SELECT + sensitive-read audit INSERT only, no dependency locks/publication.
    assert!(
        service
            .get_bound_created_subscription(profile, id)
            .await
            .is_ok()
    );
    audit_expiry(f, p, service, profile, id).await;
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::Subscription, id)
        .await
        .unwrap()
        .unwrap();
    let snapshot = f
        .store
        .subscription_creation_read_snapshot(&binding, STRIPE_API_VERSION)
        .await
        .unwrap();
    update(
        f,
        "UPDATE fl_billing.provider_bindings SET provider_id=provider_id WHERE kind='subscription'",
    )
    .await;
    assert!(
        f.store
            .read_subscription_creation_snapshot(&snapshot)
            .await
            .is_err()
    );
    // Same content with a different Customer xmin invalidates original dependencies.
    update(
        f,
        "UPDATE fl_billing.records SET data=data WHERE kind='customer'",
    )
    .await;
    let before = p.requests.load(Ordering::SeqCst);
    assert!(
        service
            .get_bound_created_subscription(profile, id)
            .await
            .is_err()
    );
    assert_eq!(before, p.requests.load(Ordering::SeqCst));
    assert_eq!(p.posts.load(Ordering::SeqCst), 2);
}
async fn local_denials(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundSubscriptionCreationProfile,
    id: Uuid,
) {
    let before = p.requests.load(Ordering::SeqCst);
    let changed = BoundSubscriptionCreationProfile::trusted(
        f.scope.clone(),
        vec![(f.plan, f.plan_operation), (Uuid::new_v4(), Uuid::new_v4())],
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
    )
    .unwrap();
    assert!(
        service
            .get_bound_created_subscription(&changed, id)
            .await
            .is_err()
    );
    let other = StripeBillingService::new(
        BaseBillingService::new(Arc::new(
            f.store
                .with_audit_context(AuditContext::actor("other", "read"))
                .unwrap(),
        )),
        service.client().clone(),
    );
    assert!(
        other
            .get_bound_created_subscription(profile, id)
            .await
            .is_err()
    );
    assert!(
        service
            .get_bound_created_subscription(profile, Uuid::new_v4())
            .await
            .is_err()
    );
    let managed = StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        StripeBillingClient::managed(
            "http://127.0.0.1:9",
            Uuid::new_v4().to_string(),
            "key".into(),
        )
        .unwrap(),
    );
    assert!(
        managed
            .get_bound_created_subscription(profile, id)
            .await
            .is_err()
    );
    assert_eq!(before, p.requests.load(Ordering::SeqCst));
}
async fn provider_mismatches(
    _f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundSubscriptionCreationProfile,
    id: Uuid,
    good: &Value,
) {
    for (path, bad) in [
        ("/id", json!("sub_other")),
        ("/object", json!("customer")),
        ("/livemode", json!(true)),
        ("/customer", json!("cus_other")),
        ("/status", json!("canceled")),
        ("/start_date", json!(1)),
        ("/items/has_more", json!(true)),
        ("/items/data/0/id", json!("si_other")),
        ("/items/data/0/quantity", json!(3)),
        ("/items/data/0/plan/id", json!("wrong")),
        ("/items/data/0/price/unit_amount", json!(1201)),
    ] {
        let mut bad_value = good.clone();
        *bad_value.pointer_mut(path).unwrap() = bad;
        *p.readback.lock().unwrap() = Some(bad_value);
        assert!(
            service
                .get_bound_created_subscription(profile, id)
                .await
                .is_err(),
            "{path}"
        );
    }
    *p.readback.lock().unwrap() = Some(good.clone());
    for bad in [
        json!({"id":"cus_other","object":"customer","livemode":false}),
        json!({"id":"cus_owned","object":"customer","livemode":true}),
        json!({"id":"cus_owned","object":"customer","livemode":false,"deleted":true}),
    ] {
        *p.customer_response.lock().unwrap() = Some(bad);
        assert!(
            service
                .get_bound_created_subscription(profile, id)
                .await
                .is_err()
        );
    }
    *p.customer_response.lock().unwrap() = None;
    for (path, bad) in [
        ("/id", json!("wrong")),
        ("/amount", json!(1201)),
        ("/product/id", json!("wrong")),
        ("/product/livemode", json!(true)),
    ] {
        let mut bad_value = plan();
        *bad_value.pointer_mut(path).unwrap() = bad;
        *p.plan_response.lock().unwrap() = Some(bad_value);
        assert!(
            service
                .get_bound_created_subscription(profile, id)
                .await
                .is_err()
        );
    }
    *p.plan_response.lock().unwrap() = None;
}
async fn active_regression(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundSubscriptionCreationProfile,
    publisher: &TrustedSubscriptionPublisher,
) {
    let mut active = subscription(2);
    active["status"] = json!("active");
    *p.response.lock().unwrap() = Some(active.clone());
    *p.readback.lock().unwrap() = Some(active);
    let Outcome::Completed { record, .. } = service
        .create_bound_subscription(profile, publisher, Uuid::new_v4(), request(f))
        .await
        .unwrap()
    else {
        panic!("active created")
    };
    *p.readback.lock().unwrap() = Some(subscription(2));
    assert!(
        service
            .get_bound_created_subscription(profile, Uuid::parse_str(&record.id).unwrap())
            .await
            .is_err()
    );
    *p.response.lock().unwrap() = None;
}
async fn update(f: &Fixture, sql: &str) {
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
async fn waiting(f: &Fixture) {
    for _ in 0..100 {
        let found:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%INSERT INTO fl_billing.sensitive_read_audit%')").fetch_one(&f.pool).await.unwrap();
        if found {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("audit did not wait");
}
async fn audit_expiry(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundSubscriptionCreationProfile,
    id: Uuid,
) {
    p.pause.store(2, Ordering::SeqCst);
    let call = service.get_bound_created_subscription(profile, id);
    tokio::pin!(call);
    tokio::select! {()=p.entered.notified()=>{}, result=&mut call=>panic!("must reach GET {}",result.is_ok())}
    let mut lock = f.owner.begin().await.unwrap();
    sqlx::query("LOCK TABLE fl_billing.sensitive_read_audit IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    p.resume.notify_one();
    tokio::select! {()=waiting(f)=>{},result=&mut call=>panic!("must await audit {}",result.is_ok())}
    tokio::time::sleep(Duration::from_millis(5100)).await;
    lock.commit().await.unwrap();
    assert!(call.await.is_err());
    p.pause.store(0, Ordering::SeqCst);
}
