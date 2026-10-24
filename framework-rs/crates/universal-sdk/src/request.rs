use std::{collections::BTreeMap, fmt};

/// Marker for requests without multipart payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoMultipart;

#[derive(Clone, PartialEq, Eq)]
pub struct SdkRequest<M = NoMultipart> {
    pub method: &'static str,
    pub url: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    pub body_bytes: Option<Vec<u8>>,
    pub multipart: Option<M>,
    pub content_type: &'static str,
}

impl<M> fmt::Debug for SdkRequest<M> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SdkRequest")
            .field("method", &self.method)
            .field("has_url", &!self.url.is_empty())
            .field("query_fields", &self.query.keys().collect::<Vec<_>>())
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("has_body", &self.body.is_some())
            .field("has_body_bytes", &self.body_bytes.is_some())
            .field("has_multipart", &self.multipart.is_some())
            .finish_non_exhaustive()
    }
}

fn encode_path_segment(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

#[must_use]
pub fn path_with_params(path: &str, params: &BTreeMap<String, String>) -> String {
    path.split('/')
        .map(|segment| {
            let name = segment.strip_prefix(':').or_else(|| {
                segment
                    .strip_prefix('{')
                    .and_then(|name| name.strip_suffix('}'))
            });
            name.and_then(|name| params.get(name))
                .map_or_else(|| segment.to_owned(), |value| encode_path_segment(value))
        })
        .collect::<Vec<_>>()
        .join("/")
}

// Schema-typed request metadata uses JSON for structured values and literal scalar text.
/// Encode typed metadata fields without losing structured JSON values.
/// # Errors
/// Returns serialization failures from the supplied type.
pub fn request_fields<T: serde::Serialize>(
    value: &T,
) -> Result<BTreeMap<String, String>, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    let mut fields = BTreeMap::new();
    if let serde_json::Value::Object(values) = value {
        for (name, value) in values {
            let wire = match value {
                serde_json::Value::String(value) => value,
                value => value.to_string(),
            };
            fields.insert(name, wire);
        }
    }
    Ok(fields)
}
