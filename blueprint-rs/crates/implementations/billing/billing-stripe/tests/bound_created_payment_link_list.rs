#![cfg(feature = "postgres-identity")]
#[path = "support/bound_creation_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::{Fixture, request};
use forklaunch_billing_stripe::{
    BoundPaymentLinkListLimit as Limit,
    bound_creation::{BoundPaymentLinkCreationOutcome, BoundPaymentLinkCreationProfile},
};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

#[tokio::test]
#[ignore = "owned PostgreSQL and authenticated loopback provider"]
async fn created_payment_link_list_matrix() {
    let f = Arc::new(Fixture::new().await);
    let run = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    result.unwrap();
}
async fn matrix(f: &Fixture) {
    let mut ids = Vec::new();
    let mut records = Vec::new();
    for active in [true, false, true] {
        let BoundPaymentLinkCreationOutcome::Completed { record, .. } = f
            .service
            .create_bound_payment_link(&f.profile, Uuid::new_v4(), request())
            .await
            .unwrap()
        else {
            panic!("completed");
        };
        let provider = record.provider_fields.as_ref().unwrap()["id"]
            .as_str()
            .unwrap();
        f.provider.readbacks.lock().unwrap().insert(
            provider.into(),
            json!({"id":provider,"object":"payment_link","livemode":false,"active":active}),
        );
        ids.push(Uuid::parse_str(&record.id).unwrap());
        records.push(record);
    }
    denied_selection(f, &ids).await;
    let got = f
        .service
        .list_bound_created_payment_links(
            &f.profile,
            &[ids[2], ids[1], ids[0], ids[2]],
            Limit::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        got.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        vec![
            records[2].id.as_str(),
            records[0].id.as_str(),
            records[2].id.as_str()
        ]
    );
    for value in &got {
        assert_eq!(value.amount.to_bits(), records[0].amount.to_bits());
        assert_eq!(value.status, records[0].status);
    }
    let got = f
        .service
        .list_bound_created_payment_links(
            &f.profile,
            &vec![ids[0]; 150],
            Limit::trusted(150).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(got.len(), 150);
    let provider = records[0].provider_fields.as_ref().unwrap()["id"]
        .as_str()
        .unwrap();
    for active in [Value::Null, json!("true"), json!(1), json!({})] {
        f.provider
            .readbacks
            .lock()
            .unwrap()
            .get_mut(provider)
            .unwrap()["active"] = active;
        assert!(
            f.service
                .list_bound_created_payment_links(&f.profile, &[ids[0]], Limit::default())
                .await
                .is_err()
        );
    }
    f.provider
        .readbacks
        .lock()
        .unwrap()
        .get_mut(provider)
        .unwrap()["active"] = json!(true);
    tamper_and_deadline(f, ids[0], false).await;
    tamper_and_deadline(f, ids[0], true).await;
    assert_eq!(f.posts(), 3);
}
async fn tamper_and_deadline(f: &Fixture, id: Uuid, expiry: bool) {
    f.provider.pause_read.store(true, Ordering::SeqCst);
    let service = f.service.clone();
    let scope = f.scope.clone();
    let read = tokio::spawn(async move {
        let profile =
            BoundPaymentLinkCreationProfile::trusted(scope, vec!["price_owned".into()]).unwrap();
        service
            .list_bound_created_payment_links(&profile, &[id, id], Limit::default())
            .await
    });
    f.provider.entered.notified().await;
    if expiry {
        tokio::time::sleep(std::time::Duration::from_millis(5100)).await;
    } else {
        let mut tx = f.owner.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
            .bind(f.tenant.id())
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE fl_billing.provider_bindings SET provider_id=provider_id WHERE tenant_id=$1",
        )
        .bind(f.tenant.id())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    f.provider.pause_read.store(false, Ordering::SeqCst);
    f.provider.resume.notify_one();
    assert!(read.await.unwrap().is_err());
}

async fn denied_selection(f: &Fixture, ids: &[Uuid]) {
    let before = f.provider.requests.load(Ordering::SeqCst);
    assert!(Limit::trusted(0).is_err());
    assert!(Limit::trusted(1001).is_err());
    for selection in [
        vec![],
        vec![Uuid::nil()],
        vec![ids[0]; 101],
        vec![ids[0], Uuid::new_v4()],
    ] {
        assert!(
            f.service
                .list_bound_created_payment_links(&f.profile, &selection, Limit::default())
                .await
                .is_err()
        );
        assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
    }
    let foreign = forklaunch_billing_stripe::services::StripeBillingService::new(
        forklaunch_billing_base::services::BaseBillingService::new(Arc::new(
            f.store
                .with_audit_context(forklaunch_core::domain::AuditContext::actor(
                    "foreign", "list",
                ))
                .unwrap(),
        )),
        f.client.clone(),
    );
    assert!(
        foreign
            .list_bound_created_payment_links(&f.profile, ids, Limit::default())
            .await
            .is_err()
    );
    let changed =
        BoundPaymentLinkCreationProfile::trusted(f.scope.clone(), vec!["price_changed".into()])
            .unwrap();
    assert!(
        f.service
            .list_bound_created_payment_links(&changed, ids, Limit::default())
            .await
            .is_err()
    );
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
}
