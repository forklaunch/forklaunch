//! Bounded native SSE profile for the actual managed gateway framing contract.
use super::ModelGatewayError;
use crate::domain::BlueprintError;
use crate::domain::BlueprintResult;
use crate::http::provider_http::ProviderByteStream;
use crate::http::provider_http::normalize_browser_numbers;
use serde_json::Value;
use std::time::Duration;

/// Finite native profile limits; smaller bounds may be chosen per request.
#[derive(Clone, Copy, Debug)]
pub struct ModelStreamLimits {
    total_bytes: usize,
    frame_bytes: usize,
    frames: usize,
    timeout: Duration,
}
impl Default for ModelStreamLimits {
    fn default() -> Self {
        Self {
            total_bytes: 16 * 1024 * 1024,
            frame_bytes: 256 * 1024,
            frames: 4096,
            timeout: Duration::from_secs(120),
        }
    }
}
impl ModelStreamLimits {
    /// Sets raw byte, individual frame, frame-count and total request-time bounds.
    /// Frame count includes malformed/comment/empty frames, not just emitted JSON.
    /// # Errors
    /// Requires positive limits no larger than defaults, with frame <= total bytes.
    pub fn new(
        total_bytes: usize,
        frame_bytes: usize,
        frames: usize,
        timeout: Duration,
    ) -> BlueprintResult<Self> {
        let max = Self::default();
        if total_bytes == 0
            || total_bytes > max.total_bytes
            || frame_bytes == 0
            || frame_bytes > max.frame_bytes
            || frame_bytes > total_bytes
            || frames == 0
            || frames > max.frames
            || timeout.is_zero()
            || timeout > max.timeout
        {
            return Err(BlueprintError::new(
                "model_gateway_stream_config",
                "Invalid model stream limits",
            ));
        }
        Ok(Self {
            total_bytes,
            frame_bytes,
            frames,
            timeout,
        })
    }
    pub(super) const fn total_bytes(self) -> usize {
        self.total_bytes
    }
    pub(super) const fn timeout(self) -> Duration {
        self.timeout
    }
}
/// Incremental JSON items from the fixed signed model stream route.
/// Holds only one transport chunk plus a bounded frame. No background reader,
/// automatic retry, or accumulated response exists. Drop/cancel closes the native
/// response body, stronger than the upstream generator's releaseLock-only behavior.
pub struct ModelGatewayStream {
    response: Option<ProviderByteStream>,
    limits: ModelStreamLimits,
    pending: Vec<u8>,
    cursor: usize,
    frame: Vec<u8>,
    frames: usize,
    initial: bool,
}
impl std::fmt::Debug for ModelGatewayStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelGatewayStream")
            .field("limits", &self.limits)
            .field("frames", &self.frames)
            .finish_non_exhaustive()
    }
}
impl ModelGatewayStream {
    pub(super) fn new(
        response: ProviderByteStream,
        limits: ModelStreamLimits,
    ) -> Result<Self, ModelGatewayError> {
        if !(200..300).contains(&response.status) {
            return Err(ModelGatewayError {
                code: "model_gateway_request_failed".into(),
                status: Some(response.status),
                retry_after_seconds: response.retry_after_seconds,
            });
        }
        Ok(Self {
            response: Some(response),
            limits,
            pending: Vec::new(),
            cursor: 0,
            frame: Vec::new(),
            frames: 0,
            initial: true,
        })
    }
    /// Releases the native response and buffered bytes. Further `next` calls end.
    pub fn cancel(&mut self) {
        self.response = None;
        self.pending = Vec::new();
        self.frame = Vec::new();
        self.cursor = 0;
    }
    /// Reads the next model chunk. Exact `[DONE]` ends the stream successfully;
    /// comment-only and empty `data:` frames are skipped. Matching the TypeScript
    /// reader, a partial or failed answer is never success: invalid JSON, a value
    /// that is not a chunk object with a `choices` array, a truthy `error` field,
    /// or a body that ends before `[DONE]` fails the stream with status 502.
    /// # Errors
    /// Invalid/interrupted streams and limit/time/transport failures terminate the
    /// stream with sanitized errors and status 502. Items already returned stay with the caller.
    /// Dropping this future does not replace explicit stream cancel/drop.
    pub async fn next(&mut self) -> Result<Option<Value>, ModelGatewayError> {
        let result = self.next_inner().await;
        result.map_err(|mut error| {
            self.cancel();
            // Like the TypeScript reader, any failure after the stream opened
            // (transport, time or size) is reported as an interrupted 502.
            error.status.get_or_insert(502);
            error
        })
    }
    async fn next_inner(&mut self) -> Result<Option<Value>, ModelGatewayError> {
        loop {
            let Some(response) = self.response.as_ref() else {
                return Ok(None);
            };
            response.check_time()?;
            while self.cursor < self.pending.len() {
                self.frame.push(self.pending[self.cursor]);
                self.cursor += 1;
                if let Some(separator) = boundary_end(&self.frame) {
                    self.frame.truncate(self.frame.len() - separator);
                    let event = self.take_frame()?;
                    match event {
                        Event::Value(value) => return Ok(Some(value)),
                        Event::Done => {
                            self.cancel();
                            return Ok(None);
                        }
                        Event::Skip => {}
                        Event::Invalid => {
                            return Err(stream_error("model_gateway_stream_invalid"));
                        }
                    }
                } else if self.frame.len() > self.limits.frame_bytes + 3 {
                    return Err(limit_error());
                }
            }
            self.pending.clear();
            self.cursor = 0;
            let chunk = self
                .response
                .as_mut()
                .ok_or_else(limit_error)?
                .next_chunk()
                .await?;
            if let Some(chunk) = chunk {
                self.pending = chunk;
            } else {
                // A body that ends without an exact, terminated [DONE] is a
                // partial answer, including any unterminated trailing frame.
                return Err(stream_error("model_gateway_stream_interrupted"));
            }
        }
    }
    fn take_frame(&mut self) -> Result<Event, ModelGatewayError> {
        self.frames += 1;
        if self.frames > self.limits.frames || self.frame.len() > self.limits.frame_bytes {
            return Err(limit_error());
        }
        let bytes = std::mem::take(&mut self.frame);
        let result = decode_event(&bytes, self.initial);
        self.initial = false;
        Ok(result)
    }
}
/// Invalid or interrupted answers carry the same 502 the TypeScript reader uses.
fn stream_error(code: &str) -> ModelGatewayError {
    ModelGatewayError {
        code: code.into(),
        status: Some(502),
        retry_after_seconds: None,
    }
}
fn limit_error() -> ModelGatewayError {
    ModelGatewayError::validation("model_gateway_stream_limit")
}
fn boundary_end(bytes: &[u8]) -> Option<usize> {
    if bytes.ends_with(b"\r\n\r\n") {
        Some(4)
    } else if bytes.ends_with(b"\r\n\n") || bytes.ends_with(b"\n\r\n") {
        Some(3)
    } else if bytes.ends_with(b"\n\n") {
        Some(2)
    } else {
        None
    }
}
enum Event {
    Skip,
    Done,
    Value(Value),
    Invalid,
}
fn js_whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
/// JavaScript truthiness of a JSON value, for the reader's `value.error` check.
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn decode_event(bytes: &[u8], initial: bool) -> Event {
    let decoded = String::from_utf8_lossy(bytes);
    let decoded = if initial {
        decoded.strip_prefix('\u{feff}').unwrap_or(&decoded)
    } else {
        &decoded
    };
    let data = decoded
        .split_inclusive('\n')
        .filter_map(|line| {
            let line = if let Some(line) = line.strip_suffix('\n') {
                line.strip_suffix('\r').unwrap_or(line)
            } else {
                line
            };
            line.strip_prefix("data:")
                .map(|value| value.trim_start_matches(js_whitespace))
        })
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        return Event::Skip;
    }
    if data == "[DONE]" {
        return Event::Done;
    }
    let Ok(mut value) = serde_json::from_str::<Value>(&data) else {
        return Event::Invalid;
    };
    let chunk = value.as_object().is_some_and(|object| {
        object.get("choices").is_some_and(Value::is_array)
            && !object.get("error").is_some_and(js_truthy)
    });
    if !chunk {
        return Event::Invalid;
    }
    normalize_browser_numbers(&mut value);
    Event::Value(value)
}
#[cfg(test)]
mod tests;
