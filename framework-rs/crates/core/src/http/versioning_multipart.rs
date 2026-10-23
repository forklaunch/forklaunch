//! Multipart version matching over an already buffered body; no I/O is polled.
use super::{ContractValidationError, ContractValidationLocation, body_too_large, invalid_body};
use futures_util::FutureExt;
use serde_json::{Map, Value};

/// Shared limits for version matching and typed Axum multipart extraction.
/// Overwritten repeated parts still count toward every limit.
#[derive(Debug, Clone, Copy)]
pub struct MultipartLimits {
    pub parts: usize,
    pub field_bytes: usize,
    pub total_bytes: usize,
}
impl Default for MultipartLimits {
    fn default() -> Self {
        Self {
            parts: 128,
            field_bytes: 1024 * 1024,
            total_bytes: 2 * 1024 * 1024,
        }
    }
}

#[cfg(test)]
pub(super) fn parse(
    bytes: &[u8],
    content_type: &str,
    limits: MultipartLimits,
) -> Result<Value, ContractValidationError> {
    parse_with_text(bytes, content_type, limits, None)
}
pub(super) fn parse_with_text(
    bytes: &[u8],
    content_type: &str,
    limits: MultipartLimits,
    text: Option<&crate::http::BusboyTextOptions>,
) -> Result<Value, ContractValidationError> {
    parse_profile(bytes, content_type, limits, text, false)
}

pub(super) fn parse_portable(
    bytes: &[u8],
    content_type: &str,
) -> Result<Value, ContractValidationError> {
    parse_profile(bytes, content_type, MultipartLimits::default(), None, true)
}

fn parse_profile(
    bytes: &[u8],
    content_type: &str,
    limits: MultipartLimits,
    text: Option<&crate::http::BusboyTextOptions>,
    portable: bool,
) -> Result<Value, ContractValidationError> {
    if limits.parts == 0 || limits.field_bytes == 0 || limits.total_bytes == 0 {
        return Err(ContractValidationError::new(
            ContractValidationLocation::BodyParserConfiguration,
            "invalid multipart limits",
        ));
    }
    let boundary = multer::parse_boundary(content_type)
        .map_err(|_| invalid_body("invalid multipart boundary"))?;
    // This stream only contains immediately ready, owned memory. Poll once;
    // never nest an executor or block a runtime thread waiting for I/O.
    let stream = futures_util::stream::iter([Ok::<_, std::io::Error>(bytes.to_vec())]);
    async move {
        let mut multipart = multer::Multipart::new(stream, boundary);
        let mut fields = Map::new();
        let mut count = 0_usize;
        let mut total = 0_usize;
        while let Some(mut field) = multipart
            .next_field()
            .await
            .map_err(|_| invalid_body("invalid multipart body"))?
        {
            count += 1;
            if count > limits.parts {
                return Err(body_too_large("multipart body exceeds limit"));
            }
            let name = field
                .name()
                .ok_or_else(|| invalid_body("multipart field has no name"))?
                .to_owned();
            if portable && !portable_field(&field, &name, &fields) {
                return Err(invalid_body(
                    "multipart request is outside the portable profile",
                ));
            }
            let file = field.file_name().is_some()
                || field.content_type().is_some_and(|mime| {
                    mime.essence_str()
                        .eq_ignore_ascii_case("application/octet-stream")
                });
            let charset = field
                .content_type()
                .and_then(|mime| mime.get_param("charset"))
                .map(|value| value.as_str().to_owned());
            let mut bytes = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|_| invalid_body("invalid multipart body"))?
            {
                if chunk.len() > limits.field_bytes.saturating_sub(bytes.len())
                    || chunk.len() > limits.total_bytes.saturating_sub(total)
                {
                    return Err(body_too_large("multipart body exceeds limit"));
                }
                total += chunk.len();
                bytes.extend_from_slice(&chunk);
            }
            if !file && let Some(options) = text {
                match options
                    .decode(&bytes, charset.as_deref())
                    .map_err(|_| invalid_body("unrepresentable multipart text"))?
                {
                    Some(value) => {
                        fields.insert(name, Value::String(value));
                    }
                    None => {
                        fields.remove(&name);
                    }
                }
                continue;
            }
            let value = if file {
                Value::Array(bytes.into_iter().map(Value::from).collect())
            } else {
                Value::String(
                    String::from_utf8(bytes)
                        .map_err(|_| invalid_body("invalid multipart UTF-8 field"))?,
                )
            };
            fields.insert(name, value);
        }
        Ok(Value::Object(fields))
    }
    .now_or_never()
    .unwrap_or_else(|| Err(invalid_body("multipart parser did not complete")))
}

fn portable_field(field: &multer::Field<'_>, name: &str, fields: &Map<String, Value>) -> bool {
    !(name.is_empty()
        || !name.is_ascii()
        || name.bytes().any(|b| b < 32 || b == 127)
        || fields.contains_key(name)
        || field
            .file_name()
            .is_some_and(|name| name.is_empty() || !name.is_ascii())
        || field
            .headers()
            .keys()
            .any(|name| !matches!(name.as_str(), "content-disposition" | "content-type"))
        || field.content_type().is_some_and(|mime| {
            mime.get_param("charset")
                .is_some_and(|charset| !charset.as_str().eq_ignore_ascii_case("utf-8"))
        })
        || (field.file_name().is_none()
            && field.content_type().is_some_and(|mime| {
                mime.essence_str()
                    .eq_ignore_ascii_case("application/octet-stream")
            })))
}

#[cfg(test)]
mod tests {
    use super::*;
    const MIME: &str = "multipart/form-data; boundary=b";
    const BODY: &[u8] = b"--b\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\none\r\n--b\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\ntwo\r\n--b--\r\n";
    #[test]
    fn repeated_parts_count_towards_every_shared_limit() {
        let limits = MultipartLimits {
            parts: 2,
            field_bytes: 3,
            total_bytes: 6,
        };
        assert_eq!(
            parse(BODY, MIME, limits),
            Ok(serde_json::json!({"a":"two"}))
        );
        for limits in [
            MultipartLimits { parts: 1, ..limits },
            MultipartLimits {
                field_bytes: 2,
                ..limits
            },
            MultipartLimits {
                total_bytes: 5,
                ..limits
            },
            MultipartLimits { parts: 0, ..limits },
        ] {
            assert!(parse(BODY, MIME, limits).is_err());
        }
        let errors = [parse(BODY, MIME, MultipartLimits { parts: 0, ..limits }).unwrap_err()];
        assert_eq!(
            crate::http::validation_error_response(&errors, "fixture").status_code,
            500
        );
        let errors = [parse(BODY, MIME, MultipartLimits { parts: 1, ..limits }).unwrap_err()];
        assert_eq!(
            crate::http::validation_error_response(&errors, "fixture").status_code,
            413
        );
        assert!(parse(BODY, "multipart/form-data", limits).is_err());
        assert!(parse(&BODY[..BODY.len() - 10], MIME, limits).is_err());
    }
}

#[cfg(test)]
mod busboy_profile_tests {
    use super::*;
    fn wire(bytes: &[u8], charset: &str) -> Vec<u8> {
        let mime = if charset == "default" {
            String::new()
        } else {
            format!("Content-Type: text/plain; charset={charset}\r\n")
        };
        let mut data =
            format!("--b\r\nContent-Disposition: form-data; name=\"field\"\r\n{mime}\r\n")
                .into_bytes();
        data.extend_from_slice(bytes);
        data.extend_from_slice(b"\r\n--b--\r\n");
        data
    }
    #[test]
    fn busboy_text_actual_parser_differential() {
        let Ok(path) = std::env::var("FORKLAUNCH_MULTIPART_REFERENCE_OUTPUT") else {
            return;
        };
        let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut compared = 0;
        for case in cases {
            if case.get("collision").is_some() {
                continue;
            }
            let bytes: Vec<u8> = serde_json::from_value(case["bytes"].clone()).unwrap();
            let data = wire(&bytes, case["charset"].as_str().unwrap());
            let actual = parse_with_text(
                &data,
                "multipart/form-data; boundary=b",
                MultipartLimits::default(),
                Some(&crate::http::BusboyTextOptions::default()),
            )
            .unwrap();
            let expected = if case["undefined"] == true {
                serde_json::json!({})
            } else {
                serde_json::json!({"field":case["value"]})
            };
            assert_eq!(actual, expected, "{case}");
            compared += 1;
        }
        assert_eq!(compared, 45);
    }
    #[test]
    fn busboy_text_defaults_bounds_and_unrepresentable_values() {
        let mime = "multipart/form-data; boundary=b";
        let options = crate::http::BusboyTextOptions::default().with_default_charset("latin1");
        let data = wire(&[128, 255], "default");
        assert_eq!(
            parse_with_text(&data, mime, MultipartLimits::default(), Some(&options)).unwrap(),
            serde_json::json!({"field":"\u{80}ÿ"})
        );
        assert!(parse(&data, mime, MultipartLimits::default()).is_err());
        assert!(
            parse_with_text(
                &data,
                mime,
                MultipartLimits {
                    field_bytes: 1,
                    ..MultipartLimits::default()
                },
                Some(&options)
            )
            .is_err()
        );
        assert!(
            parse_with_text(
                &wire(&[0, 216], "utf-16le"),
                mime,
                MultipartLimits::default(),
                Some(&options)
            )
            .is_err()
        );
        assert_eq!(
            parse_with_text(
                &wire(&[195, 40], "utf-8"),
                mime,
                MultipartLimits::default(),
                Some(&options)
            )
            .unwrap(),
            serde_json::json!({"field":"�("})
        );
    }
}

#[cfg(test)]
mod busboy_version_tests {
    use super::*;
    use crate::http::{
        Body, FrameworkRequest, HttpMethod, RequestContext, VersionBodyHooksRegistry,
        VersionedRoutePlan,
    };
    use forklaunch_core::contract::{
        BodyContract, BodyKind, RequestContract, ResponseContract, RouteContract, VersionedContract,
    };
    #[test]
    fn busboy_text_registry_snapshot_and_selection_identity() {
        let mut body = BodyContract::json("Upload");
        body.kind = BodyKind::Multipart;
        body.content_type = Some("multipart/form-data".into());
        body.schema = body.schema.with_json_schema(
            r#"{"type":"object","properties":{"field":{"type":"string"}},"required":["field"]}"#,
        );
        let route =
            RouteContract::post("Upload", "Upload", "/upload").with_version(VersionedContract {
                version: "one".into(),
                request: RequestContract::default().with_body(body),
                responses: vec![ResponseContract::new(200, None)],
            });
        let mut hooks = VersionBodyHooksRegistry::default();
        hooks
            .enable_busboy_text(&route, crate::http::BusboyTextOptions::default())
            .unwrap();
        assert!(
            hooks
                .enable_busboy_text(&route, crate::http::BusboyTextOptions::default())
                .is_err()
        );
        let mut combined = VersionBodyHooksRegistry::default();
        combined.extend(hooks);
        let plan = combined.compile(&route).unwrap();
        let mut request = FrameworkRequest::new(
            HttpMethod::Post,
            "/upload",
            RequestContext::new("multipart"),
        )
        .with_body(Body::Binary(
            b"--b\r\nContent-Disposition: form-data; name=\"field\"\r\n\r\n\xff\r\n--b--\r\n"
                .to_vec(),
        ));
        request.headers.insert(
            "content-type".into(),
            "multipart/form-data; boundary=b".into(),
        );
        let prepared = plan.prepare_selection(&request, MultipartLimits::default());
        let (_, selected) = plan
            .apply_prepared(&prepared, &mut request)
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            selected.parsed_body.as_deref(),
            Some(&serde_json::json!({"field":"�"}))
        );
        let changed = plan
            .with_busboy_text(
                crate::http::BusboyTextOptions::default().with_default_charset("latin1"),
            )
            .unwrap();
        assert!(changed.resolve_prepared(&prepared, &request).is_err());
        let mut stale = route.clone();
        stale.options.request_validation = forklaunch_core::contract::ValidationMode::Warning;
        assert!(combined.compile(&stale).is_err());
        assert!(
            VersionedRoutePlan::compile(&RouteContract::get("Plain", "Plain", "/plain"))
                .unwrap()
                .with_busboy_text(crate::http::BusboyTextOptions::default())
                .is_err()
        );
    }
}
