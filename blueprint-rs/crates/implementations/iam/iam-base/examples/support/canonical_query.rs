//! Fixture-owned rules for the canonical Zod query schema, not a general translator.
use forklaunch_core::http::{FrameworkRequest, FrameworkResponse};
use forklaunch_core::http::{ParseError, typescript_request_validation_error_response};

pub fn reject(request: &FrameworkRequest, raw: Option<&str>) -> Option<FrameworkResponse> {
    let fields: Vec<_> = form_urlencoded::parse(raw.unwrap_or_default().as_bytes()).collect();
    let mut errors = Vec::new();
    for name in ["limit", "cursor"] {
        let values: Vec<_> = fields
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_ref())
            .collect();
        let message = match (name, values.as_slice()) {
            (_, []) | ("cursor", [_]) => None,
            ("cursor", _) => Some("Expected string, received array".to_owned()),
            (_, values) => {
                let input = match values {
                    [value] => serde_json::Value::String((*value).to_owned()),
                    values => serde_json::json!(values),
                };
                forklaunch_validator::typescript_coerce::integer_number(Some(&input))
                    .err()
                    .map(|error| error.message)
            }
        };
        if let Some(message) = message {
            errors.push(ParseError {
                path: vec!["query".into(), name.into()],
                message,
            });
        }
    }
    (!errors.is_empty()).then(|| {
        typescript_request_validation_error_response(
            Some(&errors),
            Some(&request.context.correlation_id),
        )
    })
}
