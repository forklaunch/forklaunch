//! Account observations bound to one immutable ecommerce client and scope.
use crate::{StripeClient, StripeConnect};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const LIFETIME: Duration = Duration::from_secs(5);

/// Non-serializable account/mode observation, not permission to move funds or
/// provision ownership. Clones of the same client share its generation.
pub struct VerifiedDirectStripeContext {
    generation: Arc<()>,
    started_at: Instant,
    account_id: String,
    livemode: bool,
}
impl VerifiedDirectStripeContext {
    #[must_use]
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    #[must_use]
    pub const fn livemode(&self) -> bool {
        self.livemode
    }
    /// # Errors
    /// Rejects another client generation and observations older than five seconds.
    pub fn ensure_current(&self, client: &StripeClient) -> BlueprintResult<()> {
        if !Arc::ptr_eq(&self.generation, &client.generation)
            || self.started_at.elapsed() >= LIFETIME
        {
            return Err(mismatch());
        }
        Ok(())
    }
}
pub(crate) fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_provider_context_mismatch",
        "Payment provider context does not match",
    )
}
impl StripeClient {
    /// Authenticated direct-account GETs using the same immutable bearer and URL.
    /// No Connect header, key-prefix inference, provider write or ownership grant.
    /// Trusted expectations must come from server configuration, not request fields.
    /// # Errors
    /// Rejects mismatched/malformed account or mode, denied reads and stale context.
    pub async fn verify_direct_context(
        &self,
        account_id: &str,
        livemode: bool,
    ) -> BlueprintResult<VerifiedDirectStripeContext> {
        if !account_id.starts_with("acct_")
            || account_id.len() <= 5
            || account_id.len() > 128
            || !account_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(mismatch());
        }
        let context = VerifiedDirectStripeContext {
            generation: self.generation.clone(),
            started_at: Instant::now(),
            account_id: account_id.into(),
            livemode,
        };
        let account = self.context_get("account", None).await?;
        context.ensure_current(self)?;
        if account["object"] != "account" || account["id"] != account_id {
            return Err(mismatch());
        }
        let balance = self.context_get("balance", None).await?;
        context.ensure_current(self)?;
        if balance["object"] != "balance" || balance["livemode"].as_bool() != Some(livemode) {
            return Err(mismatch());
        }
        Ok(context)
    }
    async fn context_get(
        &self,
        resource: &str,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<Value> {
        let mut request = self
            .http
            .request(Method::GET, &["v1", resource])?
            .bearer_auth(self.secret.as_str());
        if let Some(connect) = connect {
            request = request.header("Stripe-Account", &connect.account_id);
        }
        let response = self.http.execute_json(request).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "Stripe",
                if connect.is_some() {
                    "verify connected payment context"
                } else {
                    "verify direct payment context"
                },
                response.status,
            ));
        }
        Ok(response.body)
    }
    /// Reads an intent within a direct-account observation. Returns raw sensitive
    /// provider data only to trusted callers; do not log or persist the body.
    /// # Errors
    /// Rejects stale/foreign context and mismatched intent identity/type/mode.
    pub async fn retrieve_intent_in_context(
        &self,
        context: &VerifiedDirectStripeContext,
        id: &str,
    ) -> BlueprintResult<Value> {
        context.ensure_current(self)?;
        let intent = self.retrieve_intent_details(id, None).await?;
        context.ensure_current(self)?;
        if intent["object"] != "payment_intent"
            || intent["livemode"].as_bool() != Some(context.livemode)
        {
            return Err(mismatch());
        }
        Ok(intent)
    }
}

/// Sealed connected-account observation. The copied Connect configuration and
/// immutable bearer/transport generation must still match at every use.
pub struct VerifiedConnectedStripeContext {
    observation: VerifiedDirectStripeContext,
    connect: StripeConnect,
}
impl VerifiedConnectedStripeContext {
    #[must_use]
    pub fn account_id(&self) -> &str {
        self.observation.account_id()
    }
    #[must_use]
    pub const fn livemode(&self) -> bool {
        self.observation.livemode()
    }
    /// # Errors
    /// Rejects a foreign client, changed Connect configuration or expired observation.
    pub fn ensure_current(
        &self,
        client: &StripeClient,
        connect: &StripeConnect,
    ) -> BlueprintResult<()> {
        self.observation.ensure_current(client)?;
        if connect.account_id != self.connect.account_id || connect.fee_bps != self.connect.fee_bps
        {
            return Err(mismatch());
        }
        Ok(())
    }
}
impl StripeClient {
    /// Authenticates account and mode using the exact Stripe-Account header on
    /// both GETs. Trusted expected account must equal the configured Connect ID.
    /// # Errors
    /// Rejects mismatched/malformed scope, denied reads, changed shape or deadline.
    pub async fn verify_connected_context(
        &self,
        connect: &StripeConnect,
        expected_account: &str,
        livemode: bool,
    ) -> BlueprintResult<VerifiedConnectedStripeContext> {
        if connect.account_id != expected_account
            || !expected_account.starts_with("acct_")
            || expected_account.len() <= 5
            || expected_account.len() > 128
            || !expected_account
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(mismatch());
        }
        let context = VerifiedConnectedStripeContext {
            observation: VerifiedDirectStripeContext {
                generation: self.generation.clone(),
                started_at: Instant::now(),
                account_id: expected_account.into(),
                livemode,
            },
            connect: connect.clone(),
        };
        let account = self.context_get("account", Some(&context.connect)).await?;
        context.ensure_current(self, connect)?;
        if account["object"] != "account" || account["id"] != expected_account {
            return Err(mismatch());
        }
        let balance = self.context_get("balance", Some(&context.connect)).await?;
        context.ensure_current(self, connect)?;
        if balance["object"] != "balance" || balance["livemode"].as_bool() != Some(livemode) {
            return Err(mismatch());
        }
        Ok(context)
    }
    /// Reads only with the same sealed Connect configuration and client generation.
    /// Raw provider data may contain secrets/PII; trusted callers must not log it.
    /// # Errors
    /// Rejects stale/foreign context/configuration and substituted intent type/mode.
    pub async fn retrieve_intent_in_connected_context(
        &self,
        context: &VerifiedConnectedStripeContext,
        connect: &StripeConnect,
        id: &str,
    ) -> BlueprintResult<Value> {
        context.ensure_current(self, connect)?;
        let intent = self
            .retrieve_intent_details(id, Some(&context.connect))
            .await?;
        context.ensure_current(self, connect)?;
        if intent["object"] != "payment_intent"
            || intent["livemode"].as_bool() != Some(context.livemode())
        {
            return Err(mismatch());
        }
        Ok(intent)
    }
}
#[cfg(feature = "postgres")]
pub(crate) enum ServiceContext {
    Direct(VerifiedDirectStripeContext),
    Connected(VerifiedConnectedStripeContext),
}
#[cfg(feature = "postgres")]
impl ServiceContext {
    // Request construction uses the copied configuration sealed into the context,
    // never a separate caller-supplied account after verification.
    pub(crate) fn sealed_connect_config(&self) -> Option<&StripeConnect> {
        match self {
            Self::Direct(_) => None,
            Self::Connected(context) => Some(&context.connect),
        }
    }
    pub(crate) fn ensure_current(
        &self,
        client: &StripeClient,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<()> {
        match (self, connect) {
            (Self::Direct(context), None) => context.ensure_current(client),
            (Self::Connected(context), Some(connect)) => context.ensure_current(client, connect),
            _ => Err(mismatch()),
        }
    }
    pub(crate) const fn livemode(&self) -> bool {
        match self {
            Self::Direct(context) => context.livemode(),
            Self::Connected(context) => context.livemode(),
        }
    }
    pub(crate) async fn retrieve(
        &self,
        client: &StripeClient,
        connect: Option<&StripeConnect>,
        id: &str,
    ) -> BlueprintResult<Value> {
        match (self, connect) {
            (Self::Direct(context), None) => client.retrieve_intent_in_context(context, id).await,
            (Self::Connected(context), Some(connect)) => {
                client
                    .retrieve_intent_in_connected_context(context, connect, id)
                    .await
            }
            _ => Err(mismatch()),
        }
    }
}
#[cfg(feature = "postgres")]
impl StripeClient {
    pub(crate) async fn verify_service_context(
        &self,
        account: &str,
        livemode: bool,
        connect: Option<&StripeConnect>,
    ) -> BlueprintResult<ServiceContext> {
        match connect {
            Some(connect) => self
                .verify_connected_context(connect, account, livemode)
                .await
                .map(ServiceContext::Connected),
            None => self
                .verify_direct_context(account, livemode)
                .await
                .map(ServiceContext::Direct),
        }
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
    }
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn direct_context_exact_account_mode_generation_expiry_and_wire() {
        let state = Fixture {
            account: Arc::new(Mutex::new(json!({"object":"account","id":"acct_owned"}))),
            balance: Arc::new(Mutex::new(json!({"object":"balance","livemode":false}))),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route(
                "/v1/account",
                get(|State(s): State<Fixture>, headers: HeaderMap| async move {
                    assert_eq!(headers["authorization"], "Bearer owned-secret");
                    assert!(!headers.contains_key("stripe-account"));
                    s.calls.fetch_add(1, Ordering::SeqCst);
                    let value = s.account.lock().unwrap().clone();
                    (
                        if value.is_null() {
                            StatusCode::UNAUTHORIZED
                        } else {
                            StatusCode::OK
                        },
                        Json(value),
                    )
                }),
            )
            .route(
                "/v1/balance",
                get(|State(s): State<Fixture>, headers: HeaderMap| async move {
                    assert_eq!(headers["authorization"], "Bearer owned-secret");
                    assert!(!headers.contains_key("stripe-account"));
                    s.calls.fetch_add(1, Ordering::SeqCst);
                    Json(s.balance.lock().unwrap().clone())
                }),
            )
            .route(
                "/v1/payment_intents/pi_owned",
                get(|| async {
                    Json(json!({"id":"pi_owned","object":"payment_intent","livemode":false}))
                }),
            )
            .route(
                "/v1/payment_intents/pi_slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    Json(json!({"id":"pi_slow","object":"payment_intent","livemode":false}))
                }),
            )
            .route(
                "/v1/payment_intents/pi_denied",
                get(|| async { (StatusCode::UNAUTHORIZED, "private-provider-body") }),
            )
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = StripeClient::with_transport(
            "owned-secret".into(),
            ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(1), 1024)
                .unwrap(),
        )
        .unwrap();
        let mut context = client
            .verify_direct_context("acct_owned", false)
            .await
            .unwrap();
        assert!(context.ensure_current(&client.clone()).is_ok());
        assert!(
            client
                .retrieve_intent_in_context(&context, "pi_owned")
                .await
                .is_ok()
        );
        let error = client
            .retrieve_intent_in_context(&context, "pi_denied")
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("private-provider-body"));
        let other =
            StripeClient::with_transport("owned-secret".into(), client.http.clone()).unwrap();
        assert!(context.ensure_current(&other).is_err());
        context.started_at =
            Instant::now().checked_sub(LIFETIME).unwrap() + Duration::from_millis(10);
        assert!(
            client
                .retrieve_intent_in_context(&context, "pi_slow")
                .await
                .is_err()
        );
        context.started_at = Instant::now().checked_sub(LIFETIME).unwrap();
        assert!(
            client
                .retrieve_intent_in_context(&context, "pi_owned")
                .await
                .is_err()
        );
        for account in [
            Value::Null,
            json!({"object":"account","id":"acct_other"}),
            json!({"object":"wrong","id":"acct_owned"}),
            json!({"id":"acct_owned"}),
        ] {
            *state.account.lock().unwrap() = account;
            assert!(
                client
                    .verify_direct_context("acct_owned", false)
                    .await
                    .is_err()
            );
        }
        *state.account.lock().unwrap() = json!({"object":"account","id":"acct_owned"});
        for balance in [
            json!({"object":"balance","livemode":true}),
            json!({"object":"balance","livemode":"false"}),
            json!({"object":"balance"}),
            json!({"object":"wrong","livemode":false}),
        ] {
            *state.balance.lock().unwrap() = balance;
            assert!(
                client
                    .verify_direct_context("acct_owned", false)
                    .await
                    .is_err()
            );
        }
        let count = state.calls.load(Ordering::SeqCst);
        assert!(
            client
                .verify_direct_context("acct_unsafe/path", false)
                .await
                .is_err()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), count);
        server.abort();
    }
}

#[cfg(test)]
mod connected_tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Path, State},
        http::HeaderMap,
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
        values: Arc<Mutex<Value>>,
        calls: Arc<AtomicUsize>,
    }
    fn headers(headers: &HeaderMap) {
        assert_eq!(headers["authorization"], "Bearer owned-platform-secret");
        assert_eq!(headers["stripe-account"], "acct_owned");
    }
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn connected_context_pins_header_config_generation_mode_and_deadline() {
        let state = Fixture {
            values: Arc::new(Mutex::new(
                json!({"account":{"object":"account","id":"acct_owned"},"balance":{"object":"balance","livemode":false},"intent":{"object":"payment_intent","id":"pi_owned","livemode":false}}),
            )),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let app=Router::new().route("/v1/{resource}",get(|State(s):State<Fixture>,Path(resource):Path<String>,h:HeaderMap|async move{headers(&h);s.calls.fetch_add(1,Ordering::SeqCst);Json(s.values.lock().unwrap()[resource].clone())}))
            .route("/v1/payment_intents/pi_owned",get(|State(s):State<Fixture>,h:HeaderMap|async move{headers(&h);s.calls.fetch_add(1,Ordering::SeqCst);tokio::time::sleep(Duration::from_millis(30)).await;Json(s.values.lock().unwrap()["intent"].clone())})).with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = StripeClient::with_transport(
            "owned-platform-secret".into(),
            ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(2), 4096)
                .unwrap(),
        )
        .unwrap();
        let connect = StripeConnect::new("acct_owned".into(), 0).unwrap();
        assert!(
            client
                .verify_connected_context(&connect, "acct_other", false)
                .await
                .is_err()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), 0);
        let mut context = client
            .verify_connected_context(&connect, "acct_owned", false)
            .await
            .unwrap();
        assert_eq!(context.account_id(), "acct_owned");
        assert!(!context.livemode());
        assert!(
            client
                .retrieve_intent_in_connected_context(&context, &connect, "pi_owned")
                .await
                .is_ok()
        );
        let before = state.calls.load(Ordering::SeqCst);
        for different in [
            StripeConnect::new("acct_other".into(), 0).unwrap(),
            StripeConnect::new("acct_owned".into(), 1).unwrap(),
        ] {
            assert!(
                client
                    .retrieve_intent_in_connected_context(&context, &different, "pi_owned")
                    .await
                    .is_err()
            );
        }
        let other =
            StripeClient::with_transport("owned-platform-secret".into(), client.http.clone())
                .unwrap();
        assert!(
            other
                .retrieve_intent_in_connected_context(&context, &connect, "pi_owned")
                .await
                .is_err()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), before);
        for (field, value) in [
            ("id", json!("pi_other")),
            ("object", json!("charge")),
            ("livemode", json!(true)),
            ("livemode", Value::Null),
        ] {
            state.values.lock().unwrap()["intent"][field] = value;
            assert!(
                client
                    .retrieve_intent_in_connected_context(&context, &connect, "pi_owned")
                    .await
                    .is_err()
            );
            state.values.lock().unwrap()["intent"] =
                json!({"object":"payment_intent","id":"pi_owned","livemode":false});
        }
        context.observation.started_at =
            Instant::now().checked_sub(LIFETIME).unwrap() + Duration::from_millis(10);
        assert!(
            client
                .retrieve_intent_in_connected_context(&context, &connect, "pi_owned")
                .await
                .is_err()
        );
        for account in [
            json!({"object":"account","id":"acct_platform"}),
            json!({"id":"acct_owned"}),
            Value::Null,
        ] {
            state.values.lock().unwrap()["account"] = account;
            assert!(
                client
                    .verify_connected_context(&connect, "acct_owned", false)
                    .await
                    .is_err()
            );
        }
        state.values.lock().unwrap()["account"] = json!({"object":"account","id":"acct_owned"});
        for balance in [
            json!({"object":"balance","livemode":true}),
            json!({"object":"balance","livemode":"false"}),
            json!({"object":"balance"}),
        ] {
            state.values.lock().unwrap()["balance"] = balance;
            assert!(
                client
                    .verify_connected_context(&connect, "acct_owned", false)
                    .await
                    .is_err()
            );
        }
        server.abort();
    }
}
