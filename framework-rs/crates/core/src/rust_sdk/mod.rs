#![forbid(unsafe_code)]

mod http;
mod websocket;
pub use http::{generate_rust_http_sdk_files, generate_rust_http_sdk_source};
pub use websocket::generate_rust_websocket_sdk_source;

use std::fmt::Write as _;

use forklaunch_core::codegen::{BodyKind, SdkGenerationPlan, SdkOperation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustSdkOptions {
    pub crate_name: String,
    pub client_struct: String,
}

impl RustSdkOptions {
    #[must_use]
    pub fn new(crate_name: impl Into<String>) -> Self {
        Self {
            crate_name: crate_name.into(),
            client_struct: "ForkLaunchClient".to_owned(),
        }
    }

    #[must_use]
    pub fn with_client_struct(mut self, client_struct: impl Into<String>) -> Self {
        self.client_struct = client_struct.into();
        self
    }
}

#[must_use]
pub fn rust_plan_summary(plan: &SdkGenerationPlan) -> String {
    format!("rust sdk operations: {}", plan.operations.len())
}

#[must_use]
pub fn generate_rust_sdk_source(plan: &SdkGenerationPlan, options: &RustSdkOptions) -> String {
    let mut source = String::new();
    let _ = writeln!(source, "#![forbid(unsafe_code)]");
    let _ = writeln!(source);
    let _ = writeln!(source, "use std::collections::BTreeMap;");
    let _ = writeln!(source);
    let _ = writeln!(source, "#[derive(Clone, PartialEq, Eq)]");
    let _ = writeln!(source, "pub struct SdkRequest {{");
    let _ = writeln!(source, "    pub method: &'static str,");
    let _ = writeln!(source, "    pub url: String,");
    let _ = writeln!(source, "    pub query: BTreeMap<String, String>,");
    let _ = writeln!(source, "    pub headers: BTreeMap<String, String>,");
    let _ = writeln!(source, "    pub body: Option<String>,");
    let _ = writeln!(source, "    pub body_bytes: Option<Vec<u8>>,");
    let _ = writeln!(source, "    pub body_content_type: Option<String>,");
    let _ = writeln!(source, "    pub body_schema: Option<&'static str>,");
    let _ = writeln!(source, "}}");
    let _ = writeln!(source);
    source.push_str(
        r#"impl std::fmt::Debug for SdkRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SdkRequest")
            .field("method", &self.method)
            .field("has_url", &!self.url.is_empty())
            .field("query_fields", &self.query.keys().collect::<Vec<_>>())
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("has_body", &self.body.is_some())
            .field("has_body_bytes", &self.body_bytes.is_some())
            .finish_non_exhaustive()
    }
}
"#,
    );
    if plan.operations.iter().any(multipart_body) {
        source.push_str(include_str!("multipart.rs"));
        source.push('\n');
    }
    source.push_str(
        r#"fn encode_path_segment(value: &str) -> String {
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

fn path_with_params(path: &str, params: &BTreeMap<String, String>) -> String {
    path.split('/')
        .map(|segment| {
            let name = segment.strip_prefix(':').or_else(|| {
                segment.strip_prefix('{').and_then(|name| name.strip_suffix('}'))
            });
            name.and_then(|name| params.get(name))
                .map_or_else(|| segment.to_owned(), |value| encode_path_segment(value))
        })
        .collect::<Vec<_>>()
        .join("/")
}
"#,
    );
    let _ = writeln!(source);
    for operation in &plan.operations {
        write_request_struct(&mut source, operation);
    }

    let _ = writeln!(source, "#[derive(Clone)]");
    let _ = writeln!(source, "pub struct {} {{", options.client_struct);
    let _ = writeln!(source, "    base_url: String,");
    let _ = writeln!(source, "}}");
    let _ = writeln!(source);
    let _ = writeln!(
        source,
        "impl std::fmt::Debug for {} {{\n    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        formatter.debug_struct(\"{}\").field(\"has_base_url\", &!self.base_url.is_empty()).finish_non_exhaustive()\n    }}\n}}",
        options.client_struct, options.client_struct
    );
    let _ = writeln!(source, "impl {} {{", options.client_struct);
    let _ = writeln!(source, "    #[must_use]");
    let _ = writeln!(
        source,
        "    pub fn new(base_url: impl Into<String>) -> Self {{"
    );
    let _ = writeln!(
        source,
        "        Self {{ base_url: base_url.into().trim_end_matches('/').to_owned() }}"
    );
    let _ = writeln!(source, "    }}");

    for operation in &plan.operations {
        write_operation(&mut source, operation);
    }

    let _ = writeln!(source, "}}");
    source
}

fn write_operation(source: &mut String, operation: &SdkOperation) {
    let operation_ident = rust_identifier(&operation.name);
    let request_type = format!("{}Request", rust_type_name(&operation.name));
    let method_name = format!("{operation_ident}_request");
    let body_schema = operation
        .request_body
        .as_ref()
        .map_or_else(|| "None".to_owned(), |schema| format!("Some({schema:?})"));

    let _ = writeln!(source);
    let _ = writeln!(source, "    #[must_use]");
    let _ = writeln!(
        source,
        "    pub fn {method_name}(&self, input: {request_type}) -> SdkRequest {{"
    );
    let _ = writeln!(
        source,
        "        let path = path_with_params(\"{}\", &input.params);",
        escape_rust_string(&operation.path)
    );
    if multipart_body(operation) {
        source.push_str("        let mut headers = input.headers;\n        let body_bytes = input.body.map(|form| {\n            let (bytes, content_type) = form.encode();\n            headers.retain(|name, _| !name.eq_ignore_ascii_case(\"content-type\"));\n            headers.insert(\"content-type\".into(), content_type);\n            bytes\n        });\n        let body_content_type = body_bytes.as_ref().and_then(|_| headers.get(\"content-type\").cloned());\n");
    }
    let _ = writeln!(source, "        SdkRequest {{");
    let _ = writeln!(
        source,
        "            method: \"{}\",",
        operation.method.as_str()
    );
    let _ = writeln!(
        source,
        "            url: format!(\"{{}}{{}}\", self.base_url, path),"
    );
    let _ = writeln!(source, "            query: input.query,");
    if multipart_body(operation) {
        source.push_str("            headers,\n            body: None,\n            body_bytes,\n");
    } else if binary_body(operation) {
        let _ = writeln!(source, "            headers: input.headers,");
        let _ = writeln!(source, "            body: None,");
        let _ = writeln!(source, "            body_bytes: input.body,");
    } else {
        let _ = writeln!(source, "            headers: input.headers,");
        let _ = writeln!(source, "            body: input.body,");
        let _ = writeln!(source, "            body_bytes: None,");
    }
    let default_content_type = match operation.request_body_kind {
        Some(BodyKind::Text) => "text/plain",
        Some(BodyKind::Binary | BodyKind::File) => "application/octet-stream",
        Some(BodyKind::UrlEncoded) => "application/x-www-form-urlencoded",
        Some(BodyKind::Multipart) => "multipart/form-data",
        Some(BodyKind::ServerSentEvent) => "text/event-stream",
        _ => "application/json",
    };
    let content_type = operation.request_body.as_ref().map(|_| {
        operation
            .request_body_content_type
            .as_deref()
            .unwrap_or(default_content_type)
    });
    let content_type = if multipart_body(operation) {
        "body_content_type".into()
    } else {
        content_type.map_or_else(|| "None".into(), |mime| format!("Some({mime:?}.into())"))
    };
    let _ = writeln!(source, "            body_content_type: {content_type},");
    let _ = writeln!(source, "            body_schema: {body_schema},");
    let _ = writeln!(source, "        }}");
    let _ = writeln!(source, "    }}");
}

fn write_request_struct(source: &mut String, operation: &SdkOperation) {
    let request_type = format!("{}Request", rust_type_name(&operation.name));
    let _ = writeln!(source, "#[derive(Clone, Default, PartialEq, Eq)]");
    let _ = writeln!(source, "pub struct {request_type} {{");
    let _ = writeln!(source, "    pub params: BTreeMap<String, String>,");
    let _ = writeln!(source, "    pub query: BTreeMap<String, String>,");
    let _ = writeln!(source, "    pub headers: BTreeMap<String, String>,");
    let body_type = if multipart_body(operation) {
        "MultipartForm"
    } else if binary_body(operation) {
        "Vec<u8>"
    } else {
        "String"
    };
    let _ = writeln!(source, "    pub body: Option<{body_type}>,");
    let _ = writeln!(source, "}}");
    let _ = writeln!(source, "impl std::fmt::Debug for {request_type} {{");
    source.push_str(
        "    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n",
    );
    let _ = writeln!(source, "        formatter.debug_struct({request_type:?})");
    source.push_str("            .field(\"param_names\", &self.params.keys().collect::<Vec<_>>())\n            .field(\"query_names\", &self.query.keys().collect::<Vec<_>>())\n            .field(\"header_names\", &self.headers.keys().collect::<Vec<_>>())\n            .field(\"has_body\", &self.body.is_some())\n            .finish_non_exhaustive()\n    }\n}\n");
    let _ = writeln!(source);
}

fn multipart_body(operation: &SdkOperation) -> bool {
    matches!(operation.request_body_kind, Some(BodyKind::Multipart))
}

fn binary_body(operation: &SdkOperation) -> bool {
    matches!(
        operation.request_body_kind,
        Some(BodyKind::Binary | BodyKind::File)
    )
}

fn rust_identifier(input: &str) -> String {
    let mut output = String::new();
    for (index, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 && !output.ends_with('_') {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_alphanumeric() {
            output.push(ch);
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }

    let output = output.trim_matches('_');
    if output.is_empty() {
        return "operation".to_owned();
    }

    if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        format!("operation_{output}")
    } else {
        output.to_owned()
    }
}

fn rust_type_name(input: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                output.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                output.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }

    if output.is_empty() {
        "Operation".to_owned()
    } else if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        format!("Operation{output}")
    } else {
        output
    }
}

fn escape_rust_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use forklaunch_core::codegen::SdkTarget;
    use forklaunch_core::contract::{BodyContract, FieldContract, HttpMethod, RouteContract};

    use super::*;

    #[test]
    fn native_sdk_preserves_body_kinds_and_wire_content_types() {
        use forklaunch_core::contract::{BodyKind, SchemaRef};
        let mut registry = forklaunch_core::contract::ContractRegistry::new();
        for (name, kind, mime) in [
            ("json", BodyKind::Json, "application/vnd.forklaunch+json"),
            ("text", BodyKind::Text, "text/plain; charset=utf-8"),
            ("binary", BodyKind::Binary, "application/octet-stream"),
            ("file", BodyKind::File, "image/png"),
            ("form", BodyKind::Multipart, "multipart/form-data"),
            (
                "urlencoded",
                BodyKind::UrlEncoded,
                "application/x-www-form-urlencoded",
            ),
        ] {
            registry.register(
                RouteContract::new(
                    format!("post {name}"),
                    "Media upload",
                    HttpMethod::Post,
                    format!("/{name}"),
                )
                .with_request_body(BodyContract {
                    kind,
                    schema: SchemaRef::named("Payload"),
                    content_type: Some(mime.into()),
                })
                .with_response(200, None),
            );
        }
        let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
        assert_eq!(plan.operations[2].request_body_kind, Some(BodyKind::Binary));
        let source = generate_rust_sdk_source(&plan, &RustSdkOptions::new("media-sdk"));
        assert!(source.contains("pub body: Option<Vec<u8>>"));
        assert!(source.contains("body_bytes: input.body"));
        assert!(source.contains("body_content_type: Some(\"image/png\".into())"));
        if let Ok(directory) = std::env::var("FORKLAUNCH_TEST_NATIVE_SDK_OUTPUT") {
            std::fs::write(
                std::path::Path::new(&directory).join("native-sdk.rs"),
                source,
            )
            .unwrap();
        }
    }

    #[test]
    fn generates_request_builders_for_rust_sdk() {
        let mut registry = forklaunch_core::contract::ContractRegistry::new();
        registry.register(
            RouteContract::new(
                "Create User",
                "Create a user",
                HttpMethod::Post,
                "/users/:id",
            )
            .with_param(FieldContract::required("id", "String"))
            .with_query(FieldContract::optional("dryRun", "Boolean"))
            .with_request_body(BodyContract::json("CreateUserBody"))
            .with_response(201, Some(BodyContract::json("User"))),
        );
        let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);

        let source = generate_rust_sdk_source(&plan, &RustSdkOptions::new("accounts-sdk"));

        assert!(source.contains("pub struct ForkLaunchClient"));
        assert!(source.contains("pub struct CreateUserRequest"));
        assert!(source.contains("pub fn create_user_request(&self, input: CreateUserRequest)"));
        assert!(source.contains("path_with_params"));
        assert!(source.contains("method: \"POST\""));
        assert!(source.contains("query: input.query"));
        assert!(source.contains("body_schema: Some(\"CreateUserBody\")"));
    }
}
