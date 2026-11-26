//! Read portable contracts into the same plan used by build-time SDK generation.
use crate::codegen::{BodyKind, SdkField, SdkGenerationPlan, SdkOperation, SdkResponse, SdkTarget};
use anyhow::{Context, Result, bail};
use forklaunch_core::contract::HttpMethod;
use serde_json::Value;

impl SdkGenerationPlan {
    /// Parse an exported framework contract without evaluating application code.
    /// Both build-time and CLI generation use the same schema/media/status compiler.
    /// # Errors
    /// Rejects unsupported artifact versions and malformed contract fields.
    pub fn from_artifact_json(target: SdkTarget, source: &str) -> Result<Self> {
        let mut artifact: Value = super::canonical_json(serde_json::from_str(source)?);
        if text(&artifact, "formatVersion")? != forklaunch_core::contract::CONTRACT_ARTIFACT_VERSION
        {
            bail!("Unsupported SDK contract artifact version");
        }
        if array(&artifact, "routes")?
            .iter()
            .any(|route| route.get("x-forklaunch-version-branches").is_some())
        {
            bail!("Version branch metadata is compiler-owned; declare route versions instead");
        }
        crate::codegen::versions::expand_contract(&mut artifact)?;
        let operations = array(&artifact, "routes")?
            .iter()
            .map(operation)
            .collect::<Result<_>>()?;
        Ok(Self { target, operations })
    }
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .with_context(|| format!("Contract {field} must be a string"))
}
fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .with_context(|| format!("Contract {field} must be an array"))
}
fn fields(value: &Value, field: &str) -> Result<Vec<SdkField>> {
    array(value, field)?
        .iter()
        .map(|field| {
            let schema = field.get("schema").context("Field schema is required")?;
            Ok(SdkField {
                name: text(field, "name")?.into(),
                schema: text(schema, "name")?.into(),
                json_schema: schema.get("jsonSchema").map(Value::to_string),
                required: field
                    .get("required")
                    .and_then(Value::as_bool)
                    .context("Field required must be boolean")?,
            })
        })
        .collect()
}
fn object_schema(request: &Value, name: &str) -> Result<Option<String>> {
    request
        .get(name)
        .map(|schema| {
            schema
                .get("jsonSchema")
                .map(Value::to_string)
                .with_context(|| format!("{name} requires jsonSchema"))
        })
        .transpose()
}
struct Body {
    name: String,
    schema: Option<String>,
    kind: BodyKind,
    media: Option<String>,
}
fn body(value: &Value) -> Result<Option<Body>> {
    let Some(value) = value.get("body") else {
        return Ok(None);
    };
    let schema = value.get("schema").context("Body schema is required")?;
    let kind = match text(value, "kind")? {
        "json" => BodyKind::Json,
        "text" => BodyKind::Text,
        "binary" => BodyKind::Binary,
        "urlEncoded" => BodyKind::UrlEncoded,
        "file" => BodyKind::File,
        "multipart" => BodyKind::Multipart,
        "serverSentEvent" => BodyKind::ServerSentEvent,
        "unknown" => BodyKind::Unknown,
        other => bail!("Unsupported contract body kind {other}"),
    };
    let media = value
        .get("contentType")
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .context("Body contentType must be string")
        })
        .transpose()?;
    Ok(Some(Body {
        name: text(schema, "name")?.into(),
        schema: schema.get("jsonSchema").map(Value::to_string),
        kind,
        media,
    }))
}
fn operation(route: &Value) -> Result<SdkOperation> {
    let method =
        HttpMethod::parse(text(route, "method")?).context("Unsupported contract method")?;
    let request = route
        .get("request")
        .context("Contract request is required")?;
    let request_body = body(request)?;
    let responses = array(route, "responses")?
        .iter()
        .map(|response| {
            let body = body(response)?;
            Ok(SdkResponse {
                status_code: u16::try_from(
                    response
                        .get("statusCode")
                        .and_then(Value::as_u64)
                        .context("Response statusCode must be integer")?,
                )?,
                body: body.as_ref().map(|b| b.name.clone()),
                body_json_schema: body.as_ref().and_then(|b| b.schema.clone()),
                body_kind: body.as_ref().map(|b| b.kind),
                body_content_type: body.and_then(|b| b.media),
                headers: fields(response, "headers")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SdkOperation {
        request_parser_options: route
            .get("options")
            .and_then(|options| options.get("bodyParser"))
            .map(Value::to_string),
        version_branches: route
            .get("x-forklaunch-version-branches")
            .and_then(Value::as_array)
            .map(|branches| branches.iter().map(operation).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default(),
        name: text(route, "operationName")?.into(),
        method,
        path: text(route, "path")?.into(),
        request_params: fields(request, "params")?,
        request_query: fields(request, "query")?,
        request_headers: fields(request, "headers")?,
        request_query_schema: object_schema(request, "querySchema")?,
        request_headers_schema: object_schema(request, "headersSchema")?,
        request_body_optional: request
            .get("bodyOptional")
            .map(|v| v.as_bool().context("bodyOptional must be boolean"))
            .transpose()?
            .unwrap_or(false),
        request_body: request_body.as_ref().map(|b| b.name.clone()),
        request_body_json_schema: request_body.as_ref().and_then(|b| b.schema.clone()),
        request_body_kind: request_body.as_ref().map(|b| b.kind),
        request_body_content_type: request_body.and_then(|b| b.media),
        response_status_codes: responses.iter().map(|r| r.status_code).collect(),
        responses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::contract::{
        BodyContract, ContractRegistry, FieldContract, RouteContract, ServiceContract,
    };
    #[test]
    fn portable_plan_equals_direct_registry_plan() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1"));
        let mut route = RouteContract::post("Create", "Create note", "/notes/:id")
            .with_request_body(BodyContract::json("Note"))
            .with_response(201, Some(BodyContract::json("Created")))
            .with_response(
                400,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    schema: crate::codegen::SchemaRef::named("Error"),
                    content_type: Some("text/plain".into()),
                }),
            );
        route
            .request
            .params
            .push(FieldContract::required("id", "String"));
        route
            .request
            .query
            .push(FieldContract::optional("include", "String"));
        route
            .request
            .headers
            .push(FieldContract::required("x-version", "String"));
        route
            .responses
            .push(forklaunch_core::contract::ResponseContract::new(
                201,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    schema: crate::codegen::SchemaRef::named("TextCreated"),
                    content_type: Some("text/plain".into()),
                }),
            ));
        registry.register(route.clone());
        let mut versioned = route;
        versioned.operation_name = "createVersioned".into();
        let request = versioned.request.clone();
        let responses = versioned.responses.clone();
        versioned = versioned.with_version(forklaunch_core::contract::VersionedContract {
            version: "v2".into(),
            request,
            responses,
        });
        registry.register(versioned);
        for target in [SdkTarget::Rust, SdkTarget::TypeScript] {
            assert_eq!(
                SdkGenerationPlan::from_artifact_json(
                    target,
                    &registry.clone().into_artifact().to_json()
                )
                .unwrap(),
                SdkGenerationPlan::from_registry(target, &registry)
            );
        }
    }
}
