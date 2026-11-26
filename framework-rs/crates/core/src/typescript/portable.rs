//! Type-only service exports for the framework universal SDK.
use std::fmt::Write as _;

use super::{TypeScriptHttpSdkOptions, http::prepare_http_sdk};
use crate::codegen::{SdkGenerationPlan, response::type_identifier};
use anyhow::Result;

/// Generate service-owned DTO and client declarations without service runtime imports.
///
/// # Errors
/// Rejects the same unsupported or ambiguous contracts as standalone SDK generation.
pub fn generate_typescript_service_declarations(
    plan: &SdkGenerationPlan,
    options: &TypeScriptHttpSdkOptions,
) -> Result<String> {
    let data = prepare_http_sdk(plan, options)?;
    validate_portable_names(&data)?;
    let mut source = String::from(
        "import type {\n  ForkLaunchSdkRequest,\n  ForkLaunchSdkResponse,\n  ForkLaunchErrorResponses,\n  ForkLaunchHeaders,\n} from '@forklaunch/universal-sdk';\n\n",
    );
    let client_name = format!("{}SdkClient", type_identifier(&options.service_name));
    let mut names = std::collections::BTreeSet::from([
        client_name.clone(),
        "ForkLaunchSdkRequest".into(),
        "ForkLaunchSdkResponse".into(),
        "ForkLaunchErrorResponses".into(),
        "ForkLaunchHeaders".into(),
    ]);
    for model in &data.schema_models {
        anyhow::ensure!(
            names.insert(model.name.clone()),
            "Conflicting service declaration type {}",
            model.name
        );
        writeln!(
            source,
            "export type {} = {};\n",
            model.name,
            readable_type(&model.target_type, 0)
        )?;
    }
    let mut client = format!("export type {client_name} = {{\n");
    for service in &data.services {
        for router in &service.routers {
            writeln!(client, "  {}: {{", router.router_key)?;
            for operation in &router.operations {
                let prefix =
                    operation_prefix(&mut names, &router.router_key, &operation.operation_key);
                let request = format!("{prefix}Request");
                let response = format!("{prefix}Response");
                let types = data
                    .operation_types
                    .iter()
                    .find(|types| types.request_type == operation.request_type)
                    .ok_or_else(|| anyhow::anyhow!("Missing operation declarations"))?;
                writeln!(source, "export type {request} = ForkLaunchSdkRequest<")?;
                let mut arguments = vec![
                    &types.request_params_type,
                    &types.request_query_type,
                    &types.request_body_type,
                    &types.request_headers_type,
                ];
                let optional = "true".to_owned();
                if types.request_body_optional {
                    arguments.push(&optional);
                }
                for (index, value) in arguments.iter().enumerate() {
                    writeln!(
                        source,
                        "  {}{}",
                        readable_type(value, 1),
                        if index + 1 < arguments.len() { "," } else { "" }
                    )?;
                }
                writeln!(source, ">;\n\nexport type {response} =")?;
                render_responses(&mut source, &types.responses)?;
                writeln!(
                    client,
                    "    {}: (input{}: {request}) => Promise<{response}>;",
                    operation.operation_key,
                    if operation.optional_input { "?" } else { "" }
                )?;
            }
            writeln!(client, "  }};")?;
        }
    }
    writeln!(client, "}};")?;
    source.push_str(&client);
    Ok(source)
}

fn render_responses(
    source: &mut String,
    responses: &[crate::codegen::render::SdkResponseVariant],
) -> Result<()> {
    let mut groups: Vec<Vec<&crate::codegen::render::SdkResponseVariant>> = Vec::new();
    for variant in responses {
        if let Some(group) = groups.iter_mut().find(|group| {
            let first = group[0];
            first.response_body_type == variant.response_body_type
                && first.content_type == variant.content_type
                && first.typescript_headers_type == variant.typescript_headers_type
        }) {
            group.push(variant);
        } else {
            groups.push(vec![variant]);
        }
    }
    for group in groups {
        let variant = group[0];
        if group.len() > 1 {
            let codes = group
                .iter()
                .map(|item| item.status_code.to_string())
                .collect::<Vec<_>>()
                .join(" | ");
            writeln!(
                source,
                "  | ForkLaunchSdkResponse<\n      Record<{codes}, {}>,\n      {},\n      Record<number, '{}'>\n    >",
                readable_type(&variant.response_body_type, 3),
                if variant.typescript_headers_type.is_empty() {
                    "never".to_owned()
                } else {
                    readable_type(&variant.typescript_headers_type, 3)
                },
                variant.content_type
            )?;
        } else {
            writeln!(
                source,
                "  | {{\n      code: {};\n      status: {};\n      contentType: '{}';\n      response: {};",
                variant.status_code,
                variant.status_code,
                variant.content_type,
                readable_type(&variant.response_body_type, 3)
            )?;
            if !variant.typescript_headers_type.is_empty() {
                writeln!(
                    source,
                    "      headers: {};",
                    readable_type(&variant.typescript_headers_type, 3)
                )?;
            }
            writeln!(source, "    }}")?;
        }
    }
    let statuses = responses
        .iter()
        .map(|variant| variant.status_code.to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(" | ");
    writeln!(
        source,
        "  | ForkLaunchSdkResponse<Omit<ForkLaunchErrorResponses, {}>>;\n",
        if statuses.is_empty() {
            "never"
        } else {
            &statuses
        }
    )?;
    Ok(())
}

// Reserve each operation's complete public surface as one unit. Named DTOs win.
fn operation_prefix(
    names: &mut std::collections::BTreeSet<String>,
    router: &str,
    operation: &str,
) -> String {
    let short = type_identifier(operation);
    let qualified = format!("{}{short}", type_identifier(router));
    for index in 0_u32.. {
        let candidate = match index {
            0 => short.clone(),
            1 => qualified.clone(),
            _ => format!("{qualified}{index}"),
        };
        let request = format!("{candidate}Request");
        let response = format!("{candidate}Response");
        if !names.contains(&request) && !names.contains(&response) {
            names.insert(request);
            names.insert(response);
            return candidate;
        }
    }
    unreachable!("Type name space exhausted")
}

// Format the compiler's object type expressions without altering quoted keys/literals.
fn readable_type(value: &str, base: usize) -> String {
    let mut output = String::new();
    let mut depth = base;
    let mut quote = None;
    let mut escaped = false;
    let mut pending_space = false;
    for ch in value.chars() {
        if let Some(delimiter) = quote {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !output.ends_with([' ', '\n']) && !output.is_empty() {
            output.push(' ');
        }
        pending_space = false;
        match ch {
            '\'' | '"' | '`' => {
                quote = Some(ch);
                output.push(ch);
            }
            '{' => {
                depth += 1;
                output.push_str("{\n");
                output.push_str(&"  ".repeat(depth));
            }
            ';' => {
                output.push_str(";\n");
                output.push_str(&"  ".repeat(depth));
            }
            '}' => {
                depth = depth.saturating_sub(1);
                while output.ends_with(' ') {
                    output.pop();
                }
                if !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str(&"  ".repeat(depth));
                output.push('}');
            }
            _ => output.push(ch),
        }
    }
    output.trim_end().to_owned()
}

/// Produce checked registry descriptors for the framework universal SDK engine.
/// Keys are original `OpenAPI` operation identifiers, never rewritten client names.
/// # Errors
/// Rejects invalid contracts, duplicate identifiers, or inconsistent compiler output.
pub fn generate_typescript_operation_descriptors(
    plan: &SdkGenerationPlan,
    options: &TypeScriptHttpSdkOptions,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    use serde_json::{Value, json};
    let data = prepare_http_sdk(plan, options)?;
    validate_portable_names(&data)?;
    let plan = crate::codegen::version_responses::prepare(plan)?;
    let mut descriptors = std::collections::BTreeMap::new();
    for op in &plan.operations {
        let (router, compiled) = data
            .services
            .iter()
            .flat_map(|s| &s.routers)
            .flat_map(|r| r.operations.iter().map(move |op| (r, op)))
            .find(|(_, c)| {
                c.operation_name == crate::codegen::escape_typescript_string(&op.name)
                    && c.path == crate::codegen::escape_typescript_string(&op.path)
            })
            .ok_or_else(|| anyhow::anyhow!("Missing compiled operation {}", op.name))?;
        let mut kinds = serde_json::Map::new();
        let mut schemas = serde_json::Map::new();
        let mut headers = serde_json::Map::new();
        for response in &compiled.responses {
            let key = format!("{}:{}", response.status_code, response.content_type);
            kinds.insert(key.clone(), json!(response.response_kind));
            schemas.insert(
                key.clone(),
                crate::codegen::runtime_schemas::schema(&serde_json::from_str(
                    &response.body_schema_json,
                )?)?,
            );
            if !response.headers_type.is_empty() {
                headers.insert(key, serde_json::from_str(&response.header_rules)?);
            }
        }
        let mut descriptor = json!({
            "format":"forklaunch.sdk-operation.v1",
            "clientPath":[router.router_key,compiled.operation_key],
            "service":options.service_name, "router":router.router_key,
            "operation":op.name, "method":op.method.as_str(), "path":op.path,
            "requestKind":compiled.request_kind,
            "requestContentType":super::http::request_body(op, "Descriptor", &mut std::collections::BTreeMap::new())?.1,
            "requestSchema":crate::codegen::runtime_schemas::schema(&serde_json::from_str::<Value>(op.request_body_json_schema.as_deref().unwrap_or("true"))?)?,
            "responseKinds":kinds, "responseSchemas":schemas, "responseHeaders":headers
        });
        for (key, metadata) in [
            (
                "requestObjects",
                crate::codegen::request_objects::metadata(op)?,
            ),
            (
                "versionMetadata",
                crate::codegen::version_responses::metadata(op)?,
            ),
        ] {
            if !metadata.is_empty() {
                let metadata = if key == "requestObjects" {
                    crate::codegen::runtime_schemas::request_objects(&metadata)?
                } else {
                    crate::codegen::runtime_schemas::versions(&metadata)?
                };
                descriptor[key] = serde_json::from_str(&metadata)?;
            }
        }
        anyhow::ensure!(
            !descriptors.contains_key(&op.name),
            "Duplicate portable operation identifier {}",
            op.name
        );
        descriptors.insert(op.name.clone(), crate::codegen::canonical_json(descriptor));
    }
    Ok(descriptors)
}

fn validate_portable_names(data: &crate::codegen::render::TypeScriptSdkTemplate) -> Result<()> {
    for router in data.services.iter().flat_map(|service| &service.routers) {
        for name in std::iter::once(&router.router_key)
            .chain(router.operations.iter().map(|op| &op.operation_key))
        {
            anyhow::ensure!(
                !matches!(
                    name.as_str(),
                    "then" | "catch" | "finally" | "__proto__" | "constructor" | "prototype"
                ),
                "Unsupported universal SDK member {name}"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_aliases_reserve_both_names_and_preserve_dto_names() {
        let mut names =
            std::collections::BTreeSet::from(["FetchRequest".into(), "ItemsFetchResponse".into()]);
        assert_eq!(
            operation_prefix(&mut names, "items", "fetch"),
            "ItemsFetch2"
        );
        assert_eq!(
            operation_prefix(&mut names, "items", "fetch"),
            "ItemsFetch3"
        );
        assert_eq!(operation_prefix(&mut names, "items", "create"), "Create");
    }

    #[test]
    fn type_formatting_preserves_punctuation_inside_literals() {
        let source = r"{ 'odd;|{}': 'a;|{}'; nested: { value: string; }; }";
        let formatted = readable_type(source, 0);
        assert!(formatted.contains("'odd;|{}': 'a;|{}';\n"));
        assert!(formatted.contains("nested: {\n    value: string;\n  }"));
    }
}
