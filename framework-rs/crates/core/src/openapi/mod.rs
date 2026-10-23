#[cfg(test)]
mod component_tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::Path,
};

use forklaunch_common::{JsonField, json_array, json_object, json_string};
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, ContractRegistry, FieldContract, HttpMethod,
    ResponseContract, RouteContract, SchemaRef,
};

#[must_use]
pub fn operation_listing(registry: &ContractRegistry) -> Vec<String> {
    registry
        .routes()
        .iter()
        .map(|route| format!("{} {}", route.method.as_str(), route.openapi_path))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiRoute {
    pub operation_id: String,
    pub method: HttpMethod,
    pub path: String,
    pub summary: String,
    pub request_content_type: Option<String>,
    pub response_status_codes: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiServer {
    pub url: String,
    pub description: Option<String>,
}

impl OpenApiServer {
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            description: None,
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        let mut fields = vec![JsonField::new("url", json_string(&self.url))];
        if let Some(description) = &self.description {
            fields.push(JsonField::new("description", json_string(description)));
        }
        json_object(fields)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiOptions {
    pub title: String,
    pub version: String,
    pub description: Option<String>,
    pub servers: Vec<OpenApiServer>,
    /// Document missing 401/403 responses as TypeScript plain-text auth errors.
    /// Enable only when the HTTP adapter uses the corresponding wire format.
    pub typescript_auth_errors: bool,
    /// Infer URI-decoding 400 text responses only for simple unversioned path contracts.
    pub typescript_path_errors: bool,
}

impl OpenApiOptions {
    #[must_use]
    pub fn from_registry(registry: &ContractRegistry) -> Self {
        registry.service().map_or_else(
            || Self {
                title: "ForkLaunch Service".to_owned(),
                version: "0.1.0".to_owned(),
                description: None,
                servers: Vec::new(),
                typescript_auth_errors: false,
                typescript_path_errors: false,
            },
            |service| Self {
                title: service.name.clone(),
                version: service.version.clone(),
                description: None,
                servers: Vec::new(),
                typescript_auth_errors: false,
                typescript_path_errors: false,
            },
        )
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Include the TypeScript middleware's implicit 401/403 text responses on
    /// authenticated routes. Explicit response contracts always take precedence.
    #[must_use]
    pub fn with_typescript_auth_errors(mut self) -> Self {
        self.typescript_auth_errors = true;
        self
    }

    /// Document missing URI-decoding 400 responses where no other native
    /// body/query/header or constrained parameter validation can emit JSON 400.
    /// Explicit responses and versioned routes are never overwritten.
    #[must_use]
    pub fn with_typescript_path_errors(mut self) -> Self {
        self.typescript_path_errors = true;
        self
    }

    #[must_use]
    pub fn with_server(mut self, server: OpenApiServer) -> Self {
        self.servers.push(server);
        self
    }
}

#[must_use]
pub fn openapi_routes(registry: &ContractRegistry) -> Vec<OpenApiRoute> {
    registry
        .routes()
        .iter()
        .filter(|route| route.options.openapi)
        .map(|route| {
            route
                .versions
                .last()
                .and_then(|(version, _)| route.for_version(version))
                .unwrap_or_else(|| route.clone())
        })
        .map(|route| OpenApiRoute {
            operation_id: route.operation_name.clone(),
            method: route.method,
            path: route.openapi_path.clone(),
            summary: route.summary.clone(),
            request_content_type: route
                .request
                .body
                .as_ref()
                .and_then(|body| body.content_type.clone()),
            response_status_codes: route
                .responses
                .iter()
                .map(|response| response.status_code)
                .collect(),
        })
        .collect()
}

#[must_use]
pub fn generate_openapi_json(registry: &ContractRegistry) -> String {
    generate_openapi_json_with_options(registry, &OpenApiOptions::from_registry(registry))
}

#[must_use]
pub fn generate_openapi_json_with_options(
    registry: &ContractRegistry,
    options: &OpenApiOptions,
) -> String {
    let projected = registry
        .routes()
        .iter()
        .filter(|route| route.options.openapi)
        .map(|route| {
            let mut selected = route
                .versions
                .last()
                .and_then(|(version, _)| route.for_version(version))
                .unwrap_or_else(|| route.clone());
            selected.versions.clone_from(&route.versions);
            selected
        })
        .collect::<Vec<_>>();
    let routes = projected.iter().collect::<Vec<_>>();
    let components = components_json(&routes);

    let mut fields = vec![
        JsonField::new("openapi", json_string("3.1.0")),
        JsonField::new("info", info_json(options)),
        JsonField::new(
            "paths",
            paths_json(
                &routes,
                registry.service().map(|service| &service.name),
                options.typescript_auth_errors,
                options.typescript_path_errors,
            ),
        ),
    ];

    if !options.servers.is_empty() {
        fields.push(JsonField::new(
            "servers",
            json_array(options.servers.iter().map(OpenApiServer::to_json)),
        ));
    }

    if !components.is_empty() {
        fields.push(JsonField::new("components", components));
    }

    json_object(fields)
}

/// Export a public registry with executable, checked universal-SDK descriptors.
///
/// # Errors
/// Rejects contracts the shared portable compiler cannot enforce faithfully.
pub fn generate_portable_openapi_json(registry: &ContractRegistry) -> anyhow::Result<String> {
    generate_portable_openapi_json_with_options(registry, &OpenApiOptions::from_registry(registry))
}

/// Export checked SDK discovery while preserving configured documentation options.
///
/// # Errors
/// Rejects contracts the shared portable compiler cannot enforce faithfully.
pub fn generate_portable_openapi_json_with_options(
    registry: &ContractRegistry,
    options: &OpenApiOptions,
) -> anyhow::Result<String> {
    let mut public = registry
        .service()
        .map_or_else(ContractRegistry::new, |service| {
            ContractRegistry::for_service(service.clone())
        });
    for route in registry
        .routes()
        .iter()
        .filter(|route| route.options.openapi)
    {
        public.register(route.clone());
    }
    let service = registry
        .service()
        .map_or("service", |service| service.name.as_str());
    let plan = crate::codegen::SdkGenerationPlan::from_registry(
        crate::codegen::SdkTarget::TypeScript,
        &public,
    );
    let descriptors = crate::typescript::generate_typescript_operation_descriptors(
        &plan,
        &crate::typescript::TypeScriptHttpSdkOptions::new(service),
    )?;
    let mut document: serde_json::Value =
        serde_json::from_str(&generate_openapi_json_with_options(&public, options))?;
    attach_portable_operation_descriptors(&mut document, &descriptors)?;
    mark_internal_operations(&mut document, &public);
    Ok(crate::codegen::canonical_json(document).to_string())
}

/// Marks `access: internal` operations with `x-forklaunch-access: "internal"`
/// so discovery consumers (the agent's service tools) can skip
/// service-to-service routes even when they declare no `security` scheme.
fn mark_internal_operations(document: &mut serde_json::Value, registry: &ContractRegistry) {
    for route in registry.routes() {
        if route.access != AccessLevel::Internal || !route.method.is_openapi_standard() {
            continue;
        }
        let pointer = format!(
            "/paths/{}/{}",
            route.openapi_path.replace('~', "~0").replace('/', "~1"),
            route.method.as_str().to_ascii_lowercase()
        );
        if let Some(operation) = document
            .pointer_mut(&pointer)
            .and_then(serde_json::Value::as_object_mut)
        {
            operation.insert(
                "x-forklaunch-access".to_owned(),
                serde_json::Value::String("internal".to_owned()),
            );
        }
    }
}

/// Attach checked compiler operations to a standard `OpenAPI` document.
/// Versioned routes retain one HTTP operation and expose each typed projection
/// in `x-forklaunch-operations`; unversioned routes retain the singular extension.
/// Existing descriptors must match the compiled contract exactly.
///
/// # Errors
/// Rejects missing operations, invalid version lists and conflicting descriptors.
pub fn attach_portable_operation_descriptors(
    document: &mut serde_json::Value,
    descriptors: &std::collections::BTreeMap<String, serde_json::Value>,
) -> anyhow::Result<()> {
    fn attach(
        operation: &mut serde_json::Value,
        descriptors: &std::collections::BTreeMap<String, serde_json::Value>,
    ) -> anyhow::Result<()> {
        let Some(name) = operation
            .get("operationId")
            .and_then(serde_json::Value::as_str)
        else {
            return Ok(());
        };
        let name = name.to_owned();
        let versions = operation.get("x-forklaunch-versions");
        let (key, other_key, value) = if let Some(versions) = versions {
            let versions = versions
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Operation versions must be an array"))?;
            anyhow::ensure!(!versions.is_empty(), "Operation versions cannot be empty");
            let mut names = std::collections::BTreeSet::new();
            let mut values = Vec::new();
            for version in versions {
                let version = version
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Operation version must be a string"))?;
                let projected = crate::codegen::versions::operation_name(&name, version);
                anyhow::ensure!(
                    names.insert(projected.clone()),
                    "Duplicate projected operation {projected}"
                );
                values.push(
                    descriptors
                        .get(&projected)
                        .ok_or_else(|| {
                            anyhow::anyhow!("Missing checked operation descriptor {projected}")
                        })?
                        .clone(),
                );
            }
            (
                "x-forklaunch-operations",
                "x-forklaunch-operation",
                serde_json::Value::Array(values),
            )
        } else {
            (
                "x-forklaunch-operation",
                "x-forklaunch-operations",
                descriptors
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("Missing checked operation descriptor {name}"))?
                    .clone(),
            )
        };
        anyhow::ensure!(
            operation.get(other_key).is_none(),
            "Conflicting portable operation extensions for {name}"
        );
        if let Some(existing) = operation.get(key) {
            anyhow::ensure!(
                existing == &value,
                "OpenAPI descriptor does not match exported contract for {name}"
            );
        }
        operation
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("OpenAPI operation must be an object"))?
            .insert(key.into(), value);
        Ok(())
    }
    let paths = document
        .get_mut("paths")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("OpenAPI paths missing"))?;
    for path in paths
        .values_mut()
        .filter_map(serde_json::Value::as_object_mut)
    {
        for method in [
            "get", "put", "post", "delete", "options", "head", "patch", "trace",
        ] {
            if let Some(operation) = path.get_mut(method) {
                attach(operation, descriptors)?;
            }
        }
        if let Some(methods) = path
            .get_mut("x-forklaunch-http-methods")
            .and_then(serde_json::Value::as_object_mut)
        {
            for operation in methods.values_mut() {
                attach(operation, descriptors)?;
            }
        }
    }
    Ok(())
}

/// Export the TypeScript-compatible document collection: the empty key holds
/// unversioned routes, and each named document contains that version's routes.
/// Every document has its own schema/security components and version metadata.
#[must_use]
pub fn generate_versioned_openapi_json(
    registry: &ContractRegistry,
    options: &OpenApiOptions,
) -> String {
    let mut versions = vec![String::new()];
    for route in registry
        .routes()
        .iter()
        .filter(|route| route.options.openapi)
    {
        for version in route.versions.keys() {
            if !versions.contains(version) {
                versions.push(version.clone());
            }
        }
    }
    json_object(versions.into_iter().map(|version| {
        let mut selected = registry
            .service()
            .map_or_else(ContractRegistry::new, |service| {
                ContractRegistry::for_service(service.clone())
            });
        for route in registry.routes() {
            if version.is_empty() && route.versions.is_empty() {
                selected.register(route.clone());
            } else if let Some(route) = route.for_version(&version) {
                selected.register(route);
            }
        }
        let mut options = options.clone();
        if !version.is_empty() {
            options.version.clone_from(&version);
            options.typescript_path_errors = false;
        }
        JsonField::new(
            version,
            generate_openapi_json_with_options(&selected, &options),
        )
    }))
}

/// # Errors
///
/// Returns an error if the parent directory cannot be created or the output file cannot be written.
pub fn write_openapi_json(
    registry: &ContractRegistry,
    options: &OpenApiOptions,
    path: impl AsRef<Path>,
) -> std::io::Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, generate_openapi_json_with_options(registry, options))
}

#[must_use]
pub const fn default_content_type(kind: BodyKind) -> Option<&'static str> {
    match kind {
        BodyKind::Json => Some("application/json"),
        BodyKind::Text => Some("text/plain"),
        BodyKind::File | BodyKind::Binary => Some("application/octet-stream"),
        BodyKind::Multipart => Some("multipart/form-data"),
        BodyKind::UrlEncoded => Some("application/x-www-form-urlencoded"),
        BodyKind::ServerSentEvent => Some("text/event-stream"),
        BodyKind::Unknown => None,
    }
}

fn info_json(options: &OpenApiOptions) -> String {
    let mut fields = vec![
        JsonField::new("title", json_string(&options.title)),
        JsonField::new("version", json_string(&options.version)),
    ];
    if let Some(description) = &options.description {
        fields.push(JsonField::new("description", json_string(description)));
    }
    json_object(fields)
}

fn paths_json(
    routes: &[&RouteContract],
    service_name: Option<&String>,
    typescript_auth_errors: bool,
    typescript_path_errors: bool,
) -> String {
    let mut routes_by_path = BTreeMap::<String, Vec<&RouteContract>>::new();
    for route in routes {
        routes_by_path
            .entry(route.openapi_path.clone())
            .or_default()
            .push(route);
    }

    json_object(routes_by_path.into_iter().map(|(path, routes)| {
        let mut standard = Vec::new();
        let mut extended = Vec::new();
        for route in routes {
            let operation = operation_json(
                route,
                service_name,
                typescript_auth_errors,
                typescript_path_errors,
            );
            if route.method.is_openapi_standard() {
                standard.push(JsonField::new(
                    route.method.as_str().to_ascii_lowercase(),
                    operation,
                ));
            } else {
                extended.push(JsonField::new(route.method.as_str(), operation));
            }
        }
        if !extended.is_empty() {
            standard.push(JsonField::new(
                "x-forklaunch-http-methods",
                json_object(extended),
            ));
        }
        JsonField::new(path, json_object(standard))
    }))
}

fn operation_json(
    route: &RouteContract,
    service_name: Option<&String>,
    typescript_auth_errors: bool,
    typescript_path_errors: bool,
) -> String {
    let mut fields = vec![
        JsonField::new("operationId", json_string(&route.operation_name)),
        JsonField::new(
            "responses",
            responses_json(
                &route.responses,
                typescript_auth_errors && route.auth.is_some(),
                typescript_path_errors && simple_path_validation_only(route),
            ),
        ),
        JsonField::new("summary", json_string(&route.summary)),
    ];

    if let Some(service_name) = service_name {
        fields.push(JsonField::new(
            "tags",
            json_array([json_string(service_name)]),
        ));
    }

    let parameters = parameters_json(route);
    if parameters != "[]" {
        fields.push(JsonField::new("parameters", parameters));
    }

    for (name, schema) in [
        ("x-forklaunch-query-schema", &route.request.query_schema),
        ("x-forklaunch-headers-schema", &route.request.headers_schema),
    ] {
        if let Some(schema) = schema
            .as_ref()
            .and_then(|schema| schema.json_schema.as_ref())
        {
            fields.push(JsonField::new(name, schema.clone()));
        }
    }

    if let Some(body) = &route.request.body {
        fields.push(JsonField::new(
            "requestBody",
            request_body_json(body, !route.request.body_optional),
        ));
    }

    if let Some(auth) = &route.auth {
        fields.push(JsonField::new("security", security_json(auth)));
    }

    if !route.versions.is_empty() {
        fields.push(JsonField::new(
            "x-forklaunch-versions",
            json_array(route.versions.keys().map(|version| json_string(version))),
        ));
        fields.push(JsonField::new(
            "x-forklaunch-version-contracts",
            json_array(
                route
                    .versions
                    .values()
                    .map(forklaunch_core::contract::VersionedContract::to_json),
            ),
        ));
    }

    json_object(fields)
}

fn simple_path_validation_only(route: &RouteContract) -> bool {
    route.versions.is_empty()
        && route.request.body.is_none()
        && route.request.query.is_empty()
        && route.request.headers.is_empty()
        && !route.request.params.is_empty()
        && route.request.params.iter().all(|field| {
            field.schema.json_schema.is_none()
                && matches!(field.schema.name.as_str(), "String" | "string" | "str")
                && [route.path.as_str(), route.openapi_path.as_str()]
                    .iter()
                    .all(|path| {
                        path.split('/').any(|segment| {
                            segment.strip_prefix(':') == Some(field.name.as_str())
                                || segment
                                    .strip_prefix('{')
                                    .and_then(|name| name.strip_suffix('}'))
                                    == Some(field.name.as_str())
                        })
                    })
        })
}

fn parameters_json(route: &RouteContract) -> String {
    let params = route
        .request
        .params
        .iter()
        .map(|field| parameter_json("path", field, true));
    let query = route
        .request
        .query
        .iter()
        .map(|field| parameter_json("query", field, field.required));
    let headers = route
        .request
        .headers
        .iter()
        .map(|field| parameter_json("header", field, field.required));

    json_array(params.chain(query).chain(headers))
}

fn parameter_json(location: &str, field: &FieldContract, required: bool) -> String {
    json_object([
        JsonField::new("in", json_string(location)),
        JsonField::new("name", json_string(&field.name)),
        JsonField::new("required", required.to_string()),
        JsonField::new("schema", schema_ref_json(&field.schema)),
    ])
}

fn request_body_json(body: &BodyContract, required: bool) -> String {
    json_object([
        JsonField::new("content", content_json(body)),
        JsonField::new("required", required.to_string()),
    ])
}

fn responses_json(
    responses: &[ResponseContract],
    typescript_auth_errors: bool,
    typescript_path_errors: bool,
) -> String {
    let mut fields = if responses.is_empty() {
        vec![JsonField::new("204", response_json(None, &[]))]
    } else {
        let mut groups = std::collections::BTreeMap::<u16, Vec<&ResponseContract>>::new();
        for response in responses {
            groups
                .entry(response.status_code)
                .or_default()
                .push(response);
        }
        groups
            .into_iter()
            .map(|(status, variants)| {
                let value = if variants.len() == 1 {
                    response_json(variants[0].body.as_ref(), &variants[0].headers)
                } else {
                    let mut fields = vec![JsonField::new("description", json_string("Response"))];
                    let bodies = variants
                        .iter()
                        .filter_map(|response| response.body.as_ref());
                    fields.push(JsonField::new(
                        "content",
                        json_object(bodies.filter_map(|body| {
                            let media = body
                                .content_type
                                .as_deref()
                                .or_else(|| default_content_type(body.kind))?;
                            Some(JsonField::new(
                                media,
                                json_object([JsonField::new(
                                    "schema",
                                    schema_ref_json(&body_document_schema(body)),
                                )]),
                            ))
                        })),
                    ));
                    let mut headers = std::collections::BTreeMap::new();
                    for response in &variants {
                        for header in &response.headers {
                            headers.insert(&header.name, header);
                        }
                    }
                    if !headers.is_empty() {
                        fields.push(JsonField::new(
                            "headers",
                            json_object(headers.values().map(|header| {
                                JsonField::new(
                                    &header.name,
                                    json_object([JsonField::new(
                                        "schema",
                                        schema_ref_json(&header.schema),
                                    )]),
                                )
                            })),
                        ));
                    }
                    json_object(fields)
                };
                JsonField::new(status.to_string(), value)
            })
            .collect::<Vec<_>>()
    };
    // Inline primitives avoid collisions with application schema names.
    // Keep this projection out of the registry and runtime validation.
    let defaults = [
        (400, "Bad Request", typescript_path_errors),
        (401, "Unauthorized", typescript_auth_errors),
        (403, "Forbidden", typescript_auth_errors),
    ];
    for (status, description, enabled) in defaults {
        if enabled
            && !responses
                .iter()
                .any(|response| response.status_code == status)
        {
            fields.push(JsonField::new(
                status.to_string(),
                json_object([
                    JsonField::new("description", json_string(description)),
                    JsonField::new("content", r#"{"text/plain":{"schema":{"type":"string"}}}"#),
                ]),
            ));
        }
    }
    json_object(fields)
}

fn response_json(body: Option<&BodyContract>, headers: &[FieldContract]) -> String {
    let mut fields = vec![JsonField::new("description", json_string("Response"))];

    if !headers.is_empty() {
        fields.push(JsonField::new(
            "headers",
            json_object(headers.iter().map(|header| {
                JsonField::new(
                    &header.name,
                    json_object([JsonField::new("schema", schema_ref_json(&header.schema))]),
                )
            })),
        ));
    }

    if let Some(body) = body {
        fields.push(JsonField::new("content", content_json(body)));
    }

    json_object(fields)
}

fn content_json(body: &BodyContract) -> String {
    let content_type = body
        .content_type
        .as_deref()
        .or_else(|| default_content_type(body.kind));

    content_type.map_or_else(
        || json_object([]),
        |content_type| {
            json_object([JsonField::new(
                content_type,
                json_object([JsonField::new(
                    "schema",
                    schema_ref_json(&body_document_schema(body)),
                )]),
            )])
        },
    )
}

fn schema_ref_json(schema: &SchemaRef) -> String {
    json_object([JsonField::new(
        "$ref",
        json_string(&format!(
            "#/components/schemas/{}",
            component_schema_name(&schema.name)
        )),
    )])
}

// OpenAPI component keys are narrower than contract schema display names.
// Reserve the encoding prefix too, so a literal name cannot collide with an
// encoded name. Existing ordinary component names stay byte-for-byte stable.
fn component_schema_name(name: &str) -> String {
    const PREFIX: &str = "_fl_encoded_";
    if !name.is_empty()
        && !name.starts_with(PREFIX)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return name.to_owned();
    }
    let mut encoded = String::from(PREFIX);
    for byte in name.bytes() {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn components_json(routes: &[&RouteContract]) -> String {
    let schemas = component_schemas_json(routes);
    let security_schemes = security_schemes_json(routes);

    let mut fields = Vec::new();
    if !schemas.is_empty() {
        fields.push(JsonField::new("schemas", schemas));
    }
    if !security_schemes.is_empty() {
        fields.push(JsonField::new("securitySchemes", security_schemes));
    }

    json_object(fields)
}

fn component_schemas_json(routes: &[&RouteContract]) -> String {
    let mut schemas = BTreeMap::<String, String>::new();

    for route in routes {
        collect_request_schemas(&route.request.params, &mut schemas);
        collect_request_schemas(&route.request.query, &mut schemas);
        collect_request_schemas(&route.request.headers, &mut schemas);
        if let Some(body) = &route.request.body {
            collect_body_schema(body, &mut schemas);
        }
        for response in &route.responses {
            collect_request_schemas(&response.headers, &mut schemas);
            if let Some(body) = &response.body {
                collect_body_schema(body, &mut schemas);
            }
        }
    }

    if schemas.is_empty() {
        return String::new();
    }

    json_object(
        schemas
            .into_iter()
            .map(|(name, schema)| JsonField::new(name, schema)),
    )
}

fn collect_request_schemas(fields: &[FieldContract], schemas: &mut BTreeMap<String, String>) {
    for field in fields {
        collect_schema_ref(&field.schema, schemas);
    }
}

fn body_document_schema(body: &BodyContract) -> SchemaRef {
    if body.kind == BodyKind::ServerSentEvent {
        return forklaunch_core::contract::sse_event_schema(&body.schema);
    }
    let mut schema = body.schema.clone();
    if schema.json_schema.is_none() {
        schema.json_schema = match body.kind {
            BodyKind::Text => Some(r#"{"type":"string"}"#.into()),
            BodyKind::Binary | BodyKind::File => {
                Some(r#"{"type":"string","format":"binary"}"#.into())
            }
            _ => None,
        };
    }
    schema
}

fn collect_body_schema(body: &BodyContract, schemas: &mut BTreeMap<String, String>) {
    collect_schema_ref(&body_document_schema(body), schemas);
}

fn collect_schema_ref(schema: &SchemaRef, schemas: &mut BTreeMap<String, String>) {
    let component_name = component_schema_name(&schema.name);
    schemas.entry(component_name.clone()).or_insert_with(|| {
        schema.json_schema.as_ref().map_or_else(
            || default_schema_json(&schema.name),
            |value| {
                forklaunch_core::contract::relocate_schema_json(
                    value,
                    &["components", "schemas", &component_name],
                )
            },
        )
    });
}

fn default_schema_json(name: &str) -> String {
    let kind = match name.to_ascii_lowercase().as_str() {
        "string" | "str" | "uuid" | "datetime" | "date" => "string",
        "bool" | "boolean" => "boolean",
        "usize" | "u64" | "u32" | "i64" | "i32" | "integer" | "int" => "integer",
        "number" | "float" | "f64" | "f32" => "number",
        _ => "object",
    };
    json_object([JsonField::new("type", json_string(kind))])
}

fn security_json(auth: &AuthConfig) -> String {
    match auth {
        AuthConfig::Cookie { name } => json_array([json_object([JsonField::new(
            cookie_scheme_name(name),
            json_array([]),
        )])]),
        AuthConfig::Jwt(_) | AuthConfig::Bearer => {
            json_array([json_object([JsonField::new("bearerAuth", json_array([]))])])
        }
        AuthConfig::Basic(_) => {
            json_array([json_object([JsonField::new("basicAuth", json_array([]))])])
        }
        AuthConfig::Hmac(_) => {
            json_array([json_object([JsonField::new("hmacAuth", json_array([]))])])
        }
    }
}

fn cookie_scheme_name(name: &str) -> String {
    let mut key = String::from("cookieAuth_");
    for byte in name.bytes() {
        let _ = write!(key, "{byte:02x}");
    }
    key
}

fn security_schemes_json(routes: &[&RouteContract]) -> String {
    let mut schemes = BTreeSet::<&str>::new();
    let mut hmac_header = None;
    let mut cookies = BTreeSet::new();

    for route in routes {
        match &route.auth {
            Some(AuthConfig::Cookie { name }) => {
                cookies.insert(name);
            }
            Some(AuthConfig::Jwt(_) | AuthConfig::Bearer) => {
                schemes.insert("bearerAuth");
            }
            Some(AuthConfig::Basic(_)) => {
                schemes.insert("basicAuth");
            }
            Some(AuthConfig::Hmac(hmac)) => {
                schemes.insert("hmacAuth");
                hmac_header = hmac.header_name.as_deref();
            }
            None => {}
        }
    }

    if schemes.is_empty() && cookies.is_empty() {
        return String::new();
    }

    let fields = schemes.into_iter().map(|scheme| match scheme {
        "bearerAuth" => JsonField::new(
            "bearerAuth",
            json_object([
                JsonField::new("type", json_string("http")),
                JsonField::new("scheme", json_string("bearer")),
            ]),
        ),
        "basicAuth" => JsonField::new(
            "basicAuth",
            json_object([
                JsonField::new("type", json_string("http")),
                JsonField::new("scheme", json_string("basic")),
            ]),
        ),
        _ => JsonField::new(
            "hmacAuth",
            json_object([
                JsonField::new("type", json_string("apiKey")),
                JsonField::new("in", json_string("header")),
                JsonField::new(
                    "name",
                    json_string(hmac_header.unwrap_or("x-forklaunch-signature")),
                ),
            ]),
        ),
    });
    json_object(fields.chain(cookies.into_iter().map(|name| {
        JsonField::new(
            cookie_scheme_name(name),
            json_object([
                JsonField::new("type", json_string("apiKey")),
                JsonField::new("in", json_string("cookie")),
                JsonField::new("name", json_string(name)),
            ]),
        )
    })))
}

#[cfg(test)]
mod tests {
    #[test]
    fn portable_documents_mark_internal_operations() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1.2.3"));
        registry.register(
            RouteContract::post("Sync", "Sync notes", "/internal/notes/:id/sync")
                .with_param(FieldContract::required("id", "String"))
                .with_access(AccessLevel::Internal)
                .with_response(204, None),
        );
        registry.register(
            RouteContract::get("Read", "Read note", "/notes/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_response(200, None),
        );
        let portable: serde_json::Value =
            serde_json::from_str(&generate_portable_openapi_json(&registry).unwrap()).unwrap();
        assert_eq!(
            portable["paths"]["/internal/notes/{id}/sync"]["post"]["x-forklaunch-access"],
            "internal"
        );
        assert!(
            portable["paths"]["/notes/{id}"]["get"]
                .get("x-forklaunch-access")
                .is_none()
        );
    }

    #[test]
    fn portable_document_options_preserve_standard_metadata_and_error_policy() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1.2.3"));
        registry.register(
            RouteContract::get("Read", "Read note", "/notes/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_auth(AuthConfig::cookie("session"))
                .with_response(200, None),
        );
        let options = OpenApiOptions::from_registry(&registry)
            .with_description("Configured description")
            .with_server(OpenApiServer::new("https://notes.example").with_description("Primary"))
            .with_typescript_auth_errors()
            .with_typescript_path_errors();
        let plain: serde_json::Value =
            serde_json::from_str(&generate_openapi_json_with_options(&registry, &options)).unwrap();
        let mut portable: serde_json::Value = serde_json::from_str(
            &generate_portable_openapi_json_with_options(&registry, &options).unwrap(),
        )
        .unwrap();
        let operation = portable["paths"]["/notes/{id}"]["get"]
            .as_object_mut()
            .unwrap();
        assert!(operation.remove("x-forklaunch-operation").is_some());
        assert_eq!(portable, plain);
        for status in ["400", "401", "403"] {
            assert!(
                portable["paths"]["/notes/{id}"]["get"]["responses"][status]["content"]
                    .get("text/plain")
                    .is_some()
            );
        }
    }
    #[test]
    fn portable_version_projections_preserve_standard_document_and_checked_metadata() {
        use crate::contract::{BodyContract, RequestContract, ResponseContract, VersionedContract};
        let mut route = RouteContract::post("Write", "Versioned input", "/versions");
        for (version, kind, status) in [("v1", "string", 201), ("v2", "integer", 202)] {
            let mut body = BodyContract::json("VersionValue");
            body.schema = body.schema.with_json_schema(format!(
                r#"{{"type":"object","additionalProperties":false,"required":["value"],"properties":{{"value":{{"type":"{kind}"}}}}}}"#
            ));
            route = route.with_version(VersionedContract {
                version: version.into(),
                request: RequestContract::default().with_body(body.clone()),
                responses: vec![ResponseContract::new(status, Some(body))],
            });
        }
        let mut registry = ContractRegistry::new();
        registry.register(route);
        registry
            .register(RouteContract::get("Health", "Health", "/health").with_response(200, None));
        let mut private = RouteContract::get("Secret", "Secret", "/secret");
        private.options.openapi = false;
        registry.register(private);
        let document: serde_json::Value =
            serde_json::from_str(&generate_portable_openapi_json(&registry).unwrap()).unwrap();
        assert_eq!(
            document["paths"]["/versions"]["post"]["operationId"],
            "write"
        );
        assert_eq!(
            document["paths"]["/versions"]["post"]["x-forklaunch-version-contracts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let projections = document["paths"]["/versions"]["post"]["x-forklaunch-operations"]
            .as_array()
            .unwrap();
        assert_eq!(projections.len(), 2);
        assert_eq!(projections[0]["operation"], "writeVersionV1");
        assert_eq!(projections[1]["operation"], "writeVersionV2");
        assert_eq!(
            projections[0]["requestSchema"]["properties"]["value"]["type"],
            "string"
        );
        assert_eq!(
            projections[1]["requestSchema"]["properties"]["value"]["type"],
            "integer"
        );
        assert!(
            projections
                .iter()
                .all(|projection| projection.get("versionMetadata").is_some())
        );
        assert!(
            document["paths"]["/versions"]["post"]
                .get("x-forklaunch-operation")
                .is_none()
        );
        assert!(
            document["paths"]["/health"]["get"]
                .get("x-forklaunch-operation")
                .is_some()
        );
        assert!(document["paths"].get("/secret").is_none());
        let plan = crate::codegen::SdkGenerationPlan::from_registry(
            crate::codegen::SdkTarget::TypeScript,
            &registry,
        );
        let descriptors = crate::typescript::generate_typescript_operation_descriptors(
            &plan,
            &crate::typescript::TypeScriptHttpSdkOptions::new("service"),
        )
        .unwrap();
        let mut repeated = document.clone();
        attach_portable_operation_descriptors(&mut repeated, &descriptors).unwrap();
        assert_eq!(repeated, document);
        repeated["paths"]["/versions"]["post"]["x-forklaunch-operations"][0]["path"] =
            "/forged".into();
        assert!(attach_portable_operation_descriptors(&mut repeated, &descriptors).is_err());
        let mut missing = descriptors;
        missing.remove("writeVersionV2");
        assert!(attach_portable_operation_descriptors(&mut document.clone(), &missing).is_err());
    }

    use forklaunch_core::contract::{
        AuthConfig, BodyContract, FieldContract, HmacAuth, HttpMethod, RouteContract,
        ServiceContract,
    };

    use super::*;

    #[test]
    fn named_version_documents_preserve_distinct_schemas_responses_and_common_params() {
        use forklaunch_core::contract::{RequestContract, VersionedContract};
        let mut route = RouteContract::post("Versioned", "Versioned", "/items/:id")
            .with_param(FieldContract::required("id", "Id"));
        for (version, kind, status) in [("z-first", "string", 201), ("a-last", "integer", 202)] {
            let mut body = BodyContract::json("Input");
            body.schema = body
                .schema
                .with_json_schema(format!(r#"{{"type":"{kind}"}}"#));
            route = route.with_version(VersionedContract {
                version: version.into(),
                request: RequestContract::default().with_body(body.clone()),
                responses: vec![ResponseContract::new(status, Some(body))],
            });
        }
        let mut registry = ContractRegistry::new();
        registry.register(route);
        registry
            .register(RouteContract::get("Health", "Health", "/health").with_response(200, None));
        let documents =
            generate_versioned_openapi_json(&registry, &OpenApiOptions::from_registry(&registry));
        let documents: serde_json::Value = serde_json::from_str(&documents).unwrap();
        assert!(documents[""]["paths"].get("/health").is_some());
        assert!(documents[""]["paths"].get("/items/{id}").is_none());
        for (version, kind, status) in [("z-first", "string", "201"), ("a-last", "integer", "202")]
        {
            let document = &documents[version];
            let operation = &document["paths"]["/items/{id}"]["post"];
            assert_eq!(document["info"]["version"], version);
            assert!(document["paths"].get("/health").is_none());
            assert_eq!(operation["parameters"][0]["name"], "id");
            assert!(operation["responses"].get(status).is_some());
            assert_eq!(document["components"]["schemas"]["Input"]["type"], kind);
        }
    }

    #[test]
    fn typescript_auth_error_docs_are_opt_in_and_preserve_explicit_responses() {
        let mut registry = ContractRegistry::new();
        registry.register(RouteContract::get("Public", "Public", "/public"));
        registry.register(
            RouteContract::get("Private", "Private", "/private")
                .with_auth(AuthConfig::cookie("session")),
        );
        registry.register(
            RouteContract::get("Explicit", "Explicit", "/explicit")
                .with_auth(AuthConfig::cookie("session"))
                .with_response(401, Some(BodyContract::json("CustomError"))),
        );
        let native: serde_json::Value =
            serde_json::from_str(&generate_openapi_json(&registry)).unwrap();
        assert!(
            native["paths"]["/private"]["get"]["responses"]
                .get("401")
                .is_none()
        );
        let options = OpenApiOptions::from_registry(&registry).with_typescript_auth_errors();
        let document: serde_json::Value =
            serde_json::from_str(&generate_openapi_json_with_options(&registry, &options)).unwrap();
        let private = &document["paths"]["/private"]["get"]["responses"];
        assert!(
            private.get("204").is_some(),
            "retain implicit no-content success"
        );
        for status in ["401", "403"] {
            assert_eq!(
                private[status]["content"]["text/plain"]["schema"]["type"],
                "string"
            );
            assert!(
                document["paths"]["/public"]["get"]["responses"]
                    .get(status)
                    .is_none()
            );
        }
        assert!(
            document["paths"]["/explicit"]["get"]["responses"]["401"]["content"]
                .get("application/json")
                .is_some()
        );
        assert_eq!(
            registry.routes()[1].responses.len(),
            0,
            "projection must not change runtime validation"
        );
        let versioned: serde_json::Value =
            serde_json::from_str(&generate_versioned_openapi_json(&registry, &options)).unwrap();
        assert_eq!(versioned[""]["paths"], document["paths"]);
    }

    #[test]
    fn maps_registered_routes_to_openapi_routes() {
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::new("Create User", "Create a user", HttpMethod::Post, "/users")
                .with_request_body(BodyContract::json("CreateUserBody"))
                .with_response(201, Some(BodyContract::json("User"))),
        );

        let routes = openapi_routes(&registry);

        assert_eq!(routes[0].operation_id, "createUser");
        assert_eq!(routes[0].path, "/users");
        assert_eq!(
            routes[0].request_content_type.as_deref(),
            Some("application/json")
        );
        assert_eq!(routes[0].response_status_codes, vec![201]);
    }

    #[test]
    fn generates_openapi_31_json_from_contract_registry() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("users", "1.0.0"));
        registry.register(
            RouteContract::post("Create User", "Create a user", "/users")
                .with_request_body(BodyContract::json("CreateUserBody"))
                .with_response(201, Some(BodyContract::json("User")))
                .with_response_header(201, FieldContract::required("etag", "String"))
                .with_auth(AuthConfig::Hmac(
                    HmacAuth::new().with_header_name("x-service-signature"),
                )),
        );

        let json = generate_openapi_json_with_options(
            &registry,
            &OpenApiOptions::from_registry(&registry)
                .with_description("Users service")
                .with_server(
                    OpenApiServer::new("https://api.example.com").with_description("prod"),
                ),
        );

        assert!(json.contains("\"openapi\":\"3.1.0\""));
        assert!(json.contains("\"operationId\":\"createUser\""));
        assert!(json.contains("\"/users\""));
        assert!(json.contains("\"201\""));
        assert!(json.contains("\"hmacAuth\""));
        assert!(json.contains("\"x-service-signature\""));
        assert!(json.contains("\"CreateUserBody\""));
    }

    #[test]
    fn distinct_cookie_names_have_distinct_security_schemes() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("sessions", "1"));
        for (path, name) in [("/a", "a-b"), ("/b", "a_b")] {
            registry
                .register(RouteContract::get(path, path, path).with_auth(AuthConfig::cookie(name)));
        }
        let document: serde_json::Value =
            serde_json::from_str(&generate_openapi_json(&registry)).unwrap();
        let schemes = document["components"]["securitySchemes"]
            .as_object()
            .unwrap();
        assert_eq!(schemes.len(), 2);
        for name in ["a-b", "a_b"] {
            assert_eq!(schemes[&cookie_scheme_name(name)]["name"], name);
            assert_eq!(schemes[&cookie_scheme_name(name)]["in"], "cookie");
        }
    }

    #[test]
    fn openapi_json_is_stable_for_snapshot_review() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("users", "1.0.0"));
        registry.register(
            RouteContract::get("Get User", "Get a user", "/users/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_response(200, Some(BodyContract::json("User"))),
        );

        let json = generate_openapi_json(&registry);

        assert_eq!(
            json,
            "{\"components\":{\"schemas\":{\"String\":{\"type\":\"string\"},\"User\":{\"type\":\"object\"}}},\"info\":{\"title\":\"users\",\"version\":\"1.0.0\"},\"openapi\":\"3.1.0\",\"paths\":{\"/users/{id}\":{\"get\":{\"operationId\":\"getUser\",\"parameters\":[{\"in\":\"path\",\"name\":\"id\",\"required\":true,\"schema\":{\"$ref\":\"#/components/schemas/String\"}}],\"responses\":{\"200\":{\"content\":{\"application/json\":{\"schema\":{\"$ref\":\"#/components/schemas/User\"}}},\"description\":\"Response\"}},\"summary\":\"Get a user\",\"tags\":[\"users\"]}}}}"
        );
    }
}

#[cfg(test)]
mod primitive_schema_tests {
    use super::*;
    #[test]
    fn named_primitive_parameters_export_primitive_schemas_for_sdk_imports() {
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::get("Find", "Find", "/items/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_query(FieldContract::optional("enabled", "Boolean"))
                .with_query(FieldContract::optional("limit", "Integer"))
                .with_query(FieldContract::optional("score", "Number"))
                .with_response(200, Some(BodyContract::json("CustomRecord"))),
        );
        let document: serde_json::Value =
            serde_json::from_str(&generate_openapi_json(&registry)).unwrap();
        for (name, kind) in [
            ("String", "string"),
            ("Boolean", "boolean"),
            ("Integer", "integer"),
            ("Number", "number"),
            ("CustomRecord", "object"),
        ] {
            assert_eq!(document["components"]["schemas"][name]["type"], kind);
        }
    }
}

#[cfg(test)]
mod extended_method_tests {
    use super::*;
    use forklaunch_core::contract::HttpMethod;

    #[test]
    fn all_methods_export_without_nonstandard_openapi_operation_keys() {
        let mut registry = ContractRegistry::new();
        for method in HttpMethod::ALL {
            registry.register(
                RouteContract::new(method.as_str(), "Probe", method, "/probe")
                    .with_response(204, None),
            );
        }
        let document: serde_json::Value =
            serde_json::from_str(&generate_openapi_json(&registry)).unwrap();
        let path = &document["paths"]["/probe"];
        assert_eq!(path.as_object().unwrap().len(), 9);
        assert_eq!(
            path["x-forklaunch-http-methods"].as_object().unwrap().len(),
            19
        );
        for method in HttpMethod::ALL {
            let operation = if method.is_openapi_standard() {
                &path[method.as_str().to_ascii_lowercase()]
            } else {
                &path["x-forklaunch-http-methods"][method.as_str()]
            };
            assert_eq!(operation["responses"]["204"]["description"], "Response");
            assert!(operation["operationId"].is_string());
        }
    }
}
