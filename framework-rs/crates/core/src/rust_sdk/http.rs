//! Checked, transport-capable Rust SDK generation using the shared CLI compiler.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Result, bail};
use forklaunch_core::codegen::{
    BodyKind, RustModelTemplate, RustTypeAliasTemplate, SdkGenerationPlan,
    render::{RustOperationTemplate, RustSdkTemplate},
    render_rust_models,
    response::{
        compile_response, field_identifier, normalized_media_type, type_identifier,
        validate_identifier,
    },
};

use crate::rust_sdk::RustSdkOptions;

#[path = "http_runtime_schemas.rs"]
mod runtime_schemas;

/// Generate a reqwest client with typed status responses and owned SSE streams.
///
/// The generated crate requires reqwest (`json`, `query`, and `multipart` when
/// used), serde (`derive`), `serde_json`, and forklaunch-universal-sdk.
/// JSON request bodies use generated
/// schema types; explicit `_raw` methods accept encoded JSON as an escape hatch.
/// Binary bodies and multipart forms use their owned transport types.
/// Transport, auth, checked responses, SSE and multipart decoding share the native SDK runtime.
///
/// # Errors
/// Rejects invalid schemas/media, unsupported request streams, duplicate names,
/// conflicting types and invalid status codes before returning generated source.
#[allow(clippy::too_many_lines)] // Assemble correlated request/response template models in one pass.
fn prepare_http_sdk(
    plan: &SdkGenerationPlan,
    options: &RustSdkOptions,
    modern: bool,
) -> Result<RustSdkTemplate> {
    let plan = crate::codegen::version_responses::prepare(plan)?;
    crate::codegen::validate_model_identities(&plan)?;
    validate_identifier(&options.client_struct)?;
    let has_multipart = plan
        .operations
        .iter()
        .any(|op| op.request_body_kind == Some(BodyKind::Multipart));
    let mut models = BTreeMap::<String, RustModelTemplate>::new();
    let mut aliases = BTreeMap::<String, RustTypeAliasTemplate>::new();
    let mut names = reserved_type_names();
    if !names.insert(options.client_struct.clone()) {
        bail!(
            "Conflicting generated Rust SDK client type {}",
            options.client_struct
        );
    }
    let mut methods = reserved_method_names();
    let mut operations = Vec::new();
    for op in &plan.operations {
        if op
            .request_body_json_schema
            .as_deref()
            .is_some_and(|schema| schema.contains("x-forklaunch-date-codec"))
            && op.request_body_kind.unwrap_or(BodyKind::Json) != BodyKind::Json
        {
            bail!("Date request codecs currently require a JSON body");
        }
        if !op.path.starts_with('/') {
            bail!("SDK operation path must start with /");
        }
        let prefix = type_identifier(&op.name);
        let request_type = format!("{prefix}Request");
        let responses_type = format!("{prefix}Responses");
        for name in [&request_type, &responses_type] {
            if !names.insert(name.clone()) {
                bail!("Duplicate generated Rust SDK type {name}");
            }
        }
        let execute_method_name = field_identifier(&op.name);
        let method_name = format!("{execute_method_name}_request");
        for name in [&execute_method_name, &method_name] {
            if !methods.insert(name.clone()) {
                bail!("Duplicate generated Rust SDK method {name}");
            }
        }
        let (kind, content_type) = request_content_type(op)?;
        let json_request_type = compile_request(op, &prefix, &mut models, &mut aliases)?;
        if !json_request_type.is_empty() {
            reserve_raw_methods(&mut methods, &execute_method_name)?;
        }
        let responses = compile_responses(op, &prefix, &mut models, &mut aliases, modern)?;
        let params_type = compile_fields(
            &op.request_params,
            &format!("{prefix}Params"),
            &mut models,
            &mut aliases,
        )?;
        let query_type = compile_fields(
            &op.request_query,
            &format!("{prefix}Query"),
            &mut models,
            &mut aliases,
        )?;
        let headers_type = compile_fields(
            &op.request_headers,
            &format!("{prefix}Headers"),
            &mut models,
            &mut aliases,
        )?;
        let no_input = modern
            && op.request_body.is_none()
            && op.request_params.is_empty()
            && op.request_query.is_empty()
            && op.request_headers.is_empty()
            && op.request_query_schema.is_none()
            && op.request_headers_schema.is_none()
            && op.version_branches.is_empty();
        if no_input && !methods.insert(format!("{execute_method_name}_with_options")) {
            bail!("Conflicting generated Rust SDK options method {execute_method_name}");
        }
        operations.push(RustOperationTemplate {
            has_body: op.request_body.is_some(),
            no_input,
            shared_response_decoders: modern,
            request_objects: {
                let metadata = crate::codegen::request_objects::metadata(op)?;
                if metadata.is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&runtime_schemas::request_objects(&metadata)?)?
                }
            },
            typed_fields: !params_type.is_empty()
                || !query_type.is_empty()
                || !headers_type.is_empty(),
            params_type,
            query_type,
            headers_type,
            json_request_type,
            form_request: kind == BodyKind::UrlEncoded,
            version_metadata: {
                let metadata = crate::codegen::version_responses::metadata(op)?;
                if metadata.is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&runtime_schemas::versions(&metadata)?)?
                }
            },
            request_type,
            responses_type,
            streaming_response: responses.iter().any(|response| response.streaming_response),
            responses,
            execute_method_name,
            method_name,
            http_method: op.method.as_str().into(),
            path: op.path.replace('\\', "\\\\").replace('"', "\\\""),
            binary_request: matches!(kind, BodyKind::Binary | BodyKind::File),
            multipart_request: kind == BodyKind::Multipart,
            multipart_field: if has_multipart {
                format!(
                    "multipart: {},",
                    if kind == BodyKind::Multipart {
                        "input.body"
                    } else {
                        "None"
                    }
                )
            } else {
                String::new()
            },
            request_content_type: content_type.replace('\\', "\\\\").replace('"', "\\\""),
        });
    }
    for name in models.keys().chain(aliases.keys()) {
        if names.contains(name) || models.contains_key(name) && aliases.contains_key(name) {
            bail!("Conflicting generated Rust SDK type {name}");
        }
    }
    let has_sse = operations.iter().any(|op| op.streaming_response);
    let data = RustSdkTemplate {
        model_source: render_rust_models(
            &models.into_values().collect::<Vec<_>>(),
            &aliases.into_values().collect::<Vec<_>>(),
        )?,
        has_operations: !operations.is_empty(),
        has_multipart,
        has_multipart_response: operations.iter().any(|op| {
            op.responses
                .iter()
                .any(|response| response.multipart_response)
        }),
        has_sse,
        has_serde: !operations.is_empty(),
        client_name: options.client_struct.clone(),
        operations,
    };
    Ok(data)
}

fn reserved_method_names() -> BTreeSet<String> {
    BTreeSet::from([
        "new".to_string(),
        "with_http_client".into(),
        "with_plaintext_development_transport".into(),
        "with_sse_event_limit".into(),
        "with_multipart_response_limit".into(),
        "with_default_header".into(),
        "with_bearer_token".into(),
        "with_jwt".into(),
        "with_basic_auth".into(),
        "with_hmac_authorization".into(),
        "with_hmac_secret".into(),
        "with_hmac_signer".into(),
        "with_auth_hook".into(),
    ])
}

fn reserve_raw_methods(methods: &mut BTreeSet<String>, method: &str) -> Result<()> {
    for name in [format!("{method}_raw"), format!("{method}_raw_request")] {
        if !methods.insert(name.clone()) {
            bail!("Duplicate generated Rust SDK method {name}");
        }
    }
    Ok(())
}

fn compile_request(
    op: &forklaunch_core::codegen::SdkOperation,
    prefix: &str,
    models: &mut BTreeMap<String, RustModelTemplate>,
    aliases: &mut BTreeMap<String, RustTypeAliasTemplate>,
) -> Result<String> {
    if !matches!(
        op.request_body_kind.unwrap_or(BodyKind::Json),
        BodyKind::Json | BodyKind::UrlEncoded
    ) {
        return Ok(String::new());
    }
    let Some(name) = &op.request_body else {
        return Ok(String::new());
    };
    let compiled = forklaunch_core::codegen::compile_schema_ref(
        &format!("{prefix}RequestBody"),
        &forklaunch_core::codegen::SchemaRef {
            name: name.clone(),
            json_schema: op.request_body_json_schema.clone(),
        },
    )?;
    let ty = compiled.rust_type.clone();
    merge_models(compiled, models, aliases)?;
    Ok(ty)
}

fn request_content_type(op: &forklaunch_core::codegen::SdkOperation) -> Result<(BodyKind, String)> {
    if let Some(name) = &op.request_body {
        forklaunch_core::codegen::compile_schema_ref(
            "RequestBody",
            &forklaunch_core::codegen::SchemaRef {
                name: name.clone(),
                json_schema: op.request_body_json_schema.clone(),
            },
        )?;
    }
    for field in op
        .request_params
        .iter()
        .chain(&op.request_query)
        .chain(&op.request_headers)
    {
        forklaunch_core::codegen::compile_schema_ref(
            "RequestField",
            &forklaunch_core::codegen::SchemaRef {
                name: field.schema.clone(),
                json_schema: field.json_schema.clone(),
            },
        )?;
    }
    let kind = op.request_body_kind.unwrap_or(BodyKind::Json);
    if kind == BodyKind::ServerSentEvent {
        bail!("SSE request bodies are unsupported");
    }
    let content_type = forklaunch_core::codegen::media::request_media_type(
        op.request_body_content_type
            .as_deref()
            .unwrap_or(match kind {
                BodyKind::Text => "text/plain",
                BodyKind::Binary | BodyKind::File => "application/octet-stream",
                BodyKind::Multipart => "multipart/form-data",
                BodyKind::UrlEncoded => "application/x-www-form-urlencoded",
                _ => "application/json",
            }),
    )?;
    let normalized_content_type = normalized_media_type(&content_type)?;
    if normalized_content_type == "text/event-stream" {
        bail!("SSE request bodies are unsupported");
    }
    if normalized_content_type.starts_with("multipart/") != (kind == BodyKind::Multipart)
        || kind == BodyKind::Multipart && normalized_content_type != "multipart/form-data"
    {
        bail!("Multipart SDK requests require multipart/form-data and the multipart body kind");
    }
    Ok((kind, content_type))
}

fn compile_responses(
    op: &forklaunch_core::codegen::SdkOperation,
    prefix: &str,
    models: &mut BTreeMap<String, RustModelTemplate>,
    aliases: &mut BTreeMap<String, RustTypeAliasTemplate>,
    modern: bool,
) -> Result<Vec<forklaunch_core::codegen::render::SdkResponseVariant>> {
    let mut declared = BTreeMap::new();
    for response in &op.responses {
        if !(100..=599).contains(&response.status_code) {
            bail!("Invalid SDK response status code");
        }
        if response.status_code < 200 {
            continue;
        }
        if declared
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
    let counts = declared
        .keys()
        .fold(BTreeMap::<u16, usize>::new(), |mut counts, (status, _)| {
            *counts.entry(*status).or_default() += 1;
            counts
        });
    for (index, ((status, media), response)) in declared.into_iter().enumerate() {
        let multiple = counts[&status] > 1;
        let media_name = forklaunch_core::codegen::response::response_media_name(&media);
        let root = if multiple {
            format!("{prefix}Status{status}{media_name}Response")
        } else if index == 0 {
            format!("{prefix}Response")
        } else {
            format!("{prefix}Status{status}Response")
        };
        let (mut variant, mut compiled) = compile_response(op.method, response, &root)?;
        // Model compilation already resolves this resource graph. Feed the same
        // checked graph to runtime validators instead of unresolved anchors.
        if variant.json_response || variant.text_response || variant.streaming_response {
            variant.rust_body_schema = format!(
                "{:?}",
                runtime_schemas::schema(&serde_json::from_str(&variant.body_schema_json)?)?
                    .to_string()
            );
        }
        variant.rust_header_rules =
            format!("{:?}", runtime_schemas::fields(&variant.header_rules)?);
        variant.shared_response_decoders = modern;
        if modern
            && let Some(alias) = compiled
                .rust_aliases
                .iter()
                .find(|alias| alias.name == root)
        {
            let target = alias.target_type.clone();
            variant.rust_response_type = if variant.rust_response_type == root {
                target
            } else if variant.rust_response_type == format!("Box<{root}>") {
                format!("Box<{target}>")
            } else if variant.rust_response_type == format!("Box<SdkEventStream<{root}>>") {
                format!("Box<SdkEventStream<{target}>>")
            } else {
                variant.rust_response_type.clone()
            };
            compiled.rust_aliases.retain(|alias| alias.name != root);
        }
        variant.media_discriminator = multiple;
        if multiple {
            variant.status_variant = format!("Status{status}{media_name}");
        }
        merge_models(compiled, models, aliases)?;
        responses.push(variant);
    }
    Ok(responses)
}

fn merge_models(
    compiled: forklaunch_core::codegen::SchemaModels,
    models: &mut BTreeMap<String, RustModelTemplate>,
    aliases: &mut BTreeMap<String, RustTypeAliasTemplate>,
) -> Result<()> {
    for model in compiled.rust_models {
        if models
            .get(&model.name)
            .is_some_and(|existing| existing != &model)
        {
            bail!("Conflicting generated Rust SDK model {}", model.name);
        }
        models.insert(model.name.clone(), model);
    }
    for alias in compiled.rust_aliases {
        if aliases
            .get(&alias.name)
            .is_some_and(|existing| existing != &alias)
        {
            bail!("Conflicting generated Rust SDK alias {}", alias.name);
        }
        aliases.insert(alias.name.clone(), alias);
    }
    Ok(())
}

fn reserved_type_names() -> BTreeSet<String> {
    [
        "runtime",
        "ClientRuntime",
        "SdkRequest",
        "SdkError",
        "ResponseValidator",
        "SdkAuth",
        "HmacSigner",
        "SdkEventStream",
        "MultipartResponse",
        "MultipartResponsePart",
        "SdkRequestHook",
        "String",
        "Vec",
        "Option",
        "Result",
        "Box",
        "BTreeMap",
        "Arc",
        "Error",
        "Serialize",
        "Deserialize",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn compile_fields(
    fields: &[forklaunch_core::codegen::SdkField],
    name: &str,
    models: &mut BTreeMap<String, RustModelTemplate>,
    aliases: &mut BTreeMap<String, RustTypeAliasTemplate>,
) -> Result<String> {
    if fields.is_empty() {
        return Ok(String::new());
    }
    let mut names = BTreeSet::new();
    let mut members = Vec::new();
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
        let member = field_identifier(&field.name);
        if !names.insert(member.clone()) {
            bail!("Conflicting generated request field {}", field.name);
        }
        let compiled = forklaunch_core::codegen::compile_schema_ref(
            &format!("{name}Field{index}"),
            &forklaunch_core::codegen::SchemaRef {
                name: field.schema.clone(),
                json_schema: field.json_schema.clone(),
            },
        )?;
        let ty = compiled.rust_type.clone();
        merge_models(compiled, models, aliases)?;
        members.push(forklaunch_core::codegen::RustModelFieldTemplate {
            date_codec: String::new(),
            optional: !field.required,
            required: field.required,
            flatten: false,
            wire_name_literal: format!("{:?}", field.name),
            field_name: member,
            field_type: if field.required {
                ty
            } else {
                format!("Option<{ty}>")
            },
        });
    }
    let model = RustModelTemplate {
        identity_name: String::new(),
        name: name.into(),
        fields: members,
        union: false,
        sequence: false,
        sequence_len: 0,
        sequence_min: 0,
        sequence_max: 0,
        sequence_rest: false,
        sequence_rest_type: String::new(),
        deny_unknown_fields: true,
        transparent: false,
        has_default: true,
        variants: vec![],
        string_enum: false,
        enum_variants: vec![],
    };
    if models.insert(name.into(), model).is_some() {
        bail!("Conflicting generated request model {name}");
    }
    Ok(name.into())
}

/// Render the standalone compatibility source from the same canonical model.
/// # Errors
/// Rejects unsupported or conflicting contract declarations.
pub fn generate_rust_http_sdk_source(
    plan: &SdkGenerationPlan,
    options: &RustSdkOptions,
) -> Result<String> {
    let data = prepare_http_sdk(plan, options, false)?;
    Ok(ramhorns::Template::new(include_str!("http_client.rs.template"))?.render(&data))
}

/// Emit an export-only crate facade, one model owner and one client implementation.
/// Transport and validation continue to delegate to forklaunch-universal-sdk.
/// # Errors
/// Rejects invalid contracts or inconsistent framework template sections.
pub fn generate_rust_http_sdk_files(
    plan: &SdkGenerationPlan,
    options: &RustSdkOptions,
) -> Result<BTreeMap<String, String>> {
    let mut data = prepare_http_sdk(plan, options, true)?;
    let schemas = extract_schemas(&mut data);
    let template = include_str!("http_client.rs.template");
    let (imports, rest) = template
        .split_once("{{! forklaunch:models}}")
        .ok_or_else(|| anyhow::anyhow!("Missing model template section"))?;
    let (models, client) = rest
        .split_once("{{! forklaunch:client}}")
        .ok_or_else(|| anyhow::anyhow!("Missing client template section"))?;
    let render =
        |source: &str| -> Result<String> { Ok(ramhorns::Template::new(source)?.render(&data)) };
    let model_imports = imports
        .lines()
        .filter(|line| {
            !line.starts_with("type ClientRuntime =")
                && (data.has_operations || *line != "use std::fmt;")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let models = render(&format!("{model_imports}\n{models}"))?;
    let schema_import = if schemas.is_empty() {
        ""
    } else {
        "use super::schemas;\n"
    };
    let client_imports = format!(
        "use std::fmt;\nuse forklaunch_universal_sdk as runtime;\nuse super::models::*;\n{schema_import}type ClientRuntime = runtime::UniversalSdk{{{{#has_multipart}}}}<MultipartForm>{{{{/has_multipart}}}};\n"
    );
    let client = render(&format!("{client_imports}{client}"))?;
    let facade = format!(
        "mod models;\nmod client;\nmod schemas;\npub use models::*;\npub use client::{};\n",
        options.client_struct
    );
    Ok(BTreeMap::from([
        ("lib.rs".into(), format_rust(&facade)?),
        ("models.rs".into(), format_rust(&models)?),
        ("client.rs".into(), format_rust(&client)?),
        ("schemas.rs".into(), format_rust(&schemas)?),
    ]))
}

/// Format deterministically in-process: exporting a contract needs no rustfmt executable.
fn format_rust(source: &str) -> Result<String> {
    use syn::visit_mut::VisitMut as _;
    // These names are reserved by the HTTP model compiler, so shortening their
    // type paths cannot bind to a user schema. Expressions and schema strings
    // retain their exact contents.
    struct BuiltinTypes;
    impl syn::visit_mut::VisitMut for BuiltinTypes {
        fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
            syn::visit_mut::visit_type_path_mut(self, ty);
            let parts = ty
                .path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>();
            let parts = parts.iter().map(String::as_str).collect::<Vec<_>>();
            if ty.qself.is_none()
                && matches!(
                    parts.as_slice(),
                    ["std", "string", "String"]
                        | ["std", "vec", "Vec"]
                        | ["std", "option", "Option"]
                        | ["std", "result", "Result"]
                        | ["std", "boxed", "Box"]
                        | ["std", "collections", "BTreeMap"]
                )
            {
                let last = ty.path.segments.last().unwrap().clone();
                ty.path.segments = std::iter::once(last).collect();
                ty.path.leading_colon = None;
            }
        }
    }
    let mut syntax = syn::parse_file(source)?;
    BuiltinTypes.visit_file_mut(&mut syntax);
    Ok(prettyplease::unparse(&syntax))
}

fn extract_schemas(data: &mut RustSdkTemplate) -> String {
    let mut constants = BTreeMap::<String, String>::new();
    let mut by_value = BTreeMap::<String, String>::new();
    let mut constant = |name: String, value: &mut String| {
        if value.is_empty() {
            return;
        }
        let name = by_value.entry(value.clone()).or_insert_with(|| {
            constants.insert(name.clone(), value.clone());
            name
        });
        *value = format!("schemas::{name}");
    };
    for operation in &mut data.operations {
        let prefix = operation
            .execute_method_name
            .trim_start_matches("r#")
            .to_ascii_uppercase();
        constant(
            format!("{prefix}_VERSIONS"),
            &mut operation.version_metadata,
        );
        constant(
            format!("{prefix}_REQUEST_FIELDS"),
            &mut operation.request_objects,
        );
        for response in &mut operation.responses {
            let suffix = response.status_variant.to_ascii_uppercase();
            if response.json_response || response.text_response || response.streaming_response {
                constant(
                    format!("{prefix}_{suffix}_BODY"),
                    &mut response.rust_body_schema,
                );
            }
            if !response.headers_type.is_empty() {
                constant(
                    format!("{prefix}_{suffix}_HEADERS"),
                    &mut response.rust_header_rules,
                );
            }
        }
    }
    constants
        .into_iter()
        .fold(String::new(), |mut source, (name, value)| {
            let _ = writeln!(source, "pub(super) const {name}: &str = {value};");
            source
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::codegen::SdkTarget;
    use forklaunch_core::contract::{
        BodyContract, ContractRegistry, HttpMethod, RouteContract, SchemaRef,
    };

    fn plan(routes: Vec<RouteContract>) -> SdkGenerationPlan {
        let mut registry = ContractRegistry::new();
        for route in routes {
            registry.register(route);
        }
        SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry)
    }

    #[test]
    fn native_http_rejects_invalid_graphs_names_and_transports() {
        let options = RustSdkOptions::new("test-sdk");
        for schema in [
            r#"{"$ref":"https://example.invalid/schema"}"#,
            r##"{"$ref":"#"}"##,
        ] {
            let route = RouteContract::get("Read", "Invalid graph", "/value").with_response(
                200,
                Some(BodyContract {
                    schema: SchemaRef::named("Value").with_json_schema(schema),
                    ..BodyContract::json("Value")
                }),
            );
            assert!(generate_rust_http_sdk_source(&plan(vec![route]), &options).is_err());
        }
        for name in ["new", "With Bearer Token", "With Http Client"] {
            assert!(
                generate_rust_http_sdk_source(
                    &plan(vec![RouteContract::get(name, "Collision", "/")]),
                    &options
                )
                .is_err()
            );
        }
        assert!(
            generate_rust_http_sdk_source(
                &plan(vec![]),
                &options.clone().with_client_struct("type")
            )
            .is_err()
        );
        for name in [
            "runtime",
            "ClientRuntime",
            "SdkRequest",
            "String",
            "SdkAuth",
            "SdkEventStream",
            "MultipartResponse",
            "MultipartResponsePart",
            "Deserialize",
        ] {
            assert!(
                generate_rust_http_sdk_source(
                    &plan(vec![]),
                    &options.clone().with_client_struct(name)
                )
                .is_err()
            );
        }
        for kind in [BodyKind::Multipart, BodyKind::UrlEncoded, BodyKind::Unknown] {
            let route = RouteContract::get("Read", "Unsupported response", "/").with_response(
                200,
                Some(BodyContract {
                    kind,
                    ..BodyContract::json("Value")
                }),
            );
            assert!(generate_rust_http_sdk_source(&plan(vec![route]), &options).is_err());
        }
        let route = RouteContract::get("Read", "Duplicate status", "/")
            .with_response(200, None)
            .with_response(200, None);
        assert!(generate_rust_http_sdk_source(&plan(vec![route]), &options).is_err());
    }

    #[test]
    fn json_requests_are_schema_typed_and_raw_is_explicit() {
        let body = BodyContract {
            schema: SchemaRef::named("Input").with_json_schema(
                r#"{"type":"object","properties":{"name":{"type":"string"},"amount":{"type":"number"}},"required":["name"]}"#,
            ),
            ..BodyContract::json("Input")
        };
        let route = RouteContract::post("Create", "Typed request", "/")
            .with_request_body(body)
            .with_response(200, Some(BodyContract::json("String")));
        let source = generate_rust_http_sdk_source(
            &plan(vec![route.clone()]),
            &RustSdkOptions::new("test-sdk"),
        )
        .unwrap();
        assert!(source.contains("pub struct CreateRequestBody"));
        assert!(source.contains("pub struct CreateRequest<T = CreateRequestBody>"));
        assert!(source.contains("pub async fn create(&self, input: CreateRequest)"));
        assert!(source.contains("pub async fn create_raw(&self, input: CreateRequest<String>)"));
        assert!(source.contains("serde_json::to_string(&body)"));
        assert!(source.contains("pub type CreateResponse = ::std::string::String;"));
        for collision in ["Create Raw", "Create Raw Request"] {
            assert!(
                generate_rust_http_sdk_source(
                    &plan(vec![
                        route.clone(),
                        RouteContract::get(collision, "Collision", "/collision")
                    ]),
                    &RustSdkOptions::new("test-sdk")
                )
                .is_err()
            );
        }
    }

    #[test]
    fn native_http_preserves_status_media_null_and_head() {
        let route = RouteContract::get("Read", "All statuses", "/value")
            .with_response(200, Some(BodyContract::json("String")))
            .with_response(
                201,
                Some(BodyContract {
                    schema: SchemaRef::named("Null").with_json_schema(r#"{"type":"null"}"#),
                    ..BodyContract::json("Null")
                }),
            )
            .with_response(204, None)
            .with_response(
                409,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    content_type: Some("text/plain; charset=utf-8".into()),
                    ..BodyContract::json("String")
                }),
            );
        let source =
            generate_rust_http_sdk_source(&plan(vec![route]), &RustSdkOptions::new("test-sdk"))
                .unwrap();
        assert!(source.contains("pub type ReadResponse = ::std::string::String;"));
        assert!(source.contains("pub type ReadStatus201Response = ();"));
        assert!(source.contains("204 => {"));
        assert!(source.contains("let bytes=response.bytes().await?;"));
        assert!(source.contains("let _=bytes; let body=();"));
        assert!(source.contains("Ok(ReadResponses::Status204(body))"));
        assert!(source.contains("409 if _content_type == \"text/plain\" => {"));
        assert!(source.contains("let body=response.text().await?;"));
        assert!(source.contains("Ok(ReadResponses::Status409(body))"));
        let head = RouteContract::new("Inspect", "HEAD", HttpMethod::Head, "/")
            .with_response(200, Some(BodyContract::json("String")));
        let source =
            generate_rust_http_sdk_source(&plan(vec![head]), &RustSdkOptions::new("test-sdk"))
                .unwrap();
        assert!(source.contains("pub type InspectResponse = ();"));
        assert!(!source.contains("response.json::<"));
    }
}
