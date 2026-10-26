use forklaunch_core::persistence::{
    TenantContext,
    retention::{self, EnforcementOptions, RetentionAction, RetentionPolicy, RetentionStore},
};
use forklaunch_ecommerce_base::{
    retention::OrderCompliance,
    store::{EcommerceStore, Mutation, Query, Record, RecordKind, Selector},
};
use forklaunch_ecommerce_interface::OrderStatus;
use std::{sync::Arc, time::SystemTime};
fn date(value: &str) -> SystemTime {
    chrono::DateTime::parse_from_rfc3339(value).unwrap().into()
}

#[allow(clippy::too_many_lines)]
pub async fn exercise<S: EcommerceStore + RetentionStore>(store: Arc<S>, tenant: TenantContext) {
    for (id, created) in [
        ("retention-first", "2018-01-01T00:00:00Z"),
        ("retention-next", "2019-01-01T00:00:00Z"),
        ("retention-exact", "2020-01-01T00:00:00Z"),
    ] {
        let Record::Order(mut order) = super::compliance::order(id, "retention-owner") else {
            unreachable!()
        };
        order.created_at = Some(created.into());
        store.insert(Record::Order(order)).await.unwrap();
    }
    let cutoff = date("2020-01-01T00:00:00Z");
    let now = date("2028-01-01T00:00:00Z");
    let anon =
        RetentionPolicy::for_entity::<OrderCompliance>(cutoff, RetentionAction::Anonymize).unwrap();
    assert!(
        anon.anonymize_fields().is_empty(),
        "required address must not be nulled"
    );
    let options = EnforcementOptions {
        batch_size: 1,
        ..EnforcementOptions::default()
    };
    let scope = [tenant.clone()];
    let dry = retention::enforce(
        store.as_ref(),
        &scope,
        std::slice::from_ref(&anon),
        now,
        &EnforcementOptions {
            dry_run: true,
            ..options.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(dry.anonymized, 2);
    assert_eq!(dry.errors, 0);
    let foreign = retention::enforce(
        store.as_ref(),
        &[TenantContext::new("foreign-retention-tenant").unwrap()],
        std::slice::from_ref(&anon),
        now,
        &options,
    )
    .await
    .unwrap();
    assert_eq!(foreign.errors, 1);
    let (first, second) = tokio::join!(
        retention::enforce(
            store.as_ref(),
            &scope,
            std::slice::from_ref(&anon),
            now,
            &options
        ),
        retention::enforce(
            store.as_ref(),
            &scope,
            std::slice::from_ref(&anon),
            now,
            &options
        ),
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(first.anonymized + second.anonymized, 2);
    assert_eq!(first.errors + second.errors, 0);
    let rows = store
        .find(Query {
            ids: Some(vec!["retention-first".into()]),
            ..Query::all(RecordKind::Order)
        })
        .await
        .unwrap();
    let Record::Order(order) = &rows[0] else {
        unreachable!()
    };
    assert_eq!(order.shipping_address.name, "Compliance Person");
    assert_eq!(
        order.updated_at.as_deref(),
        Some("2028-01-01T00:00:00.000Z")
    );
    store
        .mutate(
            Selector::id(RecordKind::Order, "retention-first"),
            Mutation::TransitionOrder {
                expected: OrderStatus::Pending,
                to: OrderStatus::Paid,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        retention::enforce(
            store.as_ref(),
            &scope,
            std::slice::from_ref(&anon),
            now,
            &options
        )
        .await
        .unwrap()
        .processed,
        0,
        "ordinary order changes must preserve retention markers"
    );
    let delete =
        RetentionPolicy::for_entity::<OrderCompliance>(cutoff, RetentionAction::Delete).unwrap();
    let result = retention::enforce(store.as_ref(), &scope, &[delete], now, &options)
        .await
        .unwrap();
    assert_eq!(result.deleted, 2);
    assert_eq!(result.errors, 0);
    let mut recreated = order.clone();
    recreated.created_at = Some("2018-01-01T00:00:00Z".into());
    store.insert(Record::Order(recreated)).await.unwrap();
    assert_eq!(
        retention::enforce(store.as_ref(), &scope, &[anon], now, &options)
            .await
            .unwrap()
            .anonymized,
        1,
        "deleted order IDs must not retain stale markers"
    );
}
#[tokio::test]
async fn memory_explicit_policy_required_address_markers_replay_and_tenant_binding() {
    let tenant = TenantContext::new("memory-retention").unwrap();
    exercise(
        Arc::new(
            forklaunch_ecommerce_base::store::InMemoryEcommerceStore::for_tenant(tenant.clone()),
        ),
        tenant,
    )
    .await;
}
