//! Bounded server-sent event framing, independent of an HTTP client or executor.
use std::fmt::{self, Write as _};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub data: String,
    pub id: Option<String>,
    pub event: Option<String>,
    pub retry_ms: Option<u64>,
}

impl SseEvent {
    #[must_use]
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            ..Self::default()
        }
    }

    /// Encodes one event, rejecting metadata that could inject another field.
    ///
    /// # Errors
    /// Returns `InvalidMetadata` when an ID or event name contains CR, LF or NUL.
    pub fn encode(&self) -> Result<Vec<u8>, SseError> {
        for value in [self.id.as_deref(), self.event.as_deref()]
            .into_iter()
            .flatten()
        {
            if value.contains(['\r', '\n', '\0']) {
                return Err(SseError::InvalidMetadata);
            }
        }
        let mut output = String::new();
        if let Some(id) = &self.id {
            let _ = writeln!(output, "id: {id}");
        }
        if let Some(event) = &self.event {
            let _ = writeln!(output, "event: {event}");
        }
        if let Some(retry) = self.retry_ms {
            let _ = writeln!(output, "retry: {retry}");
        }
        let data = self.data.replace("\r\n", "\n").replace('\r', "\n");
        for line in data.split('\n') {
            let _ = writeln!(output, "data: {line}");
        }
        output.push('\n');
        Ok(output.into_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseError {
    InvalidLimit,
    InvalidMetadata,
    EventTooLarge,
    Closed,
}
impl fmt::Display for SseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimit => "SSE event limit must be positive",
            Self::InvalidMetadata => "Invalid SSE event metadata",
            Self::EventTooLarge => "SSE event exceeds configured limit",
            Self::Closed => "SSE decoder is closed",
        })
    }
}
impl std::error::Error for SseError {}

/// Incremental UTF-8/CRLF framing with a bound on each event, including comments.
/// Calling `finish` discards an unterminated event; it does not invent a delimiter.
#[derive(Debug)]
pub struct SseDecoder {
    line: Vec<u8>,
    data: String,
    last_id: Option<String>,
    event: Option<String>,
    retry_ms: Option<u64>,
    frame_retry_ms: Option<u64>,
    skip_lf: bool,
    first_line: bool,
    frame_bytes: usize,
    max_event_bytes: usize,
    closed: bool,
    failure: Option<SseError>,
}
impl SseDecoder {
    /// Creates a decoder with a positive per-event byte limit.
    ///
    /// # Errors
    /// Returns `InvalidLimit` when the limit is zero.
    pub fn new(max_event_bytes: usize) -> Result<Self, SseError> {
        if max_event_bytes == 0 {
            return Err(SseError::InvalidLimit);
        }
        Ok(Self {
            line: Vec::new(),
            data: String::new(),
            last_id: None,
            event: None,
            retry_ms: None,
            frame_retry_ms: None,
            skip_lf: false,
            first_line: true,
            frame_bytes: 0,
            max_event_bytes,
            closed: false,
            failure: None,
        })
    }

    #[must_use]
    pub const fn retry_ms(&self) -> Option<u64> {
        self.retry_ms
    }

    /// Returns the terminal framing failure, including after a batch of valid events.
    #[must_use]
    pub const fn failure(&self) -> Option<SseError> {
        self.failure
    }

    /// Decodes complete events while retaining an incomplete frame.
    ///
    /// # Errors
    /// Returns `EventTooLarge` and closes the decoder on overflow, or `Closed`
    /// after overflow or `finish`. If valid events precede overflow in the same
    /// chunk, returns those events first; `failure()` reports the terminal error.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        if self.closed {
            return Err(SseError::Closed);
        }
        let mut events = Vec::new();
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            if self.frame_bytes >= self.max_event_bytes {
                self.closed = true;
                self.failure = Some(SseError::EventTooLarge);
                self.line.clear();
                self.data.clear();
                return if events.is_empty() {
                    Err(SseError::EventTooLarge)
                } else {
                    Ok(events)
                };
            }
            self.frame_bytes += 1;
            if byte == b'\r' || byte == b'\n' {
                self.skip_lf = byte == b'\r';
                if let Some(event) = self.process_line() {
                    events.push(event);
                }
            } else {
                self.line.push(byte);
            }
        }
        Ok(events)
    }

    pub fn finish(&mut self) {
        self.closed = true;
        self.line.clear();
        self.data.clear();
    }

    fn process_line(&mut self) -> Option<SseEvent> {
        let bytes = std::mem::take(&mut self.line);
        let line = String::from_utf8_lossy(&bytes);
        let line = if self.first_line {
            self.first_line = false;
            line.strip_prefix('\u{feff}').unwrap_or(&line)
        } else {
            &line
        };
        if line.is_empty() {
            self.frame_bytes = 0;
            let event = self.event.take();
            let retry_ms = self.frame_retry_ms.take();
            if self.data.is_empty() {
                return None;
            }
            self.data.pop();
            return Some(SseEvent {
                data: std::mem::take(&mut self.data),
                id: self.last_id.clone(),
                event,
                retry_ms,
            });
        }
        if line.starts_with(':') {
            return None;
        }
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match name {
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            "id" if !value.contains('\0') => self.last_id = Some(value.into()),
            "event" => self.event = (!value.is_empty()).then(|| value.into()),
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                if let Ok(retry) = value.parse::<u64>() {
                    self.retry_ms = Some(retry);
                    self.frame_retry_ms = Some(retry);
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
