#![forbid(unsafe_code)]

mod artifact;
mod identity;
pub mod media;
mod models;
mod portable_date;
mod portable_integer;
pub mod render;
#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
pub(crate) mod request_objects;
pub mod response;
#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
pub(crate) mod runtime_schemas;
pub(crate) mod schema;
mod sse_models;
#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
pub(crate) mod version_responses;
pub(crate) mod versions;
pub mod websocket;

pub use models::{
    RustModelFieldTemplate, RustModelTemplate, RustTypeAliasTemplate, RustUnionVariantTemplate,
    render_rust_models,
};

/// Resolve bundled resource IDs and dynamic scopes into document-local references.
/// Resource-free schemas retain their original annotations and pointer layout.
/// This prepares standalone schemas for relocation into another document.
/// Mixed supported drafts are translated to 2020-12.
///
/// # Errors
/// Rejects unsupported scopes, missing resources, unguarded recursion and
/// schema expansion limits. No network or file retrieval occurs.
pub fn bundle_schema_resources(value: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    schema::scoped_runtime_schema(value)
}

/// Extract a schema from an `OpenAPI` or `AsyncAPI` document, resolving bundled
/// resource identities and preserving guarded recursive references.
/// Resource-free documents retain the existing literal-reference bundler.
///
/// # Errors
/// Rejects missing or unsupported references, invalid resource scopes and
/// shared depth/node/byte limits. Remote resources are never retrieved.
pub fn bundle_document_schema_resources(
    document: &serde_json::Value,
    location: &str,
) -> anyhow::Result<serde_json::Value> {
    if let Some(schema) = schema::bundle_document_resources(document, location)? {
        Ok(schema)
    } else {
        forklaunch_core::contract::bundle_document_schema(document, location)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

/// A bounded schema graph compiled into both client languages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaModels {
    pub rust_models: Vec<RustModelTemplate>,
    pub rust_aliases: Vec<RustTypeAliasTemplate>,
    pub rust_type: String,
    pub typescript_type: String,
    pub typescript_definitions: std::collections::BTreeMap<String, String>,
}

/// Resolve an explicit model identity into its checked public type name.
///
/// # Errors
/// Rejects malformed compiler identity metadata.
pub fn model_identity(value: &serde_json::Value) -> anyhow::Result<Option<(String, String)>> {
    identity::model(value)
}

/// Compile local JSON Schema references, retaining guarded recursive graph edges.
///
/// # Errors
/// Rejects unsupported references, unguarded aliases and schema expansion limits.
pub fn compile_schema(root_name: &str, value: &serde_json::Value) -> anyhow::Result<SchemaModels> {
    let ordered = canonical_json(value.clone());
    let value = &ordered;
    portable_date::check_wire(value)?;
    let mut value = schema::normalize(
        value,
        // Keep cross-language graph names stable when Rust escapes `Self`.
        &format!(
            "{}Schema",
            models::rust_type_name(root_name).trim_end_matches('_')
        ),
    )?;
    value.sort_all_objects();
    portable_integer::check(&value)?;
    portable_date::check(&value)?;
    let legacy_models = models::rust_types_from_schema(root_name, &value).0;
    let mut graph_aliases = identity::canonicalize(&mut value)?;
    for model in legacy_models {
        if !model.identity_name.is_empty()
            && model.name != model.identity_name
            && model.name != models::rust_type_name(root_name)
        {
            graph_aliases.insert(model.name, model.identity_name);
        }
    }
    if identity::model(&value)?.is_some_and(|(_, name)| name == root_name)
        && let Some(definitions) = value
            .get_mut(schema::DEFINITIONS)
            .and_then(serde_json::Value::as_object_mut)
    {
        definitions.remove(root_name);
    }

    let (rust_models, mut rust_aliases, rust_type) =
        models::rust_types_from_schema(root_name, &value);
    rust_aliases.extend(
        graph_aliases
            .iter()
            .filter(|(old, target)| old != target)
            .map(|(old, target)| RustTypeAliasTemplate {
                name: old.clone(),
                target_type: target.clone(),
            }),
    );
    let mut typescript_definitions: std::collections::BTreeMap<String, String> = value
        .get(schema::DEFINITIONS)
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, value)| (name.clone(), models::ts_type_from_schema(value)))
        .collect();
    typescript_definitions.extend(
        graph_aliases
            .into_iter()
            .filter(|(old, target)| old != target),
    );
    Ok(SchemaModels {
        rust_models,
        rust_aliases,
        rust_type,
        typescript_type: models::ts_type_from_schema(&value),
        typescript_definitions,
    })
}

/// Compile a contract schema, using known primitive names when JSON Schema is absent.
///
/// # Errors
/// Returns schema parsing or bounded graph compilation failures.
pub fn compile_schema_ref(
    root_name: &str,
    schema: &forklaunch_core::contract::SchemaRef,
) -> anyhow::Result<SchemaModels> {
    let value = if let Some(json) = &schema.json_schema {
        if json.len() > 32 * 1024 * 1024 {
            anyhow::bail!("SDK schema graph exceeds 32 MiB");
        }
        serde_json::from_str(json)?
    } else {
        match models::ts_type_from_schema_name(&schema.name).as_str() {
            "string" => serde_json::json!({"type": "string"}),
            "number" => serde_json::json!({"type": "number"}),
            "boolean" => serde_json::json!({"type": "boolean"}),
            _ => serde_json::json!({}),
        }
    };
    identity::compile(root_name, &value)
}

/// Escape data embedded in a single-quoted TypeScript string literal.
#[must_use]
pub fn escape_typescript_string(value: &str) -> String {
    models::escape_ts_string(value)
}

pub use forklaunch_core::contract::{BodyKind, SchemaRef};
use forklaunch_core::contract::{ContractRegistry, HttpMethod};
#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
pub(crate) use identity::validate as validate_model_identities;
#[cfg(feature = "build")]
pub(crate) use identity::validate_schemas as validate_schema_identities;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkTarget {
    TypeScript,
    Rust,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkGenerationPlan {
    pub target: SdkTarget,
    pub operations: Vec<SdkOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkOperation {
    pub version_branches: Vec<SdkOperation>,
    pub request_parser_options: Option<String>,
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub request_params: Vec<SdkField>,
    pub request_query: Vec<SdkField>,
    pub request_headers: Vec<SdkField>,
    pub request_query_schema: Option<String>,
    pub request_headers_schema: Option<String>,
    pub request_body: Option<String>,
    pub request_body_optional: bool,
    pub request_body_json_schema: Option<String>,
    pub request_body_kind: Option<BodyKind>,
    pub request_body_content_type: Option<String>,
    pub responses: Vec<SdkResponse>,
    pub response_status_codes: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkField {
    pub name: String,
    pub schema: String,
    pub json_schema: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkResponse {
    pub status_code: u16,
    pub body: Option<String>,
    pub body_json_schema: Option<String>,
    pub body_kind: Option<BodyKind>,
    pub body_content_type: Option<String>,
    pub headers: Vec<SdkField>,
}

impl SdkGenerationPlan {
    #[must_use]
    pub fn from_registry(target: SdkTarget, registry: &ContractRegistry) -> Self {
        let mut plan = Self {
            target,
            operations: registry
                .routes()
                .iter()
                .flat_map(|route| {
                    if route.versions.is_empty() {
                        return vec![route.clone()];
                    }
                    route
                        .versions
                        .keys()
                        .filter_map(|version| {
                            let mut projected = route.for_version(version)?;
                            projected.operation_name =
                                versions::operation_name(&route.operation_name, version);
                            Some(projected)
                        })
                        .collect()
                })
                .map(|route| SdkOperation {
                    version_branches: Vec::new(),
                    request_parser_options: (route.options.body_parser
                        != crate::contract::BodyParserOptions::default())
                    .then(|| route.options.body_parser.to_json()),
                    name: route.operation_name.clone(),
                    method: route.method,
                    path: route.path.clone(),
                    request_params: route.request.params.iter().map(SdkField::from).collect(),
                    request_query: route.request.query.iter().map(SdkField::from).collect(),
                    request_headers: route.request.headers.iter().map(SdkField::from).collect(),
                    request_query_schema: route
                        .request
                        .query_schema
                        .as_ref()
                        .and_then(|s| s.json_schema.clone()),
                    request_headers_schema: route
                        .request
                        .headers_schema
                        .as_ref()
                        .and_then(|s| s.json_schema.clone()),
                    request_body_optional: route.request.body_optional,
                    request_body: route
                        .request
                        .body
                        .as_ref()
                        .map(|body| body.schema.name.clone()),
                    request_body_json_schema: route
                        .request
                        .body
                        .as_ref()
                        .and_then(|body| body.schema.json_schema.clone()),
                    request_body_kind: route.request.body.as_ref().map(|body| body.kind),
                    request_body_content_type: route
                        .request
                        .body
                        .as_ref()
                        .and_then(|body| body.content_type.clone()),
                    responses: route.responses.iter().map(SdkResponse::from).collect(),
                    response_status_codes: route
                        .responses
                        .iter()
                        .map(|response| response.status_code)
                        .collect(),
                })
                .collect(),
        };
        for route in registry
            .routes()
            .iter()
            .filter(|route| !route.versions.is_empty())
        {
            let names: Vec<_> = route
                .versions
                .keys()
                .map(|version| versions::operation_name(&route.operation_name, version))
                .collect();
            let branches: Vec<_> = plan
                .operations
                .iter()
                .filter(|op| names.contains(&op.name) && op.path == route.path)
                .cloned()
                .collect();
            for operation in &mut plan.operations {
                if names.contains(&operation.name) && operation.path == route.path {
                    operation.version_branches.clone_from(&branches);
                }
            }
        }
        plan
    }
}

impl From<&crate::contract::ResponseContract> for SdkResponse {
    fn from(response: &crate::contract::ResponseContract) -> Self {
        Self {
            status_code: response.status_code,
            body: response.body.as_ref().map(|body| body.schema.name.clone()),
            body_json_schema: response
                .body
                .as_ref()
                .and_then(|body| body.schema.json_schema.clone()),
            body_kind: response.body.as_ref().map(|body| body.kind),
            body_content_type: response
                .body
                .as_ref()
                .and_then(|body| body.content_type.clone()),
            headers: response.headers.iter().map(SdkField::from).collect(),
        }
    }
}

impl From<&forklaunch_core::contract::FieldContract> for SdkField {
    fn from(field: &forklaunch_core::contract::FieldContract) -> Self {
        Self {
            name: field.name.clone(),
            schema: field.schema.name.clone(),
            json_schema: field.schema.json_schema.clone(),
            required: field.required,
        }
    }
}

/// Canonical object ordering independent of `serde_json` feature unification.
pub(crate) fn canonical_json(mut value: serde_json::Value) -> serde_json::Value {
    value.sort_all_objects();
    value
}

#[cfg(test)]
mod tests {
    use forklaunch_core::contract::{BodyContract, FieldContract, HttpMethod, RouteContract};

    use super::*;

    #[test]
    fn reserved_rust_root_preserves_typescript_graph_names() {
        use serde_json::json;
        let compiled = crate::codegen::compile_schema(
            "Self",
            &json!({
                "$schema":"https://json-schema.org/draft/2020-12/schema",
                "$defs":{"Self":{"type":"object","properties":{"next":{"$ref":"#/$defs/Self"}}}},
                "$ref":"#/$defs/Self"
            }),
        )
        .unwrap();
        assert_eq!(compiled.rust_type, "Self_");
        assert_eq!(compiled.typescript_type, "{ next?: SelfSchemaRef1 }");
        assert!(
            compiled
                .typescript_definitions
                .contains_key("SelfSchemaRef1")
        );
        assert!(
            compiled
                .typescript_definitions
                .keys()
                .all(|name| !name.starts_with("Self_"))
        );
    }

    #[test]
    fn versioned_plans_include_every_request_and_response() {
        use forklaunch_core::contract::{RequestContract, ResponseContract, VersionedContract};
        let mut route = RouteContract::post("Write", "Write", "/versions");
        for (version, name, status) in [("v1", "StringBody", 201), ("v2", "IntegerBody", 202)] {
            route = route.with_version(VersionedContract {
                version: version.into(),
                request: RequestContract::default().with_body(BodyContract::json(name)),
                responses: vec![ResponseContract::new(
                    status,
                    Some(BodyContract::json(name)),
                )],
            });
        }
        let mut registry = ContractRegistry::new();
        registry.register(route);
        let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
        assert_eq!(plan.operations.len(), 2);
        assert_eq!(plan.operations[0].name, "writeVersionV1");
        assert_eq!(plan.operations[1].name, "writeVersionV2");
        assert_eq!(
            plan.operations[0].request_body.as_deref(),
            Some("StringBody")
        );
        assert_eq!(plan.operations[1].response_status_codes, vec![202]);
    }

    #[test]
    fn sdk_plan_retains_schema_graphs_and_response_media() {
        use forklaunch_core::contract::{BodyKind, SchemaRef};
        let schema = r##"{"type":"object","properties":{"children":{"type":"array","items":{"$ref":"#"}}}}"##;
        let field = FieldContract {
            schema: SchemaRef::named("Tree").with_json_schema(schema),
            ..FieldContract::optional("filter", "Tree")
        };
        let body = BodyContract {
            schema: SchemaRef::named("Tree").with_json_schema(schema),
            ..BodyContract::json("Tree")
        };
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::get("Read Tree", "Graph", "/tree")
                .with_query(field)
                .with_request_body(body.clone())
                .with_response(
                    200,
                    Some(BodyContract {
                        kind: BodyKind::ServerSentEvent,
                        content_type: Some("text/event-stream".into()),
                        ..body
                    }),
                ),
        );
        let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
        let operation = &plan.operations[0];
        assert_eq!(operation.request_body_json_schema.as_deref(), Some(schema));
        assert_eq!(
            operation.request_query[0].json_schema.as_deref(),
            Some(schema)
        );
        let response = &operation.responses[0];
        assert_eq!(response.body_json_schema.as_deref(), Some(schema));
        assert_eq!(response.body_kind, Some(BodyKind::ServerSentEvent));
        assert_eq!(
            response.body_content_type.as_deref(),
            Some("text/event-stream")
        );
        let (variant, compiled) =
            crate::codegen::response::compile_response(HttpMethod::Get, response, "TreeEvent")
                .unwrap();
        assert!(variant.streaming_response);
        assert!(!compiled.typescript_definitions.is_empty());
        assert!(compiled.rust_models.iter().any(|model| {
            model.fields.iter().any(|field| {
                field
                    .field_type
                    .starts_with("::std::option::Option<::std::vec::Vec<")
            })
        }));
    }

    #[test]
    fn sdk_generation_plan_preserves_live_typed_operation_shape() {
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::new("Create User", "Create a user", HttpMethod::Post, "/users")
                .with_query(FieldContract::optional("dryRun", "Boolean"))
                .with_header(FieldContract::required("authorization", "String"))
                .with_request_body(BodyContract::json("CreateUserBody"))
                .with_response(201, Some(BodyContract::json("User"))),
        );

        let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);

        assert_eq!(plan.operations[0].name, "createUser");
        assert_eq!(
            plan.operations[0].request_body.as_deref(),
            Some("CreateUserBody")
        );
        assert_eq!(
            plan.operations[0].request_body_content_type.as_deref(),
            Some("application/json")
        );
        assert_eq!(plan.operations[0].request_query[0].name, "dryRun");
        assert!(!plan.operations[0].request_query[0].required);
        assert_eq!(plan.operations[0].request_headers[0].name, "authorization");
        assert!(plan.operations[0].request_headers[0].required);
        assert_eq!(
            plan.operations[0].responses[0].body.as_deref(),
            Some("User")
        );
        assert_eq!(plan.operations[0].response_status_codes, vec![201]);
    }
}
