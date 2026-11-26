//! Direct and generation-scoped managed account verification.
use super::{BillingTransport, StripeBillingClient};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const CONTEXT_LIFETIME: Duration = Duration::from_secs(5);

/// Sealed, non-serializable observation tied to one immutable client generation.
/// Cloning that client preserves identity; constructing/reconfiguring a client
/// requires fresh verification. This is not a financial-operation capability or
/// authorization to provision local ownership. Future integrated operations must
/// verify again for each operation and check freshness immediately before use.
pub struct VerifiedStripeContext {
    generation: Arc<()>,
    started_at: Instant,
    account_id: String,
    livemode: bool,
    managed: Option<(String, Value)>,
}
impl VerifiedStripeContext {
    #[cfg(feature = "postgres-reconciliation")]
    pub(crate) fn connected_account(&self) -> Option<&str> {
        self.managed.as_ref().map(|_| self.account_id.as_str())
    }

    #[must_use]
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    #[must_use]
    pub const fn livemode(&self) -> bool {
        self.livemode
    }
    /// Checks the exact immutable client generation and a five-second window
    /// starting before provider verification. Does not refresh cached evidence.
    /// # Errors
    /// Rejects foreign/reconfigured clients and stale observations.
    pub fn ensure_current(&self, client: &StripeBillingClient) -> BlueprintResult<()> {
        if !Arc::ptr_eq(&self.generation, &client.config_generation)
            || self.started_at.elapsed() >= CONTEXT_LIFETIME
        {
            return Err(mismatch());
        }
        Ok(())
    }
}
impl StripeBillingClient {
    /// Authenticates the current direct account using GET /v1/account and its
    /// mode using GET /v1/balance, under the same immutable bearer/version/URL.
    /// Managed clients use the dedicated signed JSON scope protocol instead.
    /// Expectations must come from trusted server configuration, not request DTOs.
    /// Restricted keys without these read permissions fail closed. No key-prefix
    /// inference, cache, provider writes, Connect header or local binding mutation.
    /// # Errors
    /// Rejects account/mode mismatch, malformed responses, expired verification,
    /// provider errors or an unsupported/malformed managed gateway protocol.
    pub async fn verify_context(
        &self,
        expected_account: &str,
        expected_livemode: bool,
    ) -> BlueprintResult<VerifiedStripeContext> {
        if !expected_account.starts_with("acct_")
            || expected_account.len() <= 5
            || expected_account.len() > 128
            || !expected_account
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(mismatch());
        }
        let started_at = Instant::now();
        if let BillingTransport::Managed(gateway) = &self.transport {
            let response = gateway.request_json(Method::POST, &["payments", "context"], Some(&json!({
                "accountId": expected_account, "livemode": expected_livemode, "apiVersion": self.api_version.as_str()
            }))).await?;
            if !(200..300).contains(&response.status) {
                return Err(mismatch());
            }
            let scope = &response.body["scope"];
            if scope["version"] != 1
                || scope["instanceId"] != gateway.instance_id()
                || scope["accountId"] != expected_account
                || scope["livemode"].as_bool() != Some(expected_livemode)
                || scope["apiVersion"] != self.api_version.as_str()
                || scope["organizationId"].as_str().is_none_or(str::is_empty)
                || scope["bindingGeneration"]
                    .as_str()
                    .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
                || scope["issuedAt"]
                    .as_i64()
                    .zip(scope["expiresAt"].as_i64())
                    .is_none_or(|(a, b)| b.checked_sub(a) != Some(5000))
            {
                return Err(mismatch());
            }
            let token = response.body["contextToken"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 4096)
                .ok_or_else(mismatch)?;
            let context = VerifiedStripeContext {
                generation: self.config_generation.clone(),
                started_at,
                account_id: expected_account.into(),
                livemode: expected_livemode,
                managed: Some((token.into(), scope.clone())),
            };
            context.ensure_current(self)?;
            return Ok(context);
        }
        let account = self.context_get("account").await?;
        if account["object"] != "account" || account["id"] != expected_account {
            return Err(mismatch());
        }
        let balance = self.context_get("balance").await?;
        if balance["object"] != "balance"
            || balance["livemode"].as_bool() != Some(expected_livemode)
        {
            return Err(mismatch());
        }
        let context = VerifiedStripeContext {
            generation: self.config_generation.clone(),
            started_at,
            account_id: expected_account.into(),
            livemode: expected_livemode,
            managed: None,
        };
        context.ensure_current(self)?;
        Ok(context)
    }
    /// Reads a checkout through the same verified direct or managed scope.
    /// # Errors
    /// Rejects stale/foreign contexts and changed gateway scope envelopes.
    pub async fn retrieve_checkout_in_context(
        &self,
        context: &VerifiedStripeContext,
        id: &str,
    ) -> BlueprintResult<Value> {
        context.ensure_current(self)?;
        super::validate_id(id)?;
        let value = match (&self.transport, &context.managed) {
            (BillingTransport::Direct { .. }, None) => {
                self.retrieve(super::StripeResource::CheckoutSession, id, &json!({}))
                    .await?
            }
            (BillingTransport::Managed(gateway), Some((token, scope))) => {
                let response = gateway
                    .request_json(
                        Method::POST,
                        &["payments", "context", "checkout-read"],
                        Some(&json!({
                            "contextToken": token, "id": id, "apiVersion": self.api_version.as_str()
                        })),
                    )
                    .await?;
                if !(200..300).contains(&response.status) || response.body["scope"] != *scope {
                    return Err(mismatch());
                }
                response.body["object"].clone()
            }
            _ => return Err(mismatch()),
        };
        context.ensure_current(self)?;
        if value["object"] != "checkout.session"
            || value["id"] != id
            || value["livemode"].as_bool() != Some(context.livemode)
        {
            return Err(mismatch());
        }
        Ok(value)
    }
    /// Reads a payment link through the same verified direct or managed scope.
    /// # Errors
    /// Rejects stale/foreign contexts and changed gateway scope envelopes.
    pub async fn retrieve_payment_link_in_context(
        &self,
        context: &VerifiedStripeContext,
        id: &str,
    ) -> BlueprintResult<Value> {
        context.ensure_current(self)?;
        if !id.starts_with("plink_")
            || id.len() <= 6
            || id.len() > 128
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(mismatch());
        }
        let value = match (&self.transport, &context.managed) {
            (BillingTransport::Direct { .. }, None) => {
                self.retrieve(super::StripeResource::PaymentLink, id, &json!({}))
                    .await?
            }
            (BillingTransport::Managed(gateway), Some((token, scope))) => {
                let response = gateway
                    .request_json(
                        Method::POST,
                        &["payments", "context", "payment-link-read"],
                        Some(&json!({
                            "contextToken": token, "id": id, "apiVersion": self.api_version.as_str()
                        })),
                    )
                    .await?;
                if !(200..300).contains(&response.status) || response.body["scope"] != *scope {
                    return Err(mismatch());
                }
                response.body["object"].clone()
            }
            _ => return Err(mismatch()),
        };
        context.ensure_current(self)?;
        if value["object"] != "payment_link"
            || value["id"] != id
            || value["livemode"].as_bool() != Some(context.livemode)
        {
            return Err(mismatch());
        }
        Ok(value)
    }
    /// Retrieves a fixed event/subscription pair in one verified direct or managed scope.
    /// Provider identity/mode and the exact managed envelope are checked here;
    /// callers must additionally verify event semantics and trusted local ownership.
    /// # Errors
    /// Rejects malformed IDs, stale/foreign contexts and remapped managed scopes.
    pub async fn retrieve_paused_effect_in_context(
        &self,
        context: &VerifiedStripeContext,
        event_id: &str,
        subscription_id: &str,
    ) -> BlueprintResult<(Value, Value)> {
        for (id, prefix) in [(event_id, "evt_"), (subscription_id, "sub_")] {
            if !id.starts_with(prefix)
                || id.len() <= prefix.len()
                || id.len() > 128
                || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(mismatch());
            }
        }
        context.ensure_current(self)?;
        let pair = match (&self.transport, &context.managed) {
            (BillingTransport::Direct { .. }, None) => {
                let event = self
                    .retrieve(super::StripeResource::Event, event_id, &json!({}))
                    .await?;
                context.ensure_current(self)?;
                let subscription = self
                    .retrieve(
                        super::StripeResource::Subscription,
                        subscription_id,
                        &json!({}),
                    )
                    .await?;
                (event, subscription)
            }
            (BillingTransport::Managed(gateway), Some((token, scope))) => {
                let response = gateway
                    .request_json(
                        Method::POST,
                        &["payments", "context", "paused-subscription-read"],
                        Some(&json!({
                            "contextToken": token,
                            "eventId": event_id,
                            "subscriptionId": subscription_id,
                            "apiVersion": self.api_version.as_str()
                        })),
                    )
                    .await?;
                if !(200..300).contains(&response.status) || response.body["scope"] != *scope {
                    return Err(mismatch());
                }
                (
                    response.body["event"].clone(),
                    response.body["subscription"].clone(),
                )
            }
            _ => return Err(mismatch()),
        };
        context.ensure_current(self)?;
        if pair.0["object"] != "event"
            || pair.0["id"] != event_id
            || pair.0["livemode"].as_bool() != Some(context.livemode)
            || pair.1["object"] != "subscription"
            || pair.1["id"] != subscription_id
            || pair.1["livemode"].as_bool() != Some(context.livemode)
        {
            return Err(mismatch());
        }
        Ok(pair)
    }
    async fn context_get(&self, resource: &str) -> BlueprintResult<Value> {
        let BillingTransport::Direct { http, secret } = &self.transport else {
            return Err(mismatch());
        };
        let request = http
            .request(Method::GET, &["v1", resource])?
            .bearer_auth(secret.as_str())
            .header("stripe-version", self.api_version.as_str());
        let response = http.execute_json(request).await?;
        if !(200..300).contains(&response.status) {
            return Err(provider_status_error("stripe", "billing", response.status));
        }
        if !response.body.is_object() {
            return Err(mismatch());
        }
        Ok(response.body)
    }
}
fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "billing_provider_context_mismatch",
        "Stripe account context is invalid, changed or expired",
    )
}

#[cfg(feature = "postgres-identity")]
impl StripeBillingClient {
    pub(crate) fn direct_creation_version(&self) -> BlueprintResult<&str> {
        if !matches!(self.transport, BillingTransport::Direct { .. }) {
            return Err(BlueprintError::new(
                "billing_bound_creation_unsupported",
                "Bound creation requires a direct Stripe client",
            ));
        }
        Ok(self.api_version.as_str())
    }
    pub(crate) async fn create_payment_link_in_context(
        &self,
        context: &VerifiedStripeContext,
        idempotency_key: &str,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        self.direct_creation_version()?;
        if idempotency_key.len() > 255
            || !idempotency_key.starts_with("fl-plink-")
            || !idempotency_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(mismatch());
        }
        let BillingTransport::Direct { http, secret } = &self.transport else {
            return Err(mismatch());
        };
        let params = super::form_fields(fields)?;
        let request = http
            .request(Method::POST, &["v1", "payment_links"])?
            .bearer_auth(secret.as_str())
            .header("stripe-version", self.api_version.as_str())
            .header("idempotency-key", idempotency_key)
            .form(&params);
        context.ensure_current(self)?;
        let response = http.execute_json(request).await?;
        context.ensure_current(self)?;
        if !(200..300).contains(&response.status) {
            return Err(provider_status_error("stripe", "billing", response.status));
        }
        Ok(response.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::get,
    };
    use forklaunch_core::http::provider_http::ProviderHttpClient;
    use serde_json::json;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct Fixture {
        account: Arc<Mutex<Value>>,
        balance: Arc<Mutex<Value>>,
        calls: Arc<AtomicUsize>,
        status: Arc<Mutex<StatusCode>>,
    }
    fn headers(headers: &HeaderMap) {
        assert_eq!(
            headers["authorization"],
            "Bearer opaque-credential-with-no-mode-prefix"
        );
        assert_eq!(headers["stripe-version"], super::super::STRIPE_API_VERSION);
        assert!(!headers.contains_key("stripe-account"));
    }
    async fn fixture() -> (
        StripeBillingClient,
        Fixture,
        tokio::task::JoinHandle<()>,
        String,
    ) {
        let state = Fixture {
            account: Arc::new(Mutex::new(json!({"id":"acct_owned","object":"account"}))),
            balance: Arc::new(Mutex::new(json!({"object":"balance","livemode":false}))),
            calls: Arc::new(AtomicUsize::new(0)),
            status: Arc::new(Mutex::new(StatusCode::OK)),
        };
        let app = Router::new()
            .route(
                "/v1/account",
                get(|State(state): State<Fixture>, h: HeaderMap| async move {
                    headers(&h);
                    state.calls.fetch_add(1, Ordering::SeqCst);
                    (
                        *state.status.lock().unwrap(),
                        Json(state.account.lock().unwrap().clone()),
                    )
                }),
            )
            .route(
                "/v1/balance",
                get(|State(state): State<Fixture>, h: HeaderMap| async move {
                    headers(&h);
                    state.calls.fetch_add(1, Ordering::SeqCst);
                    (
                        *state.status.lock().unwrap(),
                        Json(state.balance.lock().unwrap().clone()),
                    )
                }),
            )
            .fallback(|State(state): State<Fixture>| async move {
                state.calls.fetch_add(1, Ordering::SeqCst);
                StatusCode::NOT_FOUND
            })
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let http = ProviderHttpClient::new(&base, Duration::from_secs(2), 1024 * 1024).unwrap();
        let client = StripeBillingClient::with_transport(
            "opaque-credential-with-no-mode-prefix".into(),
            http,
        )
        .unwrap();
        (client, state, server, base)
    }
    #[tokio::test]
    async fn direct_context_requires_exact_fresh_immutable_client() {
        let (client, state, server, base) = fixture().await;
        let mut context = client.verify_context("acct_owned", false).await.unwrap();
        assert_eq!(state.calls.load(Ordering::SeqCst), 2);
        assert_eq!(context.account_id(), "acct_owned");
        assert!(!context.livemode());
        context.ensure_current(&client.clone()).unwrap();
        let other = StripeBillingClient::with_transport(
            "opaque-credential-with-no-mode-prefix".into(),
            ProviderHttpClient::new(&base, Duration::from_secs(2), 1024 * 1024).unwrap(),
        )
        .unwrap();
        assert!(context.ensure_current(&other).is_err());
        assert!(
            context
                .ensure_current(
                    &client
                        .clone()
                        .with_api_version("2026-08-26.dahlia".into())
                        .unwrap()
                )
                .is_err()
        );
        context.started_at = Instant::now().checked_sub(CONTEXT_LIFETIME).unwrap();
        assert!(context.ensure_current(&client).is_err());
        *state.account.lock().unwrap() = json!({"id":"acct_changed","object":"account"});
        assert!(client.verify_context("acct_owned", false).await.is_err());
        *state.account.lock().unwrap() = json!({"id":"acct_owned","object":"account"});
        *state.balance.lock().unwrap() = json!({"object":"balance","livemode":true});
        assert!(client.verify_context("acct_owned", false).await.is_err());
        assert!(
            client
                .verify_context("acct_owned", true)
                .await
                .unwrap()
                .livemode()
        );
        let calls = state.calls.load(Ordering::SeqCst);
        let managed =
            StripeBillingClient::managed(&base, "instance-owned".into(), "owned-hmac-key".into())
                .unwrap();
        assert_eq!(
            managed
                .verify_context("acct_owned", false)
                .await
                .err()
                .unwrap()
                .code,
            "billing_provider_context_mismatch"
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), calls + 1);
        server.abort();
    }
    #[tokio::test]
    async fn provider_context_denies_malformed_and_auth_failure() {
        let (client, state, server, _) = fixture().await;
        for account in [
            json!(null),
            json!([]),
            json!({"id":"acct_owned"}),
            json!({"id":"acct_owned","object":"customer"}),
            json!({"object":"account"}),
        ] {
            *state.account.lock().unwrap() = account;
            assert!(client.verify_context("acct_owned", false).await.is_err());
        }
        *state.account.lock().unwrap() = json!({"id":"acct_owned","object":"account"});
        for balance in [
            json!(null),
            json!([]),
            json!({"object":"balance"}),
            json!({"object":"balance","livemode":"false"}),
            json!({"object":"customer","livemode":false}),
        ] {
            *state.balance.lock().unwrap() = balance;
            assert!(client.verify_context("acct_owned", false).await.is_err());
        }
        *state.balance.lock().unwrap() = json!({"object":"balance","livemode":false});
        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            *state.status.lock().unwrap() = status;
            let error = client
                .verify_context("acct_owned", false)
                .await
                .err()
                .unwrap();
            assert!(!error.message.contains("acct_owned"));
            assert_eq!(error.code, "provider_request_failed");
        }
        let calls = state.calls.load(Ordering::SeqCst);
        assert!(client.verify_context(" ", false).await.is_err());
        assert_eq!(state.calls.load(Ordering::SeqCst), calls);
        server.abort();
    }
}
