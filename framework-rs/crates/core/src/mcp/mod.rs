#![forbid(unsafe_code)]

//! Model Context Protocol (MCP) tool generation from `ForkLaunch` contracts.
//!
//! Eligible HTTP routes (those with `options.mcp == true`) are projected into
//! MCP tool descriptors whose JSON-Schema `inputSchema`/`outputSchema` are
//! derived from the contract's params/query/headers/body and `schemars`
//! metadata. Additional tools can be registered manually, and a transport
//! authentication callback gates access. Enable `rmcp-server` for the streaming
//! HTTP runtime, dispatching tools through the same authenticated HTTP router.

/// Streamable HTTP client for external MCP servers.
#[cfg(feature = "mcp-client")]
pub mod client;
#[cfg(feature = "rmcp-server")]
pub mod server;

use forklaunch_common::{JsonField, json_array, json_object, json_string};
use forklaunch_core::contract::{ContractRegistry, FieldContract, RouteContract, SchemaRef};

/// An MCP tool projected from an HTTP contract (or registered manually).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolDefinition {
    pub name: String,
    pub summary: String,
    pub http_method: String,
    pub path: String,
    /// JSON Schema describing the tool's input arguments.
    pub input_schema_json: String,
    /// JSON Schema describing the tool's success output, when known.
    pub output_schema_json: Option<String>,
}

impl McpToolDefinition {
    /// Builds a manually registered tool with an explicit input schema.
    #[must_use]
    pub fn manual(
        name: impl Into<String>,
        summary: impl Into<String>,
        input_schema_json: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            http_method: "POST".to_owned(),
            path: String::new(),
            input_schema_json: input_schema_json.into(),
            output_schema_json: None,
        }
    }

    /// Serializes the tool as an MCP `tools/list` entry.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut fields = vec![
            JsonField::new("name", json_string(&self.name)),
            JsonField::new("description", json_string(&self.summary)),
            JsonField::new("inputSchema", self.input_schema_json.clone()),
        ];
        if let Some(output_schema) = &self.output_schema_json {
            fields.push(JsonField::new("outputSchema", output_schema.clone()));
        }
        json_object(fields)
    }
}

/// Returns the names of the MCP-eligible tools in a registry.
#[must_use]
pub fn tool_names(registry: &ContractRegistry) -> Vec<String> {
    mcp_tools(registry)
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

/// Projects MCP-eligible HTTP contracts into tool definitions.
///
/// Routes opt out by setting `options.mcp = false`.
#[must_use]
pub fn mcp_tools(registry: &ContractRegistry) -> Vec<McpToolDefinition> {
    mcp_routes(registry).iter().map(tool_from_route).collect()
}

fn mcp_routes(registry: &ContractRegistry) -> Vec<RouteContract> {
    registry
        .routes()
        .iter()
        .filter(|route| route.options.mcp)
        .flat_map(|route| {
            if route.versions.is_empty() {
                return vec![route.clone()];
            }
            route
                .versions
                .keys()
                .map(|version| {
                    let mut selected = route.for_version(version).expect("indexed version");
                    if route.versions.len() > 1 {
                        selected.operation_name = format!("{} [v{version}]", route.operation_name);
                    }
                    selected
                })
                .collect()
        })
        .collect()
}

fn tool_from_route(route: &RouteContract) -> McpToolDefinition {
    McpToolDefinition {
        name: route.operation_name.clone(),
        summary: route.summary.clone(),
        http_method: route.method.as_str().to_owned(),
        path: route.path.clone(),
        input_schema_json: input_schema(route),
        output_schema_json: output_schema(route),
    }
}

fn group_required(schema: Option<&SchemaRef>, fields: &[FieldContract]) -> bool {
    fields.iter().any(|field| field.required)
        || schema.map_or_else(
            || false,
            |schema| {
                checked_group(schema).expect("invalid MCP request group schema")(
                    &serde_json::json!({}),
                )
                .is_err()
            },
        )
}
fn checked_group(
    schema: &SchemaRef,
) -> Result<std::sync::Arc<crate::contract::schema_validation::CheckedJsonValidator>, String> {
    let value: serde_json::Value = serde_json::from_str(
        schema
            .json_schema
            .as_deref()
            .ok_or("missing MCP request group schema")?,
    )
    .map_err(|_| "invalid MCP request group schema")?;
    if value.get("type").and_then(serde_json::Value::as_str) != Some("object") {
        return Err("MCP request group schema must declare an object".into());
    }
    crate::contract::schema_validation::compile_checked(&value)
}
fn group_input_schema(
    fields: &[FieldContract],
    schema: Option<&SchemaRef>,
    location: &str,
) -> String {
    let base = fields_schema(fields, location);
    let Some(schema) = schema else {
        return base;
    };
    let mut combined: serde_json::Value =
        serde_json::from_str(&base).expect("generated field schema");
    combined["allOf"] = serde_json::json!([serde_json::from_str::<serde_json::Value>(
        &input_schema_at(schema, &["properties", location, "allOf", "0"])
    )
    .expect("checked request group schema")]);
    combined.to_string()
}
fn input_schema(route: &RouteContract) -> String {
    let mut properties = Vec::new();
    let mut required = Vec::new();

    if !route.request.params.is_empty() {
        properties.push(JsonField::new(
            "params",
            fields_schema(&route.request.params, "params"),
        ));
        // path params are always required
        required.push(json_string("params"));
    }
    if !route.request.query.is_empty() || route.request.query_schema.is_some() {
        properties.push(JsonField::new(
            "query",
            group_input_schema(
                &route.request.query,
                route.request.query_schema.as_ref(),
                "query",
            ),
        ));
        if group_required(route.request.query_schema.as_ref(), &route.request.query) {
            required.push(json_string("query"));
        }
    }
    if !route.request.headers.is_empty() || route.request.headers_schema.is_some() {
        properties.push(JsonField::new(
            "headers",
            group_input_schema(
                &route.request.headers,
                route.request.headers_schema.as_ref(),
                "headers",
            ),
        ));
        if group_required(
            route.request.headers_schema.as_ref(),
            &route.request.headers,
        ) {
            required.push(json_string("headers"));
        }
    }
    if let Some(body) = &route.request.body {
        properties.push(JsonField::new(
            "body",
            input_schema_at(&body.schema, &["properties", "body"]),
        ));
        if !route.request.body_optional {
            required.push(json_string("body"));
        }
    }

    let mut fields = vec![
        JsonField::new("type", json_string("object")),
        JsonField::new("properties", json_object(properties)),
    ];
    if !required.is_empty() {
        fields.push(JsonField::new("required", json_array(required)));
    }
    json_object(fields)
}

fn output_schema(route: &RouteContract) -> Option<String> {
    route
        .responses
        .iter()
        .find(|response| (200..300).contains(&response.status_code) && response.body.is_some())
        .and_then(|response| response.body.as_ref())
        .map(|body| schema_for_name(&body.schema))
}

fn fields_schema(fields: &[FieldContract], location: &str) -> String {
    let properties = json_object(fields.iter().map(|field| {
        JsonField::new(
            &field.name,
            input_schema_at(
                &field.schema,
                &["properties", location, "properties", &field.name],
            ),
        )
    }));
    let required = fields
        .iter()
        .filter(|field| field.required)
        .map(|field| json_string(&field.name))
        .collect::<Vec<_>>();

    let mut fields = vec![
        JsonField::new("type", json_string("object")),
        JsonField::new("properties", properties),
    ];
    if !required.is_empty() {
        fields.push(JsonField::new("required", json_array(required)));
    }
    json_object(fields)
}

// A schemars root becomes a nested MCP input property. Its local references
// must follow that move; otherwise #/definitions points at the envelope root.
fn input_schema_at(schema: &SchemaRef, location: &[&str]) -> String {
    forklaunch_core::contract::relocate_schema_json(&schema_for_name(schema), location)
}

fn schema_for_name(schema: &SchemaRef) -> String {
    // Prefer inline `schemars` metadata when the contract carries it.
    if let Some(json_schema) = &schema.json_schema {
        return json_schema.clone();
    }
    json_object([
        JsonField::new("type", json_string(json_type_for(&schema.name))),
        JsonField::new("title", json_string(&schema.name)),
    ])
}

fn json_type_for(schema_name: &str) -> &'static str {
    match schema_name {
        "String" | "Uuid" | "DateTime" => "string",
        "Integer" => "integer",
        "Number" | "Float" => "number",
        "Boolean" => "boolean",
        "Array" => "array",
        _ => "object",
    }
}

/// A callback the MCP transport invokes to authorize a connection.
pub trait McpAuthCallback {
    /// Returns whether the presented bearer token (if any) is authorized.
    fn authorize(&self, token: Option<&str>) -> bool;
}

/// Authorizes every connection (useful for local/dev transports).
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAllAuth;

impl McpAuthCallback for AllowAllAuth {
    fn authorize(&self, _token: Option<&str>) -> bool {
        true
    }
}

/// A collection of MCP tools combining contract-derived and manual tools.
#[derive(Debug, Clone, Default)]
pub struct McpToolRegistry {
    tools: Vec<McpToolDefinition>,
}

impl McpToolRegistry {
    /// Builds a tool registry from the MCP-eligible routes of a contract registry.
    #[must_use]
    pub fn from_registry(registry: &ContractRegistry) -> Self {
        Self {
            tools: mcp_tools(registry),
        }
    }

    /// Registers an additional, manually defined tool.
    #[must_use]
    pub fn with_tool(mut self, tool: McpToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    /// Returns the registered tools.
    #[must_use]
    pub fn tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }

    /// Serializes the registry as an MCP `tools/list` response payload.
    #[must_use]
    pub fn to_tools_list_json(&self) -> String {
        json_object([JsonField::new(
            "tools",
            json_array(self.tools.iter().map(McpToolDefinition::to_json)),
        )])
    }
}

#[cfg(test)]
mod tests {
    use forklaunch_core::contract::{
        BodyContract, FieldContract, HttpMethod, RouteContract, RouteOptions,
    };

    use super::*;

    #[test]
    fn versioned_tools_keep_declaration_order_and_distinct_input_output_schemas() {
        use forklaunch_core::contract::{RequestContract, ResponseContract, VersionedContract};
        let mut route = RouteContract::post("Versioned", "Versioned", "/versioned/:id")
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
        let definitions = mcp_tools(&registry);
        assert_eq!(
            definitions
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            vec!["versioned [vz-first]", "versioned [va-last]"]
        );
        for (tool, kind) in definitions.iter().zip(["string", "integer"]) {
            assert!(
                tool.input_schema_json
                    .contains(&format!(r#""type":"{kind}""#))
            );
            assert!(tool.input_schema_json.contains("params"));
            assert!(tool.output_schema_json.as_ref().unwrap().contains(kind));
        }
        let routes = mcp_routes(&registry);
        assert_eq!(routes[0].responses[0].status_code, 201);
        assert_eq!(routes[1].responses[0].status_code, 202);
    }

    #[test]
    fn whole_group_constraints_and_empty_object_optionality_are_preserved() {
        for requires in [true, false] {
            let mut route = RouteContract::get("Group", "Group", "/group");
            let mut schema = serde_json::json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"left":{"type":"string"},"right":{"type":"string"}},"dependentRequired":{"left":["right"]},"additionalProperties":false});
            if requires {
                schema["anyOf"] = serde_json::json!([{"required":["left"]},{"required":["right"]}]);
            }
            route.request.query_schema =
                Some(SchemaRef::named("Group").with_json_schema(schema.to_string()));
            let input: serde_json::Value = serde_json::from_str(&input_schema(&route)).unwrap();
            let validate = crate::contract::schema_validation::compile_checked(&input).unwrap();
            assert_eq!(validate(&serde_json::json!({})).is_ok(), !requires);
            assert!(validate(&serde_json::json!({"query":{"right":"yes"}})).is_ok());
            assert!(validate(&serde_json::json!({"query":{"left":"yes"}})).is_err());
            assert!(
                validate(&serde_json::json!({"query":{"right":"yes","unknown":"no"}})).is_err()
            );
        }
    }

    #[test]
    fn whole_groups_do_not_replace_individual_field_constraints() {
        let mut field = FieldContract::required("right", "String");
        field.schema =
            SchemaRef::named("Bounded").with_json_schema(r#"{"type":"string","minLength":3}"#);
        let mut route = RouteContract::get("Group", "Group", "/group").with_query(field);
        route.request.query_schema = Some(
            SchemaRef::named("Group")
                .with_json_schema(r#"{"type":"object","properties":{"right":{"type":"string"}}}"#),
        );
        let input: serde_json::Value = serde_json::from_str(&input_schema(&route)).unwrap();
        let validate = crate::contract::schema_validation::compile_checked(&input).unwrap();
        assert!(validate(&serde_json::json!({})).is_err());
        assert!(validate(&serde_json::json!({"query":{"right":"x"}})).is_err());
        assert!(validate(&serde_json::json!({"query":{"right":"valid"}})).is_ok());
    }

    #[test]
    fn required_headers_are_required_in_the_tool_envelope() {
        for mandatory in [true, false] {
            let mut header = FieldContract::required("x-request-id", "String");
            header.required = mandatory;
            let route = RouteContract::get("Read", "Read", "/read").with_header(header);
            let schema: serde_json::Value = serde_json::from_str(&input_schema(&route)).unwrap();
            let required = schema.get("required").and_then(serde_json::Value::as_array);
            assert_eq!(
                required.is_some_and(|values| values.iter().any(|v| v == "headers")),
                mandatory
            );
            assert!(schema["properties"]["headers"]["properties"]["x-request-id"].is_object());
        }
    }

    #[test]
    fn maps_routes_to_mcp_tool_definitions() {
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::new("Get User", "Get a user", HttpMethod::Get, "/users/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_response(200, Some(BodyContract::json("User"))),
        );

        let tools = mcp_tools(&registry);

        assert_eq!(tools[0].name, "getUser");
        assert_eq!(tools[0].http_method, "GET");
        assert_eq!(tools[0].path, "/users/:id");
        // input schema describes the required path param
        assert!(tools[0].input_schema_json.contains("\"params\""));
        assert!(tools[0].input_schema_json.contains("\"id\""));
        assert!(
            tools[0]
                .input_schema_json
                .contains("\"required\":[\"params\"]")
        );
        // output schema derived from the 200 response body
        assert!(
            tools[0]
                .output_schema_json
                .as_deref()
                .is_some_and(|schema| schema.contains("User"))
        );
    }

    #[test]
    fn nested_input_references_resolve_inside_the_mcp_envelope() {
        let mut body = BodyContract::json("Recursive");
        body.schema = SchemaRef::named("Recursive").with_json_schema(r##"{"type":"object","properties":{"child":{"$ref":"#/definitions/Node"}},"definitions":{"Node":{"type":"object","properties":{"next":{"$ref":"#/definitions/Node"}}}}}"##);
        let mut registry = ContractRegistry::new();
        registry
            .register(RouteContract::post("Create", "Create", "/items").with_request_body(body));
        let tools = mcp_tools(&registry);
        let schema: serde_json::Value = serde_json::from_str(&tools[0].input_schema_json).unwrap();
        let reference = schema["properties"]["body"]["properties"]["child"]["$ref"]
            .as_str()
            .unwrap();
        assert_eq!(reference, "#/properties/body/definitions/Node");
        assert!(
            schema
                .pointer(reference.strip_prefix('#').unwrap())
                .is_some()
        );
        assert_eq!(
            schema["properties"]["body"]["definitions"]["Node"]["properties"]["next"]["$ref"],
            reference
        );
    }

    #[test]
    fn relocating_recursive_inputs_preserves_literal_refs_and_escapes_field_names() {
        let source = serde_json::json!({"type":"object","properties":{
            "$ref":{"type":"string"},"child":{"$ref":"#"}
        },"default":{"$ref":"#"},"const":{"$ref":"#/literal"},
        "enum":[{"$ref":"#"}],"examples":[{"$ref":"#"}],"x-data":{"$ref":"#"}});
        let mut body = BodyContract::json("Recursive");
        body.schema = SchemaRef::named("Recursive").with_json_schema(source.to_string());
        let mut query = FieldContract::optional("filter/name~", "Recursive");
        query.schema = body.schema.clone();
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::post("Create", "Create", "/items")
                .with_query(query)
                .with_request_body(body),
        );
        let tools = mcp_tools(&registry);
        let schema: serde_json::Value = serde_json::from_str(&tools[0].input_schema_json).unwrap();
        for (input, pointer) in [
            (&schema["properties"]["body"], "/properties/body"),
            (
                &schema["properties"]["query"]["properties"]["filter/name~"],
                "/properties/query/properties/filter~1name~0",
            ),
        ] {
            assert_eq!(input["properties"]["child"]["$ref"], format!("#{pointer}"));
            assert!(schema.pointer(pointer).is_some());
            assert_eq!(input["properties"]["$ref"], source["properties"]["$ref"]);
            for keyword in ["default", "const", "enum", "examples", "x-data"] {
                assert_eq!(input[keyword], source[keyword], "{keyword}");
            }
        }
    }

    #[test]
    fn respects_per_route_opt_out() {
        let mut registry = ContractRegistry::new();
        registry.register(
            RouteContract::new("Hidden", "Hidden route", HttpMethod::Get, "/internal")
                .with_options(RouteOptions {
                    mcp: false,
                    ..RouteOptions::default()
                }),
        );
        registry.register(RouteContract::new(
            "Visible",
            "Visible route",
            HttpMethod::Get,
            "/public",
        ));

        let names = tool_names(&registry);
        assert_eq!(names, vec!["visible".to_owned()]);
    }

    #[test]
    fn registry_includes_manual_tools_in_tools_list() {
        let mut registry = ContractRegistry::new();
        registry.register(RouteContract::new(
            "List Users",
            "List users",
            HttpMethod::Get,
            "/users",
        ));

        let tool_registry =
            McpToolRegistry::from_registry(&registry).with_tool(McpToolDefinition::manual(
                "search",
                "Full-text search",
                "{\"type\":\"object\",\"properties\":{\"q\":{\"type\":\"string\"}}}",
            ));

        assert_eq!(tool_registry.tools().len(), 2);
        let payload = tool_registry.to_tools_list_json();
        assert!(payload.contains("\"listUsers\""));
        assert!(payload.contains("\"search\""));
        assert!(payload.contains("\"inputSchema\""));
    }

    #[test]
    fn auth_callback_gates_connections() {
        assert!(AllowAllAuth.authorize(Some("token")));
        assert!(AllowAllAuth.authorize(None));
    }
}
