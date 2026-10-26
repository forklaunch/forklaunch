#![forbid(unsafe_code)]

mod http;
mod portable;
#[cfg(feature = "build")]
pub(crate) use http::generate_typescript_http_sdk_files_with_runtime;
pub use portable::{
    generate_typescript_operation_descriptors, generate_typescript_service_declarations,
};
mod websocket;
pub use http::{
    TypeScriptHttpSdkOptions, generate_typescript_http_sdk_files,
    generate_typescript_http_sdk_source,
};
pub use websocket::{
    TypeScriptWebSocketSdkOptions, generate_typescript_browser_websocket_sdk_source,
    generate_typescript_websocket_sdk_source,
};

use forklaunch_common::json_string as js_string;
use std::fmt::Write as _;

use forklaunch_core::codegen::{BodyKind, SdkGenerationPlan, SdkOperation};

mod schema_runtime;
pub use schema_runtime::typescript_schema_runtime_source;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScriptSdkOptions {
    pub client_name: String,
    pub import_fetch: bool,
}

impl TypeScriptSdkOptions {
    #[must_use]
    pub fn new(client_name: impl Into<String>) -> Self {
        Self {
            client_name: client_name.into(),
            import_fetch: false,
        }
    }

    #[must_use]
    pub const fn with_fetch_import(mut self, import_fetch: bool) -> Self {
        self.import_fetch = import_fetch;
        self
    }
}

#[must_use]
pub fn typescript_plan_summary(plan: &SdkGenerationPlan) -> String {
    format!("typescript sdk operations: {}", plan.operations.len())
}

#[must_use]
pub fn generate_typescript_sdk_source(
    plan: &SdkGenerationPlan,
    options: &TypeScriptSdkOptions,
) -> String {
    let mut source = String::new();
    if options.import_fetch {
        let _ = writeln!(source, "import fetch from 'cross-fetch';");
        let _ = writeln!(source);
    }

    let _ = writeln!(
        source,
        "export type SdkQueryValue = string | number | boolean | null | readonly SdkQueryValue[] | {{ readonly [key: string]: SdkQueryValue }} | undefined;"
    );
    let _ = writeln!(
        source,
        "export type SdkPathValue = string | number | boolean;"
    );
    let _ = writeln!(source);
    let _ = writeln!(source, "export interface SdkRequestOptions {{");
    let _ = writeln!(source, "  params?: Record<string, SdkPathValue>;");
    let _ = writeln!(source, "  query?: Record<string, SdkQueryValue>;");
    let _ = writeln!(source, "  body?: unknown;");
    let _ = writeln!(source, "  headers?: Record<string, string>;");
    let _ = writeln!(source, "}}");
    let _ = writeln!(source);
    let _ = writeln!(
        source,
        "function pathWithParams(path: string, params?: Record<string, SdkPathValue>) {{"
    );
    let _ = writeln!(source, "  if (!params) return path;");
    source.push_str(
        r"  return path.split('/').map(segment => {
    const name = segment.startsWith(':') ? segment.slice(1)
      : segment.startsWith('{') && segment.endsWith('}') ? segment.slice(1, -1) : undefined;
    return name !== undefined && Object.prototype.hasOwnProperty.call(params, name)
      ? encodeURIComponent(String(params[name])) : segment;
  }).join('/');
",
    );
    let _ = writeln!(source, "}}");
    let _ = writeln!(source);
    source.push_str(include_str!("transport.ts"));
    let _ = writeln!(source, "export class {} {{", options.client_name);
    let _ = writeln!(
        source,
        "  constructor(private readonly baseUrl: string, private readonly fetcher: typeof fetch = fetch) {{}}"
    );

    for operation in &plan.operations {
        write_operation(&mut source, operation);
    }

    let _ = writeln!(source, "}}");
    source
}

fn write_operation(source: &mut String, operation: &SdkOperation) {
    let options_default = if operation.request_body.is_none()
        && operation.request_params.iter().all(|field| !field.required)
    {
        " = {}"
    } else {
        ""
    };
    let (body_expression, body_type, default_content_type) = match operation.request_body_kind {
        Some(BodyKind::Json | BodyKind::Unknown) => (
            "options.body === undefined ? undefined : JSON.stringify(options.body)",
            "unknown",
            "application/json",
        ),
        Some(BodyKind::Text) => ("options.body", "string", "text/plain"),
        Some(BodyKind::Binary | BodyKind::File) => (
            "options.body instanceof Uint8Array ? Uint8Array.from(options.body).buffer : options.body",
            "Uint8Array | ArrayBuffer | Blob",
            "application/octet-stream",
        ),
        Some(BodyKind::Multipart) => ("options.body", "FormData", ""),
        Some(BodyKind::UrlEncoded) => (
            "options.body",
            "URLSearchParams",
            "application/x-www-form-urlencoded",
        ),
        Some(BodyKind::ServerSentEvent) | None => ("undefined", "never", ""),
    };
    let content_type = operation
        .request_body_content_type
        .as_deref()
        .unwrap_or(default_content_type);
    // Multipart boundaries belong to the browser's FormData encoder.
    let content_type = if matches!(operation.request_body_kind, Some(BodyKind::Multipart)) {
        ""
    } else {
        content_type
    };
    let request_options = format!("Omit<SdkRequestOptions, 'body'> & {{ body?: {body_type} }}");

    let _ = writeln!(source);
    let _ = writeln!(
        source,
        "  async {}(options: {}{}): Promise<Response> {{",
        typescript_identifier(&operation.name),
        request_options,
        options_default
    );
    if matches!(operation.request_body_kind, Some(BodyKind::ServerSentEvent)) {
        let _ = writeln!(
            source,
            "    throw new TypeError('SSE request bodies are unsupported');"
        );
    }
    let _ = writeln!(
        source,
        "    const path = pathWithParams(`{}`, options.params);",
        escape_template_literal(&operation.path)
    );
    let _ = writeln!(
        source,
        "    const query = options.query ? `?${{new URLSearchParams(Object.entries(options.query).filter(([, value]) => value !== undefined).map(([key, value]) => [key, typeof value === 'object' ? JSON.stringify(value) : String(value)]))}}` : '';"
    );
    source.push_str("    const headers = new Headers(options.headers);\n");
    if matches!(operation.request_body_kind, Some(BodyKind::Multipart)) {
        source.push_str("    headers.delete('content-type');\n");
    } else if !content_type.is_empty() {
        let _ = writeln!(
            source,
            "    if (options.body !== undefined && !headers.has('content-type')) headers.set('content-type', {});",
            js_string(content_type)
        );
    }
    let _ = writeln!(
        source,
        "    const body = await compatibleFetchBody({body_expression}, headers, this.fetcher);"
    );
    let _ = writeln!(
        source,
        "    return this.fetcher(`${{this.baseUrl}}${{path}}${{query}}`, {{"
    );
    let _ = writeln!(source, "      method: '{}',", operation.method.as_str());
    source.push_str("      headers,\n      body,\n");
    let _ = writeln!(source, "    }});");
    let _ = writeln!(source, "  }}");
}

fn typescript_identifier(input: &str) -> String {
    let mut output = String::new();
    for (index, ch) in input.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if index == 0 && ch.is_ascii_digit() {
                output.push('_');
            }
            output.push(ch);
        }
    }

    if output.is_empty() {
        "operation".to_owned()
    } else {
        output
    }
}

fn escape_template_literal(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
}

#[cfg(test)]
mod tests {
    use forklaunch_core::codegen::SdkTarget;
    use forklaunch_core::contract::{BodyContract, FieldContract, HttpMethod, RouteContract};

    use super::*;

    fn echo_body() -> BodyContract {
        BodyContract {
            schema: forklaunch_core::contract::SchemaRef::named("Echo").with_json_schema(r#"{"type":"object","required":["method","body","contentType"],"properties":{"method":{"type":"string"},"body":{"type":"string"},"contentType":{"type":"string"}}}"#),
            ..BodyContract::json("Echo")
        }
    }

    fn register_response_fixture(registry: &mut forklaunch_core::contract::ContractRegistry) {
        use forklaunch_core::contract::{BodyKind, SchemaRef};
        for (name, path, kind, mime, schema) in [
            (
                "Read Text",
                "/responses/text",
                BodyKind::Text,
                "text/plain; charset=utf-8",
                r#"{"type":"string"}"#,
            ),
            (
                "Read Binary",
                "/responses/binary",
                BodyKind::Binary,
                "application/octet-stream",
                r#"{"type":"string"}"#,
            ),
            (
                "Read Null",
                "/responses/null",
                BodyKind::Json,
                "application/json",
                r#"{"type":"null"}"#,
            ),
            (
                "Read Invalid Json",
                "/responses/invalid",
                BodyKind::Json,
                "application/json",
                r#"{"type":"object"}"#,
            ),
        ] {
            registry.register(
                RouteContract::get(name, "Typed response fixture", path).with_response(
                    200,
                    Some(BodyContract {
                        kind,
                        content_type: Some(mime.into()),
                        schema: SchemaRef::named("Payload").with_json_schema(schema),
                    }),
                ),
            );
        }
        registry.register(
            RouteContract::get("Read Empty", "Empty response", "/responses/empty")
                .with_response(204, None),
        );
        registry.register(
            RouteContract::new(
                "Read Head",
                "HEAD response",
                HttpMethod::Head,
                "/responses/head",
            )
            .with_response(200, Some(echo_body())),
        );
        registry.register(RouteContract::get("Read Error", "Business error", "/responses/error").with_response(409, Some(BodyContract {
            schema: SchemaRef::named("Conflict").with_json_schema(r#"{"type":"object","required":["errorCode"],"properties":{"errorCode":{"type":"string"}}}"#),
            content_type: Some("application/problem+json".into()), ..BodyContract::json("Conflict")
        })));
        registry.register(RouteContract::get("Read Several", "Multiple success responses", "/responses/several/:mode")
            .with_param(FieldContract::required("mode", "String"))
            .with_response(200, Some(echo_body()))
            .with_response(201, Some(BodyContract { schema: SchemaRef::named("Created").with_json_schema(r#"{"type":"object","required":["createdAt"],"properties":{"createdAt":{"type":"string"}}}"#), ..BodyContract::json("Created") }))
            .with_response(202, Some(BodyContract { schema: SchemaRef::named("Queued").with_json_schema(r#"{"type":"object","required":["jobId"],"properties":{"jobId":{"type":"integer"}}}"#), ..BodyContract::json("Queued") }))
            .with_response(204, None));
    }

    #[test]
    fn native_sdk_preserves_body_kinds_and_wire_content_types() {
        use forklaunch_core::contract::{BodyKind, SchemaRef};
        let mut registry = forklaunch_core::contract::ContractRegistry::new();
        for (name, kind, mime) in [
            (
                "json",
                BodyKind::Json,
                "application/vnd.forklaunch+json; profile=\"https://example.test/v1\"",
            ),
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
                    if name == "json" {
                        "/json/a'b&c".into()
                    } else {
                        format!("/{name}")
                    },
                )
                .with_request_body(BodyContract {
                    kind,
                    schema: SchemaRef::named("Payload"),
                    content_type: Some(mime.into()),
                })
                .with_response(200, Some(echo_body())),
            );
        }
        let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
        assert_eq!(plan.operations[2].request_body_kind, Some(BodyKind::Binary));
        let source =
            generate_typescript_sdk_source(&plan, &TypeScriptSdkOptions::new("MediaClient"));
        assert!(source.contains("Uint8Array.from(options.body).buffer"));
        assert!(source.contains("body?: string"));
        assert!(source.contains("body?: FormData"));
        assert!(source.contains("body?: URLSearchParams"));
        assert!(!source.contains("'content-type': \"multipart/form-data\""));
        register_response_fixture(&mut registry);
        let typed = generate_typescript_http_sdk_source(
            &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
            &TypeScriptHttpSdkOptions::new("media"),
        )
        .unwrap();
        if let Ok(directory) = std::env::var("FORKLAUNCH_TEST_NATIVE_SDK_OUTPUT") {
            std::fs::write(
                std::path::Path::new(&directory).join("native-sdk.ts"),
                source,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("typed-native-sdk.ts"),
                typed,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("native-sdk-import.ts"),
                generate_typescript_sdk_source(
                    &plan,
                    &TypeScriptSdkOptions::new("MediaClient").with_fetch_import(true),
                ),
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join("typed-native-sdk-import.ts"),
                generate_typescript_http_sdk_source(
                    &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
                    &TypeScriptHttpSdkOptions::new("media").with_fetch_import(true),
                )
                .unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn typed_generator_rejects_invalid_contracts_before_emitting_source() {
        use forklaunch_core::contract::SchemaRef;
        let make_plan = |body: BodyContract| {
            let mut registry = forklaunch_core::contract::ContractRegistry::new();
            registry.register(
                RouteContract::get("Read Item", "Read", "/items")
                    .with_request_body(body)
                    .with_response(200, Some(BodyContract::json("String"))),
            );
            SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry)
        };
        let options = TypeScriptHttpSdkOptions::new("items");
        for schema in [
            r#"{"$ref":"https://example.test/schema"}"#,
            r##"{"$ref":"#/$defs/A","$defs":{"A":{"$ref":"#/$defs/A"}}}"##,
        ] {
            let plan = make_plan(BodyContract {
                schema: SchemaRef::named("Body").with_json_schema(schema),
                ..BodyContract::json("Body")
            });
            assert!(generate_typescript_http_sdk_source(&plan, &options).is_err());
        }
        let mut plan = make_plan(BodyContract::json("String"));
        assert!(
            generate_typescript_http_sdk_source(&plan, &TypeScriptHttpSdkOptions::new("bad.name"))
                .is_err()
        );
        plan.operations[0].request_body_content_type =
            Some("application/json\r\nX-Injected: yes".into());
        assert!(generate_typescript_http_sdk_source(&plan, &options).is_err());
        plan.operations[0].request_body_content_type = None;
        plan.operations[0].responses[0].status_code = 700;
        assert!(generate_typescript_http_sdk_source(&plan, &options).is_err());
        plan.operations[0].responses[0].status_code = 200;
        plan.operations.push(plan.operations[0].clone());
        assert!(generate_typescript_http_sdk_source(&plan, &options).is_err());
    }

    #[test]
    fn native_query_transport_emits_composite_query_types_and_serialization() {
        let mut registry = forklaunch_core::contract::ContractRegistry::new();
        registry.register(
            RouteContract::get("Read Query", "Read query", "/query/:id")
                .with_param(FieldContract::required("id", "String")),
        );
        let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
        let source =
            generate_typescript_sdk_source(&plan, &TypeScriptSdkOptions::new("QueryClient"));
        assert!(source.contains("readonly SdkQueryValue[]"));
        assert!(source.contains("JSON.stringify(value)"));
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_QUERY_TRANSPORT_OUTPUT") {
            std::fs::write(std::path::Path::new(&output).join("query-sdk.ts"), source).unwrap();
        }
    }

    #[test]
    fn generates_frontend_client_matching_live_typed_syntax() {
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
        let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);

        let source = generate_typescript_sdk_source(&plan, &TypeScriptSdkOptions::new("ApiClient"));

        assert!(source.contains("export class ApiClient"));
        assert!(source.contains("pathWithParams"));
        assert!(source.contains("createUser(options: Omit<SdkRequestOptions, 'body'>"));
        assert!(source.contains("options.query"));
        assert!(source.contains("method: 'POST'"));
        assert!(source.contains("JSON.stringify(options.body)"));
    }
}

#[cfg(all(test, feature = "rust-sdk", feature = "openapi"))]
mod snapshots;
