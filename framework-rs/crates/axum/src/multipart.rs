//! Bounded, typed multipart form extraction.
//!
//! Text fields deserialize as strings and file fields as byte arrays (`Vec<u8>`).
//! Repeated names use the last part. Homogeneous text/file repeats match the
//! TypeScript parser; mixed text/file name collisions have no portable ordering.
//! Filenames and part content types are not model fields. Numeric/JSON coercion
//! is explicit in the model's Serde implementation. Place this extractor last.
use axum::{
    extract::{FromRequest, Multipart as AxumMultipart, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use forklaunch_validator::FrameworkSchema;
use serde_json::{Map, Value};

/// Limits in addition to Axum's request body limit and the framework router limit.
/// Install with `Extension(MultipartLimits { .. })`; version matching shares it.
pub use forklaunch_core::http::{BusboyTextOptions, MultipartLimits};

/// A multipart model decoded with Serde and validated with the framework's garde
/// rules before entering the handler. Error responses never include field values.
pub struct Multipart<T>(pub T);

impl<S, T> FromRequest<S> for Multipart<T>
where
    S: Send + Sync,
    T: FrameworkSchema + Send,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let text_options = request.extensions().get::<BusboyTextOptions>().cloned();
        let limits = request
            .extensions()
            .get::<MultipartLimits>()
            .copied()
            .unwrap_or_default();
        if limits.parts == 0 || limits.field_bytes == 0 || limits.total_bytes == 0 {
            return Err(reject(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid multipart limits",
            ));
        }
        let mut multipart = AxumMultipart::from_request(request, state)
            .await
            .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid multipart body"))?;
        let mut fields = Map::new();
        let mut count = 0usize;
        let mut total = 0usize;
        while let Some(mut field) = multipart
            .next_field()
            .await
            .map_err(|error| parser_error(&error))?
        {
            count += 1;
            if count > limits.parts {
                return Err(too_large());
            }
            let name = field
                .name()
                .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "multipart field has no name"))?
                .to_owned();
            // Busboy also classifies octet-stream parts without filenames as files.
            let file = field.file_name().is_some()
                || field.content_type().is_some_and(|mime| {
                    mime.split(';')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .eq_ignore_ascii_case("application/octet-stream")
                });
            let charset = field
                .content_type()
                .and_then(forklaunch_core::http::multipart_text_charset);
            let mut bytes = Vec::new();
            while let Some(chunk) = field.chunk().await.map_err(|error| parser_error(&error))? {
                if chunk.len() > limits.field_bytes.saturating_sub(bytes.len())
                    || chunk.len() > limits.total_bytes.saturating_sub(total)
                {
                    return Err(too_large());
                }
                total += chunk.len();
                bytes.extend_from_slice(&chunk);
            }
            if !file && let Some(options) = &text_options {
                match options.decode(&bytes, charset.as_deref()).map_err(|_| {
                    reject(StatusCode::BAD_REQUEST, "unrepresentable multipart text")
                })? {
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
                        .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid multipart text"))?,
                )
            };
            fields.insert(name, value);
        }
        let model: T = serde_json::from_value(Value::Object(fields))
            .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid multipart model"))?;
        forklaunch_validator::validate_value(&model)
            .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid multipart model"))?;
        Ok(Self(model))
    }
}

fn reject(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
fn too_large() -> Response {
    reject(
        StatusCode::PAYLOAD_TOO_LARGE,
        "multipart body exceeds limit",
    )
}
fn parser_error(error: &axum::extract::multipart::MultipartError) -> Response {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        too_large()
    } else {
        reject(StatusCode::BAD_REQUEST, "invalid multipart body")
    }
}

#[cfg(test)]
mod tests;
