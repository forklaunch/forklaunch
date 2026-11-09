//! Typed access to selected version fields without rewriting the wire request.
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use forklaunch_core::http::SelectedVersion;
use serde::de::DeserializeOwned;

/// Decode declared path parameters produced by the selected version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedParams<T>(pub T);
/// Decode declared query fields produced by the selected version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedQuery<T>(pub T);
/// Decode original header keys with truthy transformed values overlaid.
/// Authentication and ordinary Axum extractors continue using wire headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedHeaders<T>(pub T);

macro_rules! extractor {
    ($name:ident, $field:ident) => {
        impl<T: DeserializeOwned + Send> $name<T> {
            fn from_verified_parts(parts: &Parts) -> Result<Self, (StatusCode, &'static str)> {
                let version = parts.extensions.get::<SelectedVersion>().ok_or((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    concat!(stringify!($name), " requires a selected version"),
                ))?;
                let value = serde_json::Value::Object(
                    version.parsed_fields.$field.clone().into_iter().collect(),
                );
                T::deserialize(value).map(Self).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Selected version fields do not match the handler type",
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
extractor!(VersionedParams, params);
extractor!(VersionedQuery, query);
extractor!(VersionedHeaders, headers);

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::http::VersionFieldValues;
    use serde_json::json;
    use std::{collections::BTreeMap, sync::Arc};

    #[tokio::test]
    async fn typed_fields_require_selection_and_preserve_original_request_parts() {
        let request = axum::http::Request::builder()
            .uri("/fields/one?count=1")
            .header("x-count", "original")
            .body(())
            .unwrap();
        let (mut parts, ()) = request.into_parts();
        assert_eq!(
            VersionedQuery::<serde_json::Value>::from_request_parts(&mut parts, &())
                .await
                .unwrap_err()
                .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        parts.extensions.insert(SelectedVersion {
            version: "one".into(),
            matched_versions: vec!["one".into()],
            parsed_body: None,
            parsed_fields: Arc::new(VersionFieldValues {
                params: BTreeMap::from([("id".into(), json!("ONE"))]),
                query: BTreeMap::from([("count".into(), json!(42))]),
                headers: BTreeMap::from([("x-count".into(), json!(false))]),
            }),
        });
        let VersionedParams(params) =
            VersionedParams::<BTreeMap<String, String>>::from_request_parts(&mut parts, &())
                .await
                .unwrap();
        let VersionedQuery(query) =
            VersionedQuery::<BTreeMap<String, u32>>::from_request_parts(&mut parts, &())
                .await
                .unwrap();
        let VersionedHeaders(headers) =
            VersionedHeaders::<BTreeMap<String, bool>>::from_request_parts(&mut parts, &())
                .await
                .unwrap();
        assert_eq!(params["id"], "ONE");
        assert_eq!(query["count"], 42);
        assert!(!headers["x-count"]);
        let error = VersionedQuery::<BTreeMap<String, String>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(
            error,
            (
                StatusCode::BAD_REQUEST,
                "Selected version fields do not match the handler type"
            )
        );
        assert_eq!(parts.uri.to_string(), "/fields/one?count=1");
        assert_eq!(parts.headers["x-count"], "original");
    }
}
