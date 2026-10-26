//! Checked `AsyncAPI` message runtime expressions (RFC 6901 URI fragments).
use forklaunch_common::FrameworkResult;
use serde_json::Value;

/// The source of a message correlation identifier or reply address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSource {
    Header,
    Payload,
}

/// A bounded, validated expression. Evaluation borrows the referenced value
/// without coercion; missing input/path differs from a present JSON null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExpression {
    location: String,
    pointer: String,
    source: MessageSource,
}

impl RuntimeExpression {
    /// Parse `$message.header` or `$message.payload`, optionally followed by a
    /// `#` JSON Pointer URI fragment. Limits:4096 bytes and64 pointer segments.
    /// # Errors
    /// Rejects unknown sources, malformed percent/UTF-8/pointer escapes or limits.
    pub fn parse(location: &str) -> FrameworkResult<Self> {
        if location.len() > 4096 {
            return Err(super::invalid("Runtime expression exceeds byte limit"));
        }
        let (source, suffix) = if let Some(suffix) = location.strip_prefix("$message.header") {
            (MessageSource::Header, suffix)
        } else if let Some(suffix) = location.strip_prefix("$message.payload") {
            (MessageSource::Payload, suffix)
        } else {
            return Err(super::invalid("Unknown message runtime expression source"));
        };
        let fragment = if suffix.is_empty() {
            ""
        } else {
            suffix.strip_prefix('#').ok_or_else(|| {
                super::invalid("Runtime expression requires a JSON Pointer fragment")
            })?
        };
        let bytes = fragment.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && (bytes
                    .get(index + 1)
                    .is_none_or(|byte| !byte.is_ascii_hexdigit())
                    || bytes
                        .get(index + 2)
                        .is_none_or(|byte| !byte.is_ascii_hexdigit()))
            {
                return Err(super::invalid(
                    "Malformed runtime expression percent escape",
                ));
            }
            if !byte.is_ascii_alphanumeric() && !b"-._~!$&'()*+,;=:@/?%".contains(byte) {
                return Err(super::invalid("Invalid runtime expression URI fragment"));
            }
        }
        let pointer = percent_encoding::percent_decode_str(fragment)
            .decode_utf8()
            .map_err(|_| super::invalid("Runtime expression fragment is not UTF-8"))?
            .into_owned();
        if !pointer.is_empty() && !pointer.starts_with('/') {
            return Err(super::invalid(
                "Runtime expression fragment is not a JSON Pointer",
            ));
        }
        if pointer.bytes().filter(|byte| *byte == b'/').count() > 64 {
            return Err(super::invalid("Runtime expression exceeds pointer depth"));
        }
        let mut chars = pointer.chars();
        while let Some(ch) = chars.next() {
            if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(super::invalid("Invalid runtime expression pointer escape"));
            }
        }
        Ok(Self {
            location: location.into(),
            pointer,
            source,
        })
    }

    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }

    #[must_use]
    pub const fn source(&self) -> MessageSource {
        self.source
    }

    /// Resolve against application message headers/payload, preserving value type.
    /// Does not interpret WebSocket upgrade headers as application message headers.
    #[must_use]
    pub fn resolve<'a>(
        &self,
        headers: Option<&'a Value>,
        payload: Option<&'a Value>,
    ) -> Option<&'a Value> {
        let input = match self.source {
            MessageSource::Header => headers,
            MessageSource::Payload => payload,
        }?;
        input.pointer(&self.pointer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_sources_types_null_and_missing() {
        let headers = json!({"MQMD":{"CorrelId":17}});
        let payload = json!({"id":false,"nil":null,"items":["a","b"]});
        for (location, expected) in [
            ("$message.header#/MQMD/CorrelId", Some(json!(17))),
            ("$message.payload#/id", Some(json!(false))),
            ("$message.payload#/nil", Some(Value::Null)),
            ("$message.payload#/items/1", Some(json!("b"))),
            ("$message.payload#/items/01", None),
            ("$message.payload#/items/-", None),
            ("$message.payload#/missing", None),
            ("$message.payload#", Some(payload.clone())),
            ("$message.header", Some(headers.clone())),
        ] {
            let expression = RuntimeExpression::parse(location).unwrap();
            assert_eq!(
                expression.resolve(Some(&headers), Some(&payload)),
                expected.as_ref(),
                "{location}"
            );
            assert_eq!(expression.resolve(None, None), None);
            assert_eq!(expression.location(), location);
        }
    }

    #[test]
    fn pointer_escaping_and_uri_decoding_are_exact() {
        let payload = json!({"a/b":{"~key":3},"a b":4,"😀":5,"":6,"~1":7,"\u{0}":8});
        for (fragment, expected) in [
            ("/a~1b/~0key", 3),
            ("/a%20b", 4),
            ("/%F0%9F%98%80", 5),
            ("/", 6),
            ("/~01", 7),
            ("/%00", 8),
        ] {
            let expression =
                RuntimeExpression::parse(&format!("$message.payload#{fragment}")).unwrap();
            assert_eq!(
                expression.resolve(None, Some(&payload)),
                Some(&json!(expected))
            );
        }
    }

    #[test]
    fn rejects_malformed_expressions_before_evaluation() {
        for location in [
            "",
            "$message",
            "$message.headers#/id",
            "$message.payloads",
            "$message.payload/id",
            "$message.payload#id",
            "$message.payload#/a~",
            "$message.payload#/a~2",
            "$message.payload#/%",
            "$message.payload#/%0",
            "$message.payload#/%GG",
            "$message.payload#/%FF",
            "$message.payload#/a b",
            "$message.payload#/a#b",
            "$message.payload#/a\n",
        ] {
            assert!(RuntimeExpression::parse(location).is_err(), "{location:?}");
        }
        assert!(
            RuntimeExpression::parse(&format!("$message.payload#{}", "/a".repeat(65))).is_err()
        );
        assert!(
            RuntimeExpression::parse(&format!("$message.payload#/{}", "a".repeat(4096))).is_err()
        );
        assert!(RuntimeExpression::parse(&format!("$message.payload#{}", "/a".repeat(64))).is_ok());
    }
}
