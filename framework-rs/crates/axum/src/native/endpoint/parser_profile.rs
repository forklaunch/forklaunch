//! Decode only after the transport's original-byte authorization boundary.
use super::{EndpointError, Parts, RequestPlan};
use forklaunch_core::{
    contract::{BodyKind, HttpMethod},
    http::{Body, ContractValidationLocation, FrameworkRequest, RequestContext},
};

impl RequestPlan {
    pub(super) fn parse_body(
        &self,
        parts: &Parts,
        bytes: &[u8],
        optional: bool,
    ) -> Result<Option<serde_json::Value>, EndpointError> {
        let Some(body) = &self.body else {
            return Ok(None);
        };
        if optional && bytes.is_empty() {
            return Ok(None);
        }
        // Multipart retains its typed file extractor and independent part limits.
        if matches!(body.kind, BodyKind::Multipart | BodyKind::ServerSentEvent) {
            return Ok(None);
        }
        let mut request = FrameworkRequest::new(
            HttpMethod::Post,
            parts.uri.path(),
            RequestContext::new("native-body-parser"),
        );
        request.body = Body::Binary(bytes.to_vec());
        for name in ["content-type", "content-encoding"] {
            if parts.headers.get_all(name).iter().count() > 1 {
                return Err(EndpointError::InvalidRequest);
            }
            if let Some(value) = parts.headers.get(name) {
                request.headers.insert(
                    name.into(),
                    value
                        .to_str()
                        .map_err(|_| EndpointError::InvalidRequest)?
                        .into(),
                );
            }
        }
        forklaunch_core::http::parse_request_body(body.kind, &self.parsers, &request).map_err(
            |error| match error.location {
                ContractValidationLocation::BodySize => EndpointError::PayloadTooLarge,
                ContractValidationLocation::BodyEncoding => EndpointError::UnsupportedContentType,
                _ => EndpointError::InvalidRequest,
            },
        )
    }
}

impl RequestPlan {
    pub(super) async fn decode_with_mode<I: super::NativeBody>(
        &self,
        parts: &Parts,
        bytes: &[u8],
        content_type: &str,
        path: &std::collections::HashMap<String, String>,
    ) -> Result<I::Value, EndpointError> {
        let parsed = self.parse_body(parts, bytes, I::body_optional())?;
        let metadata = self.fields.decode_path(path).and_then(|path| {
            self.fields
                .decode(parts)
                .map(|(query, headers)| (query, headers, path))
        });
        let result = match metadata {
            Ok(metadata) => {
                I::decode_profile_request(
                    bytes,
                    content_type,
                    self.validator.as_deref(),
                    parsed.clone(),
                    metadata,
                )
                .await
            }
            Err(error) => Err(error),
        };
        if !matches!(result, Err(EndpointError::RequestSchemaAssertion))
            || self.request_mode == forklaunch_core::contract::ValidationMode::Error
        {
            return result;
        }
        if self.request_mode == forklaunch_core::contract::ValidationMode::Warning {
            tracing::warn!(operation=%self.operation,reason="schema_rejection", "request schema observation failed; original parser values retained");
        }
        I::decode_profile_request(
            bytes,
            content_type,
            None,
            parsed,
            self.fields.raw_metadata(parts, path)?,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use forklaunch_core::contract::{BodyParserOptions, HttpMethod};
    use tower::ServiceExt;

    fn descriptor<I: EndpointBody>(options: BodyParserOptions) -> Endpoint<I, TextBody> {
        let base =
            Endpoint::<I, TextBody>::new("profile", "profile", HttpMethod::Post, "/profile", 200)
                .unwrap();
        let mut route = base.contract();
        route.options.body_parser = options;
        Endpoint::from_contract(route).unwrap()
    }
    fn registry(route: RouteContract) -> ContractRegistry {
        let mut result = ContractRegistry::new();
        result.register(route);
        result
    }
    async fn send(
        app: Router,
        media: &str,
        bytes: Vec<u8>,
        encoding: Option<&str>,
    ) -> (u16, Vec<u8>) {
        let mut request = Request::builder()
            .method("POST")
            .uri("/profile")
            .header("content-type", media);
        if let Some(encoding) = encoding {
            request = request.header("content-encoding", encoding);
        }
        let response = app
            .oneshot(request.body(Body::from(bytes)).unwrap())
            .await
            .unwrap();
        (
            response.status().as_u16(),
            to_bytes(response.into_body(), 10000)
                .await
                .unwrap()
                .to_vec(),
        )
    }

    #[tokio::test]
    async fn native_parser_profile_enforces_json_strict_and_explicit_scalar_opt_in() {
        for strict in [true, false] {
            let options = BodyParserOptions {
                json_strict: strict,
                ..Default::default()
            };
            let endpoint = descriptor::<JsonBody<String>>(options);
            let app = ContractRouter::<()>::new(registry(endpoint.contract()))
                .bind(endpoint, |_, text| async { Ok(text) })
                .unwrap()
                .finish()
                .unwrap();
            let (status, body) = send(
                app,
                "application/json",
                br#""private scalar""#.to_vec(),
                None,
            )
            .await;
            assert_eq!(status, if strict { 400 } else { 200 });
            if strict {
                assert!(!String::from_utf8_lossy(&body).contains("private scalar"));
            }
        }
    }

    #[tokio::test]
    async fn native_parser_profile_text_decodes_charset_and_bounds_inflation() {
        let mut options = BodyParserOptions::default();
        options.text_wire.limit_bytes = 5;
        let endpoint = descriptor::<TextBody>(options.clone());
        let app = ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind(endpoint, |_, text| async { Ok(text) })
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(
            send(
                app.clone(),
                "text/plain; charset=iso-8859-1",
                vec![0xe9],
                None
            )
            .await,
            (200, "é".as_bytes().to_vec())
        );
        // gzip("hello") with deterministic timestamp; wire length exceeds the decompressed limit.
        let compressed = vec![
            31, 139, 8, 0, 0, 0, 0, 0, 2, 3, 203, 72, 205, 201, 201, 7, 0, 134, 166, 16, 54, 5, 0,
            0, 0,
        ];
        assert_eq!(
            send(app.clone(), "text/plain", compressed.clone(), Some("gzip")).await,
            (200, b"hello".to_vec())
        );
        assert_eq!(
            send(app, "text/plain", b"secret".to_vec(), None).await.0,
            413
        );
        options.text_wire.inflate = false;
        let endpoint = descriptor::<TextBody>(options);
        let app = ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind(endpoint, |_, text| async { Ok(text) })
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(
            send(app, "text/plain", compressed, Some("gzip")).await.0,
            415
        );
    }

    #[forklaunch_core::contract::schema]
    struct Form {
        value: String,
    }
    #[tokio::test]
    async fn native_parser_profile_form_uses_declared_charset_and_parameter_limit() {
        let mut options = BodyParserOptions {
            form_default_charset: "iso-8859-1".into(),
            form_parameter_limit: 1,
            ..Default::default()
        };
        options.form_wire.limit_bytes = 30;
        let endpoint = descriptor::<FormBody<Form>>(options);
        let app = ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind(endpoint, |_, form| async { Ok(form.value) })
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(
            send(
                app.clone(),
                "application/x-www-form-urlencoded",
                b"value=%E9".to_vec(),
                None
            )
            .await,
            (200, "é".as_bytes().to_vec())
        );
        assert_eq!(
            send(
                app.clone(),
                "application/x-www-form-urlencoded",
                b"value=x&other=y".to_vec(),
                None
            )
            .await
            .0,
            413
        );
        assert_eq!(
            send(
                app,
                "application/x-www-form-urlencoded; charset=utf-16",
                b"value=x".to_vec(),
                None
            )
            .await
            .0,
            415
        );
    }

    #[derive(Clone)]
    struct OriginalBytes(Vec<u8>);
    impl EndpointPolicy<()> for OriginalBytes {
        type Context = ();
        fn authorize(
            &self,
            _: &mut Parts,
            (): &(),
            _: &RouteContract,
        ) -> impl Future<Output = Result<(), EndpointError>> {
            std::future::ready(Ok(()))
        }
        fn authorize_body(
            &self,
            (): (),
            _: &mut Parts,
            bytes: &[u8],
            (): &(),
            _: &RouteContract,
        ) -> impl Future<Output = Result<(), EndpointError>> {
            std::future::ready(if bytes == self.0 {
                Ok(())
            } else {
                Err(EndpointError::Forbidden)
            })
        }
    }
    #[tokio::test]
    async fn native_parser_profile_authorizes_original_bytes_before_charset_decoding() {
        let endpoint = descriptor::<TextBody>(BodyParserOptions::default());
        let app = ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind_authorized(endpoint, OriginalBytes(vec![0xe9]), |_, (), text| async {
                Ok(text)
            })
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(
            send(
                app.clone(),
                "text/plain; charset=iso-8859-1",
                vec![0xe9],
                None
            )
            .await,
            (200, "é".as_bytes().to_vec())
        );
        assert_eq!(
            send(
                app,
                "text/plain; charset=utf-8",
                "é".as_bytes().to_vec(),
                None
            )
            .await
            .0,
            403
        );
    }

    #[test]
    fn native_parser_profile_invalid_configuration_fails_mount() {
        let endpoint = descriptor::<TextBody>(BodyParserOptions {
            text_default_charset: "not-a-charset".into(),
            ..Default::default()
        });
        assert!(
            ContractRouter::<()>::new(registry(endpoint.contract()))
                .bind(endpoint, |_, text| async { Ok(text) })
                .is_err()
        );
    }
}
