use crate::{MultipartForm, NoMultipart, SdkError, SdkRequest};
use std::{collections::BTreeMap, fmt, sync::Arc};

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::NoMultipart {}
    impl Sealed for super::MultipartForm {}
}

/// Supported multipart payloads. Nonmultipart requests retain Clone/Eq across feature unification.
pub trait MultipartPayload: sealed::Sealed {
    #[doc(hidden)]
    fn attach(self, builder: reqwest::RequestBuilder) -> (reqwest::RequestBuilder, Option<String>);
}
impl MultipartPayload for NoMultipart {
    fn attach(self, builder: reqwest::RequestBuilder) -> (reqwest::RequestBuilder, Option<String>) {
        (builder, None)
    }
}
impl MultipartPayload for MultipartForm {
    fn attach(self, builder: reqwest::RequestBuilder) -> (reqwest::RequestBuilder, Option<String>) {
        let boundary = self.boundary().to_owned();
        (builder.multipart(self), Some(boundary))
    }
}

#[derive(Clone)]
pub enum SdkAuth {
    Bearer(String),
    Basic {
        username: String,
        password: String,
    },
    /// A fixed header value. A request HMAC credential covers one request's
    /// method, path, body, timestamp and nonce, so a fixed value verifies at most
    /// once. Prefer [`SdkAuth::Hmac`] for service-to-service calls.
    HmacHeader {
        header_name: String,
        value: String,
    },
    /// Sign every request with the request HMAC (see [`HmacSigner`]).
    Hmac(HmacSigner),
}

/// Per-request HMAC signing, compatible with the TypeScript
/// `createHmacToken` / `generateHmacAuthHeaders` profile and the Rust verifiers:
/// `HMAC keyId=<id> ts=<ISO-8601 ms> nonce=<random> signature=<base64>` over
/// `METHOD\n<path>\n<body + "\n" | "undefined"><ts>\n<nonce>`.
///
/// The signed path is the final request URL's path, without the query string,
/// exactly as sent on the wire (`POST /payment-link`, not a router-relative `/`).
/// The body is the exact bytes sent; an absent or empty body signs as absent.
/// Each call gets a fresh timestamp and a random nonce, so servers that claim
/// nonces reject replays. The secret is never formatted.
#[derive(Clone)]
pub struct HmacSigner {
    key_id: String,
    secret: Arc<[u8]>,
}
impl fmt::Debug for HmacSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HmacSigner")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}
impl HmacSigner {
    /// A signer for the server's `default` key.
    #[must_use]
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self::with_key_id("default", secret)
    }
    /// A signer for a named server key (`keyId=<key_id>`).
    #[must_use]
    pub fn with_key_id(key_id: impl Into<String>, secret: impl Into<Vec<u8>>) -> Self {
        Self {
            key_id: key_id.into(),
            secret: secret.into().into(),
        }
    }
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    /// The `authorization` header value for one request, with a fresh timestamp and nonce.
    /// # Errors
    /// Rejects empty secrets, invalid key IDs and newline-injected methods or paths.
    pub fn sign(&self, method: &str, path: &str, body: Option<&[u8]>) -> Result<String, SdkError> {
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        self.sign_at(method, path, body, &timestamp, &nonce)
    }
    /// Deterministic signing for tests and fixtures. Never reuse a nonce for real traffic.
    /// # Errors
    /// Rejects empty secrets, invalid key IDs/timestamps and newline-injected inputs.
    pub fn sign_at(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        timestamp: &str,
        nonce: &str,
    ) -> Result<String, SdkError> {
        use forklaunch_core::auth::request_hmac::{SignedBody, generate_hmac_auth_header};
        let body = match body {
            Some(bytes) if !bytes.is_empty() => SignedBody::Bytes(bytes),
            _ => SignedBody::Absent,
        };
        generate_hmac_auth_header(
            &self.secret,
            &self.key_id,
            method,
            path,
            body,
            timestamp,
            nonce,
        )
        .map_err(|_| SdkError::Contract("invalid HMAC signing input"))
    }
}

type RequestHook<M> = Arc<dyn Fn(&mut SdkRequest<M>) + Send + Sync>;

/// Shared transport configuration used by generated, statically typed service clients.
pub struct UniversalSdk<M = NoMultipart> {
    base_url: String,
    allow_plaintext_development: bool,
    max_multipart_response_bytes: usize,
    max_sse_event_bytes: usize,
    http: reqwest::Client,
    default_headers: BTreeMap<String, String>,
    auth: Option<SdkAuth>,
    request_hook: Option<RequestHook<M>>,
}
impl<M> Clone for UniversalSdk<M> {
    fn clone(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            allow_plaintext_development: self.allow_plaintext_development,
            max_multipart_response_bytes: self.max_multipart_response_bytes,
            max_sse_event_bytes: self.max_sse_event_bytes,
            http: self.http.clone(),
            default_headers: self.default_headers.clone(),
            auth: self.auth.clone(),
            request_hook: self.request_hook.clone(),
        }
    }
}
impl<M> fmt::Debug for UniversalSdk<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UniversalSdk")
            .field("has_base_url", &!self.base_url.is_empty())
            .field(
                "allow_plaintext_development",
                &self.allow_plaintext_development,
            )
            .field(
                "default_header_names",
                &self.default_headers.keys().collect::<Vec<_>>(),
            )
            .field("has_auth", &self.auth.is_some())
            .field("has_request_hook", &self.request_hook.is_some())
            .finish_non_exhaustive()
    }
}
impl<M: MultipartPayload> UniversalSdk<M> {
    /// # Panics
    /// Panics if the platform TLS transport cannot be initialized.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_http_client(
            base_url,
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("valid SDK transport configuration"),
        )
    }
    /// Custom transports must enforce TLS verification and disable credential-bearing redirects.
    #[must_use]
    pub fn with_http_client(base_url: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            allow_plaintext_development: false,
            max_multipart_response_bytes: 8_388_608,
            max_sse_event_bytes: 1_048_576,
            http,
            default_headers: BTreeMap::new(),
            auth: None,
            request_hook: None,
        }
    }
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    #[must_use]
    pub const fn multipart_response_limit(&self) -> usize {
        self.max_multipart_response_bytes
    }
    #[must_use]
    pub const fn sse_event_limit(&self) -> usize {
        self.max_sse_event_bytes
    }
    /// Retained for compatibility: plain HTTP is accepted for every host.
    #[must_use]
    pub fn with_plaintext_development_transport(mut self) -> Self {
        self.allow_plaintext_development = true;
        self
    }
    /// # Errors
    /// Rejects a zero byte limit.
    pub fn with_multipart_response_limit(mut self, max_bytes: usize) -> Result<Self, SdkError> {
        if max_bytes == 0 {
            return Err(SdkError::Multipart("Invalid multipart response limit"));
        }
        self.max_multipart_response_bytes = max_bytes;
        Ok(self)
    }
    /// # Errors
    /// Rejects a zero event limit.
    pub fn with_sse_event_limit(mut self, max_event_bytes: usize) -> Result<Self, SdkError> {
        if max_event_bytes == 0 {
            return Err(crate::sse::SseError::InvalidLimit.into());
        }
        self.max_sse_event_bytes = max_event_bytes;
        Ok(self)
    }
    #[must_use]
    pub fn with_default_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.default_headers.insert(name.into(), value.into());
        self
    }
    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(SdkAuth::Bearer(token.into()));
        self
    }
    #[must_use]
    pub fn with_jwt(self, token: impl Into<String>) -> Self {
        self.with_bearer_token(token)
    }
    #[must_use]
    pub fn with_basic_auth(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.auth = Some(SdkAuth::Basic {
            username: username.into(),
            password: password.into(),
        });
        self
    }
    /// Send one fixed `authorization` value. A request HMAC credential is valid
    /// for a single request; use [`Self::with_hmac_secret`] to sign every call.
    #[must_use]
    pub fn with_hmac_authorization(mut self, value: impl Into<String>) -> Self {
        self.auth = Some(SdkAuth::HmacHeader {
            header_name: "authorization".to_owned(),
            value: value.into(),
        });
        self
    }
    /// Sign every request with the request HMAC under the `default` key.
    /// Signing happens last, over the final method, URL path and body bytes,
    /// after defaults and the request hook have run.
    #[must_use]
    pub fn with_hmac_secret(self, secret: impl Into<Vec<u8>>) -> Self {
        self.with_hmac_signer(HmacSigner::new(secret))
    }
    /// Sign every request with the given [`HmacSigner`] (for a named key ID).
    #[must_use]
    pub fn with_hmac_signer(mut self, signer: HmacSigner) -> Self {
        self.auth = Some(SdkAuth::Hmac(signer));
        self
    }
    #[must_use]
    pub fn with_auth_hook<F>(mut self, hook: F) -> Self
    where
        F: Fn(&mut SdkRequest<M>) + Send + Sync + 'static,
    {
        self.request_hook = Some(Arc::new(hook));
        self
    }
    /// Execute a prepared request after merging defaults and applying the explicit request hook.
    /// # Errors
    /// Denies unsafe URLs and invalid methods, and returns redacted transport failures.
    pub async fn execute(&self, request: SdkRequest<M>) -> Result<reqwest::Response, SdkError> {
        self.execute_inner(request, None, None)
            .await
            .map(|(response, _)| response)
    }
    /// Execute a versioned call with admission based on the actual post-hook wire request.
    /// # Errors
    /// Rejects unsupported predicates, unmatched requests and transport failures.
    pub async fn execute_versioned(
        &self,
        request: SdkRequest<M>,
        metadata: &str,
    ) -> Result<(reqwest::Response, crate::MatchedVersionResponses), SdkError> {
        let (response, matched) = self.execute_inner(request, Some(metadata), None).await?;
        Ok((
            response,
            matched.ok_or(SdkError::Contract("missing version matching state"))?,
        ))
    }
    /// Execute after validating whole-object field constraints on actual wire inputs.
    /// # Errors
    /// Rejects field constraints and transport failures after applying request hooks.
    pub async fn execute_with_fields(
        &self,
        request: SdkRequest<M>,
        fields: &str,
    ) -> Result<reqwest::Response, SdkError> {
        self.execute_inner(request, None, Some(fields))
            .await
            .map(|(response, _)| response)
    }
    /// Execute version matching and whole-object request constraints together.
    /// # Errors
    /// Rejects field constraints, unmatched versions and transport failures.
    pub async fn execute_versioned_with_fields(
        &self,
        request: SdkRequest<M>,
        metadata: &str,
        fields: &str,
    ) -> Result<(reqwest::Response, crate::MatchedVersionResponses), SdkError> {
        let (response, matched) = self
            .execute_inner(request, Some(metadata), Some(fields))
            .await?;
        Ok((
            response,
            matched.ok_or(SdkError::Contract("missing version matching state"))?,
        ))
    }
    async fn execute_inner(
        &self,
        mut request: SdkRequest<M>,
        metadata: Option<&str>,
        fields: Option<&str>,
    ) -> Result<(reqwest::Response, Option<crate::MatchedVersionResponses>), SdkError> {
        let mut headers = self.default_headers.clone();
        headers.extend(request.headers);
        request.headers = headers;
        if let Some(hook) = &self.request_hook {
            hook(&mut request);
        }
        let endpoint = reqwest::Url::parse(&request.url).map_err(|_| SdkError::TransportPolicy)?;
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
            || !matches!(endpoint.scheme(), "https" | "http")
        {
            return Err(SdkError::TransportPolicy);
        }
        let method = request
            .method
            .parse::<reqwest::Method>()
            .map_err(|_| SdkError::TransportPolicy)?;
        let signer = match &self.auth {
            Some(SdkAuth::Hmac(signer)) => {
                // The signature covers the final header; a caller value would be replaced.
                request
                    .headers
                    .retain(|name, _| !name.eq_ignore_ascii_case("authorization"));
                Some(signer)
            }
            _ => None,
        };
        let mut builder = self.http.request(method, &request.url);
        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        for (name, value) in request.headers {
            if request.multipart.is_some() && name.eq_ignore_ascii_case("content-type") {
                continue;
            }
            builder = builder.header(name, value);
        }
        if let Some(auth) = &self.auth {
            builder = match auth {
                SdkAuth::Bearer(token) => builder.bearer_auth(token),
                SdkAuth::Basic { username, password } => {
                    builder.basic_auth(username.as_str(), Some(password.as_str()))
                }
                SdkAuth::HmacHeader { header_name, value } => {
                    builder.header(header_name.as_str(), value.as_str())
                }
                SdkAuth::Hmac(_) => builder,
            };
        }
        if let Some(body) = request.body {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, request.content_type)
                .body(body);
        }
        if let Some(body) = request.body_bytes {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, request.content_type)
                .body(body);
        }
        let mut boundary = None;
        if let Some(form) = request.multipart {
            (builder, boundary) = form.attach(builder);
        }
        let mut prepared = builder.build()?;
        // Empty multipart bodies still need the MIME closing delimiter.
        if let Some(boundary) = boundary.filter(|_| {
            prepared
                .body()
                .and_then(reqwest::Body::as_bytes)
                .is_some_and(<[u8]>::is_empty)
        }) {
            let closing = format!("--{boundary}--\r\n");
            prepared.headers_mut().insert(
                reqwest::header::CONTENT_LENGTH,
                reqwest::header::HeaderValue::from(closing.len()),
            );
            *prepared.body_mut() = Some(closing.into());
        }
        if let Some(fields) = fields {
            crate::request_objects::validate(&prepared, fields)?;
        }
        if metadata.is_some() {
            buffer_versioned_body(&mut prepared, std::time::Duration::from_secs(30)).await?;
        }
        let matched = metadata
            .map(|metadata| {
                crate::MatchedVersionResponses::from_prepared(&prepared, metadata, &self.base_url)
            })
            .transpose()?;
        if let Some(signer) = signer {
            sign_prepared(&mut prepared, signer).await?;
        }
        self.http
            .execute(prepared)
            .await
            .map(|response| (response, matched))
            .map_err(SdkError::from)
    }
}

// Sign the exact wire request: method, URL path (no query) and the transmitted bytes.
async fn sign_prepared(
    request: &mut reqwest::Request,
    signer: &HmacSigner,
) -> Result<(), SdkError> {
    // Streaming bodies (multipart) are buffered so the signed and sent bytes match.
    buffer_versioned_body(request, std::time::Duration::from_secs(30)).await?;
    let header = signer.sign(
        request.method().as_str(),
        request.url().path(),
        request.body().and_then(reqwest::Body::as_bytes),
    )?;
    request.headers_mut().insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&header)
            .map_err(|_| SdkError::Contract("invalid HMAC header"))?,
    );
    Ok(())
}

// Match and transmit the same bounded bytes, including multipart serialization.
async fn buffer_versioned_body(
    request: &mut reqwest::Request,
    timeout: std::time::Duration,
) -> Result<(), SdkError> {
    use http_body_util::BodyExt as _;
    if request.body().is_none_or(|body| body.as_bytes().is_some()) {
        return Ok(());
    }
    let body = request
        .body_mut()
        .take()
        .ok_or(SdkError::Contract("missing version body"))?;
    let collected = tokio::time::timeout(
        timeout,
        http_body_util::Limited::new(body, 2 * 1024 * 1024).collect(),
    )
    .await
    .map_err(|_| SdkError::Contract("version body buffering timed out"))?
    .map_err(|_| SdkError::Contract("version body buffering failed or exceeded limit"))?;
    if collected.trailers().is_some() {
        return Err(SdkError::Contract("version body trailers are unsupported"));
    }
    let bytes = collected.to_bytes();
    request
        .headers_mut()
        .remove(reqwest::header::TRANSFER_ENCODING);
    request.headers_mut().insert(
        reqwest::header::CONTENT_LENGTH,
        reqwest::header::HeaderValue::from(bytes.len()),
    );
    *request.body_mut() = Some(bytes.into());
    Ok(())
}

#[cfg(test)]
mod version_buffer_tests {
    use super::buffer_versioned_body;
    use std::time::Duration;

    fn request(body: reqwest::Body) -> reqwest::Request {
        let mut request = reqwest::Request::new(
            reqwest::Method::POST,
            "https://example.invalid/upload".parse().unwrap(),
        );
        *request.body_mut() = Some(body);
        request
    }

    #[tokio::test]
    async fn buffers_exact_stream_bytes_and_normalizes_framing() {
        let stream = futures_util::stream::iter([
            Ok::<_, std::io::Error>(vec![0, 255]),
            Ok(vec![13, 10, 42]),
        ]);
        let mut request = request(reqwest::Body::wrap_stream(stream));
        request
            .headers_mut()
            .insert("transfer-encoding", "chunked".parse().unwrap());
        buffer_versioned_body(&mut request, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(
            request.body().unwrap().as_bytes().unwrap(),
            &[0, 255, 13, 10, 42]
        );
        assert_eq!(request.headers()["content-length"], "5");
        assert!(!request.headers().contains_key("transfer-encoding"));
    }

    #[tokio::test]
    async fn fails_closed_for_oversized_stalled_and_failed_streams() {
        let cases: Vec<reqwest::Body> = vec![
            reqwest::Body::wrap_stream(futures_util::stream::iter([Ok::<_, std::io::Error>(
                vec![0; 2 * 1024 * 1024 + 1],
            )])),
            reqwest::Body::wrap_stream(futures_util::stream::pending::<
                Result<Vec<u8>, std::io::Error>,
            >()),
            reqwest::Body::wrap_stream(futures_util::stream::iter([Err::<Vec<u8>, _>(
                std::io::Error::other("SECRET_BODY_CANARY"),
            )])),
        ];
        for body in cases {
            let mut request = request(body);
            let error = buffer_versioned_body(&mut request, Duration::from_millis(10))
                .await
                .unwrap_err();
            assert!(!format!("{error:?}").contains("SECRET_BODY_CANARY"));
            assert!(request.body().is_none());
        }
    }
}
