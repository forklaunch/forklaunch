//! Scalar sources and safe HTML rendering for the built-in documentation UIs.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) fn document_hash(document: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hasher::write(&mut hasher, document.as_bytes());
    format!("{:016x}", std::hash::Hasher::finish(&hasher))
}

/// A Scalar source loads either a remote document or an inline JSON document.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ScalarDocument {
    Url { url: String },
    Content { content: Value },
}

/// Additional API reference document, following Scalar's `sources` format.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScalarSource {
    pub title: String,
    #[serde(flatten)]
    pub document: ScalarDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

impl ScalarSource {
    #[must_use]
    pub fn url(title: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            document: ScalarDocument::Url { url: url.into() },
            slug: None,
            default: None,
        }
    }

    #[must_use]
    pub fn inline(title: impl Into<String>, content: Value) -> Self {
        Self {
            title: title.into(),
            document: ScalarDocument::Content { content },
            slug: None,
            default: None,
        }
    }
}

fn html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn script_json(value: &Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

pub(crate) fn scalar_html(title: &str, spec_url: &str, extra: &[ScalarSource]) -> String {
    let sources = std::iter::once(ScalarSource::url("API Reference", spec_url))
        .chain(extra.iter().cloned())
        .collect::<Vec<_>>();
    let configuration = script_json(&json!({"sources":sources}));
    let title = html_text(title);
    format!(
        "<!doctype html>\n<html>\n  <head>\n    <title>{title}</title>\n    <meta charset=\"utf-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n  </head>\n  <body>\n    <div id=\"app\"></div>\n    <script src=\"https://cdn.jsdelivr.net/npm/@scalar/api-reference\"></script>\n    <script>Scalar.createApiReference('#app', {configuration});</script>\n  </body>\n</html>\n"
    )
}

pub(crate) fn swagger_html(title: &str, spec_url: &str) -> String {
    let title = html_text(title);
    let spec_url = script_json(&json!(spec_url));
    format!(
        "<!doctype html>\n<html>\n  <head>\n    <title>{title}</title>\n    <meta charset=\"utf-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n    <link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/swagger-ui-dist/swagger-ui.css\" />\n  </head>\n  <body>\n    <div id=\"swagger-ui\"></div>\n    <script src=\"https://cdn.jsdelivr.net/npm/swagger-ui-dist/swagger-ui-bundle.js\"></script>\n    <script>\n      window.onload = () => {{ window.ui = SwaggerUIBundle({{ url: {spec_url}, dom_id: '#swagger-ui' }}); }};\n    </script>\n  </body>\n</html>\n"
    )
}
