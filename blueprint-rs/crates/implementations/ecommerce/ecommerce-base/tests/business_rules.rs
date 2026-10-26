#[path = "support/catalog.rs"]
mod catalog;
#[path = "support/compliance.rs"]
mod compliance;
#[path = "support/events.rs"]
mod events;
#[path = "support/retention.rs"]
mod retention;
#[path = "support/webhook.rs"]
mod webhook;
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    store::{EcommerceStore, Field, InMemoryEcommerceStore, Mutation, RecordKind, Selector},
};
use forklaunch_ecommerce_interface::*;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::sync::Arc;
fn dto<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).unwrap()
}

#[allow(clippy::too_many_lines)]
async fn business_rules<S: EcommerceStore + 'static>(store: Arc<S>) {
    let service = BaseEcommerceService::new(Arc::clone(&store));
    let product=service.create_product(dto(json!({"id":"p1","externalId":"external","handle":"coffee","title":"Coffee Beans","tags":["organic"],"images":[{"src":"https://example.test/bean.png","position":1}]}))).await.unwrap();
    assert_eq!(
        service
            .get_product_by_external_id("external")
            .await
            .unwrap(),
        product
    );
    assert_eq!(
        service.get_product_by_handle("coffee").await.unwrap(),
        product
    );
    assert!(
        service
            .create_product(dto(
                json!({"id":"p2","externalId":"external","handle":"other","title":"Other"})
            ))
            .await
            .is_err()
    );
    assert!(
        service
            .create_product(dto(
                json!({"id":"p2","externalId":"other","handle":"coffee","title":"Other"})
            ))
            .await
            .is_err()
    );
    for title in ["coffee", "B_ans", "%BEANS"] {
        assert_eq!(
            service
                .list_products(Some(dto(json!({"title":title}))))
                .await
                .unwrap()
                .len(),
            1
        );
    }
    assert!(
        service
            .list_products(Some(dto(json!({"ids":["missing"]}))))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service
            .list_products(Some(dto(json!({"ids":[]}))))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        service
            .update_product(dto(json!({"id":"p1","title":"Tea"})))
            .await
            .unwrap()
            .tags,
        product.tags
    );
    let variant=service.create_variant(dto(json!({"id":"v1","externalId":"ext-v","productId":"p1","title":"Large","priceCents":1999,"optionValues":{"Size":"Large"}}))).await.unwrap();
    assert!(variant.requires_shipping);
    assert_eq!(
        service.list_variants_by_product("p1").await.unwrap(),
        vec![variant]
    );
    service
        .create_inventory(dto(json!({"variantId":"v1","stock":10})))
        .await
        .unwrap();
    let mut jobs = Vec::new();
    for _ in 0..20 {
        let s = service.clone();
        jobs.push(tokio::spawn(async move {
            s.adjust_stock(AdjustStockDto {
                variant_id: "v1".into(),
                delta: -1,
            })
            .await
        }));
    }
    let mut successes = 0;
    for job in jobs {
        if job.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 10);
    assert_eq!(service.get_inventory("v1").await.unwrap().stock, 0);
    assert!(
        !service
            .check_stock(StockCheckDto {
                variant_id: "v1".into(),
                requested: 1
            })
            .await
            .unwrap()
            .available
    );
    assert_eq!(
        service
            .adjust_stock(AdjustStockDto {
                variant_id: "v1".into(),
                delta: 5
            })
            .await
            .unwrap()
            .stock,
        5
    );
    service
        .create_cart(dto(json!({"id":"cart"})))
        .await
        .unwrap();
    let mut jobs = Vec::new();
    for _ in 0..30 {
        let s = service.clone();
        jobs.push(tokio::spawn(async move {
            s.add_item(AddCartItemDto {
                cart_id: "cart".into(),
                variant_id: "v1".into(),
                quantity: 1,
            })
            .await
            .unwrap();
        }));
    }
    for job in jobs {
        job.await.unwrap();
    }
    assert_eq!(
        service.get_cart("cart").await.unwrap().items[0].quantity,
        30
    );
    for quantity in [0, -1] {
        assert!(
            service
                .add_item(AddCartItemDto {
                    cart_id: "cart".into(),
                    variant_id: "v1".into(),
                    quantity
                })
                .await
                .is_err()
        );
    }
    assert_eq!(
        service.get_cart("cart").await.unwrap().items[0].quantity,
        30
    );
    service
        .remove_item(RemoveCartItemDto {
            cart_id: "cart".into(),
            variant_id: "v1".into(),
        })
        .await
        .unwrap();
    service.clear_cart("cart").await.unwrap();
    assert!(service.get_cart("cart").await.unwrap().items.is_empty());
    let order_request = |id: &str| {
        dto(
            json!({"id":id,"items":[{"variantId":"v1","quantity":2,"unitPriceCents":1999}],"shippingAddress":{"name":"Private Person","line1":"123 Secret Street","city":"Oakland","state":"CA","postalCode":"94601","country":"US"},"subtotalCents":3998,"discountCents":0,"taxCents":320,"taxBreakdown":[{"jurisdiction":"CA","taxCents":320}],"shippingCents":500,"giftCardCents":0,"totalCents":4818}),
        )
    };
    service
        .create_order_for_cart(order_request("order"), Some("cart".into()))
        .await
        .unwrap();
    assert!(
        service
            .create_order_for_cart(order_request("duplicate"), Some("cart".into()))
            .await
            .is_err()
    );
    let mut jobs = Vec::new();
    for _ in 0..12 {
        let s = service.clone();
        jobs.push(tokio::spawn(async move {
            s.transition_order(TransitionOrderDto {
                id: "order".into(),
                to: OrderStatus::Paid,
            })
            .await
        }));
    }
    let mut successes = 0;
    for job in jobs {
        if job.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 1);
    service
        .create_order_for_cart(order_request("order2"), Some("cart".into()))
        .await
        .unwrap();
    for to in [
        OrderStatus::Fulfilled,
        OrderStatus::Shipped,
        OrderStatus::Delivered,
    ] {
        service
            .transition_order(TransitionOrderDto {
                id: "order".into(),
                to,
            })
            .await
            .unwrap();
    }
    assert!(
        service
            .transition_order(TransitionOrderDto {
                id: "order".into(),
                to: OrderStatus::Cancelled
            })
            .await
            .is_err()
    );
    assert_eq!(service.list_orders(None).await.unwrap().len(), 2);
    service
        .create_payment_with_reference(
            dto(json!({"id":"payment","orderId":"order","amountCents":4818,"currency":"usd"})),
            Some("provider-ref".into()),
        )
        .await
        .unwrap();
    assert!(!service.claim_for_confirmation("missing").await.unwrap());
    assert!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "payment".into()
            })
            .await
            .is_err()
    );
    let mut jobs = Vec::new();
    for _ in 0..12 {
        let s = service.clone();
        jobs.push(tokio::spawn(async move {
            s.claim_for_confirmation("provider-ref").await.unwrap()
        }));
    }
    let mut successes = 0;
    for job in jobs {
        if job.await.unwrap() {
            successes += 1;
        }
    }
    assert_eq!(successes, 1);
    service
        .release_confirmation_claim("provider-ref")
        .await
        .unwrap();
    assert_eq!(
        service.get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    service
        .fail_payment(FailPaymentDto {
            provider_ref: "provider-ref".into(),
        })
        .await
        .unwrap();
    assert!(
        !service
            .claim_for_confirmation("provider-ref")
            .await
            .unwrap()
    );
    let confirmed = service
        .confirm_payment(ConfirmPaymentDto {
            provider_ref: "provider-ref".into(),
        })
        .await
        .unwrap();
    assert_eq!(confirmed.status, PaymentStatus::Succeeded);
    assert_eq!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "provider-ref".into()
            })
            .await
            .unwrap(),
        confirmed
    );
    assert_eq!(
        service
            .fail_payment(FailPaymentDto {
                provider_ref: "provider-ref".into()
            })
            .await
            .unwrap(),
        confirmed
    );
    assert!(
        store
            .mutate(
                Selector::new(RecordKind::Variant, Field::ProductId, "p1"),
                Mutation::ClearCart
            )
            .await
            .is_err()
    );
    service.delete_product("p1").await.unwrap();
    service.delete_product("p1").await.unwrap();
    assert!(service.get_product("p1").await.is_err());
    assert_eq!(
        service.get_variant("v1").await.unwrap().product_id,
        "p1",
        "base deletion does not cascade"
    );
    catalog::catalog_rules(Arc::clone(&store)).await;
    events::event_rules(store).await;
}
#[tokio::test]
async fn memory_business_rules_and_concurrency() {
    business_rules(Arc::new(InMemoryEcommerceStore::default())).await;
    webhook::ledger_rules(Arc::new(
        forklaunch_ecommerce_base::webhook::InMemoryWebhookEventStore::default(),
    ))
    .await;
}

#[test]
fn exhaustive_order_transitions_and_wire_nulls() {
    let statuses = [
        OrderStatus::Pending,
        OrderStatus::Paid,
        OrderStatus::Fulfilled,
        OrderStatus::Shipped,
        OrderStatus::Delivered,
        OrderStatus::Cancelled,
    ];
    let allowed = [
        (OrderStatus::Pending, OrderStatus::Paid),
        (OrderStatus::Pending, OrderStatus::Cancelled),
        (OrderStatus::Paid, OrderStatus::Fulfilled),
        (OrderStatus::Paid, OrderStatus::Cancelled),
        (OrderStatus::Fulfilled, OrderStatus::Shipped),
        (OrderStatus::Fulfilled, OrderStatus::Cancelled),
        (OrderStatus::Shipped, OrderStatus::Delivered),
    ];
    for from in statuses {
        for to in statuses {
            assert_eq!(
                from.can_transition_to(to),
                allowed.contains(&(from, to)),
                "{from:?} -> {to:?}"
            );
        }
    }
    assert!(serde_json::from_value::<CreateCartDto>(json!({"customerId":null})).is_err());
    assert!(
        serde_json::from_value::<AddCartItemDto>(
            json!({"cartId":"c","variantId":"v","quantity":1.5})
        )
        .is_err()
    );
}

#[cfg(feature = "postgres")]
#[allow(clippy::too_many_lines)]
#[tokio::test]
#[ignore = "requires an isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
async fn postgres_business_rules_tenants_encryption_and_audit() {
    use forklaunch_core::domain::AuditContext;
    use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
    use forklaunch_ecommerce_base::postgres::PostgresEcommerceStore;
    use forklaunch_ecommerce_interface::webhook::{WebhookEventOutcome, WebhookEventStore};
    use sqlx::postgres::PgPoolOptions;
    let url = std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap();
    let owner = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    forklaunch_worker_database::postgres::PostgresWorkerQueue::migrate(&owner)
        .await
        .unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("ecom_{suffix}");
    let password = uuid::Uuid::new_v4().to_string();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN PASSWORD '{password}'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.order_retention,fl_ecommerce.records TO {role}; GRANT INSERT ON fl_ecommerce.pii_read_audit TO {role}; GRANT SELECT,INSERT ON fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.payment_cancellation_audit TO {role}; GRANT SELECT,INSERT ON fl_ecommerce.inventory_event_receipts TO {role}; GRANT SELECT,INSERT,UPDATE ON fl_ecommerce.webhook_events TO {role}; GRANT USAGE ON SCHEMA fl_worker TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_worker.jobs TO {role}; GRANT USAGE ON ALL SEQUENCES IN SCHEMA fl_worker TO {role};"))).execute(&owner).await.unwrap();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let runtime = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options.username(&role).password(&password))
        .await
        .unwrap();
    let tenant_a = TenantContext::new(format!("ecom-a-{suffix}")).unwrap();
    let tenant_b = TenantContext::new(format!("ecom-b-{suffix}")).unwrap();
    let keys = Arc::new(FieldEncryptor::new("ecommerce-test-old-secret-only").unwrap());
    let audit = AuditContext::actor("verified-ecommerce-actor", format!("fixture-{suffix}"));
    assert_eq!(
        PostgresEcommerceStore::new(
            owner.clone(),
            tenant_a.clone(),
            Arc::clone(&keys),
            audit.clone()
        )
        .await
        .err()
        .unwrap()
        .code,
        "ecommerce_unsafe_database_role"
    );
    let a = Arc::new(
        PostgresEcommerceStore::new(
            runtime.clone(),
            tenant_a.clone(),
            Arc::clone(&keys),
            audit.clone(),
        )
        .await
        .unwrap(),
    );
    let b = Arc::new(
        PostgresEcommerceStore::new(
            runtime.clone(),
            tenant_b.clone(),
            Arc::clone(&keys),
            audit.clone(),
        )
        .await
        .unwrap(),
    );
    // Operation locks are tenant/namespace scoped and release with transaction drop.
    let held = a.lock_operation("fixture", "shared-cart").await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            a.lock_operation("fixture", "shared-cart")
        )
        .await
        .is_err()
    );
    let isolated = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        b.lock_operation("fixture", "shared-cart"),
    )
    .await
    .unwrap()
    .unwrap();
    let namespace = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        a.lock_operation("other-fixture", "shared-cart"),
    )
    .await
    .unwrap()
    .unwrap();
    drop(isolated);
    drop(namespace);
    drop(held);
    let released = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        a.lock_operation("fixture", "shared-cart"),
    )
    .await
    .unwrap()
    .unwrap();
    drop(released);
    webhook::ledger_rules(Arc::clone(&a)).await;
    let event = forklaunch_ecommerce_interface::webhook::WebhookEvent {
        provider: "stripe".to_owned(),
        provider_event_id: "ledger-replay".to_owned(),
        event_type: "payment_intent.succeeded".to_owned(),
    };
    assert_eq!(
        b.begin_processing(event.clone()).await.unwrap(),
        WebhookEventOutcome::New
    );
    let reopened = PostgresEcommerceStore::new(
        runtime.clone(),
        tenant_a.clone(),
        Arc::clone(&keys),
        audit.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        reopened.begin_processing(event).await.unwrap(),
        WebhookEventOutcome::AlreadyProcessed
    );
    let (visible,): (i64,) = sqlx::query_as("SELECT count(*) FROM fl_ecommerce.webhook_events")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(visible, 0);
    business_rules(Arc::clone(&a)).await;
    compliance::exercise(Arc::clone(&a)).await;
    retention::exercise(Arc::clone(&a), tenant_a.clone()).await;
    let compliance_a = forklaunch_ecommerce_base::compliance::EcommerceComplianceService::new(
        Arc::clone(&a),
        forklaunch_ecommerce_base::compliance::OrderSubjectField::CustomerId,
    );
    let compliance_b = forklaunch_ecommerce_base::compliance::EcommerceComplianceService::new(
        Arc::clone(&b),
        forklaunch_ecommerce_base::compliance::OrderSubjectField::CustomerId,
    );
    assert!(
        compliance_b
            .export("foreign-owner")
            .await
            .unwrap()
            .entities
            .is_empty()
    );
    assert_eq!(
        compliance_b
            .erase("foreign-owner")
            .await
            .unwrap()
            .records_deleted,
        0
    );
    let service = BaseEcommerceService::new(Arc::clone(&a));
    let other = BaseEcommerceService::new(Arc::clone(&b));
    assert!(other.get_order("order").await.is_err());
    assert!(other.get_variant("v1").await.is_err());
    assert_eq!(
        other
            .list_catalog(forklaunch_ecommerce_interface::catalog::CatalogQuery::default())
            .await
            .unwrap()
            .total,
        0
    );
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM fl_ecommerce.records")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let (data,): (sqlx::types::Json<serde_json::Value>,) = sqlx::query_as(
        "SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND id='order'",
    )
    .bind(tenant_a.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    let encrypted = data.0["shippingAddress"].as_str().unwrap();
    assert!(encrypted.starts_with("v4:"));
    assert!(!data.0.to_string().contains("Private Person"));
    assert!(!data.0.to_string().contains("Secret Street"));
    assert_eq!(
        data.0["totalCents"], 4818,
        "non-PII accounting fields remain usable"
    );
    assert!(keys.decrypt(encrypted, tenant_b.id()).is_err());
    let (count,):(i64,)=sqlx::query_as("SELECT count(*) FROM fl_ecommerce.pii_read_audit WHERE tenant_id=$1 AND actor_id='verified-ecommerce-actor'").bind(tenant_a.id()).fetch_one(&owner).await.unwrap();
    assert!(count > 0);
    let mut record = service.get_order("order2").await.unwrap();
    record.id = "order3".into();
    record.cart_id = None;
    a.insert(forklaunch_ecommerce_base::store::Record::Order(record))
        .await
        .unwrap();
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{shippingAddress}',$2) WHERE tenant_id=$1 AND kind='order' AND id='order3'").bind(tenant_a.id()).bind(sqlx::types::Json(serde_json::Value::String(encrypted.into()))).execute(&owner).await.unwrap();
    assert!(
        service.get_order("order3").await.is_err(),
        "same-tenant encrypted address swaps fail closed"
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.pii_read_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(service.get_order("order2").await.is_err());
    let failed_export = compliance_a.export("foreign-owner").await.unwrap();
    assert!(failed_export.entities.is_empty());
    assert_eq!(failed_export.failures.len(), 1);
    let failed_erase = compliance_a.erase("foreign-owner").await.unwrap();
    assert_eq!(failed_erase.records_deleted, 0);
    assert_eq!(failed_erase.failures.len(), 1);
    let cutoff: std::time::SystemTime =
        chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .unwrap()
            .into();
    let policy = forklaunch_core::persistence::retention::RetentionPolicy::for_entity::<
        forklaunch_ecommerce_base::retention::OrderCompliance,
    >(
        cutoff,
        forklaunch_core::persistence::retention::RetentionAction::Delete,
    )
    .unwrap();
    assert!(
        forklaunch_core::persistence::retention::RetentionStore::apply_batch(
            a.as_ref(),
            forklaunch_core::persistence::retention::RetentionRequest {
                tenant: &tenant_a,
                policy: &policy,
                now: std::time::SystemTime::now(),
                batch_size: 1,
            }
        )
        .await
        .is_err(),
        "retention deletion must roll back on audit failure"
    );
    assert!(
        service
            .transition_order(TransitionOrderDto {
                id: "order2".into(),
                to: OrderStatus::Paid
            })
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.pii_read_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(
        service.get_order("order2").await.unwrap().status,
        OrderStatus::Pending,
        "audit failures roll mutations back"
    );
    assert_eq!(
        compliance_a.export("foreign-owner").await.unwrap().entities["Order"].len(),
        1,
        "audit failure must roll back erasure"
    );
    assert!(service.get_order("retention-first").await.is_ok());
    let markers: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_ecommerce.order_retention")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(markers, 0, "retention markers require a bound tenant");
    let rotated = Arc::new(
        FieldEncryptor::new("ecommerce-test-new-secret-only")
            .unwrap()
            .with_previous_keys(&["ecommerce-test-old-secret-only".into()])
            .unwrap(),
    );
    let reopened = BaseEcommerceService::new(Arc::new(
        PostgresEcommerceStore::new(runtime.clone(), tenant_a.clone(), rotated, audit.clone())
            .await
            .unwrap(),
    ));
    assert_eq!(
        reopened
            .get_order("order")
            .await
            .unwrap()
            .shipping_address
            .name,
        "Private Person"
    );
    let wrong = BaseEcommerceService::new(Arc::new(
        PostgresEcommerceStore::new(
            runtime.clone(),
            tenant_a.clone(),
            Arc::new(FieldEncryptor::new("unrelated-secret-only").unwrap()),
            audit,
        )
        .await
        .unwrap(),
    ));
    assert!(wrong.get_order("order").await.is_err());
    events::event_rules(Arc::clone(&b)).await;
    #[cfg(feature = "worker")]
    events::lost_ack_redelivery(&owner, &runtime, tenant_a.clone(), Arc::clone(&a)).await;
    #[cfg(feature = "worker")]
    events::atomic_transitions(&owner, &runtime, tenant_a.clone(), Arc::clone(&a), &role).await;
    sqlx::query("DELETE FROM fl_worker.jobs WHERE tenant_id=ANY($1)")
        .bind(vec![tenant_a.id(), tenant_b.id()])
        .execute(&owner)
        .await
        .unwrap();
    let (receipts,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM fl_ecommerce.inventory_event_receipts")
            .fetch_one(&runtime)
            .await
            .unwrap();
    assert_eq!(
        receipts, 0,
        "receipts enforce tenant RLS outside a bound transaction"
    );
    for table in [
        "records",
        "pii_read_audit",
        "inventory_event_receipts",
        "webhook_events",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_ecommerce.{table} WHERE tenant_id=ANY($1)"
        )))
        .bind(vec![tenant_a.id(), tenant_b.id()])
        .execute(&owner)
        .await
        .unwrap();
    }
    runtime.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

#[tokio::test]
async fn cache_failures_timeouts_and_mismatched_ids_fall_back_to_storage() {
    use forklaunch_core::domain::BlueprintError;
    use forklaunch_ecommerce_base::{CacheFuture, CartCache};
    struct BrokenCache {
        hang: bool,
    }
    impl CartCache for BrokenCache {
        fn read(&self, _: &str) -> CacheFuture<'_, Option<CartDto>> {
            Box::pin(async move {
                if self.hang {
                    std::future::pending::<()>().await;
                }
                Err(BlueprintError::new("cache_down", "cache unavailable"))
            })
        }
        fn write(&self, _: CartDto) -> CacheFuture<'_, ()> {
            Box::pin(async move {
                if self.hang {
                    std::future::pending::<()>().await;
                }
                Err(BlueprintError::new("cache_down", "cache unavailable"))
            })
        }
    }
    struct WrongCache(CartDto);
    impl CartCache for WrongCache {
        fn read(&self, _: &str) -> CacheFuture<'_, Option<CartDto>> {
            Box::pin(async { Ok(Some(self.0.clone())) })
        }
        fn write(&self, _: CartDto) -> CacheFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }
    for hang in [false, true] {
        let store = Arc::new(InMemoryEcommerceStore::default());
        let cached = BaseEcommerceService::new(Arc::clone(&store))
            .with_cart_cache(Arc::new(BrokenCache { hang }));
        let persistent = BaseEcommerceService::new(store);
        tokio::time::timeout(std::time::Duration::from_secs(4), async {
            cached.create_cart(dto(json!({"id":"cart"}))).await.unwrap();
            cached
                .add_item(AddCartItemDto {
                    cart_id: "cart".into(),
                    variant_id: "v".into(),
                    quantity: 1,
                })
                .await
                .unwrap();
            assert_eq!(
                cached.get_cart("cart").await.unwrap().items,
                persistent.get_cart("cart").await.unwrap().items
            );
        })
        .await
        .unwrap();
    }
    let service = BaseEcommerceService::new(Arc::new(InMemoryEcommerceStore::default()));
    let wrong = service
        .create_cart(dto(json!({"id":"wrong"})))
        .await
        .unwrap();
    service
        .create_cart(dto(json!({"id":"right"})))
        .await
        .unwrap();
    assert_eq!(
        service
            .with_cart_cache(Arc::new(WrongCache(wrong)))
            .get_cart("right")
            .await
            .unwrap()
            .id,
        "right"
    );
}

#[tokio::test]
async fn non_finite_inputs_and_overflow_do_not_change_records() {
    use forklaunch_ecommerce_base::store::Record;
    let store = Arc::new(InMemoryEcommerceStore::default());
    let service = BaseEcommerceService::new(Arc::clone(&store));
    let mut request: CreateProductDto =
        dto(json!({"id":"p","externalId":"ext","handle":"p","title":"P"}));
    request.images = Some(vec![ProductImage {
        src: "image".into(),
        position: f64::NAN,
    }]);
    assert!(service.create_product(request).await.is_err());
    assert!(service.get_product("p").await.is_err());
    service
        .create_inventory(dto(json!({"variantId":"v","stock":i64::MAX})))
        .await
        .unwrap();
    assert!(
        service
            .adjust_stock(AdjustStockDto {
                variant_id: "v".into(),
                delta: 1
            })
            .await
            .is_err()
    );
    assert_eq!(service.get_inventory("v").await.unwrap().stock, i64::MAX);
    service.create_cart(dto(json!({"id":"c"}))).await.unwrap();
    service
        .add_item(AddCartItemDto {
            cart_id: "c".into(),
            variant_id: "v".into(),
            quantity: i64::MAX,
        })
        .await
        .unwrap();
    assert!(
        service
            .add_item(AddCartItemDto {
                cart_id: "c".into(),
                variant_id: "v".into(),
                quantity: 1
            })
            .await
            .is_err()
    );
    let cart = service.get_cart("c").await.unwrap();
    assert_eq!(cart.items[0].quantity, i64::MAX);
    assert!(store.insert(Record::Cart(cart)).await.is_err());
}
