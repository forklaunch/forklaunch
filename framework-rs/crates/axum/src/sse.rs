//! Pull-driven SSE emission with bounded frames and explicit validation policy.
use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use forklaunch_core::http::sse::SseEvent;
use forklaunch_validator::FrameworkSchema;
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use std::fmt;

/// The configured TypeScript SSE envelope; payload validation applies to `data`.
#[derive(Debug, Clone, Serialize)]
pub struct SseFrame<T> {
    pub id: String,
    pub data: T,
}
impl<T> SseFrame<T> {
    #[must_use]
    pub fn new(id: impl Into<String>, data: T) -> Self {
        Self {
            id: id.into(),
            data,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SseValidation {
    /// Validate every event, including events produced after response headers.
    #[default]
    EveryEvent,
    /// Match the TypeScript emitter's first-event-only schema validation.
    FirstEvent,
}
#[derive(Debug, Clone, Copy)]
pub struct SseOptions {
    pub max_event_bytes: usize,
    pub validation: SseValidation,
}
impl Default for SseOptions {
    fn default() -> Self {
        Self {
            max_event_bytes: 1024 * 1024,
            validation: SseValidation::EveryEvent,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseEmitError {
    InvalidLimit,
    InvalidEvent,
    EventTooLarge,
    ProducerFailed,
}
impl fmt::Display for SseEmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimit => "Invalid SSE event byte limit",
            Self::InvalidEvent => "Invalid SSE response event",
            Self::EventTooLarge => "SSE response event exceeds byte limit",
            Self::ProducerFailed => "SSE response producer failed",
        })
    }
}
impl std::error::Error for SseEmitError {}
impl IntoResponse for SseEmitError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

/// Validate typed data with the framework's garde rules. IDs are always checked
/// for field injection. Use [`respond_with`] for whole-envelope/custom validation.
/// No producer task or unbounded queue is created. The first event is awaited
/// before returning headers, so its errors can be returned as an HTTP 500.
///
/// # Errors
/// Invalid limits, first-event validation/serialization and producer failures.
pub async fn respond<S, T, E>(stream: S, options: SseOptions) -> Result<Response, SseEmitError>
where
    S: Stream<Item = Result<SseFrame<T>, E>> + Send + 'static,
    T: FrameworkSchema + Send + 'static,
    E: Send + 'static,
{
    respond_with(stream, options, |frame| {
        forklaunch_validator::validate_value(&frame.data).is_ok()
    })
    .await
}

/// Stream validated SSE envelopes. The validator sees the complete event; error
/// details from validators and producers are never included in the response.
/// After headers, a failure terminates the body and drops the source. Dropping
/// the body also drops the source, including while waiting for its next event.
/// Event framing and byte limits apply even in first-event compatibility mode.
///
/// # Errors
/// Invalid limits or failure preparing the first event. Later failures are body errors.
pub async fn respond_with<S, T, E, V>(
    stream: S,
    options: SseOptions,
    validate: V,
) -> Result<Response, SseEmitError>
where
    S: Stream<Item = Result<SseFrame<T>, E>> + Send + 'static,
    T: Serialize + Send + 'static,
    E: Send + 'static,
    V: Fn(&SseFrame<T>) -> bool + Send + 'static,
{
    if options.max_event_bytes == 0 {
        return Err(SseEmitError::InvalidLimit);
    }
    let mut stream = Box::pin(stream);
    let first = match stream.next().await {
        Some(Ok(frame)) => Some(encode(&frame, options.max_event_bytes, &validate)?),
        Some(Err(_)) => return Err(SseEmitError::ProducerFailed),
        None => return Ok(sse_response(Body::empty())),
    };
    let body =
        futures_util::stream::unfold(Some((stream, first, validate)), move |state| async move {
            let (mut stream, first, validate) = state?;
            if let Some(bytes) = first {
                return Some((Ok(bytes), Some((stream, None, validate))));
            }
            let frame = stream.next().await?;
            let encoded = match frame {
                Ok(frame) => encode(&frame, options.max_event_bytes, &|frame| {
                    options.validation == SseValidation::FirstEvent || validate(frame)
                }),
                Err(_) => Err(SseEmitError::ProducerFailed),
            };
            match encoded {
                Ok(bytes) => Some((Ok(bytes), Some((stream, None, validate)))),
                Err(error) => Some((Err(error), None)),
            }
        });
    Ok(sse_response(Body::from_stream(body)))
}

fn sse_response(body: Body) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

fn encode<T: Serialize>(
    frame: &SseFrame<T>,
    limit: usize,
    validate: &impl Fn(&SseFrame<T>) -> bool,
) -> Result<Vec<u8>, SseEmitError> {
    if !validate(frame) || frame.id.contains(['\r', '\n', '\0']) {
        return Err(SseEmitError::InvalidEvent);
    }
    if frame.id.len() > limit {
        return Err(SseEmitError::EventTooLarge);
    }
    let mut data = BoundedBytes {
        bytes: Vec::new(),
        // JSON escaping can expand raw string data sixfold before framing.
        limit: limit.saturating_mul(6).saturating_add(2),
        exceeded: false,
    };
    serde_json::to_writer(&mut data, &frame.data).map_err(|_| {
        if data.exceeded {
            SseEmitError::EventTooLarge
        } else {
            SseEmitError::InvalidEvent
        }
    })?;
    // Strings are sent as text; all other JSON values retain JSON serialization.
    let text = if data.bytes.first() == Some(&b'"') {
        serde_json::from_slice::<String>(&data.bytes).map_err(|_| SseEmitError::InvalidEvent)?
    } else {
        String::from_utf8(data.bytes).map_err(|_| SseEmitError::InvalidEvent)?
    };
    let event = SseEvent {
        id: Some(frame.id.clone()),
        data: text,
        ..SseEvent::default()
    };
    let bytes = event.encode().map_err(|_| SseEmitError::InvalidEvent)?;
    if bytes.len() > limit {
        return Err(SseEmitError::EventTooLarge);
    }
    Ok(bytes)
}
struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl std::io::Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("SSE frame limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests;
