//! Typed successful whole-request field projections; wire request stays intact.
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use forklaunch_core::http::ParsedRequestFields;
use serde::de::DeserializeOwned;

/// Decode accepted path parameters. Requires a successful whole-request callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedParams<T>(pub T);
/// Decode accepted query values. Raw query remains available on the request URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQuery<T>(pub T);
/// Decode projected headers. Native credential extraction still uses wire headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHeaders<T>(pub T);
macro_rules! extractor {
    ($name:ident,$field:ident) => {
        impl<T: DeserializeOwned + Send> $name<T> {
            fn from_verified_parts(parts: &Parts) -> Result<Self, (StatusCode, &'static str)> {
                let fields = parts.extensions.get::<ParsedRequestFields>().ok_or((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Parsed fields require a successful request schema transform",
                ))?;
                let value =
                    serde_json::Value::Object(fields.values().$field.clone().into_iter().collect());
                T::deserialize(value).map(Self).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Parsed request fields do not match the handler type",
                    )
                })
            }
        }
        impl<S, T> FromRequestParts<S> for $name<T>
        where
            S: Send + Sync,
            T: DeserializeOwned + Send,
        {
            type Rejection = (StatusCode, &'static str);
            fn from_request_parts(
                parts: &mut Parts,
                _state: &S,
            ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
                std::future::ready(Self::from_verified_parts(parts))
            }
        }
    };
}
extractor!(ParsedParams, params);
extractor!(ParsedQuery, query);
extractor!(ParsedHeaders, headers);

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::http::RequestFieldValues;
    use serde_json::json;
    use std::collections::BTreeMap;
    #[tokio::test]
    async fn typed_projections_preserve_raw_uri_and_headers_and_require_success() {
        let (mut parts, ()) = axum::http::Request::builder()
            .uri("/fields/2?limit=1&limit=2")
            .header("x-count", "2")
            .body(())
            .unwrap()
            .into_parts();
        assert_eq!(
            ParsedParams::<BTreeMap<String, u64>>::from_request_parts(&mut parts, &())
                .await
                .unwrap_err()
                .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        parts.extensions.insert(ParsedRequestFields::from_validated(
            RequestFieldValues {
                params: [("id".into(), json!(2))].into(),
                query: [("limit".into(), json!([1, 2]))].into(),
                headers: [("x-count".into(), json!(2))].into(),
            },
            &[("x-count".into(), "2".into())].into(),
        ));
        assert_eq!(
            ParsedParams::<BTreeMap<String, u64>>::from_request_parts(&mut parts, &())
                .await
                .unwrap()
                .0["id"],
            2
        );
        assert_eq!(
            ParsedQuery::<BTreeMap<String, Vec<u64>>>::from_request_parts(&mut parts, &())
                .await
                .unwrap()
                .0["limit"],
            [1, 2]
        );
        assert_eq!(
            ParsedHeaders::<BTreeMap<String, u64>>::from_request_parts(&mut parts, &())
                .await
                .unwrap()
                .0["x-count"],
            2
        );
        assert_eq!(
            ParsedQuery::<BTreeMap<String, u64>>::from_request_parts(&mut parts, &())
                .await
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(parts.uri.to_string(), "/fields/2?limit=1&limit=2");
        assert_eq!(parts.headers["x-count"], "2");
    }
}
