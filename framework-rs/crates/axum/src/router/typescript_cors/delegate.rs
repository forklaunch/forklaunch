//! Awaited origin selection, called once before CORS or route dispatch.
use super::TypeScriptCorsOrigin;
use axum::http::HeaderValue;
use std::{fmt, future::Future, pin::Pin, sync::Arc};

/// Static origin result returned by a trusted callback.
#[derive(Debug, Clone)]
pub enum TypeScriptCorsOriginDecision {
    Any,
    Fixed(HeaderValue),
    Reflect,
    AllowList(Vec<HeaderValue>),
    Disabled,
}
impl From<TypeScriptCorsOriginDecision> for TypeScriptCorsOrigin {
    fn from(value: TypeScriptCorsOriginDecision) -> Self {
        match value {
            TypeScriptCorsOriginDecision::Any => Self::Any,
            TypeScriptCorsOriginDecision::Fixed(value) => Self::Fixed(value),
            TypeScriptCorsOriginDecision::Reflect => Self::Reflect,
            TypeScriptCorsOriginDecision::AllowList(values) => Self::AllowList(values),
            TypeScriptCorsOriginDecision::Disabled => Self::Disabled,
        }
    }
}
/// An explicit callback failure. The HTTP response never includes callback data.
#[derive(Debug, Clone, Copy)]
pub struct TypeScriptCorsOriginError;
type OriginFuture = Pin<
    Box<
        dyn Future<Output = Result<TypeScriptCorsOriginDecision, TypeScriptCorsOriginError>> + Send,
    >,
>;
/// Trusted callback receiving the joined Origin header, or None if absent.
/// An empty supplied header remains Some(empty). Header bytes preserve Latin-1.
/// Pending work is awaited once; cancellation drops its future before dispatch.
/// Panics, blocking callbacks and multiple JavaScript callback invocations are
/// outside this typed single-result interface.
#[derive(Clone)]
pub struct TypeScriptCorsOriginDelegate(
    Arc<dyn Fn(Option<HeaderValue>) -> OriginFuture + Send + Sync>,
);
impl fmt::Debug for TypeScriptCorsOriginDelegate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TypeScriptCorsOriginDelegate")
    }
}
impl TypeScriptCorsOriginDelegate {
    #[must_use]
    pub fn new<F, Fut>(callback: F) -> Self
    where
        F: Fn(Option<HeaderValue>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TypeScriptCorsOriginDecision, TypeScriptCorsOriginError>>
            + Send
            + 'static,
    {
        Self(Arc::new(move |origin| Box::pin(callback(origin))))
    }
    pub(super) async fn resolve(
        &self,
        origin: Option<HeaderValue>,
    ) -> Result<TypeScriptCorsOrigin, TypeScriptCorsOriginError> {
        (self.0)(origin).await.map(Into::into)
    }
}
