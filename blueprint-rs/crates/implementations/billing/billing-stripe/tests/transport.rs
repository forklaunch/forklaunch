use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use forklaunch_billing_stripe::client::{StripeBillingClient, StripeResource};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::json;
use std::time::Duration;
async fn provider(Path(id): Path<String>) -> Response {
    match id.as_str() {
        "large" => Json(json!({"id":id,"private":"x".repeat(4096)})).into_response(),
        "slow" => {
            tokio::time::sleep(Duration::from_millis(150)).await;
            Json(json!({"id":id})).into_response()
        }
        "error" => (StatusCode::BAD_GATEWAY, "sk_fixture private@example.test").into_response(),
        "redirect" => (StatusCode::FOUND, [("location", "/trap")]).into_response(),
        "missing-id" => Json(json!({"secret":"sk_fixture"})).into_response(),
        "invalid-id" => Json(json!({"id":""})).into_response(),
        "array" => Json(json!([])).into_response(),
        "bad-list" => Json(json!({"data":{}})).into_response(),
        _ => Json(json!({"id":id})).into_response(),
    }
}
#[tokio::test]
async fn responses_are_bounded_sanitized_and_validated_without_redirects() {
    let app = Router::new()
        .route("/v1/plans/{id}", get(provider))
        .route("/v1/plans", get(|| async { Json(json!({"data":{}})) }))
        .route(
            "/trap",
            get(|| async {
                panic!("must never follow provider redirects");
                #[allow(unreachable_code)]
                "trap"
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&base, Duration::from_millis(50), 1024).unwrap(),
    )
    .unwrap();
    assert!(!format!("{client:?}").contains("sk_fixture"));
    for id in [
        "large",
        "slow",
        "error",
        "redirect",
        "missing-id",
        "invalid-id",
        "array",
    ] {
        let error = client
            .retrieve(StripeResource::Plan, id, &json!({}))
            .await
            .unwrap_err();
        assert!(!error.message.contains("sk_fixture"));
        assert!(!error.message.contains("private@example.test"));
    }
    assert!(client.list(StripeResource::Plan, &json!({})).await.is_err());
    assert_eq!(
        client
            .retrieve(StripeResource::Plan, "good", &json!({}))
            .await
            .unwrap()["id"],
        "good"
    );
    assert!(
        client
            .action(StripeResource::Plan, "good", "resume", &json!({}))
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn pagination_rejects_missing_cursors_cycles_and_malformed_pages() {
    use axum::extract::Query;
    use std::collections::HashMap;
    let app = Router::new().route(
        "/v1/prices",
        get(|Query(query): Query<HashMap<String, String>>| async move {
            let value = match query.get("product").map(String::as_str) {
                Some("empty") => json!({"data":[],"has_more":true}),
                Some("missing-more") => json!({"data":[]}),
                Some("cycle") => json!({"data":[{"id":"same"}],"has_more":true}),
                Some("large") => json!({"data":vec![json!({"id":"item"});101],"has_more":false}),
                _ => {
                    if query.contains_key("starting_after") {
                        json!({"data":[{"id":"second"}],"has_more":false})
                    } else {
                        json!({"data":[{"id":"first"}],"has_more":true})
                    }
                }
            };
            Json(value)
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&base, Duration::from_secs(2), 1024 * 1024).unwrap(),
    )
    .unwrap();
    for product in ["empty", "missing-more", "cycle", "large"] {
        assert!(
            client
                .list_pages(StripeResource::Price, &json!({"product":product}))
                .await
                .is_err()
        );
    }
    let values = client
        .list_pages(StripeResource::Price, &json!({"product":"good"}))
        .await
        .unwrap();
    assert_eq!(
        values,
        json!([{"id":"first"},{"id":"second"}])
            .as_array()
            .unwrap()
            .clone()
    );
    server.abort();
}

#[tokio::test]
async fn managed_json_capabilities_bind_raw_body_path_and_fresh_nonce() {
    use axum::{body::Bytes, http::HeaderMap};
    use forklaunch_core::auth::request_hmac::{HmacAuthorization, SignedBody};
    use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
    use std::sync::{Arc, Mutex};
    let nonces = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let observed = Arc::clone(&nonces);
    let app = Router::new().route(
        "/prefix/instance-gateway/payments/onboarding-link",
        axum::routing::post(move |headers: HeaderMap, body: Bytes| {
            let observed = Arc::clone(&observed);
            async move {
                let auth =
                    HmacAuthorization::parse(headers["authorization"].to_str().unwrap()).unwrap();
                assert_eq!(auth.key_id(), "instance_fixture");
                assert!(auth.verify_at(
                    b"instance-secret-fixture",
                    "POST",
                    "/payments/onboarding-link",
                    SignedBody::Bytes(&body),
                    chrono::Utc::now().timestamp_millis(),
                    Duration::from_secs(5)
                ));
                assert!(observed.lock().unwrap().insert(auth.nonce().to_owned()));
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                    json!({"returnUrl":"https://example.test/💬?a=1&b=2"})
                );
                Json(json!({"url":"https://provider.example/onboard"}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/prefix/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = InstanceGatewayClient::new(
        &base,
        "instance_fixture".into(),
        "instance-secret-fixture".into(),
    )
    .unwrap();
    assert!(!format!("{client:?}").contains("instance-secret-fixture"));
    for _ in 0..2 {
        let result = client
            .request_json(
                reqwest::Method::POST,
                &["payments", "onboarding-link"],
                Some(&json!({"returnUrl":"https://example.test/💬?a=1&b=2"})),
            )
            .await
            .unwrap();
        assert_eq!(result.status, 200);
        assert_eq!(result.body["url"], "https://provider.example/onboard");
    }
    assert_eq!(nonces.lock().unwrap().len(), 2);
    for (url, id, key) in [
        (base.as_str(), "", "key"),
        (base.as_str(), "bad id", "key"),
        (base.as_str(), "id", ""),
        (base.as_str(), "id", "secret\n"),
        ("http://example.com", "id", "key"),
        ("https://user:secret@example.com", "id", "key"),
    ] {
        assert!(InstanceGatewayClient::new(url, id.into(), key.into()).is_err());
    }
    server.abort();
}
