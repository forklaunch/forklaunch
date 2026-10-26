//! Resource-scoped `PayPal` merchant evidence, not an OAuth merchant ownership claim.
use crate::{PaypalClient, PaypalOrderStatus, reconciliation::cents};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// Server-selected `PayPal` environment. Orders/OAuth do not attest a livemode bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaypalEnvironment {
    Sandbox,
    Live,
}
impl PaypalEnvironment {
    #[must_use]
    pub const fn livemode(self) -> bool {
        matches!(self, Self::Live)
    }
    #[must_use]
    pub const fn origin(self) -> &'static str {
        match self {
            Self::Sandbox => "https://api-m.sandbox.paypal.com",
            Self::Live => "https://api-m.paypal.com",
        }
    }
}
/// Redacted resource observation. COMPLETED is not proof of capture, settlement,
/// lack of refunds/disputes or permission to perform a financial operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundPaypalOrderObservation {
    pub provider_ref: String,
    pub status: PaypalOrderStatus,
    pub amount_cents: i64,
    pub currency: String,
    pub environment: PaypalEnvironment,
}
/// Opaque, nonserializable resource observation tied to one immutable client.
/// The authenticated order payee matches a trusted expected merchant; this does
/// not prove the OAuth application itself owns that merchant account.
pub struct VerifiedPaypalOrderContext {
    generation: Arc<()>,
    #[cfg_attr(not(feature = "postgres"), allow(dead_code))]
    voided_empty_history: bool,
    started_at: Instant,
    merchant_id: String,
    reference_id: String,
    observation: BoundPaypalOrderObservation,
}
impl VerifiedPaypalOrderContext {
    #[must_use]
    pub const fn observation(&self) -> &BoundPaypalOrderObservation {
        &self.observation
    }
    #[must_use]
    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }
    #[must_use]
    pub fn reference_id(&self) -> &str {
        &self.reference_id
    }
    /// # Errors
    /// Rejects changed client generation/environment and evidence older than 5s.
    pub fn ensure_current(&self, client: &PaypalClient) -> BlueprintResult<()> {
        if !Arc::ptr_eq(&self.generation, &client.generation)
            || client.scoped_environment != Some(self.observation.environment)
            || self.started_at.elapsed() >= Duration::from_secs(5)
        {
            return Err(mismatch());
        }
        Ok(())
    }
    #[cfg(feature = "postgres")]
    pub(crate) fn require_voided_empty_history(&self) -> BlueprintResult<()> {
        if !self.voided_empty_history {
            return Err(mismatch());
        }
        Ok(())
    }
    #[cfg(feature = "postgres")]
    pub(crate) fn into_observation(self) -> BoundPaypalOrderObservation {
        self.observation
    }
}
pub(crate) fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_paypal_context_mismatch",
        "PayPal resource context does not match",
    )
}
fn safe_id(value: &str) -> bool {
    !value
        .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .is_empty()
        && value.len() <= 1024
        && !value.chars().any(char::is_control)
}
// Official Orders v2 payee.account_id: ^[2-9A-HJ-NP-Z]{13}$.
fn merchant_id(value: &str) -> bool {
    value.len() == 13
        && value
            .bytes()
            .all(|b| matches!(b,b'2'..=b'9'|b'A'..=b'H'|b'J'..=b'N'|b'P'..=b'Z'))
}
impl PaypalClient {
    /// Uses only `PayPal`'s documented environment origin. No caller URL can be used
    /// as authentication evidence by this constructor.
    /// # Errors
    /// Rejects invalid credentials or HTTP transport configuration.
    pub fn for_environment(
        client_id: String,
        client_secret: String,
        environment: PaypalEnvironment,
    ) -> BlueprintResult<Self> {
        Self::with_trusted_environment_transport(
            client_id,
            client_secret,
            environment,
            ProviderHttpClient::new(environment.origin(), Duration::from_secs(15), 1024 * 1024)?,
        )
    }
    /// Explicit trusted-server proxy/fixture boundary: caller attests this immutable
    /// transport reaches the declared `PayPal` environment. This is not remote mode
    /// attestation. Never accept transport/environment from an untrusted request.
    /// # Errors
    /// Rejects invalid credentials. Legacy `with_transport`/`new` remain unprofiled.
    pub fn with_trusted_environment_transport(
        client_id: String,
        client_secret: String,
        environment: PaypalEnvironment,
        http: ProviderHttpClient,
    ) -> BlueprintResult<Self> {
        let mut client = Self::with_transport(client_id, client_secret, http)?;
        client.scoped_environment = Some(environment);
        Ok(client)
    }
    pub(crate) fn require_environment(
        &self,
        environment: PaypalEnvironment,
    ) -> BlueprintResult<()> {
        if self.scoped_environment != Some(environment) {
            return Err(mismatch());
        }
        Ok(())
    }
    /// Authenticates and reads one exact order, then verifies its single CAPTURE
    /// purchase unit's merchant ID. No email/`app_id` inference or partner headers.
    /// Environment comes from trusted constructor configuration, not the response.
    /// OAuth POST is authentication only; no capture/refund/financial POST occurs.
    /// # Errors
    /// Rejects unprofiled/wrong environment before I/O, malformed/denied auth,
    /// missing/mismatched payee/resource data, unsupported shapes and expiry.
    pub async fn verify_order_context(
        &self,
        environment: PaypalEnvironment,
        expected_merchant: &str,
        id: &str,
    ) -> BlueprintResult<VerifiedPaypalOrderContext> {
        self.require_environment(environment)?;
        if !merchant_id(expected_merchant) || !safe_id(id) {
            return Err(mismatch());
        }
        let started_at = Instant::now();
        let token = self.scoped_access_token().await?;
        if started_at.elapsed() >= Duration::from_secs(5) {
            return Err(mismatch());
        }
        let request = self
            .http
            .request(Method::GET, &["v2", "checkout", "orders", id])?
            .bearer_auth(token.as_str());
        let response = self.http.execute_json(request).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "PayPal",
                "read scoped order",
                response.status,
            ));
        }
        let (reference_id, observation) =
            verify_order(&response.body, environment, expected_merchant, id)?;
        let context = VerifiedPaypalOrderContext {
            generation: self.generation.clone(),
            voided_empty_history: voided_empty_history(&response.body),
            started_at,
            merchant_id: expected_merchant.into(),
            reference_id,
            observation,
        };
        context.ensure_current(self)?;
        Ok(context)
    }
    async fn scoped_access_token(&self) -> BlueprintResult<Zeroizing<String>> {
        let response = self
            .http
            .execute_json(
                self.http
                    .request(Method::POST, &["v1", "oauth2", "token"])?
                    .basic_auth(self.client_id.as_str(), Some(self.client_secret.as_str()))
                    .form(&[("grant_type", "client_credentials")]),
            )
            .await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "PayPal",
                "scoped auth",
                response.status,
            ));
        }
        let token = response.body["access_token"]
            .as_str()
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 8192
                    && !s.chars().any(char::is_whitespace)
                    && !s.chars().any(char::is_control)
            })
            .ok_or_else(mismatch)?;
        if response.body["token_type"] != "Bearer"
            || response.body["expires_in"].as_u64().is_none_or(|n| n == 0)
        {
            return Err(mismatch());
        }
        Ok(Zeroizing::new(token.into()))
    }
}
fn verify_order(
    order: &Value,
    environment: PaypalEnvironment,
    merchant: &str,
    id: &str,
) -> BlueprintResult<(String, BoundPaypalOrderObservation)> {
    if order["id"].as_str() != Some(id) || order["intent"] != "CAPTURE" {
        return Err(mismatch());
    }
    let units = order["purchase_units"]
        .as_array()
        .filter(|u| u.len() == 1)
        .ok_or_else(mismatch)?;
    let unit = &units[0];
    if unit["payee"]["merchant_id"].as_str() != Some(merchant) {
        return Err(mismatch());
    }
    let reference = unit["reference_id"]
        .as_str()
        .filter(|id| safe_id(id))
        .ok_or_else(mismatch)?;
    let currency = unit["amount"]["currency_code"]
        .as_str()
        .filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase()))
        .ok_or_else(mismatch)?;
    // Exact existing amountCents/100 contract; no currency conversion or net claim.
    let amount = unit["amount"]["value"]
        .as_str()
        .and_then(cents)
        .filter(|v| *v > 0)
        .ok_or_else(mismatch)?;
    let status = serde_json::from_value(order["status"].clone()).map_err(|_| mismatch())?;
    Ok((
        reference.into(),
        BoundPaypalOrderObservation {
            provider_ref: id.into(),
            status,
            amount_cents: amount,
            currency: currency.into(),
            environment,
        },
    ))
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;

// Primary Orders schema describes payments as comprehensive history. Optional
// omitted projections are not empty history. This private flag retains no payer data.
fn voided_empty_history(order: &Value) -> bool {
    order["status"] == "VOIDED"
        && ["captures", "authorizations", "refunds"].iter().all(|key| {
            order["purchase_units"][0]["payments"]
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        })
}
