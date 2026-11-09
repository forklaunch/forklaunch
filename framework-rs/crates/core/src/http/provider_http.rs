//! Bounded provider HTTP transport. Credentials are attached by typed clients;
//! redirects/retries are disabled and remote origins require TLS.
use crate::domain::BlueprintError;
use crate::domain::BlueprintResult;
use reqwest::{Client, Method, RequestBuilder, redirect::Policy};
use serde_json::Value;
use std::time::Duration;
use url::Url;
#[derive(Clone)]
pub struct ProviderHttpClient {
    client: Client,
    base: Url,
    max_bytes: usize,
}
pub struct ProviderResponse {
    pub status: u16,
    pub body: Value,
    /// Positive finite Retry-After seconds; date-form headers are not interpreted.
    pub retry_after_seconds: Option<f64>,
}
impl ProviderResponse {
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}
impl std::fmt::Debug for ProviderHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderHttpClient")
            .field("base", &self.base)
            .field("max_response_bytes", &self.max_bytes)
            .finish_non_exhaustive()
    }
}
impl ProviderHttpClient {
    /// # Errors
    /// Rejects remote plaintext, credentials, queries, fragments and invalid bounds.
    pub fn new(base: &str, timeout: Duration, max_response_bytes: usize) -> BlueprintResult<Self> {
        Self::with_local_mock_policy(base, timeout, max_response_bytes, false)
    }
    /// Explicit development-only transport to the fixed Docker gateway mock.
    /// Production callers should use `new`, which requires remote TLS.
    /// # Errors
    /// Rejects invalid timeout/response bounds or client construction failures.
    pub fn local_gateway_mock(
        timeout: Duration,
        max_response_bytes: usize,
    ) -> BlueprintResult<Self> {
        Self::with_local_mock_policy(
            "http://gateway-mock:8080",
            timeout,
            max_response_bytes,
            true,
        )
    }
    fn with_local_mock_policy(
        base: &str,
        timeout: Duration,
        max_response_bytes: usize,
        local_mock: bool,
    ) -> BlueprintResult<Self> {
        let base = Url::parse(base).map_err(|_| config_error())?;
        let loopback = base.host_str().is_some_and(|h| {
            h == "localhost"
                || h.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !(base.scheme() == "https" || (base.scheme() == "http" && (loopback || local_mock)))
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || timeout.is_zero()
            || timeout > Duration::from_secs(120)
            || max_response_bytes == 0
            || max_response_bytes > 16 * 1024 * 1024
        {
            return Err(config_error());
        }
        let client = Client::builder()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5).min(timeout))
            .timeout(timeout)
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .build()
            .map_err(|_| config_error())?;
        Ok(Self {
            client,
            base,
            max_bytes: max_response_bytes,
        })
    }
    #[cfg(feature = "instance-gateway")]
    pub(crate) async fn execute_stream(
        &self,
        request: RequestBuilder,
        max_bytes: usize,
        timeout: Duration,
    ) -> BlueprintResult<ProviderByteStream> {
        if max_bytes == 0
            || max_bytes > 16 * 1024 * 1024
            || timeout.is_zero()
            || timeout > Duration::from_secs(120)
        {
            return Err(config_error());
        }
        let deadline = std::time::Instant::now() + timeout;
        let response = request
            .timeout(timeout)
            .send()
            .await
            .map_err(|_| transport_error())?;
        let status = response.status().as_u16();
        let retry_after_seconds = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0);
        if response.status().is_success()
            && response
                .content_length()
                .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(stream_limit());
        }
        Ok(ProviderByteStream {
            response: if response.status().is_success() {
                Some(response)
            } else {
                None
            },
            status,
            retry_after_seconds,
            remaining: max_bytes,
            deadline,
        })
    }
    /// Each segment is percent-encoded independently, preventing provider IDs
    /// from changing the trusted endpoint's path, query, origin or credentials.
    /// # Errors
    /// Rejects empty/dot segments and opaque endpoint URLs.
    pub fn request(&self, method: Method, segments: &[&str]) -> BlueprintResult<RequestBuilder> {
        if segments
            .iter()
            .any(|segment| segment.is_empty() || matches!(*segment, "." | ".."))
        {
            return Err(config_error());
        }
        let mut url = self.base.clone();
        {
            let mut path = url.path_segments_mut().map_err(|()| config_error())?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
        }
        Ok(self.client.request(method, url))
    }
    /// Encodes dynamic path segments as JavaScript `encodeURIComponent` does.
    /// # Errors
    /// Rejects empty/dot segments and opaque endpoint URLs.
    pub fn request_uri_components(
        &self,
        method: Method,
        segments: &[&str],
    ) -> BlueprintResult<RequestBuilder> {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        if segments
            .iter()
            .any(|segment| segment.is_empty() || matches!(*segment, "." | ".."))
        {
            return Err(config_error());
        }
        let mut url = self.base.clone();
        let mut path = url.path().trim_end_matches('/').to_owned();
        for segment in segments {
            path.push('/');
            for byte in segment.bytes() {
                if byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
                    )
                {
                    path.push(char::from(byte));
                } else {
                    path.push('%');
                    path.push(char::from(HEX[usize::from(byte >> 4)]));
                    path.push(char::from(HEX[usize::from(byte & 15)]));
                }
            }
        }
        url.set_path(&path);
        Ok(self.client.request(method, url))
    }
    /// # Errors
    /// Rejects network failures, oversized bodies and malformed success JSON.
    /// Failure-status bodies are discarded to avoid exposing provider PII/secrets.
    pub async fn execute_json(&self, request: RequestBuilder) -> BlueprintResult<ProviderResponse> {
        self.execute(request, false, self.max_bytes, false).await
    }
    /// Like `execute_json`, but permits an empty successful body (voice hang-up).
    /// # Errors
    /// Rejects network failures, oversized bodies and nonempty malformed JSON.
    pub async fn execute_json_or_empty(
        &self,
        request: RequestBuilder,
    ) -> BlueprintResult<ProviderResponse> {
        self.execute(request, true, self.max_bytes, false).await
    }
    /// Applies a stricter response-byte limit to one JSON request.
    /// Uses browser-compatible UTF-8 decoding (replacement of malformed sequences
    /// and removal of an initial BOM), rounding unsafe integral numbers to f64.
    /// Default transport decoding, limits and
    /// error categories remain unchanged.
    /// # Errors
    /// Rejects zero/larger-than-configured limits before sending. After response
    /// headers, oversized, unreadable or malformed success bodies report
    /// `provider_invalid_response`; network send failures retain their category.
    pub async fn execute_json_with_response_limit(
        &self,
        request: RequestBuilder,
        max_response_bytes: usize,
    ) -> BlueprintResult<ProviderResponse> {
        if max_response_bytes == 0 || max_response_bytes > self.max_bytes {
            return Err(config_error());
        }
        self.execute(request, false, max_response_bytes, true).await
    }
    async fn execute(
        &self,
        request: RequestBuilder,
        allow_empty: bool,
        max_bytes: usize,
        distinguish_response: bool,
    ) -> BlueprintResult<ProviderResponse> {
        let response_error = || {
            if distinguish_response {
                BlueprintError::new(
                    "provider_invalid_response",
                    "Invalid provider HTTP response",
                )
            } else {
                transport_error()
            }
        };
        let mut response = request.send().await.map_err(|_| transport_error())?;
        let status = response.status().as_u16();
        let retry_after_seconds = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0);
        if !response.status().is_success() {
            return Ok(ProviderResponse {
                status,
                body: Value::Null,
                retry_after_seconds,
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(response_error());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| response_error())? {
            if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
                return Err(response_error());
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = if allow_empty && bytes.is_empty() {
            Value::Null
        } else if distinguish_response {
            let text = String::from_utf8_lossy(&bytes);
            let mut decoded = serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(&text))
                .map_err(|_| response_error())?;
            normalize_browser_numbers(&mut decoded);
            decoded
        } else {
            serde_json::from_slice(&bytes).map_err(|_| response_error())?
        };
        Ok(ProviderResponse {
            status,
            body,
            retry_after_seconds,
        })
    }
}
// JSON.parse represents every number as IEEE-754. Preserve safe integer storage,
// but round larger integral source values before callers observe the decoded tree.
pub(crate) fn normalize_browser_numbers(value: &mut Value) {
    match value {
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            let outside_safe_range = number
                .as_i64()
                .is_some_and(|value| value.unsigned_abs() > 9_007_199_254_740_991)
                || number
                    .as_u64()
                    .is_some_and(|value| value > 9_007_199_254_740_991);
            if outside_safe_range
                && let Some(rounded) = number.as_f64().and_then(serde_json::Number::from_f64)
            {
                *number = rounded;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_browser_numbers),
        Value::Object(fields) => fields.values_mut().for_each(normalize_browser_numbers),
        _ => {}
    }
}
#[cfg(feature = "instance-gateway")]
pub(crate) struct ProviderByteStream {
    response: Option<reqwest::Response>,
    pub(crate) status: u16,
    pub(crate) retry_after_seconds: Option<f64>,
    remaining: usize,
    deadline: std::time::Instant,
}
#[cfg(feature = "instance-gateway")]
impl ProviderByteStream {
    pub(crate) fn check_time(&self) -> BlueprintResult<()> {
        if std::time::Instant::now() >= self.deadline {
            Err(transport_error())
        } else {
            Ok(())
        }
    }
    pub(crate) async fn next_chunk(&mut self) -> BlueprintResult<Option<Vec<u8>>> {
        self.check_time()?;
        let Some(response) = &mut self.response else {
            return Ok(None);
        };
        let chunk = response.chunk().await.map_err(|_| transport_error())?;
        if let Some(chunk) = chunk {
            if chunk.len() > self.remaining {
                return Err(stream_limit());
            }
            self.remaining -= chunk.len();
            Ok(Some(chunk.to_vec()))
        } else {
            self.response = None;
            Ok(None)
        }
    }
}
#[cfg(feature = "instance-gateway")]
fn stream_limit() -> BlueprintError {
    BlueprintError::new(
        "provider_stream_limit",
        "Provider stream exceeded configured limit",
    )
}
fn config_error() -> BlueprintError {
    BlueprintError::new(
        "provider_invalid_config",
        "Invalid provider HTTP configuration",
    )
}
fn transport_error() -> BlueprintError {
    BlueprintError::new("provider_transport_error", "Provider HTTP request failed")
}
/// Sanitized provider error. No response bodies, credentials or URLs are included.
#[must_use]
pub fn provider_status_error(provider: &str, operation: &str, status: u16) -> BlueprintError {
    BlueprintError::new(
        "provider_request_failed",
        format!("{provider} {operation} failed (status {status})"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_require_tls_and_cannot_escape_with_provider_ids() {
        for base in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com?query",
            "https://example.com#fragment",
            "file:///tmp/provider",
        ] {
            assert!(ProviderHttpClient::new(base, Duration::from_secs(15), 1024).is_err());
        }
        assert!(
            ProviderHttpClient::new("http://gateway-mock:8080", Duration::from_secs(15), 1024)
                .is_err()
        );
        let local_mock =
            ProviderHttpClient::local_gateway_mock(Duration::from_secs(15), 1024).unwrap();
        assert_eq!(
            local_mock
                .request(Method::GET, &["instance-gateway", "models"])
                .unwrap()
                .build()
                .unwrap()
                .url()
                .as_str(),
            "http://gateway-mock:8080/instance-gateway/models"
        );
        let client = ProviderHttpClient::new(
            "http://127.0.0.1:1234/prefix/",
            Duration::from_secs(15),
            1024,
        )
        .unwrap();
        let request = client
            .request(Method::GET, &["orders", "id/with?query#fragment"])
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request.url().as_str(),
            "http://127.0.0.1:1234/prefix/orders/id%2Fwith%3Fquery%23fragment"
        );
        assert!(client.request(Method::GET, &[".."]).is_err());
        assert!(ProviderHttpClient::new("https://example.com", Duration::ZERO, 1024).is_err());
        assert!(
            ProviderHttpClient::new("https://example.com", Duration::from_secs(15), 0).is_err()
        );
    }
}
