//! Fixed-endpoint trusted session verification transport. Applications own claim validation
//! and role policy; this client never accepts an endpoint or identity from request headers.
use serde::{Serialize, de::DeserializeOwned};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
#[derive(Debug)]
pub enum SessionClientError {
    InvalidConfiguration,
    Unauthorized,
    Forbidden,
    Unavailable,
}
impl std::fmt::Display for SessionClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidConfiguration => "invalid verifier configuration",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Unavailable => "authentication unavailable",
        })
    }
}
impl std::error::Error for SessionClientError {}
/// Claims received only from the configured verifier, not caller-deserialized credentials.
pub struct VerifiedClaims<T>(T);
impl<T> VerifiedClaims<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}
/// Explicit transport admission; authentication secrets require TLS outside loopback.
#[derive(Clone, Copy, Debug, Default)]
pub enum SessionTransport {
    #[default]
    TlsOrLoopback,
    /// Same as the default now that internal HTTP is accepted.
    PlaintextDevelopment,
}

/// No Debug/Serialize; the authentication transport retains a private service credential.
pub struct SessionClient {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    header: reqwest::header::HeaderName,
    secret: Zeroizing<String>,
}
impl SessionClient {
    /// Create a fixed-endpoint client with secure transport admission.
    /// # Errors
    /// Rejects invalid endpoints, credentials, headers or HTTP client configuration.
    pub fn new(
        endpoint: &str,
        secret_header: &str,
        secret: String,
    ) -> Result<Self, SessionClientError> {
        Self::with_transport(
            endpoint,
            secret_header,
            secret,
            SessionTransport::TlsOrLoopback,
        )
    }
    /// # Errors
    /// Rejects invalid endpoints. Both transports accept HTTP(S); the selector is
    /// retained for compatibility.
    pub fn with_transport(
        endpoint: &str,
        secret_header: &str,
        secret: String,
        _transport: SessionTransport,
    ) -> Result<Self, SessionClientError> {
        let endpoint =
            reqwest::Url::parse(endpoint).map_err(|_| SessionClientError::InvalidConfiguration)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || secret.len() < 32
            || secret.len() > 4096
        {
            return Err(SessionClientError::InvalidConfiguration);
        }
        let header = reqwest::header::HeaderName::from_bytes(secret_header.as_bytes())
            .map_err(|_| SessionClientError::InvalidConfiguration)?;
        reqwest::header::HeaderValue::from_str(&secret)
            .map_err(|_| SessionClientError::InvalidConfiguration)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|_| SessionClientError::Unavailable)?;
        Ok(Self {
            client,
            endpoint,
            header,
            secret: Zeroizing::new(secret),
        })
    }
    pub fn authenticate_internal(&self, supplied: &str) -> bool {
        let expected = self.secret.as_bytes();
        supplied.len() == expected.len() && bool::from(supplied.as_bytes().ct_eq(expected))
    }
    /// Verify a session against the configured identity service.
    /// # Errors
    /// Rejects invalid credentials, failed requests and oversized or malformed responses.
    pub async fn verify<Q: Serialize, T: DeserializeOwned>(
        &self,
        cookie: &str,
        request: &Q,
    ) -> Result<VerifiedClaims<T>, SessionClientError> {
        if cookie.is_empty() || cookie.len() > 16384 {
            return Err(SessionClientError::Unauthorized);
        }
        let body = Zeroizing::new(
            serde_json::to_vec(request).map_err(|_| SessionClientError::Unavailable)?,
        );
        if body.len() > 4096 {
            return Err(SessionClientError::Unavailable);
        }
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .header(self.header.clone(), self.secret.as_str())
            .header("cookie", cookie)
            .header("content-type", "application/json")
            .body(body.to_vec())
            .send()
            .await
            .map_err(|_| SessionClientError::Unavailable)?;
        match response.status().as_u16() {
            200 => {}
            401 => return Err(SessionClientError::Unauthorized),
            403 => return Err(SessionClientError::Forbidden),
            _ => return Err(SessionClientError::Unavailable),
        }
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| SessionClientError::Unavailable)?
        {
            if bytes.len() + chunk.len() > 4096 {
                return Err(SessionClientError::Unavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map(VerifiedClaims)
            .map_err(|_| SessionClientError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[test]
    fn configuration_and_private_credential() {
        assert!(
            SessionClient::new(
                "https://user:password@example.test/verify",
                "x-secret",
                "x".repeat(32)
            )
            .is_err()
        );
        assert!(
            SessionClient::new(
                "https://example.test/verify?redirect=evil",
                "x-secret",
                "x".repeat(32)
            )
            .is_err()
        );
        let client =
            SessionClient::new("https://example.test/verify", "x-secret", "x".repeat(32)).unwrap();
        assert!(client.authenticate_internal(&"x".repeat(32)));
        assert!(!client.authenticate_internal(&"y".repeat(32)));
    }
    #[test]
    fn internal_http_and_https_endpoints_are_accepted() {
        // Internal service networks (Docker, VPC) use plain HTTP.
        for endpoint in [
            "http://auth:3001/verify",
            "http://example.test/verify",
            "https://auth.example.test/verify",
            "http://127.0.0.1:3001/verify",
            "http://[::1]:3001/verify",
        ] {
            assert!(SessionClient::new(endpoint, "x-secret", "x".repeat(32)).is_ok());
        }
        for endpoint in [
            "ftp://auth:3001/verify",
            "http://user:pass@auth:3001/verify",
            "http://auth:3001/verify?token=x",
            "http://auth:3001/verify#fragment",
        ] {
            assert!(SessionClient::new(endpoint, "x-secret", "x".repeat(32)).is_err());
        }
    }
    #[tokio::test]
    async fn bounded_fixed_endpoint_verification() {
        for (status, body) in [
            (200, "{\"userId\":\"verified\"}".to_owned()),
            (401, String::new()),
            (403, String::new()),
            (302, String::new()),
            (200, "x".repeat(4097)),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/verify", listener.local_addr().unwrap());
            let expected = body.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 8192];
                let n = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                assert!(request.starts_with("POST /verify "));
                assert!(request.contains("cookie: session=proof"));
                assert!(request.contains(&format!("x-secret: {}", "x".repeat(32))));
                socket.write_all(format!("HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{expected}",expected.len()).as_bytes()).await.unwrap();
            });
            let client = SessionClient::new(&endpoint, "x-secret", "x".repeat(32)).unwrap();
            let response = client
                .verify::<_, serde_json::Value>(
                    "session=proof",
                    &serde_json::json!({"workspace":"selected"}),
                )
                .await;
            if status == 200 && body.len() < 4096 {
                assert_eq!(response.unwrap().into_inner()["userId"], "verified");
            } else {
                assert!(response.is_err());
            }
            server.await.unwrap();
        }
    }
}
