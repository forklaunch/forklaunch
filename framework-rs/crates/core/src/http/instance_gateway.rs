//! Instance-signed platform transport compatible with the TypeScript gateway.
//! Credentials belong to trusted server configuration. Vendor keys never enter
//! this transport. Redirects and automatic mutation retries remain disabled.
use crate::domain::BlueprintError;
use crate::domain::BlueprintResult;
use crate::http::provider_http::ProviderHttpClient;
use crate::http::provider_http::ProviderResponse;
use forklaunch_core::auth::request_hmac::{SignedBody, generate_hmac_auth_header};
use reqwest::{Method, RequestBuilder};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;
#[derive(Clone)]
pub struct InstanceGatewayClient {
    http: ProviderHttpClient,
    instance_id: Arc<String>,
    secret: Arc<Zeroizing<String>>,
    mount_prefix: Arc<String>,
}
impl std::fmt::Debug for InstanceGatewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceGatewayClient")
            .field("http", &self.http)
            .finish_non_exhaustive()
    }
}
impl InstanceGatewayClient {
    /// Authenticated instance identity from trusted transport configuration.
    #[must_use]
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// # Errors
    /// Rejects invalid credentials, remote plaintext and unsafe gateway URLs.
    pub fn new(gateway_url: &str, instance_id: String, hmac_key: String) -> BlueprintResult<Self> {
        let http = ProviderHttpClient::new(
            gateway_url.trim_end_matches('/'),
            Duration::from_secs(15),
            1024 * 1024,
        )?;
        Self::with_http(instance_id, hmac_key, http)
    }
    /// Explicit development profile for the fixed `http://gateway-mock:8080`
    /// Docker service. Other remote plaintext origins remain rejected.
    /// # Errors
    /// Rejects invalid instance credentials or transport configuration.
    pub fn local_mock(instance_id: String, hmac_key: String) -> BlueprintResult<Self> {
        Self::with_http(
            instance_id,
            hmac_key,
            ProviderHttpClient::local_gateway_mock(Duration::from_secs(15), 1024 * 1024)?,
        )
    }
    /// Resolves trusted managed settings, with an explicit local-mock opt-in.
    /// `FORKLAUNCH_LOCAL_GATEWAY_MOCK=true` permits only the fixed Docker mock;
    /// all other gateway URLs retain the normal TLS/loopback policy.
    /// # Errors
    /// Rejects missing settings, invalid credentials/URLs or invalid opt-in values.
    pub fn from_env() -> BlueprintResult<Self> {
        fn setting(name: &str) -> BlueprintResult<String> {
            std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .ok_or_else(invalid)
        }
        let gateway_url = setting("PLATFORM_GATEWAY_URL")?;
        let instance_id = setting("INSTANCE_ID")?;
        let hmac_key = setting("INSTANCE_HMAC_KEY")?;
        let local_mock = match std::env::var("FORKLAUNCH_LOCAL_GATEWAY_MOCK").as_deref() {
            Ok("true" | "1") => true,
            Ok("" | "false" | "0") | Err(std::env::VarError::NotPresent) => false,
            _ => return Err(invalid()),
        };
        if local_mock && gateway_url.trim_end_matches('/') == "http://gateway-mock:8080" {
            Self::local_mock(instance_id, hmac_key)
        } else {
            Self::new(&gateway_url, instance_id, hmac_key)
        }
    }
    fn with_http(
        instance_id: String,
        hmac_key: String,
        http: ProviderHttpClient,
    ) -> BlueprintResult<Self> {
        if instance_id.is_empty()
            || instance_id.len() > 256
            || instance_id
                .chars()
                .any(|v| v.is_control() || v.is_whitespace())
            || hmac_key.is_empty()
            || hmac_key.len() > 4096
            || hmac_key.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let request = http
            .request(Method::GET, &["instance-gateway", "_"])?
            .build()
            .map_err(|_| invalid())?;
        let mount_prefix = request
            .url()
            .path()
            .strip_suffix("/_")
            .ok_or_else(invalid)?
            .to_owned();
        Ok(Self {
            http,
            instance_id: Arc::new(instance_id),
            secret: Arc::new(Zeroizing::new(hmac_key)),
            mount_prefix: Arc::new(mount_prefix),
        })
    }
    /// Sends a JSON capability request, binding the router-relative path and
    /// the exact serialized body, as `createInstanceGatewayTransport` does.
    /// # Errors
    /// Returns sanitized transport/configuration errors or a bounded response.
    pub async fn request_json(
        &self,
        method: Method,
        route: &[&str],
        body: Option<&Value>,
    ) -> BlueprintResult<ProviderResponse> {
        let mut request = self.request(method, route)?;
        if let Some(body) = body {
            request = request.json(body);
        }
        self.execute(request, false, false, None).await
    }
    /// Sends JSON with a stricter per-request raw response-byte bound.
    /// # Errors
    /// Rejects invalid limits before sending and preserves sanitized refusal
    /// status/retry metadata. Invalid success bodies use `provider_invalid_response`.
    pub async fn request_json_with_response_limit(
        &self,
        method: Method,
        route: &[&str],
        body: Option<&Value>,
        max_response_bytes: usize,
    ) -> BlueprintResult<ProviderResponse> {
        let mut request = self.request(method, route)?;
        if let Some(body) = body {
            request = request.json(body);
        }
        self.execute(request, false, false, Some(max_response_bytes))
            .await
    }
    /// Sends a provider form request. Stripe gateway signatures bind the full
    /// router-relative query as well as exact form bytes; empty forms are absent.
    /// # Errors
    /// Returns sanitized transport/configuration errors or a bounded response.
    pub async fn request_form(
        &self,
        method: Method,
        route: &[&str],
        fields: &[(String, String)],
        version: Option<&str>,
    ) -> BlueprintResult<ProviderResponse> {
        let mut request = self.request(method.clone(), route)?;
        if method == Method::GET {
            request = request.query(fields);
        } else {
            request = request.form(fields);
        }
        if let Some(version) = version {
            request = request.header("stripe-version", version);
        }
        self.execute(request, true, false, None).await
    }
    /// Sends a bodyless capability request allowing an empty success response.
    /// # Errors
    /// Returns sanitized errors or a bounded response; nonempty bodies must be JSON.
    pub async fn request_json_or_empty(
        &self,
        method: Method,
        route: &[&str],
    ) -> BlueprintResult<ProviderResponse> {
        self.execute(self.request(method, route)?, false, true, None)
            .await
    }
    fn request(&self, method: Method, route: &[&str]) -> BlueprintResult<RequestBuilder> {
        if route.is_empty() || route.len() > 64 {
            return Err(invalid());
        }
        let mut segments = vec!["instance-gateway"];
        segments.extend_from_slice(route);
        self.http.request_uri_components(method, &segments)
    }
    pub(crate) async fn request_stream(
        &self,
        route: &[&str],
        body: &Value,
        max_bytes: usize,
        timeout: Duration,
    ) -> BlueprintResult<crate::http::provider_http::ProviderByteStream> {
        let request = self.request(Method::POST, route)?.json(body);
        let request = self.signed_request(request, false)?;
        self.http.execute_stream(request, max_bytes, timeout).await
    }
    fn signed_request(
        &self,
        request: RequestBuilder,
        bind_query: bool,
    ) -> BlueprintResult<RequestBuilder> {
        let wire = request
            .try_clone()
            .ok_or_else(invalid)?
            .build()
            .map_err(|_| invalid())?;
        let mut path = wire
            .url()
            .path()
            .strip_prefix(self.mount_prefix.as_str())
            .ok_or_else(invalid)?
            .to_owned();
        if bind_query && let Some(query) = wire.url().query() {
            path.push('?');
            path.push_str(query);
        }
        let bytes = wire
            .body()
            .map(|body| body.as_bytes().ok_or_else(invalid))
            .transpose()?
            .unwrap_or_default();
        if bytes.len() > 1024 * 1024 || path.len() > 1024 * 1024 {
            return Err(invalid());
        }
        let body = if bytes.is_empty() {
            SignedBody::Absent
        } else {
            SignedBody::Bytes(bytes)
        };
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let authorization = generate_hmac_auth_header(
            self.secret.as_bytes(),
            &self.instance_id,
            wire.method().as_str(),
            &path,
            body,
            &timestamp,
            &uuid::Uuid::new_v4().to_string(),
        )
        .map_err(|_| invalid())?;
        Ok(request.header("authorization", authorization))
    }
    async fn execute(
        &self,
        request: RequestBuilder,
        bind_query: bool,
        allow_empty: bool,
        max_response_bytes: Option<usize>,
    ) -> BlueprintResult<ProviderResponse> {
        let request = self.signed_request(request, bind_query)?;
        if let Some(limit) = max_response_bytes {
            self.http
                .execute_json_with_response_limit(request, limit)
                .await
        } else if allow_empty {
            self.http.execute_json_or_empty(request).await
        } else {
            self.http.execute_json(request).await
        }
    }
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "instance_gateway_configuration",
        "Invalid instance gateway configuration or request",
    )
}

/// Sanitized platform refusal/transport/typed-response failure.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceGatewayRequestError {
    pub code: String,
    pub status: Option<u16>,
    pub retry_after_seconds: Option<f64>,
}
impl std::fmt::Display for InstanceGatewayRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Instance gateway request failed ({})", self.code)?;
        if let Some(status) = self.status {
            write!(f, " (status {status})")?;
        }
        Ok(())
    }
}
impl std::error::Error for InstanceGatewayRequestError {}
impl From<BlueprintError> for InstanceGatewayRequestError {
    fn from(error: BlueprintError) -> Self {
        Self {
            code: error.code,
            status: None,
            retry_after_seconds: None,
        }
    }
}
impl InstanceGatewayRequestError {
    #[must_use]
    pub fn validation(code: &str) -> Self {
        Self {
            code: code.into(),
            status: None,
            retry_after_seconds: None,
        }
    }
}
impl InstanceGatewayClient {
    /// # Errors
    /// Preserves failure status/retry hints and validates the success DTO without
    /// exposing remote error text. No automatic retry is performed.
    pub async fn json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        route: &[&str],
        body: Option<&Value>,
    ) -> Result<T, InstanceGatewayRequestError> {
        let response = self.request_json(method, route, body).await?;
        if !response.is_success() {
            return Err(InstanceGatewayRequestError {
                code: "instance_gateway_request_failed".into(),
                status: Some(response.status),
                retry_after_seconds: response.retry_after_seconds,
            });
        }
        serde_json::from_value(response.body).map_err(|_| {
            InstanceGatewayRequestError::validation("instance_gateway_invalid_response")
        })
    }
}
