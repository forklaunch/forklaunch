#![forbid(unsafe_code)]

/// V8 string-date grammar and pinned full-range local timezone resolution.
#[cfg(feature = "javascript-date")]
pub mod javascript_date;

#[cfg(feature = "zod-checks")]
pub mod zod_cardinality;

#[cfg(feature = "zod-checks")]
pub mod zod_string_predicate;

/// Exact Zod v3 numeric remainder semantics for explicitly enabled consumers.
#[cfg(feature = "zod-checks")]
pub mod zod_multiple_of;

/// Bounded, checked ECMAScript regex operations.
#[cfg(feature = "zod-regex")]
pub mod zod_regex;

use std::{error::Error, fmt};

mod naming;
pub use naming::{
    TransformationInfo, capitalize, get_transformation_info, hash_string, is_valid_identifier,
    remove_leading_numbers, remove_leading_separators, remove_non_alphanumeric,
    split_by_separators, to_camel_case_identifier, to_pretty_camel_case, to_valid_identifier,
    uncapitalize,
};

/// Shared result type for framework crates that do not need a domain-specific error.
pub type FrameworkResult<T> = Result<T, FrameworkError>;

/// Minimal framework error type used by early crates before adapter-specific errors exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameworkError {
    code: String,
    message: String,
}

impl FrameworkError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FrameworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for FrameworkError {}

/// Removes exactly one trailing slash, matching TypeScript `removeTrailingSlash`.
#[must_use]
pub fn remove_trailing_slash(path: &str) -> String {
    path.strip_suffix('/').unwrap_or(path).to_owned()
}

/// Removes one slash from a double leading slash, without adding a missing slash.
#[must_use]
pub fn remove_double_leading_slash(path: &str) -> String {
    if path.starts_with("//") {
        path[1..].to_owned()
    } else {
        path.to_owned()
    }
}

/// Applies TypeScript's single trailing/leading slash replacements in that order.
#[must_use]
pub fn sanitize_path_slashes(path: &str) -> String {
    remove_double_leading_slash(&remove_trailing_slash(path))
}

/// Native route normalization: adds a leading slash, collapses repeated leading
/// slashes and removes all trailing slashes while preserving `/` for the root.
/// Use the single-step helpers above when reproducing the TypeScript utilities.
#[must_use]
pub fn normalize_route_path(path: &str) -> String {
    let normalized = format!("/{}", path.trim_start_matches('/'));
    if normalized.len() > 1 {
        normalized.trim_end_matches('/').to_owned()
    } else {
        normalized
    }
}

/// Joins a route base path and child path while preserving root-path behavior.
#[must_use]
pub fn join_base_path(base_path: &str, child_path: &str) -> String {
    let base_path = normalize_route_path(base_path);
    let child_path = normalize_route_path(child_path);

    match (base_path.as_str(), child_path.as_str()) {
        ("/", "/") => "/".to_owned(),
        ("/", _) => child_path,
        (_, "/") => base_path,
        _ => format!("{base_path}{child_path}"),
    }
}

/// Matches TypeScript `openApiCompliantPath`: replace every colon followed by
/// ASCII word characters, including parameters embedded within path segments.
/// This is a string conversion, not a declaration of native router support.
#[must_use]
pub fn open_api_compliant_path(path: &str) -> String {
    let mut output = String::with_capacity(path.len());
    let mut characters = path.chars().peekable();
    while let Some(character) = characters.next() {
        if character == ':'
            && characters
                .peek()
                .is_some_and(|next| next.is_ascii_alphanumeric() || *next == '_')
        {
            output.push('{');
            while let Some(next) =
                characters.next_if(|next| next.is_ascii_alphanumeric() || *next == '_')
            {
                output.push(next);
            }
            output.push('}');
        } else {
            output.push(character);
        }
    }
    output
}

/// Native whole-segment parameter conversion, paired with native route matching.
/// Keep this separate from the TypeScript string utility above: mixed literal
/// and parameter segments require additional native matching support.
#[must_use]
pub fn route_open_api_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            segment
                .strip_prefix(':')
                .map_or_else(|| segment.to_owned(), |name| format!("{{{name}}}"))
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Converts arbitrary text into a conservative Rust/TypeScript-safe identifier.
#[must_use]
pub fn sanitize_identifier(input: &str) -> String {
    let mut output = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            output.push(ch);
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }

    let output = output.trim_matches('_');
    if output.is_empty() {
        return "_".to_owned();
    }

    if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        format!("_{output}")
    } else {
        output.to_owned()
    }
}

/// Escapes a string for inclusion in generated JSON.
#[must_use]
pub fn escape_json_string(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                let code = ch as u32;
                escaped.push_str("\\u");
                for shift in [12, 8, 4, 0] {
                    let nibble = ((code >> shift) & 0xf) as usize;
                    escaped.push(char::from(HEX[nibble]));
                }
            }
            ch => escaped.push(ch),
        }
    }
    escaped
}

/// Formats a JSON string literal.
#[must_use]
pub fn json_string(input: &str) -> String {
    format!("\"{}\"", escape_json_string(input))
}

/// Formats a JSON array from already-encoded JSON values.
#[must_use]
pub fn json_array(items: impl IntoIterator<Item = String>) -> String {
    let mut encoded = String::from("[");
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            encoded.push(',');
        }
        encoded.push_str(&item);
    }
    encoded.push(']');
    encoded
}

/// Formats a JSON array of string values.
#[must_use]
pub fn json_string_array(items: &[String]) -> String {
    json_array(items.iter().map(|item| json_string(item)))
}

/// A raw JSON object field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonField {
    key: String,
    value: String,
}

impl JsonField {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Formats a JSON object with deterministic key ordering for stable artifacts.
#[must_use]
pub fn json_object(fields: impl IntoIterator<Item = JsonField>) -> String {
    let mut fields = fields.into_iter().collect::<Vec<_>>();
    fields.sort_unstable_by(|left, right| left.key.cmp(&right.key));

    let mut encoded = String::from("{");
    for (index, field) in fields.into_iter().enumerate() {
        if index > 0 {
            encoded.push(',');
        }
        encoded.push_str(&json_string(&field.key));
        encoded.push(':');
        encoded.push_str(&field.value);
    }
    encoded.push('}');
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_openapi_paths() {
        assert_eq!(
            open_api_compliant_path("/users/:id/posts/:postId"),
            "/users/{id}/posts/{postId}"
        );
    }

    #[test]
    fn typescript_path_conversion_is_separate_from_native_route_segments() {
        assert_eq!(
            open_api_compliant_path("/files/:name.:ext/prefix:id/:café/:用户/:"),
            "/files/{name}.{ext}/prefix{id}/{caf}é/:用户/:"
        );
        assert_eq!(
            route_open_api_path("/files/:name.:ext/prefix:id/:café"),
            "/files/{name.:ext}/prefix:id/{café}"
        );
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(sanitize_path_slashes("//users/"), "/users");
    }

    #[test]
    fn joins_base_paths() {
        assert_eq!(join_base_path("/api/", "/users"), "/api/users");
        assert_eq!(join_base_path("/", "/users"), "/users");
        assert_eq!(join_base_path("/api", "/"), "/api");
    }

    #[test]
    fn pretty_camel_case_handles_separators() {
        assert_eq!(to_pretty_camel_case("HTTP-Response_id"), "httpResponseId");
    }

    #[test]
    fn sanitizes_identifiers() {
        assert_eq!(sanitize_identifier("123 account-id"), "_123_account_id");
        assert_eq!(sanitize_identifier("!!!"), "_");
    }

    #[test]
    fn formats_json_safely_and_deterministically() {
        assert_eq!(json_string("a\n\"b\""), "\"a\\n\\\"b\\\"\"");
        assert_eq!(
            json_object([
                JsonField::new("z", "1"),
                JsonField::new("a", json_string("first")),
            ]),
            "{\"a\":\"first\",\"z\":1}"
        );
    }

    #[test]
    fn framework_error_displays_code_and_message() {
        let error = FrameworkError::new("validation_error", "missing body");

        assert_eq!(error.code(), "validation_error");
        assert_eq!(error.to_string(), "validation_error: missing body");
    }
}

#[cfg(feature = "zod-checks")]
pub mod zod_string_format;

/// JavaScript numeric text for protocol fields.
#[cfg(feature = "javascript-number")]
pub mod javascript_number;

mod validation;
pub use validation::{
    ContractValidationError, ContractValidationLocation, ParseError, pretty_print_parse_errors,
};
