use crate::{ResponseValidator, SdkError, sse};
/// A bounded, pull-driven event stream. Dropping or closing it releases the response.
pub struct SdkEventStream<T> {
    response: Option<reqwest::Response>,
    decoder: sse::SseDecoder,
    pending: std::collections::VecDeque<sse::SseEvent>,
    marker: std::marker::PhantomData<T>,
    validator: ResponseValidator,
    versions: Option<(u16, crate::MatchedVersionResponses)>,
}
impl<T: serde::de::DeserializeOwned> SdkEventStream<T> {
    /// Validate event media and prepare a bounded pull-driven stream.
    /// # Errors
    /// Rejects wrong media, invalid limits, or unsupported schemas.
    pub fn from_response(
        response: reqwest::Response,
        max_event_bytes: usize,
        schema: &str,
    ) -> Result<Self, SdkError> {
        let content_type = crate::response_content_type(&response)?;
        if !content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("text/event-stream")
        {
            return Err(SdkError::Stream("Expected text/event-stream response"));
        }
        Ok(Self {
            response: Some(response),
            decoder: sse::SseDecoder::new(max_event_bytes)?,
            pending: std::collections::VecDeque::new(),
            marker: std::marker::PhantomData,
            validator: ResponseValidator::new(schema)?,
            versions: None,
        })
    }

    /// Validate every event against the versions that matched the actual request.
    /// # Errors
    /// Rejects invalid media, limits, or static schemas before retaining the response.
    pub fn from_versioned_response(
        response: reqwest::Response,
        max_event_bytes: usize,
        schema: &str,
        versions: crate::MatchedVersionResponses,
    ) -> Result<Self, SdkError> {
        let status = response.status().as_u16();
        let mut stream = Self::from_response(response, max_event_bytes, schema)?;
        stream.versions = Some((status, versions));
        Ok(stream)
    }

    /// Restrict subsequent frames to the actual request-matched response branches.
    #[must_use]
    pub fn with_matched_versions(mut self, versions: crate::MatchedVersionResponses) -> Self {
        if let Some(response) = &self.response {
            self.versions = Some((response.status().as_u16(), versions));
        }
        self
    }

    pub fn close(&mut self) {
        self.response = None;
        self.pending.clear();
        self.decoder.finish();
    }

    /// Read one checked typed event, closing on protocol/validation failure.
    /// # Errors
    /// Rejects transport, framing, JSON, schema, and typed decoding failures.
    pub async fn next(&mut self) -> Result<Option<T>, SdkError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                let Ok(data) = serde_json::from_str::<serde_json::Value>(&event.data) else {
                    self.close();
                    return Err(SdkError::Contract("invalid event JSON"));
                };
                let mut value = serde_json::json!({"data": data});
                if let Some(id) = event.id {
                    value["id"] = serde_json::Value::String(id);
                }
                if let Some((status, versions)) = &self.versions
                    && let Err(error) = versions.validate_body(*status, "text/event-stream", &value)
                {
                    self.close();
                    return Err(error);
                }
                if let Err(error) = self.validator.validate(&value) {
                    self.close();
                    return Err(error);
                }
                let Ok(value) = serde_json::from_value(value) else {
                    self.close();
                    return Err(SdkError::Contract("invalid typed event"));
                };
                return Ok(Some(value));
            }
            if self.response.is_some()
                && let Some(error) = self.decoder.failure()
            {
                self.close();
                return Err(error.into());
            }
            let Some(response) = self.response.as_mut() else {
                return Ok(None);
            };
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => {
                    self.close();
                    return Ok(None);
                }
                Err(error) => {
                    self.close();
                    return Err(error.into());
                }
            };
            match self.decoder.push(&chunk) {
                Ok(events) => self.pending.extend(events),
                Err(error) => {
                    self.close();
                    return Err(error.into());
                }
            }
        }
    }
}

impl<T> std::fmt::Debug for SdkEventStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SdkEventStream")
            .field("open", &self.response.is_some())
            .field("buffered_events", &self.pending.len())
            .finish_non_exhaustive()
    }
}
