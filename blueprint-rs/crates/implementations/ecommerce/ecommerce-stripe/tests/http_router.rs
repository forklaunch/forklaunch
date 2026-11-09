#![cfg(feature = "http")]
use axum::{
    Json, Router,
    extract::Form,
    http::HeaderMap,
    routing::{get, post},
};
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::{
    ServiceInfo,
    persistence::{TenantContext, encryption::FieldEncryptor},
};
use forklaunch_ecommerce_base::{BaseEcommerceService, postgres::PostgresEcommerceStore};
use forklaunch_ecommerce_interface::{InventoryService, catalog::CatalogQuery};
use forklaunch_ecommerce_paypal::PaypalClient;
use forklaunch_ecommerce_stripe::StripeClient;
use forklaunch_ecommerce_stripe::http::{EcommerceBackend, EcommerceHttp};
use forklaunch_worker_database::{postgres::PostgresWorkerQueue, runner::JobHandler};
use forklaunch_worker_interface::{AsyncWorkerQueue, RetryPolicy};
use jsonwebtoken::{Algorithm, Validation};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{collections::HashMap, sync::Arc, time::Duration};
const SECRET: &[u8] = b"ecommerce-fixture-secret";
fn sign(method: &str, path: &str, body: Option<&str>) -> String {
    generate_hmac_auth_header(
        SECRET,
        "default",
        method,
        path,
        body.map_or(SignedBody::Absent, SignedBody::Text),
        &forklaunch_ecommerce_base::timestamp(),
        &uuid::Uuid::new_v4().to_string(),
    )
    .unwrap()
}
async fn call(
    client: &Client,
    base: &str,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> reqwest::Response {
    let body = body.map(|body| body.to_string());
    let mut request = client
        .request(method.clone(), format!("{base}{path}"))
        .header(
            "authorization",
            sign(
                method.as_str(),
                path.split('?').next().unwrap(),
                body.as_deref(),
            ),
        );
    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .body(body);
    }
    request.send().await.unwrap()
}
async fn value(response: reqwest::Response) -> Value {
    assert!(
        response.status().is_success(),
        "HTTP {}: {}",
        response.status(),
        response.text().await.unwrap()
    );
    response.json().await.unwrap()
}
async fn rpc(client: &Client, base: &str, method: &str, params: Value) -> Value {
    let response = client
        .post(format!("{base}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    response.json().await.unwrap()
}
async fn serve(
    router: Router,
) -> (
    String,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    (base, stop, task)
}

#[tokio::test]
#[ignore = "requires an isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
#[allow(clippy::too_many_lines)]
async fn postgres_ecommerce_http_mcp_provider_and_inventory_worker() {
    let owner = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    PostgresWorkerQueue::migrate(&owner).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("ecom_http_{suffix}");
    let password = uuid::Uuid::new_v4().to_string();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("SELECT pg_advisory_xact_lock(7428301); CREATE ROLE {role} LOGIN PASSWORD '{password}'; GRANT USAGE ON SCHEMA fl_ecommerce,fl_worker TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_worker.jobs TO {role}; GRANT SELECT,INSERT ON fl_ecommerce.inventory_event_receipts TO {role}; GRANT SELECT,INSERT,UPDATE ON fl_ecommerce.webhook_events TO {role}; GRANT INSERT ON fl_ecommerce.pii_read_audit TO {role}; GRANT SELECT,INSERT ON fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.payment_cancellation_audit TO {role}; GRANT USAGE ON ALL SEQUENCES IN SCHEMA fl_worker TO {role};"))).execute(&owner).await.unwrap();
    let options: sqlx::postgres::PgConnectOptions = std::env::var("FORKLAUNCH_TEST_POSTGRES_URL")
        .unwrap()
        .parse()
        .unwrap();
    let runtime = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options.username(&role).password(&password))
        .await
        .unwrap();
    let tenant = TenantContext::new(format!("ecom-http-{suffix}")).unwrap();
    let other = TenantContext::new(format!("ecom-other-{suffix}")).unwrap();
    let keys = Arc::new(FieldEncryptor::new("ecommerce-http-test-key").unwrap());
    let store = PostgresEcommerceStore::new(
        runtime.clone(),
        tenant.clone(),
        Arc::clone(&keys),
        AuditContext::system("startup"),
    )
    .await
    .unwrap();
    let other_store = PostgresEcommerceStore::new(
        runtime.clone(),
        other.clone(),
        Arc::clone(&keys),
        AuditContext::system("startup"),
    )
    .await
    .unwrap();
    let queue = PostgresWorkerQueue::new(runtime.clone(), tenant.clone(), RetryPolicy::new(3, 0))
        .await
        .unwrap();
    let cancel_resume = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let resume_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cancel_during_get = cancel_resume.clone();
    let observed_reads = resume_reads.clone();
    let resume_owner = owner.clone();
    let resume_tenant = tenant.id().to_owned();
    let provider = Router::new()
        .route(
            "/v1/payment_intents",
            post(
                |headers: HeaderMap, Form(fields): Form<HashMap<String, String>>| async move {
                    assert_eq!(headers["authorization"], "Bearer owned-stripe-secret");
                    assert_eq!(fields["amount"], "200");
                    assert_eq!(fields["metadata[orderId]"], "order");
                    Json(json!({"id":"pi_owned","client_secret":"ephemeral-client-secret"}))
                },
            ),
        )
        .route("/v1/payment_intents/pi_resume", get(move || {
            let cancel = cancel_during_get.clone();
            let calls = observed_reads.clone();
            let pool = resume_owner.clone();
            let tenant = resume_tenant.clone();
            async move {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"cancelled\"') WHERE tenant_id=$1 AND kind='order' AND id='resume-order'").bind(tenant).execute(&pool).await.unwrap();
                }
                Json(json!({"id":"pi_resume","client_secret":"guarded-resume-secret"}))
            }
        }))
        .route(
            "/v1/oauth2/token",
            post(
                |headers: HeaderMap, Form(fields): Form<HashMap<String, String>>| async move {
                    assert!(
                        headers["authorization"]
                            .to_str()
                            .unwrap()
                            .starts_with("Basic ")
                    );
                    assert_eq!(fields["grant_type"], "client_credentials");
                    Json(json!({"access_token":"owned-token"}))
                },
            ),
        )
        .route(
            "/v2/checkout/orders",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(headers["authorization"], "Bearer owned-token");
                assert_eq!(body["purchase_units"][0]["amount"]["value"], "2.00");
                Json(json!({"id":"paypal-owned","status":"CREATED"}))
            }),
        );
    let (provider_base, provider_stop, provider_task) = serve(provider).await;
    let transport =
        || ProviderHttpClient::new(&provider_base, Duration::from_secs(5), 1024 * 1024).unwrap();
    let backend = EcommerceBackend::new(
        store.clone(),
        queue.clone(),
        StripeClient::with_transport("owned-stripe-secret".into(), transport()).unwrap(),
    )
    .with_paypal(
        PaypalClient::with_transport("owned-client".into(), "owned-secret".into(), transport())
            .unwrap(),
    );
    let auth = HmacJwksAuth::new(
        SECRET.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    let router = EcommerceHttp::new(backend, auth, ServiceInfo::new("ecommerce", "1.0.0")).router();
    assert_eq!(router.registry().routes().len(), 33);
    let (base, stop, task) = serve(router.into_router_with_default_mcp().unwrap()).await;
    let client = Client::new();
    // Public callbacks use provider authentication and must not require an internal session.
    for provider in ["stripe", "paypal"] {
        let response = client
            .post(format!("{base}/webhook/{provider}"))
            .header("stripe-signature", "invalid")
            .header("paypal-transmission-id", "fixture")
            .header("paypal-transmission-time", "fixture")
            .header("paypal-transmission-sig", "invalid")
            .header("paypal-cert-url", "https://example.invalid/cert")
            .header("paypal-auth-algo", "SHA256withRSA")
            .json(&json!({"id":"unverified", "event_type":"UNKNOWN", "resource":{}}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(
        client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{base}/product"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let product = value(
        call(
            &client,
            &base,
            Method::POST,
            "/product",
            Some(json!({"id":"product","externalId":"external","handle":"beans","title":"Beans"})),
        )
        .await,
    )
    .await;
    assert_eq!(product["id"], "product");
    value(
        call(
            &client,
            &base,
            Method::PUT,
            "/product",
            Some(json!({"id":"product","title":"Coffee Beans"})),
        )
        .await,
    )
    .await;
    assert_eq!(
        value(call(&client, &base, Method::GET, "/product/handle/beans", None).await).await["title"],
        "Coffee Beans"
    );
    assert_eq!(
        value(call(&client, &base, Method::GET, "/product/product", None).await).await["id"],
        "product"
    );
    value(call(&client,&base,Method::POST,"/variant",Some(json!({"id":"variant","productId":"product","externalId":"ext-variant","title":"Large","priceCents":100,"optionValues":{"Size":"Large"}}))).await).await;
    value(
        call(
            &client,
            &base,
            Method::PUT,
            "/variant",
            Some(json!({"id":"variant","priceCents":150})),
        )
        .await,
    )
    .await;
    assert_eq!(
        value(
            call(
                &client,
                &base,
                Method::GET,
                "/variant/product/product",
                None
            )
            .await
        )
        .await
        .as_array()
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        value(call(&client, &base, Method::GET, "/variant/variant", None).await).await["priceCents"],
        150
    );
    assert_eq!(
        value(call(&client, &base, Method::GET, "/variant", None).await)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for invalid in [json!({"title":null}), json!([])] {
        assert_eq!(
            call(&client, &base, Method::POST, "/product", Some(invalid))
                .await
                .status(),
            400
        );
    }
    assert_eq!(
        call(
            &client,
            &base,
            Method::POST,
            "/payment",
            Some(
                json!({"orderId":"order","amountCents":200,"currency":"usd","provider":"unknown"})
            )
        )
        .await
        .status(),
        400
    );
    let service = BaseEcommerceService::new(Arc::new(store.clone()));
    service
        .create_inventory(serde_json::from_value(json!({"variantId":"variant","stock":5})).unwrap())
        .await
        .unwrap();
    value(
        call(
            &client,
            &base,
            Method::PUT,
            "/inventory/adjust",
            Some(json!({"variantId":"variant","delta":3})),
        )
        .await,
    )
    .await;
    assert_eq!(
        value(call(&client, &base, Method::GET, "/inventory/variant", None).await).await["stock"],
        8
    );
    assert_eq!(
        value(
            call(
                &client,
                &base,
                Method::POST,
                "/inventory/check",
                Some(json!({"variantId":"variant","requested":10}))
            )
            .await
        )
        .await["available"],
        false
    );
    assert_eq!(
        value(call(&client, &base, Method::GET, "/product?inStock=false", None).await).await,
        json!([])
    );
    assert_eq!(
        value(
            call(
                &client,
                &base,
                Method::GET,
                "/product?inStock=true&maxPriceCents=150",
                None
            )
            .await
        )
        .await
        .as_array()
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        call(&client, &base, Method::GET, "/product?inStock=yes", None)
            .await
            .status(),
        400
    );
    let ids = form_urlencoded::Serializer::new(String::new())
        .append_pair("ids", r#"["product"]"#)
        .finish();
    let catalog = value(
        call(
            &client,
            &base,
            Method::GET,
            &format!("/product/catalog?{ids}&limit=1"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(catalog["total"], 1);
    assert_eq!(catalog["products"][0]["variants"][0]["stock"], 8);
    assert!(
        BaseEcommerceService::new(Arc::new(other_store))
            .list_catalog(CatalogQuery::default())
            .await
            .unwrap()
            .products
            .is_empty()
    );
    value(
        call(
            &client,
            &base,
            Method::POST,
            "/cart",
            Some(json!({"id":"cart"})),
        )
        .await,
    )
    .await;
    assert_eq!(
        call(
            &client,
            &base,
            Method::POST,
            "/cart/items",
            Some(json!({"cartId":"cart","variantId":"variant","quantity":0}))
        )
        .await
        .status(),
        400
    );
    for quantity in [json!(-1), json!(1.5)] {
        assert_eq!(
            call(
                &client,
                &base,
                Method::POST,
                "/cart/items",
                Some(json!({"cartId":"cart","variantId":"variant","quantity":quantity}))
            )
            .await
            .status(),
            400
        );
    }
    value(
        call(
            &client,
            &base,
            Method::POST,
            "/cart/items",
            Some(json!({"cartId":"cart","variantId":"variant","quantity":2})),
        )
        .await,
    )
    .await;
    assert_eq!(
        value(call(&client, &base, Method::GET, "/cart/cart", None).await).await["items"][0]["quantity"],
        2
    );
    value(
        call(
            &client,
            &base,
            Method::DELETE,
            "/cart/cart/items/variant",
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        call(&client, &base, Method::DELETE, "/cart/cart", None)
            .await
            .text()
            .await
            .unwrap(),
        "Cleared cart"
    );
    let order = json!({"id":"order","items":[{"variantId":"variant","quantity":2,"unitPriceCents":100}],"shippingAddress":{"name":"Buyer","line1":"Private Street","city":"Oakland","state":"CA","postalCode":"94601","country":"US"},"subtotalCents":200,"discountCents":0,"taxCents":0,"taxBreakdown":[],"shippingCents":0,"giftCardCents":0,"totalCents":200});
    value(call(&client, &base, Method::POST, "/order", Some(order)).await).await;
    assert_eq!(
        value(call(&client, &base, Method::GET, "/order/order", None).await).await["shippingAddress"]
            ["name"],
        "Buyer"
    );
    assert_eq!(
        value(call(&client, &base, Method::GET, "/order", None).await)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    value(
        call(
            &client,
            &base,
            Method::PUT,
            "/order/order/transition",
            Some(json!({"to":"paid"})),
        )
        .await,
    )
    .await;
    assert_eq!(queue.stats("order-events").await.unwrap().queued, 1);
    let lease = queue
        .claim("order-events", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    forklaunch_ecommerce_base::events::OrderInventoryHandler::new(
        Arc::new(store.clone()),
        "order-events",
    )
    .process(lease.job.clone())
    .await
    .unwrap();
    assert!(queue.ack(&lease).await.unwrap());
    assert_eq!(service.get_inventory("variant").await.unwrap().stock, 6);
    assert_eq!(
        call(
            &client,
            &base,
            Method::PUT,
            "/order/order/transition",
            Some(json!({"to":"paid"}))
        )
        .await
        .status(),
        400
    );
    let payment = value(
        call(
            &client,
            &base,
            Method::POST,
            "/payment",
            Some(json!({"orderId":"order","amountCents":200,"currency":"usd"})),
        )
        .await,
    )
    .await;
    assert_eq!(payment["clientSecret"], "ephemeral-client-secret");
    let stored = value(
        call(
            &client,
            &base,
            Method::GET,
            &format!("/payment/{}", payment["id"].as_str().unwrap()),
            None,
        )
        .await,
    )
    .await;
    assert!(stored.get("clientSecret").is_none());
    assert_eq!(stored["providerRef"], "pi_owned");
    // Actual generated-preset checkout dispatch uses the guarded resume path.
    value(
        call(
            &client,
            &base,
            Method::POST,
            "/cart",
            Some(json!({"id":"resume-cart"})),
        )
        .await,
    )
    .await;
    let resume_address = json!({"name":"Buyer","line1":"Private Street","city":"Oakland","state":"CA","postalCode":"94601","country":"US"});
    value(call(&client, &base, Method::POST, "/order", Some(json!({"id":"resume-order","cartId":"resume-cart","items":[],"shippingAddress":resume_address,"subtotalCents":200,"discountCents":0,"taxCents":0,"taxBreakdown":[],"shippingCents":0,"giftCardCents":0,"totalCents":200}))).await).await;
    // cartId is checkout-owned metadata, not a generic create-order input.
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{cartId}','\"resume-cart\"') WHERE tenant_id=$1 AND kind='order' AND id='resume-order'").bind(tenant.id()).execute(&owner).await.unwrap();
    service
        .create_payment_with_reference(
            forklaunch_ecommerce_interface::CreatePaymentDto {
                id: Some("resume-payment".into()),
                order_id: "resume-order".into(),
                amount_cents: 200,
                currency: "usd".into(),
            },
            Some("pi_resume".into()),
        )
        .await
        .unwrap();
    let checkout = json!({"cartId":"resume-cart","shippingAddress":resume_address});
    let resumed = value(
        call(
            &client,
            &base,
            Method::POST,
            "/checkout",
            Some(checkout.clone()),
        )
        .await,
    )
    .await;
    assert_eq!(resumed["clientSecret"], "guarded-resume-secret");
    cancel_resume.store(true, std::sync::atomic::Ordering::SeqCst);
    let denied = call(&client, &base, Method::POST, "/checkout", Some(checkout)).await;
    assert!(!denied.status().is_success());
    assert!(
        !denied
            .text()
            .await
            .unwrap()
            .contains("guarded-resume-secret")
    );
    assert_eq!(resume_reads.load(std::sync::atomic::Ordering::SeqCst), 2);
    let reference:String=sqlx::query_scalar("SELECT data->>'providerRef' FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id='resume-payment'").bind(tenant.id()).fetch_one(&owner).await.unwrap();
    assert_eq!(reference, "pi_resume");

    let paypal = value(
        call(
            &client,
            &base,
            Method::POST,
            "/payment",
            Some(json!({"orderId":"paypal-order","amountCents":200,"currency":"usd","provider":"paypal"})),
        )
        .await,
    )
    .await;
    assert_eq!(paypal["providerRef"], "paypal-owned");
    assert!(paypal.get("clientSecret").is_none());
    let initialized=rpc(&client,&base,"initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}})).await;
    assert_eq!(initialized["result"]["serverInfo"]["name"], "ecommerce");
    assert_eq!(
        rpc(&client, &base, "tools/list", json!({})).await["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        33
    );
    let tool=rpc(&client,&base,"tools/call",json!({"name":"listProducts","arguments":{"query":{"ids":["product"]},"headers":{"authorization":sign("GET","/product",None)}}})).await;
    let products: Value =
        serde_json::from_str(tool["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(products.as_array().unwrap().len(), 1);
    let denied = rpc(
        &client,
        &base,
        "tools/call",
        json!({"name":"listProducts","arguments":{}}),
    )
    .await;
    assert_eq!(denied["result"]["isError"], true);
    let body = json!({"id":"mcp-cart"});
    let raw = body.to_string();
    let created=rpc(&client,&base,"tools/call",json!({"name":"createCart","arguments":{"body":body,"headers":{"authorization":sign("POST","/cart",Some(&raw))}}})).await;
    assert_eq!(created["result"]["structuredContent"]["id"], "mcp-cart");
    assert_eq!(
        client
            .post(format!("{base}/mcp"))
            .header("origin", "https://evil.example")
            .header("accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let (audits,):(i64,)=sqlx::query_as("SELECT COUNT(*) FROM fl_ecommerce.pii_read_audit WHERE tenant_id=$1 AND actor_id='service:default'").bind(tenant.id()).fetch_one(&owner).await.unwrap();
    assert!(audits > 0);
    assert_eq!(
        call(&client, &base, Method::DELETE, "/variant/variant", None)
            .await
            .text()
            .await
            .unwrap(),
        "Deleted variant"
    );
    assert_eq!(
        call(&client, &base, Method::DELETE, "/product/product", None)
            .await
            .text()
            .await
            .unwrap(),
        "Deleted product"
    );
    stop.send(()).unwrap();
    task.await.unwrap();
    provider_stop.send(()).unwrap();
    provider_task.await.unwrap();
    for table in [
        "records",
        "pii_read_audit",
        "inventory_event_receipts",
        "webhook_events",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_ecommerce.{table} WHERE tenant_id=ANY($1)"
        )))
        .bind(vec![tenant.id(), other.id()])
        .execute(&owner)
        .await
        .unwrap();
    }
    sqlx::query("DELETE FROM fl_worker.jobs WHERE tenant_id=$1")
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
    runtime.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
