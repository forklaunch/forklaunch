//! Bounded Stripe billing transport. API version matches the repository's Stripe
//! SDK 22.6.2; mutation retries are disabled to avoid duplicate provider effects.
mod context;
#[cfg(feature = "postgres-identity")]
pub(crate) mod managed_creation;
pub use context::VerifiedStripeContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;
pub const STRIPE_API_VERSION: &str = "2026-08-26.dahlia";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeResource {
    Event,
    Customer,
    Plan,
    Product,
    Price,
    Subscription,
    CheckoutSession,
    PaymentLink,
    BillingPortal,
}
impl StripeResource {
    const fn path(self) -> &'static [&'static str] {
        match self {
            Self::Event => &["v1", "events"],
            Self::Customer => &["v1", "customers"],
            Self::Plan => &["v1", "plans"],
            Self::Product => &["v1", "products"],
            Self::Price => &["v1", "prices"],
            Self::Subscription => &["v1", "subscriptions"],
            Self::CheckoutSession => &["v1", "checkout", "sessions"],
            Self::PaymentLink => &["v1", "payment_links"],
            Self::BillingPortal => &["v1", "billing_portal", "sessions"],
        }
    }
}
#[derive(Clone)]
pub struct StripeBillingClient {
    transport: BillingTransport,
    api_version: Arc<String>,
    config_generation: Arc<()>,
}
#[derive(Clone)]
enum BillingTransport {
    Direct {
        http: ProviderHttpClient,
        secret: Arc<Zeroizing<String>>,
    },
    Managed(InstanceGatewayClient),
}
impl std::fmt::Debug for BillingTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct { http, .. } => f.debug_tuple("Direct").field(http).finish(),
            Self::Managed(gateway) => f.debug_tuple("Managed").field(gateway).finish(),
        }
    }
}
impl std::fmt::Debug for StripeBillingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeBillingClient")
            .field("transport", &self.transport)
            .field("api_version", &self.api_version)
            .finish_non_exhaustive()
    }
}
impl StripeBillingClient {
    /// # Errors
    /// Rejects missing credentials and invalid transport configuration.
    pub fn new(secret: String) -> BlueprintResult<Self> {
        Self::with_transport(
            secret,
            ProviderHttpClient::new(
                "https://api.stripe.com",
                Duration::from_secs(15),
                1024 * 1024,
            )?,
        )
    }
    /// Uses a trusted HTTPS proxy or explicit loopback fixture.
    /// # Errors
    /// Rejects missing/control-containing credentials.
    pub fn with_transport(secret: String, http: ProviderHttpClient) -> BlueprintResult<Self> {
        if secret.is_empty() || secret.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(invalid_config());
        }
        Ok(Self {
            transport: BillingTransport::Direct {
                http,
                secret: Arc::new(Zeroizing::new(secret)),
            },
            api_version: Arc::new(STRIPE_API_VERSION.to_owned()),
            config_generation: Arc::new(()),
        })
    }
    /// Uses an instance-signed platform gateway without a vendor secret key.
    /// # Errors
    /// Rejects incomplete credentials and unsafe gateway configuration.
    pub fn managed(
        gateway_url: &str,
        instance_id: String,
        hmac_key: String,
    ) -> BlueprintResult<Self> {
        Ok(Self::with_gateway(InstanceGatewayClient::new(
            gateway_url,
            instance_id,
            hmac_key,
        )?))
    }
    /// Uses a previously validated gateway transport, including the explicit
    /// local Docker mock profile. No vendor key is attached.
    #[must_use]
    pub fn with_gateway(gateway: InstanceGatewayClient) -> Self {
        Self {
            transport: BillingTransport::Managed(gateway),
            api_version: Arc::new(STRIPE_API_VERSION.to_owned()),
            config_generation: Arc::new(()),
        }
    }
    /// # Errors
    /// Rejects malformed server-owned version configuration.
    pub fn with_api_version(mut self, version: String) -> BlueprintResult<Self> {
        if version.is_empty()
            || version.len() > 128
            || !version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
        {
            return Err(invalid_config());
        }
        self.api_version = Arc::new(version);
        self.config_generation = Arc::new(());
        Ok(self)
    }
    /// # Errors
    /// Returns sanitized transport/provider/response-shape errors.
    pub async fn create(&self, resource: StripeResource, fields: &Value) -> BlueprintResult<Value> {
        self.request(Method::POST, resource, None, None, fields)
            .await
    }
    /// # Errors
    /// Returns sanitized transport/provider/response-shape errors.
    pub async fn retrieve(
        &self,
        resource: StripeResource,
        id: &str,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        self.request(Method::GET, resource, Some(id), None, fields)
            .await
    }
    /// # Errors
    /// Returns sanitized transport/provider/response-shape errors.
    pub async fn update(
        &self,
        resource: StripeResource,
        id: &str,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        self.request(Method::POST, resource, Some(id), None, fields)
            .await
    }
    /// # Errors
    /// Returns sanitized transport/provider/response-shape errors.
    pub async fn delete(&self, resource: StripeResource, id: &str) -> BlueprintResult<Value> {
        self.request(
            Method::DELETE,
            resource,
            Some(id),
            None,
            &serde_json::json!({}),
        )
        .await
    }
    /// # Errors
    /// Returns sanitized errors; only declared lifecycle actions are accepted.
    pub async fn action(
        &self,
        resource: StripeResource,
        id: &str,
        action: &str,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        if !matches!(
            (resource, action),
            (StripeResource::Subscription, "resume") | (StripeResource::CheckoutSession, "expire")
        ) {
            return Err(invalid_request());
        }
        self.request(Method::POST, resource, Some(id), Some(action), fields)
            .await
    }
    /// Fetches one provider page, matching the TS billing wrappers' list calls.
    /// # Errors
    /// Returns sanitized transport/provider/response-shape errors.
    pub async fn list(&self, resource: StripeResource, fields: &Value) -> BlueprintResult<Value> {
        let response = self
            .request(Method::GET, resource, None, None, fields)
            .await?;
        if !response["data"].is_array() {
            return Err(invalid_response());
        }
        Ok(response)
    }
    /// Visits all Stripe pages with bounded work and cursor-cycle rejection.
    /// At most 100,000 objects are collected for bounded memory use.
    /// # Errors
    /// Rejects malformed pages, cursor cycles and more than 1,000 pages.
    pub async fn list_pages(
        &self,
        resource: StripeResource,
        fields: &Value,
    ) -> BlueprintResult<Vec<Value>> {
        let mut query = fields.as_object().ok_or_else(invalid_request)?.clone();
        let mut result = Vec::new();
        let mut cursors = std::collections::HashSet::new();
        for _ in 0..1000 {
            let page = self.list(resource, &Value::Object(query.clone())).await?;
            let items = page["data"].as_array().ok_or_else(invalid_response)?;
            if items.len() > 100 || result.len() + items.len() > 100_000 {
                return Err(invalid_response());
            }
            result.extend(items.iter().cloned());
            match page["has_more"].as_bool() {
                Some(false) => return Ok(result),
                Some(true) => {
                    let cursor = items
                        .last()
                        .and_then(|item| item["id"].as_str())
                        .ok_or_else(invalid_response)?;
                    validate_id(cursor).map_err(|_| invalid_response())?;
                    if !cursors.insert(cursor.to_owned()) {
                        return Err(invalid_response());
                    }
                    query.insert("starting_after".into(), Value::String(cursor.to_owned()));
                }
                None => return Err(invalid_response()),
            }
        }
        Err(invalid_response())
    }
    async fn request(
        &self,
        method: Method,
        resource: StripeResource,
        id: Option<&str>,
        action: Option<&str>,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        self.request_with_status(method, resource, id, action, fields, false)
            .await
    }
    pub(crate) async fn retrieve_configured(
        &self,
        resource: StripeResource,
        id: &str,
    ) -> BlueprintResult<Value> {
        self.request_with_status(
            Method::GET,
            resource,
            Some(id),
            None,
            &serde_json::json!({}),
            true,
        )
        .await
    }
    pub(crate) async fn update_configured(
        &self,
        resource: StripeResource,
        id: &str,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        self.request_with_status(Method::POST, resource, Some(id), None, fields, true)
            .await
    }
    async fn request_with_status(
        &self,
        method: Method,
        resource: StripeResource,
        id: Option<&str>,
        action: Option<&str>,
        fields: &Value,
        configured_status: bool,
    ) -> BlueprintResult<Value> {
        let mut segments = resource.path().to_vec();
        if let Some(id) = id {
            validate_id(id)?;
            segments.push(id);
        }
        if let Some(action) = action {
            segments.push(action);
        }
        let params = form_fields(fields)?;
        let response = match &self.transport {
            BillingTransport::Direct { http, secret } => {
                let mut request = http
                    .request(method.clone(), &segments)?
                    .bearer_auth(secret.as_str())
                    .header("stripe-version", self.api_version.as_str());
                if method == Method::GET {
                    request = request.query(&params);
                } else {
                    request = request.form(&params);
                }
                http.execute_json(request).await?
            }
            BillingTransport::Managed(gateway) => {
                let mut route = vec!["stripe"];
                route.extend_from_slice(&segments);
                gateway
                    .request_form(method.clone(), &route, &params, Some(&self.api_version))
                    .await?
            }
        };
        if !(200..300).contains(&response.status) {
            if configured_status && (400..600).contains(&response.status) {
                return Err(BlueprintError::new(
                    "billing_configured_provider_error",
                    "Billing provider request failed",
                )
                .with_detail("status", serde_json::json!(response.status)));
            }
            return Err(provider_status_error("stripe", "billing", response.status));
        }
        if !response.body.is_object() {
            return Err(invalid_response());
        }
        if id.is_some() || method != Method::GET {
            let id = response.body["id"].as_str().ok_or_else(invalid_response)?;
            validate_id(id).map_err(|_| invalid_response())?;
        }
        Ok(response.body)
    }
}
fn form_fields(value: &Value) -> BlueprintResult<Vec<(String, String)>> {
    let object = value.as_object().ok_or_else(invalid_request)?;
    let mut fields = Vec::new();
    let mut size = 0;
    for (key, value) in object {
        if key.contains(['[', ']']) {
            return Err(invalid_request());
        }
        flatten(key, value, 0, &mut fields, &mut size)?;
    }
    Ok(fields)
}
fn flatten(
    key: &str,
    value: &Value,
    depth: usize,
    fields: &mut Vec<(String, String)>,
    size: &mut usize,
) -> BlueprintResult<()> {
    if depth > 16
        || key.is_empty()
        || key.len() > 1024
        || key.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(invalid_request());
    }
    match value {
        Value::Object(object) => {
            for (child, value) in object {
                if child.contains(['[', ']']) {
                    return Err(invalid_request());
                }
                flatten(&format!("{key}[{child}]"), value, depth + 1, fields, size)?;
            }
        }
        Value::Array(values) => {
            if values.is_empty() {
                push_field(key, String::new(), fields, size)?;
            } else {
                for (index, value) in values.iter().enumerate() {
                    flatten(&format!("{key}[{index}]"), value, depth + 1, fields, size)?;
                }
            }
        }
        Value::Null => push_field(key, String::new(), fields, size)?,
        Value::String(value) => push_field(key, value.clone(), fields, size)?,
        Value::Number(number)
            if number.is_f64()
                && number.as_f64().is_some_and(|value| {
                    value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0
                }) =>
        {
            push_field(
                key,
                format!("{:.0}", number.as_f64().ok_or_else(invalid_request)?),
                fields,
                size,
            )?;
        }
        value => push_field(key, value.to_string(), fields, size)?,
    }
    Ok(())
}
fn push_field(
    key: &str,
    value: String,
    fields: &mut Vec<(String, String)>,
    size: &mut usize,
) -> BlueprintResult<()> {
    *size = size
        .checked_add(key.len())
        .and_then(|size| size.checked_add(value.len()))
        .ok_or_else(invalid_request)?;
    if fields.len() >= 1024 || *size > 1024 * 1024 {
        return Err(invalid_request());
    }
    fields.push((key.to_owned(), value));
    Ok(())
}
fn validate_id(id: &str) -> BlueprintResult<()> {
    if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        Err(invalid_request())
    } else {
        Ok(())
    }
}
fn invalid_request() -> BlueprintError {
    BlueprintError::new("billing_validation", "Invalid Stripe billing request")
}
fn invalid_config() -> BlueprintError {
    BlueprintError::new(
        "stripe_invalid_config",
        "Invalid Stripe billing configuration",
    )
}
fn invalid_response() -> BlueprintError {
    BlueprintError::new("stripe_invalid_response", "Invalid Stripe billing response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn form_bounds_and_numeric_encoding() {
        let mut fields =
            form_fields(&json!({"amount":1999.0,"metadata":{"a":null},"empty":[]})).unwrap();
        fields.sort();
        assert_eq!(
            fields,
            vec![
                ("amount".into(), "1999".into()),
                ("empty".into(), String::new()),
                ("metadata[a]".into(), String::new())
            ]
        );
        for invalid in [
            json!([]),
            json!({"items[0]":"injected"}),
            json!({"metadata":{"a[b]":"injected"}}),
            json!({"control\n":"value"}),
            json!({"large":"x".repeat(1024*1024)}),
        ] {
            assert!(form_fields(&invalid).is_err());
        }
        let mut deep = json!("leaf");
        for _ in 0..18 {
            deep = json!({"nested":deep});
        }
        assert!(form_fields(&deep).is_err());
        let many = Value::Object((0..1025).map(|i| (format!("field{i}"), json!(i))).collect());
        assert!(form_fields(&many).is_err());
        for id in ["", "bad\n"] {
            assert!(validate_id(id).is_err());
        }
        assert!(StripeBillingClient::new(String::new()).is_err());
        assert!(StripeBillingClient::new("bad\n".into()).is_err());
        assert!(
            StripeBillingClient::new("fixture".into())
                .unwrap()
                .with_api_version("bad\n".into())
                .is_err()
        );
    }
}

#[cfg(feature = "postgres-identity")]
mod plan_creation;

#[cfg(feature = "postgres-identity")]
mod customer_registration;

#[cfg(feature = "postgres-identity")]
mod subscription_creation;

#[cfg(feature = "postgres-identity")]
mod payment_link_expiration;
#[cfg(feature = "postgres-identity")]
mod portal_creation;

#[cfg(feature = "postgres-identity")]
mod checkout_creation;
