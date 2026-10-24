//! Typed fetch SDK generation using the same schemas and runtime as the CLI.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use forklaunch_common::json_string;
use forklaunch_core::codegen::{
    BodyKind, RustTypeAliasTemplate, SchemaModels, SchemaRef, SdkField, SdkGenerationPlan,
    SdkOperation, compile_schema_ref, escape_typescript_string as escape,
    render::{
        TypeScriptAliasTemplate, TypeScriptOperationTemplate, TypeScriptOperationTypeTemplate,
        TypeScriptRouterTemplate, TypeScriptSdkTemplate, TypeScriptServiceTemplate,
    },
    response::{compile_response, normalized_media_type, type_identifier},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScriptHttpSdkOptions {
    pub service_name: String,
    pub import_fetch: bool,
}
impl TypeScriptHttpSdkOptions {
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            import_fetch: false,
        }
    }
    #[must_use]
    pub const fn with_fetch_import(mut self, import_fetch: bool) -> Self {
        self.import_fetch = import_fetch;
        self
    }
}

/// Generate typed fetch requests and status responses, including recursive models
/// and owned SSE iterators. Routers derive from each operation's first path segment.
/// The exported `createForkLaunchSdk` factory retains CLI consumer syntax.
///
/// # Errors
/// Rejects invalid schema graphs/media/statuses, unsupported identifiers and
/// conflicting generated names before returning source.
pub(super) fn prepare_http_sdk(
    plan: &SdkGenerationPlan,
    options: &TypeScriptHttpSdkOptions,
) -> Result<TypeScriptSdkTemplate> {
    let plan = crate::codegen::version_responses::prepare(plan)?;
    crate::codegen::validate_model_identities(&plan)?;
    let service_key = member_identifier(&options.service_name)?;
    let mut definitions = BTreeMap::new();
    let mut type_names = BTreeSet::new();
    let mut operation_types = Vec::new();
    let mut routers = BTreeMap::<String, Vec<TypeScriptOperationTemplate>>::new();
    let mut member_names = BTreeSet::new();
    for op in &plan.operations {
        validate_date_request_kind(op)?;
        if !op.path.starts_with('/') {
            bail!("SDK operation path must start with /");
        }
        let router = router_name(&op.path);
        let router_key = member_identifier(&router)?;
        let operation_key = member_identifier(&op.name)?;
        if !member_names.insert((router_key.clone(), operation_key.clone())) {
            bail!("Duplicate generated TypeScript SDK operation {operation_key}");
        }
        let prefix = format!(
            "{}{}{}",
            type_identifier(&options.service_name),
            type_identifier(&router),
            type_identifier(&op.name)
        );
        let request_type = format!("{prefix}Request");
        let responses_type = format!("{prefix}Responses");
        for name in [&request_type, &responses_type] {
            reserve_name(&mut type_names, name)?;
        }
        let params = field_type(
            &op.request_params,
            &format!("{prefix}Params"),
            false,
            &mut definitions,
        )?;
        let query = field_type(
            &op.request_query,
            &format!("{prefix}Query"),
            false,
            &mut definitions,
        )?;
        let headers = field_type(
            &op.request_headers,
            &format!("{prefix}Headers"),
            true,
            &mut definitions,
        )?;
        let (kind, mime, body) = request_body(op, &prefix, &mut definitions)?;
        let responses = response_types(op, &prefix, &mut definitions, &mut type_names)?;
        let optional_input = params == "never"
            && query == "never"
            && (body == "never" || op.request_body_optional)
            && !op.request_headers.iter().any(|field| field.required);
        operation_types.push(TypeScriptOperationTypeTemplate {
            request_type: request_type.clone(),
            responses_type: responses_type.clone(),
            request_params_type: params,
            request_query_type: query,
            request_body_type: body,
            request_body_optional: op.request_body_optional,
            request_headers_type: headers,
            responses: responses.clone(),
        });
        routers
            .entry(router_key)
            .or_default()
            .push(TypeScriptOperationTemplate {
                version_metadata: escape(&crate::codegen::runtime_schemas::versions(
                    &crate::codegen::version_responses::metadata(op)?,
                )?),
                request_schema: canonical_request_schema(op)?,
                request_objects: escape(&crate::codegen::runtime_schemas::request_objects(
                    &crate::codegen::request_objects::metadata(op)?,
                )?),
                operation_key,
                request_type,
                responses_type,
                optional_input,
                service_name: escape(&options.service_name),
                router_name: escape(&router),
                operation_name: escape(&op.name),
                method: op.method.as_str().into(),
                path: escape(&op.path),
                request_kind: kind.into(),
                request_content_type: escape(&mime),
                responses,
                last: false,
            });
    }
    prepare_client(
        service_key,
        definitions,
        type_names,
        operation_types,
        routers,
    )
}

fn router_name(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("health")
        .replace('-', "_")
}

fn reserve_name(names: &mut BTreeSet<String>, name: &str) -> Result<()> {
    if !names.insert(name.into()) {
        bail!("Duplicate generated TypeScript SDK type {name}");
    }
    Ok(())
}

fn member_identifier(name: &str) -> Result<String> {
    let name = name.to_case(Case::Camel);
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        || name == "__proto__"
    {
        bail!("Unsupported TypeScript SDK identifier {name}");
    }
    Ok(name)
}

fn retain_definitions(
    compiled: SchemaModels,
    definitions: &mut BTreeMap<String, String>,
) -> Result<String> {
    for (name, value) in compiled.typescript_definitions {
        if definitions
            .get(&name)
            .is_some_and(|previous| previous != &value)
        {
            bail!("Conflicting generated TypeScript model {name}");
        }
        definitions.insert(name, value);
    }
    Ok(compiled.typescript_type)
}

fn field_type(
    fields: &[SdkField],
    prefix: &str,
    headers: bool,
    definitions: &mut BTreeMap<String, String>,
) -> Result<String> {
    if fields.is_empty() {
        return Ok(if headers {
            "ForkLaunchHeaders"
        } else {
            "never"
        }
        .into());
    }
    let mut parts = Vec::new();
    let mut names = BTreeSet::new();
    for (index, field) in fields.iter().enumerate() {
        if field
            .json_schema
            .as_deref()
            .is_some_and(|schema| schema.contains("x-forklaunch-date-codec"))
        {
            bail!(
                "Date codecs are currently supported in JSON bodies and SSE payloads, not path/query/header fields"
            );
        }
        if !names.insert(&field.name) {
            bail!("Duplicate SDK request field {}", field.name);
        }
        let compiled = compile_schema_ref(
            &format!("{prefix}Field{index}"),
            &SchemaRef {
                name: field.schema.clone(),
                json_schema: field.json_schema.clone(),
            },
        )?;
        let ty = retain_definitions(compiled, definitions)?;
        parts.push(format!(
            "{}{}: {ty}",
            json_string(&field.name),
            if field.required { "" } else { "?" }
        ));
    }
    let shape = format!("{{ {} }}", parts.join("; "));
    Ok(if headers {
        format!("{shape} & ForkLaunchHeaders")
    } else {
        shape
    })
}

pub(super) fn request_body(
    op: &SdkOperation,
    prefix: &str,
    definitions: &mut BTreeMap<String, String>,
) -> Result<(&'static str, String, String)> {
    let kind = op.request_body_kind.unwrap_or(BodyKind::Json);
    let (kind_name, default_mime) = match kind {
        BodyKind::Text => ("text", "text/plain"),
        BodyKind::Binary | BodyKind::File => ("binary", "application/octet-stream"),
        BodyKind::Multipart => ("multipart", "multipart/form-data"),
        BodyKind::UrlEncoded => ("urlEncoded", "application/x-www-form-urlencoded"),
        BodyKind::ServerSentEvent => bail!("SSE request bodies are unsupported"),
        _ => ("json", "application/json"),
    };
    let mime = forklaunch_core::codegen::media::request_media_type(
        op.request_body_content_type
            .as_deref()
            .unwrap_or(default_mime),
    )?;
    let normalized_mime = normalized_media_type(&mime)?;
    if normalized_mime == "text/event-stream" {
        bail!("SSE request bodies are unsupported");
    }
    if normalized_mime.starts_with("multipart/") != (kind == BodyKind::Multipart)
        || kind == BodyKind::Multipart && normalized_mime != "multipart/form-data"
    {
        bail!("Multipart SDK requests require multipart/form-data and the multipart body kind");
    }
    let Some(name) = &op.request_body else {
        return Ok((kind_name, mime, "never".into()));
    };
    let compiled = compile_schema_ref(
        &format!("{prefix}Body"),
        &SchemaRef {
            name: name.clone(),
            json_schema: op.request_body_json_schema.clone(),
        },
    )?;
    let json = retain_definitions(compiled, definitions)?;
    let ty = match kind {
        BodyKind::Text => "string".into(),
        BodyKind::Binary | BodyKind::File => "Uint8Array | ArrayBuffer | Blob".into(),
        BodyKind::Multipart => format!("FormData | {}", form_record(&json, op)),
        BodyKind::UrlEncoded => format!("URLSearchParams | {}", form_record(&json, op)),
        _ => json,
    };
    Ok((kind_name, mime, ty))
}

fn response_types(
    op: &SdkOperation,
    prefix: &str,
    definitions: &mut BTreeMap<String, String>,
    names: &mut BTreeSet<String>,
) -> Result<Vec<forklaunch_core::codegen::render::SdkResponseVariant>> {
    let mut statuses = BTreeMap::new();
    for response in &op.responses {
        if !(100..=599).contains(&response.status_code) {
            bail!("Invalid SDK response status code");
        }
        if response.status_code < 200 {
            continue;
        }
        if statuses
            .insert(
                forklaunch_core::codegen::response::response_identity(response)?,
                response,
            )
            .is_some()
        {
            bail!("Duplicate SDK response status {}", response.status_code);
        }
    }
    let mut responses = Vec::new();
    let counts = statuses
        .keys()
        .fold(BTreeMap::<u16, usize>::new(), |mut counts, (status, _)| {
            *counts.entry(*status).or_default() += 1;
            counts
        });
    for (index, ((status, media), response)) in statuses.into_iter().enumerate() {
        let multiple = counts[&status] > 1;
        let media_name = forklaunch_core::codegen::response::response_media_name(&media);
        let root = if multiple {
            format!("{prefix}Status{status}{media_name}Response")
        } else if index == 0 {
            format!("{prefix}Response")
        } else {
            format!("{prefix}Status{status}Response")
        };
        reserve_name(names, &root)?;
        let (mut variant, compiled) = compile_response(op.method, response, &root)?;
        // Keep authored schemas for type identity; only validator constants
        // cross the bounded resource/dialect normalization boundary.
        variant.body_schema = escape(
            &crate::codegen::runtime_schemas::schema(&serde_json::from_str(
                &variant.body_schema_json,
            )?)?
            .to_string(),
        );
        variant.header_rules = crate::codegen::runtime_schemas::fields(&variant.header_rules)?;
        variant.media_discriminator = multiple;
        if multiple {
            variant.status_variant = format!("Status{status}{media_name}");
        }
        retain_definitions(compiled, definitions)?;
        responses.push(variant);
    }
    Ok(responses)
}

fn prepare_client(
    service_key: String,
    definitions: BTreeMap<String, String>,
    mut type_names: BTreeSet<String>,
    operation_types: Vec<TypeScriptOperationTypeTemplate>,
    routers: BTreeMap<String, Vec<TypeScriptOperationTemplate>>,
) -> Result<TypeScriptSdkTemplate> {
    let router_count = routers.len();
    let routers = routers
        .into_iter()
        .enumerate()
        .map(|(index, (router_key, mut operations))| {
            if let Some(last) = operations.last_mut() {
                last.last = true;
            }
            TypeScriptRouterTemplate {
                router_key,
                operations,
                last: index + 1 == router_count,
            }
        })
        .collect();
    for name in definitions.keys() {
        reserve_name(&mut type_names, name)?;
    }
    let data = TypeScriptSdkTemplate {
        schema_models: definitions
            .into_iter()
            .map(|(name, target_type)| RustTypeAliasTemplate { name, target_type })
            .collect(),
        form_stringify_source: include_str!("safeStringify.ts").into(),
        transport_source: format!(
            "{}\n{}",
            crate::typescript::typescript_schema_runtime_source(),
            include_str!("transport.ts")
        ),
        sse_source: include_str!("sse.ts").into(),
        operation_types,
        services: vec![TypeScriptServiceTemplate {
            service_key: service_key.clone(),
            routers,
            last: true,
        }],
        aliases: vec![TypeScriptAliasTemplate {
            alias: format!("{service_key}Service"),
            service_key,
        }],
    };
    Ok(data)
}

fn form_record<'a>(json: &'a str, op: &SdkOperation) -> &'a str {
    if op.request_body_json_schema.is_none() {
        "Record<string, unknown>"
    } else {
        json
    }
}

fn validate_date_request_kind(op: &SdkOperation) -> Result<()> {
    if op
        .request_body_json_schema
        .as_deref()
        .is_some_and(|schema| schema.contains("x-forklaunch-date-codec"))
        && op.request_body_kind.unwrap_or(BodyKind::Json) != BodyKind::Json
    {
        bail!("Date request codecs currently require a JSON body");
    }
    Ok(())
}

fn canonical_request_schema(op: &SdkOperation) -> Result<String> {
    let schema: serde_json::Value =
        serde_json::from_str(op.request_body_json_schema.as_deref().unwrap_or("true"))?;
    Ok(escape(
        &crate::codegen::runtime_schemas::schema(&schema)?.to_string(),
    ))
}

/// Generate the standalone compatibility source from the canonical template model.
/// # Errors
/// Rejects invalid schemas and contract declarations.
pub fn generate_typescript_http_sdk_source(
    plan: &SdkGenerationPlan,
    options: &TypeScriptHttpSdkOptions,
) -> Result<String> {
    let data = prepare_http_sdk(plan, options)?;
    let source = ramhorns::Template::new(include_str!("http_client.ts.template"))?.render(&data);
    Ok(if options.import_fetch {
        format!("import fetch from 'cross-fetch';\n{source}")
    } else {
        source
    })
}

/// Structured package files. Runtime is service-independent and may be emitted once
/// at the owning SDK package root; client/models import it from the parent directory.
/// # Errors
/// Rejects invalid contracts or an inconsistent framework template layout.
pub fn generate_typescript_http_sdk_files(
    plan: &SdkGenerationPlan,
    options: &TypeScriptHttpSdkOptions,
) -> Result<BTreeMap<String, String>> {
    generate_typescript_http_sdk_files_with_runtime(plan, options, "../runtime")
}

pub(crate) fn generate_typescript_http_sdk_files_with_runtime(
    plan: &SdkGenerationPlan,
    options: &TypeScriptHttpSdkOptions,
    runtime_module: &str,
) -> Result<BTreeMap<String, String>> {
    anyhow::ensure!(
        matches!(runtime_module, "../runtime" | "./runtime"),
        "Unsupported generated runtime location"
    );
    let data = prepare_http_sdk(plan, options)?;
    let template = include_str!("http_client.ts.template");
    // These are owned template sections, selected before rendering user schemas.
    let (runtime_before, rest) = template
        .split_once("{{! forklaunch:models}}")
        .ok_or_else(|| anyhow::anyhow!("Missing operation model template section"))?;
    let (model_tail, rest) = rest
        .split_once("{{! forklaunch:runtime-after}}")
        .ok_or_else(|| anyhow::anyhow!("Missing transport template section"))?;
    let (runtime_after, client_tail) = rest
        .split_once("{{! forklaunch:client}}")
        .ok_or_else(|| anyhow::anyhow!("Missing client template section"))?;
    let render =
        |source: &str| -> Result<String> { Ok(ramhorns::Template::new(source)?.render(&data)) };
    let mut runtime = render(&format!(
        "{runtime_before}{runtime_after}\nexport default request;\n"
    ))?;
    // The renderer drops the final newline; source files end with one.
    if !runtime.ends_with('\n') {
        runtime.push('\n');
    }
    let runtime = if options.import_fetch {
        format!("import fetch from 'cross-fetch';\n{runtime}")
    } else {
        runtime
    };
    let models = render(&format!(
        "import type {{ ForkLaunchSdkRequest, ForkLaunchSdkResponse, ForkLaunchErrorResponses, ForkLaunchHeaders }} from '{runtime_module}';\n{model_tail}"
    ))?;
    let client_template = client_tail
        .to_owned()
        .replace("{{request_type}}", "models.{{request_type}}")
        .replace("{{responses_type}}", "models.{{responses_type}}");
    let client = render(&format!(
        "import request from '{runtime_module}';\nimport type {{ ForkLaunchSdkConfig }} from '{runtime_module}';\nimport type * as models from './models';\n{client_template}"
    ))?;
    Ok(BTreeMap::from([
        (
            "index.ts".into(),
            format!(
                "export * from './client';\nexport * from './models';\nexport * from '{runtime_module}';\n"
            ),
        ),
        ("client.ts".into(), client),
        ("models.ts".into(), models),
        ("runtime.ts".into(), runtime),
    ]))
}

#[cfg(test)]
#[path = "http_runtime_schemas_tests.rs"]
mod runtime_schemas_tests;
