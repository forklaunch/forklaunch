//! Typed access to the body already parsed for API version selection.
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use forklaunch_core::http::SelectedVersion;
use serde::de::DeserializeOwned;

/// Decode the selected version's parsed body without consuming the wire body.
///
/// Place this before a raw `Bytes` extractor to use both representations. This
/// extractor requires a matched version and a declared body. It does not rerun
/// parsers or schema validation. `T` must agree with the selected body schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedBody<T>(pub T);

/// Decode an optional transformed body from a selected API version.
/// Absence returns `None`; an explicit JSON null is deserialized as a present
/// value. The extractor still requires a selected version and leaves raw bytes
/// available to subsequent body extractors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionalVersionedBody<T>(pub Option<T>);

impl<T: DeserializeOwned + Send> OptionalVersionedBody<T> {
    fn from_verified_parts(parts: &Parts) -> Result<Self, (StatusCode, &'static str)> {
        let version = parts.extensions.get::<SelectedVersion>().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "OptionalVersionedBody requires a selected version",
        ))?;
        version
            .parsed_body
            .as_deref()
            .map(T::deserialize)
            .transpose()
            .map(Self)
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Selected version body does not match the handler type",
                )
            })
    }
}
impl<S, T> FromRequestParts<S> for OptionalVersionedBody<T>
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

impl<T: DeserializeOwned + Send> VersionedBody<T> {
    fn from_verified_parts(parts: &Parts) -> Result<Self, (StatusCode, &'static str)> {
        let value = parts
            .extensions
            .get::<SelectedVersion>()
            .and_then(|version| version.parsed_body.as_deref())
            .ok_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                "VersionedBody requires a selected version with a body",
            ))?;
        T::deserialize(value).map(Self).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Selected version body does not match the handler type",
            )
        })
    }
}
impl<S, T> FromRequestParts<S> for VersionedBody<T>
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn missing_selection_and_incompatible_types_have_stable_rejections() {
        let (mut parts, ()) = axum::http::Request::new(()).into_parts();
        assert_eq!(
            VersionedBody::<String>::from_request_parts(&mut parts, &())
                .await
                .unwrap_err()
                .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        parts.extensions.insert(SelectedVersion {
            parsed_fields: Arc::default(),
            version: "v1".into(),
            matched_versions: vec!["v1".into()],
            parsed_body: Some(Arc::new(serde_json::json!({"private":"secret"}))),
        });
        let error = VersionedBody::<String>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(
            error,
            (
                StatusCode::BAD_REQUEST,
                "Selected version body does not match the handler type"
            )
        );
        let VersionedBody(value) =
            VersionedBody::<serde_json::Value>::from_request_parts(&mut parts, &())
                .await
                .unwrap();
        assert_eq!(value, serde_json::json!({"private":"secret"}));
    }

    #[tokio::test]
    async fn optional_extractor_distinguishes_absence_null_and_missing_selection() {
        let (mut parts, ()) = axum::http::Request::new(()).into_parts();
        assert_eq!(
            OptionalVersionedBody::<serde_json::Value>::from_request_parts(&mut parts, &())
                .await
                .unwrap_err()
                .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        parts.extensions.insert(SelectedVersion {
            parsed_fields: Arc::default(),
            version: "v1".into(),
            matched_versions: vec!["v1".into()],
            parsed_body: None,
        });
        assert_eq!(
            OptionalVersionedBody::<serde_json::Value>::from_request_parts(&mut parts, &())
                .await
                .unwrap()
                .0,
            None
        );
        parts
            .extensions
            .get_mut::<SelectedVersion>()
            .unwrap()
            .parsed_body = Some(Arc::new(serde_json::Value::Null));
        assert_eq!(
            OptionalVersionedBody::<serde_json::Value>::from_request_parts(&mut parts, &())
                .await
                .unwrap()
                .0,
            Some(serde_json::Value::Null)
        );
        assert_eq!(
            OptionalVersionedBody::<String>::from_request_parts(&mut parts, &())
                .await
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
    }
}
