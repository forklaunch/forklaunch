use super::*;

#[test]
fn stripe_webhook_signatures_verify_and_reject() {
    let secret = b"whsec_test";
    let payload = r#"{"id":"evt_1","type":"customer.subscription.updated"}"#;
    let timestamp = &std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string();
    let signature = webhook::sign(secret, timestamp, payload);
    let header = format!("t={timestamp},v1={signature}");

    assert!(webhook::verify(secret, payload, &header));
    // tampered payload fails
    assert!(!webhook::verify(secret, "{}", &header));
    // wrong secret fails
    assert!(!webhook::verify(b"whsec_other", payload, &header));
    let expired = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 301;
    let expired = expired.to_string();
    assert!(!webhook::verify(
        secret,
        payload,
        &format!(
            "t={expired},v1={}",
            webhook::sign(secret, &expired, payload)
        )
    ));
    // missing timestamp fails
    assert!(!webhook::verify(
        secret,
        payload,
        &format!("v1={signature}")
    ));
}

#[test]
fn legacy_adapter_cannot_fabricate_provider_results() {
    let adapter = StripeBillingAdapter::new(StripeBillingConfig::new("STRIPE_WEBHOOK_SECRET"));
    assert!(adapter.get_customer("customer_1").is_err());
    assert!(adapter.list_customers(PageRequest::first_page(10)).is_err());
    assert!(adapter.list_subscriptions("customer_1").is_err());
    assert_eq!(
        adapter
            .create_checkout_session("customer_1", "pro", &AuditContext::system("corr_1"))
            .unwrap_err()
            .code,
        "billing_async_service_required"
    );
}
