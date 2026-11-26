use forklaunch_common::{FrameworkError, FrameworkResult};
use oauth2::{AsyncHttpClient, HttpRequest, HttpResponse, url::Url};
use std::{future::Future, pin::Pin, time::Duration};

const LIMIT: usize = 1024 * 1024;

pub(super) struct Transport(reqwest::Client);
impl Transport {
    pub(super) fn new() -> FrameworkResult<Self> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map(Self)
            .map_err(|_| failed())
    }
}
impl<'a> AsyncHttpClient<'a> for Transport {
    type Error = FrameworkError;
    type Future = Pin<Box<dyn Future<Output = FrameworkResult<HttpResponse>> + Send + 'a>>;
    fn call(&'a self, request: HttpRequest) -> Self::Future {
        Box::pin(async move {
            let url = endpoint(&request.uri().to_string())?;
            let (parts, body) = request.into_parts();
            let mut response = self
                .0
                .request(parts.method, url)
                .headers(parts.headers)
                .body(body)
                .send()
                .await
                .map_err(|_| failed())?;
            if response
                .content_length()
                .is_some_and(|length| length > LIMIT as u64)
            {
                return Err(failed());
            }
            let mut result = oauth2::http::Response::builder().status(response.status());
            *result.headers_mut().ok_or_else(failed)? = response.headers().clone();
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| failed())? {
                if chunk.len() > LIMIT.saturating_sub(body.len()) {
                    return Err(failed());
                }
                body.extend_from_slice(&chunk);
            }
            result.body(body).map_err(|_| failed())
        })
    }
}
pub(super) fn endpoint(value: &str) -> FrameworkResult<Url> {
    let url = Url::parse(value).map_err(|_| failed())?;
    let loopback = match url.host() {
        Some(oauth2::url::Host::Domain(host)) => host == "localhost",
        Some(oauth2::url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(oauth2::url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if !(url.scheme() == "https" || url.scheme() == "http" && loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(failed());
    }
    Ok(url)
}
fn failed() -> FrameworkError {
    FrameworkError::new("oidc_transport_failed", "OIDC provider request failed")
}
