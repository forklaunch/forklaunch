//! Exact matched response admission for portable finite version predicates.
use crate::SdkError;
use forklaunch_core::contract::schema_validation::{
    SchemaValidationFailure, compile_checked, decode_wire_field,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Only response metadata survives request matching; no request values are retained.
pub struct MatchedVersionResponses {
    responses: Vec<Value>,
}
impl std::fmt::Debug for MatchedVersionResponses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MatchedVersionResponses")
            .field("branches", &self.responses.len())
            .finish_non_exhaustive()
    }
}
fn invalid() -> SdkError {
    SdkError::Contract("invalid or unsupported version matching contract")
}
fn schema_matches(schema: &Value, value: &Value) -> Result<bool, SdkError> {
    match compile_checked(schema).map_err(|_| invalid())?(value) {
        Ok(()) => Ok(true),
        Err(SchemaValidationFailure::Invalid(_)) => Ok(false),
        Err(SchemaValidationFailure::Operational) => Err(invalid()),
    }
}
fn fields_match(rules: &Value, values: &BTreeMap<String, Vec<String>>) -> Result<bool, SdkError> {
    for (name, rule) in rules.as_object().ok_or_else(invalid)? {
        let schema = rule.get("schema").ok_or_else(invalid)?;
        let validator = compile_checked(schema).map_err(|_| invalid())?;
        match decode_wire_field(
            validator.as_ref(),
            values.get(name).map_or(&[], Vec::as_slice),
            rule["required"].as_bool().ok_or_else(invalid)?,
        ) {
            Ok(_) => {}
            Err(SchemaValidationFailure::Invalid(_)) => return Ok(false),
            Err(SchemaValidationFailure::Operational) => return Err(invalid()),
        }
    }
    Ok(true)
}
fn response_headers_match(
    rules: &Value,
    values: &BTreeMap<String, Vec<String>>,
) -> Result<bool, SdkError> {
    for (name, rule) in rules.as_object().ok_or_else(invalid)? {
        let Some(wire) = values.get(name) else {
            if rule["required"] == true {
                return Ok(false);
            }
            continue;
        };
        if wire.len() != 1 {
            return Ok(false);
        }
        let schema = rule.get("schema").ok_or_else(invalid)?;
        if schema_matches(schema, &Value::String(wire[0].clone()))? {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(&wire[0]) else {
            return Ok(false);
        };
        if !schema_matches(schema, &parsed)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn request_body_matches(
    branch: &Value,
    bytes: &[u8],
    content_type: &str,
) -> Result<bool, SdkError> {
    let media = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !bytes.is_empty()
        && branch["bodyKind"] != "empty"
        && branch["bodyMedia"].as_str() != Some(media.as_str())
    {
        return Ok(false);
    }
    let options: forklaunch_core::contract::BodyParserOptions = branch
        .get("parserOptions")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    options.validate_portable().map_err(|_| invalid())?;
    let limit = match branch["bodyKind"].as_str() {
        Some("multipart") => 2_097_152,
        Some("json") => options.json_wire.limit_bytes,
        Some("form") => options.form_wire.limit_bytes,
        Some("binary") => options.raw_wire.limit_bytes,
        _ => options.text_wire.limit_bytes,
    };
    if branch["bodyKind"] != "empty" && bytes.len() > limit {
        return Err(invalid());
    }
    if bytes.is_empty() && branch["bodyOptional"] == true {
        return Ok(true);
    }
    match branch["bodyKind"].as_str().ok_or_else(invalid)? {
        "empty" => Ok(bytes.is_empty()),
        "multipart" => {
            if branch["bodyMedia"].as_str() != Some(media.as_str()) {
                return Ok(false);
            }
            let value = forklaunch_core::http::decode_multipart_value(bytes, content_type)
                .map_err(|_| invalid())?;
            schema_matches(&branch["bodySchema"], &value)
        }
        "form" => {
            if branch["bodyMedia"].as_str() != Some(media.as_str()) {
                return Ok(false);
            }
            let value = forklaunch_core::http::decode_urlencoded_with_options::<Value>(
                bytes,
                content_type,
                &options,
            )
            .map_err(|_| invalid())?;
            schema_matches(&branch["bodySchema"], &value)
        }
        "binary" => {
            if branch["bodyMedia"].as_str() != Some(media.as_str()) {
                return Ok(false);
            }
            schema_matches(
                &branch["bodySchema"],
                &Value::Array(bytes.iter().copied().map(Value::from).collect()),
            )
        }
        kind @ ("json" | "text") => {
            if branch["bodyMedia"].as_str() != Some(media.as_str()) {
                return Ok(false);
            }
            let value = if kind == "json" {
                match serde_json::from_str::<Value>(&String::from_utf8_lossy(
                    bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes),
                )) {
                    Ok(value) if !options.json_strict || value.is_object() || value.is_array() => {
                        value
                    }
                    Ok(_) | Err(_) => return Ok(false),
                }
            } else {
                Value::String(
                    String::from_utf8_lossy(
                        bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes),
                    )
                    .into_owned(),
                )
            };
            schema_matches(&branch["bodySchema"], &value)
        }
        _ => Err(invalid()),
    }
}
type WireFields = BTreeMap<String, Vec<String>>;
pub(crate) fn request_fields(
    request: &reqwest::Request,
) -> Result<(WireFields, WireFields), SdkError> {
    let mut query = BTreeMap::<String, Vec<String>>::new();
    for (name, value) in request.url().query_pairs() {
        query.entry(name.into()).or_default().push(value.into());
    }
    let mut headers = BTreeMap::<String, Vec<String>>::new();
    for (name, value) in request.headers() {
        headers
            .entry(name.as_str().into())
            .or_default()
            .push(value.to_str().map_err(|_| invalid())?.into());
    }
    Ok((query, headers))
}
impl MatchedVersionResponses {
    pub(crate) fn from_prepared(
        request: &reqwest::Request,
        metadata: &str,
        base_url: &str,
    ) -> Result<Self, SdkError> {
        if metadata.len() > 1_048_576 {
            return Err(invalid());
        }
        let metadata: Value = serde_json::from_str(metadata).map_err(|_| invalid())?;
        let branches = metadata["branches"]
            .as_array()
            .filter(|branches| !branches.is_empty() && branches.len() <= 64)
            .ok_or_else(invalid)?;
        if metadata["formatVersion"] != 1 {
            return Err(invalid());
        }
        let (query, headers) = request_fields(request)?;
        if headers
            .get("content-encoding")
            .is_some_and(|values| values.as_slice() != ["identity"])
        {
            return Err(invalid());
        }
        let bytes = match request.body() {
            None => &[][..],
            Some(body) => body.as_bytes().ok_or_else(invalid)?,
        };
        if bytes.len() > 2_097_152 {
            return Err(invalid());
        }
        if headers
            .get("content-type")
            .into_iter()
            .flatten()
            .any(|value| {
                value.split(';').skip(1).any(|parameter| {
                    let parameter = parameter.trim().to_ascii_lowercase();
                    parameter.starts_with("charset=")
                        && !matches!(
                            parameter.as_str(),
                            "charset=utf-8" | "charset=utf8" | "charset=\"utf-8\""
                        )
                })
            })
        {
            return Err(invalid());
        }
        let content_type = headers
            .get("content-type")
            .and_then(|values| values.first())
            .map_or("", String::as_str);
        let base = reqwest::Url::parse(base_url).map_err(|_| invalid())?;
        let mut responses = Vec::new();
        for branch in branches {
            let pattern = format!(
                "{}{}",
                base.path().trim_end_matches('/'),
                branch["path"].as_str().ok_or_else(invalid)?
            );
            let Some(params) =
                forklaunch_core::http::match_path_checked(&pattern, request.url().path())
                    .map_err(|_| invalid())?
            else {
                continue;
            };
            let params = params
                .into_iter()
                .map(|(name, value)| (name, vec![value]))
                .collect();
            if !fields_match(&branch["params"], &params)?
                || !fields_match(&branch["query"], &query)?
                || !fields_match(&branch["headers"], &headers)?
            {
                continue;
            }
            if let Some(groups) = branch.get("requestObjects")
                && !crate::request_objects::matches(request, groups)?
            {
                continue;
            }
            let matches = request_body_matches(branch, bytes, content_type)?;
            if matches {
                responses.push(branch["responses"].clone());
            }
        }
        if responses.is_empty() {
            return Err(SdkError::Contract(
                "request matches no declared API version",
            ));
        }
        Ok(Self { responses })
    }
    /// Check an actual response body only against request-matched version branches.
    /// # Errors
    /// Rejects undeclared representations, schema mismatches and evaluator failures.
    pub fn validate_body(&self, status: u16, media: &str, value: &Value) -> Result<(), SdkError> {
        let key = format!("{status}:{media}");
        for branch in &self.responses {
            if let Some(response) = branch
                .get(&key)
                .or_else(|| branch.get(format!("{status}:")))
                && schema_matches(&response["schema"], value)?
            {
                return Ok(());
            }
        }
        Err(SdkError::Contract("response violates matched API versions"))
    }
    /// Validate whole response-header alternatives independently of body alternatives.
    /// # Errors
    /// Rejects missing/invalid headers, undeclared representations and evaluator failures.
    pub fn validate_headers(
        &self,
        status: u16,
        media: &str,
        headers: &reqwest::header::HeaderMap,
    ) -> Result<(), SdkError> {
        let key = format!("{status}:{media}");
        let mut values = BTreeMap::<String, Vec<String>>::new();
        for (name, value) in headers {
            values
                .entry(name.as_str().into())
                .or_default()
                .push(value.to_str().map_err(|_| invalid())?.into());
        }
        for branch in &self.responses {
            if let Some(response) = branch
                .get(&key)
                .or_else(|| branch.get(format!("{status}:")))
                && response["headers"]
                    .as_object()
                    .ok_or_else(invalid)?
                    .keys()
                    .all(|name| values.get(name).is_none_or(|values| values.len() <= 1))
                && response_headers_match(&response["headers"], &values)?
            {
                return Ok(());
            }
        }
        Err(SdkError::ResponseHeaders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn streaming_version_mismatch_closes_before_exposing_frame() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = axum::Router::new().route(
            "/events",
            axum::routing::get(|| async {
                (
                    [("content-type", "text/event-stream")],
                    "data: {\"value\":\"old\"}\n\ndata: {\"value\":\"new\"}\n\n",
                )
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let metadata=serde_json::json!({"formatVersion":1,"branches":[{"path":"/events","params":{},"query":{},"headers":{},"bodyKind":"empty","bodyOptional":false,"responses":{"200:text/event-stream":{"schema":{"type":"object","required":["data"],"properties":{"data":{"type":"object","required":["value"],"properties":{"value":{"const":"old"}}}}},"headers":{}}}}]}).to_string();
        let client = reqwest::Client::new();
        let request = client.get(format!("{base}/events")).build().unwrap();
        let matched = MatchedVersionResponses::from_prepared(&request, &metadata, &base).unwrap();
        let response = client.execute(request).await.unwrap();
        let mut stream = crate::SdkEventStream::<Value>::from_versioned_response(
            response, 1024, "true", matched,
        )
        .unwrap();
        assert_eq!(
            stream.next().await.unwrap().unwrap()["data"]["value"],
            "old"
        );
        assert!(stream.next().await.is_err());
        assert!(stream.next().await.unwrap().is_none());
        server.abort();
    }
    #[test]
    fn default_json_parser_is_strict_bom_aware_and_bounded() {
        let branch = serde_json::json!({"bodyKind":"json","bodyMedia":"application/json","bodyOptional":false,"bodySchema":true});
        assert!(!request_body_matches(&branch, b"1", "application/json").unwrap());
        assert!(request_body_matches(&branch, b"\xef\xbb\xbf{}", "application/json").unwrap());
        assert!(request_body_matches(&branch, &vec![b' '; 102_401], "application/json").is_err());
    }
    #[test]
    fn form_predicates_use_shared_bounded_nested_parser() {
        let branch = serde_json::json!({"bodyKind":"form","bodyMedia":"application/x-www-form-urlencoded","bodyOptional":false,"bodySchema":{"type":"object","required":["user","tags"],"properties":{"user":{"type":"object","required":["name"],"properties":{"name":{"const":"café"}}},"tags":{"type":"array","items":{"enum":["a","b"]},"minItems":2}},"additionalProperties":false}});
        for wire in [
            "user[name]=caf%C3%A9&tags[]=a&tags[]=b",
            "user%5Bname%5D=caf%C3%A9&tags[0]=a&tags[1]=b",
        ] {
            assert!(
                request_body_matches(
                    &branch,
                    wire.as_bytes(),
                    "application/x-www-form-urlencoded"
                )
                .unwrap()
            );
        }
        assert!(
            !request_body_matches(
                &branch,
                b"user[name]=other&tags[]=a&tags[]=b",
                "application/x-www-form-urlencoded"
            )
            .unwrap()
        );
        assert!(
            request_body_matches(
                &branch,
                "x=1&".repeat(1001).as_bytes(),
                "application/x-www-form-urlencoded"
            )
            .is_err()
        );
    }
    #[test]
    fn binary_predicates_use_actual_bytes_without_text_decoding() {
        let branch = serde_json::json!({"bodyKind":"binary","bodyMedia":"application/octet-stream","bodyOptional":false,"bodySchema":{"type":"array","items":{"type":"integer","minimum":128},"minItems":1}});
        assert!(request_body_matches(&branch, &[255, 128], "application/octet-stream").unwrap());
        assert!(!request_body_matches(&branch, &[127], "application/octet-stream").unwrap());
        assert!(!request_body_matches(&branch, &[255], "text/plain").unwrap());
    }
    #[test]
    fn path_predicates_decode_once_and_preserve_mounts() {
        let metadata=serde_json::json!({"formatVersion":1,"branches":[{"path":"/items/{id}","params":{"id":{"required":true,"schema":{"type":"string","enum":["a/b","café"]}}},"query":{},"headers":{},"bodyKind":"empty","bodyOptional":false,"responses":{}}]}).to_string();
        for (path, valid) in [
            ("/mount/items/a%2Fb", true),
            ("/mount/items/caf%C3%A9", true),
            ("/mount/items/a%252Fb", false),
            ("/mount/items/%FF", false),
            ("/mount/items/%", false),
            ("/wrong/items/a%2Fb", false),
        ] {
            let request = reqwest::Client::new()
                .get(format!("https://example.test{path}"))
                .build()
                .unwrap();
            assert_eq!(
                MatchedVersionResponses::from_prepared(
                    &request,
                    &metadata,
                    "https://example.test/mount"
                )
                .is_ok(),
                valid,
                "{path}"
            );
        }
    }
    #[test]
    fn response_arrays_require_json_wire_arrays() {
        let rules = serde_json::json!({"x-values":{"required":true,"schema":{"type":"array","items":{"type":"string"}}}});
        assert!(
            !response_headers_match(
                &rules,
                &BTreeMap::from([("x-values".into(), vec!["value".into()])])
            )
            .unwrap()
        );
        assert!(
            response_headers_match(
                &rules,
                &BTreeMap::from([("x-values".into(), vec![r#"["value"]"#.into()])])
            )
            .unwrap()
        );
    }
    #[tokio::test]
    async fn multipart_matches_actual_reqwest_wire_and_rejects_nonportable_parts() {
        use http_body_util::BodyExt as _;
        let metadata = serde_json::json!({"formatVersion":1,"branches":[{
            "path":"/upload", "params":{}, "query":{}, "headers":{},
            "bodyKind":"multipart", "bodyMedia":"multipart/form-data", "bodyOptional":false,
            "bodySchema":{"type":"object","required":["label","file"],"properties":{
                "label":{"const":"café"},"file":{"type":"array","items":{"type":"integer","minimum":128},"minItems":2}
            },"additionalProperties":false},"responses":{}
        }]}).to_string();
        for (name, bytes, valid) in [
            ("file", vec![255, 128], true),
            ("file", vec![127], false),
            ("é", vec![255, 128], false),
        ] {
            let form = reqwest::multipart::Form::new().text("label", "café").part(
                name,
                reqwest::multipart::Part::bytes(bytes).file_name("file.bin"),
            );
            let mut request = reqwest::Client::new()
                .post("https://example.test/upload")
                .multipart(form)
                .build()
                .unwrap();
            let body = request
                .body_mut()
                .take()
                .unwrap()
                .collect()
                .await
                .unwrap()
                .to_bytes();
            *request.body_mut() = Some(body.into());
            assert_eq!(
                MatchedVersionResponses::from_prepared(&request, &metadata, "https://example.test")
                    .is_ok(),
                valid
            );
        }
        let duplicated=b"--b\r\nContent-Disposition: form-data; name=\"a\"; filename=\"x\"\r\n\r\nfile\r\n--b\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\ntext\r\n--b--\r\n";
        assert!(
            forklaunch_core::http::decode_multipart_value(
                duplicated,
                "multipart/form-data; boundary=b"
            )
            .is_err()
        );
    }
    #[test]
    fn unrelated_parser_limits_do_not_reject_other_media() {
        let branch =
            serde_json::json!({"bodyKind":"json","bodyMedia":"application/json","bodySchema":true});
        assert!(
            !request_body_matches(
                &branch,
                &vec![0; 200_000],
                "multipart/form-data; boundary=b"
            )
            .unwrap()
        );
        assert!(request_body_matches(&branch, &vec![0; 200_000], "application/json").is_err());
    }
    #[test]
    fn explicit_parser_profiles_match_shared_native_semantics() {
        let scalar = serde_json::json!({"bodyKind":"json","bodyMedia":"application/json","bodySchema":{"const":42},"parserOptions":{"jsonStrict":false,"jsonWire":{"limitBytes":2,"inflate":false}}});
        assert!(request_body_matches(&scalar, b"42", "application/json").unwrap());
        assert!(request_body_matches(&scalar, b"420", "application/json").is_err());
        let mut strict = scalar.clone();
        strict["parserOptions"]["jsonStrict"] = true.into();
        assert!(!request_body_matches(&strict, b"42", "application/json").unwrap());
        for (wire, options, expected) in [
            (
                "user[name]=x",
                serde_json::json!({"formExtended":false}),
                serde_json::json!({"user[name]":"x"}),
            ),
            (
                "utf8=%26%2310003%3B&value=%E9",
                serde_json::json!({"formCharsetSentinel":true}),
                serde_json::json!({"value":"é"}),
            ),
            (
                "utf8=%26%2310003%3B&value=%26%23233%3B",
                serde_json::json!({"formCharsetSentinel":true,"formInterpretNumericEntities":true}),
                serde_json::json!({"value":"é"}),
            ),
        ] {
            let branch = serde_json::json!({"bodyKind":"form","bodyMedia":"application/x-www-form-urlencoded","bodySchema":{"const":expected},"parserOptions":options});
            assert!(
                request_body_matches(
                    &branch,
                    wire.as_bytes(),
                    "application/x-www-form-urlencoded"
                )
                .unwrap(),
                "{wire}"
            );
        }
        for (wire, options) in [
            ("a=1&b=2", serde_json::json!({"formParameterLimit":1})),
            ("a[b][c]=1", serde_json::json!({"formDepth":1})),
        ] {
            let branch = serde_json::json!({"bodyKind":"form","bodyMedia":"application/x-www-form-urlencoded","bodySchema":true,"parserOptions":options});
            assert!(
                request_body_matches(
                    &branch,
                    wire.as_bytes(),
                    "application/x-www-form-urlencoded"
                )
                .is_err()
            );
        }
    }
}
