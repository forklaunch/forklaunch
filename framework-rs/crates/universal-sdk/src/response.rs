use crate::SdkError;
use std::{fmt, sync::Arc};
#[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
pub struct SdkResponseWithHeaders<B, H> {
    pub body: B,
    pub headers: H,
}
#[derive(Clone)]
pub struct ResponseValidator(
    Arc<forklaunch_core::contract::schema_validation::CheckedJsonValidator>,
);
impl fmt::Debug for ResponseValidator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResponseValidator")
    }
}
impl ResponseValidator {
    /// Compile a bounded response schema without external retrieval.
    /// # Errors
    /// Rejects malformed or unsupported response schemas.
    pub fn new(schema: &str) -> Result<Self, SdkError> {
        let value = serde_json::from_str(schema)
            .map_err(|_| SdkError::Contract("invalid response schema"))?;
        forklaunch_core::contract::schema_validation::compile_checked(&value)
            .map(Self)
            .map_err(|_| SdkError::Contract("unsupported response schema"))
    }
    /// Check a decoded response against its retained contract.
    /// # Errors
    /// Rejects validation and operational evaluator failures.
    pub fn validate(&self, value: &serde_json::Value) -> Result<(), SdkError> {
        (self.0)(value).map_err(|_| SdkError::Contract("response violates contract"))
    }
}

/// Decode a JSON response only after its declared and matched-version schemas pass.
/// # Errors
/// Rejects invalid JSON, contract violations, or a value that cannot inhabit `T`.
pub async fn decode_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    schema: &str,
    versions: Option<&crate::MatchedVersionResponses>,
) -> Result<T, SdkError> {
    let status = response.status().as_u16();
    let content_type = response_content_type(&response)?;
    let value = serde_json::from_slice(&response.bytes().await?)
        .map_err(|_| SdkError::Contract("invalid response JSON"))?;
    if let Some(versions) = versions {
        versions.validate_body(status, &content_type, &value)?;
    }
    ResponseValidator::new(schema)?.validate(&value)?;
    serde_json::from_value(value).map_err(|_| SdkError::Contract("invalid typed response"))
}

/// Decode text and validate its value against the declared response schema.
/// # Errors
/// Rejects transfer failures, ambiguous content types, or contract violations.
pub async fn decode_text_response(
    response: reqwest::Response,
    schema: &str,
    versions: Option<&crate::MatchedVersionResponses>,
) -> Result<String, SdkError> {
    let status = response.status().as_u16();
    let content_type = response_content_type(&response)?;
    let text = response.text().await?;
    let value = serde_json::Value::String(text.clone());
    if let Some(versions) = versions {
        versions.validate_body(status, &content_type, &value)?;
    }
    ResponseValidator::new(schema)?.validate(&value)?;
    Ok(text)
}
/// Decode and validate declared response headers, rejecting duplicates.
/// # Errors
/// Rejects missing required, malformed, or schema-invalid headers.
pub fn decode_response_headers<T: serde::de::DeserializeOwned>(
    headers: &reqwest::header::HeaderMap,
    rules: &str,
) -> Result<T, SdkError> {
    let rules: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(rules).map_err(|_| SdkError::ResponseHeaders)?;
    let mut fields = serde_json::Map::new();
    for (name, rule) in rules {
        let values = headers.get_all(&name).iter().collect::<Vec<_>>();
        if values.len() > 1 {
            return Err(SdkError::ResponseHeaders);
        }
        let Some(value) = values.first() else {
            if rule["required"] == true {
                return Err(SdkError::ResponseHeaders);
            }
            continue;
        };
        let raw = value.to_str().map_err(|_| SdkError::ResponseHeaders)?;
        let validator = ResponseValidator::new(&rule["schema"].to_string())?;
        let text = serde_json::Value::String(raw.to_owned());
        let value = if validator.validate(&text).is_ok() {
            text
        } else {
            let json = serde_json::from_str(raw).map_err(|_| SdkError::ResponseHeaders)?;
            validator
                .validate(&json)
                .map_err(|_| SdkError::ResponseHeaders)?;
            json
        };
        fields.insert(name, value);
    }
    serde_json::from_value(serde_json::Value::Object(fields)).map_err(|_| SdkError::ResponseHeaders)
}

/// Ordered multipart form parts; repeated names and file metadata are preserved.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
pub struct MultipartResponse {
    pub parts: Vec<MultipartResponsePart>,
}
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
pub struct MultipartResponsePart {
    pub name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}
/// Decode ordered multipart response parts within byte and part bounds.
/// # Errors
/// Rejects malformed media, transfer failures, or exceeded limits.
pub async fn decode_multipart_response(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<MultipartResponse, SdkError> {
    response_content_type(&response)?;
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(SdkError::Multipart(
            "Missing multipart response content type",
        ))?;
    if !mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case("multipart/form-data")
    {
        return Err(SdkError::Multipart("Expected multipart/form-data response"));
    }
    let boundary = multer::parse_boundary(mime)
        .map_err(|_| SdkError::Multipart("Invalid multipart response boundary"))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
            return Err(SdkError::Multipart("Multipart response exceeds byte limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let stream = futures_util::stream::once(std::future::ready(Ok::<_, std::io::Error>(bytes)));
    let mut multipart = multer::Multipart::new(stream, boundary);
    let mut parts = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| SdkError::Multipart("Invalid multipart response"))?
    {
        if parts.len() >= 1024 {
            return Err(SdkError::Multipart("Multipart response exceeds part limit"));
        }
        let name = field
            .name()
            .ok_or(SdkError::Multipart("Multipart response part has no name"))?
            .to_owned();
        let file_name = field.file_name().map(str::to_owned);
        let content_type = field.content_type().map(ToString::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|_| SdkError::Multipart("Invalid multipart response part"))?
            .to_vec();
        parts.push(MultipartResponsePart {
            name,
            file_name,
            content_type,
            bytes,
        });
    }
    Ok(MultipartResponse { parts })
}

/// Reject duplicate media declarations before selecting a generated response branch.
/// # Errors
/// Returns a contract error for ambiguous response content types.
pub fn response_content_type(response: &reqwest::Response) -> Result<String, SdkError> {
    if response.headers().get_all("content-type").iter().count() > 1 {
        return Err(SdkError::Contract("ambiguous response content type"));
    }
    Ok(response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase())
}
