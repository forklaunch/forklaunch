//! Configured IAM CRUD contracts shared by HTTP, MCP and generated SDKs.
use crate::services::{
    CreateOrganizationDto, CreatePermissionDto, CreateRoleDto, CreateUserDto, OrganizationDto,
    PermissionDto, RoleDto, UpdateOrganizationDto, UpdatePermissionDto, UpdateRoleDto,
    UpdateUserDto, UserDto,
};
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, ContractRegistry, FieldContract, HmacAuth,
    JwtAuthConfig, RbacConfig, RouteContract, ServiceContract,
};
use schemars::JsonSchema;
use serde_json::{Value, json};
fn body<T: JsonSchema>(name: &str, create: bool) -> BodyContract {
    let mut settings = schemars::generate::SchemaSettings::draft07();
    settings.inline_subschemas = true;
    let mut schema = serde_json::to_value(settings.into_generator().into_root_schema_for::<T>())
        .expect("package schema serializes");
    configure(&mut schema, create);
    let mut body = BodyContract::json(name);
    body.schema.json_schema = Some(schema.to_string());
    body
}
fn configure(schema: &mut Value, create: bool) {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        if create {
            properties.remove("id");
        } else if let Some(id) = properties.get_mut("id") {
            id["format"] = json!("uuid");
        }
        if let Some(email) = properties.get_mut("email") {
            email["format"] = json!("email");
        }
        if let Some(status) = properties.get_mut("status") {
            status["enum"] = json!(["active", "inactive"]);
        }
        // Permission read DTOs do not expose the write-only role assignment input.
        if properties.contains_key("slug") && properties.contains_key("createdAt") {
            properties.remove("addToRolesIds");
        }
        for property in properties.values_mut() {
            configure(property, false);
        }
    }
    if let Some(items) = schema.get_mut("items") {
        configure(items, create);
    }
}
fn text() -> BodyContract {
    let mut body = BodyContract {
        kind: BodyKind::Text,
        schema: forklaunch_core::contract::SchemaRef::named("String"),
        content_type: Some("text/plain".into()),
    };
    body.schema.json_schema = Some(json!({"type":"string"}).to_string());
    body
}
fn protect(mut route: RouteContract, family: &str, read: bool) -> RouteContract {
    if read || (family == "User" && route.method.as_str() != "POST") {
        route = route
            .with_access(AccessLevel::Protected)
            .with_auth(AuthConfig::jwt(JwtAuthConfig::JwksUrl {
                url: "${JWKS_PUBLIC_KEY_URL}".into(),
            }));
        let mut rbac = RbacConfig::default();
        if family == "Organization" {
            rbac.allowed_roles = vec!["admin".into(), "system".into()];
        } else if family == "User" && !read {
            rbac.allowed_roles = vec!["platform:read".into()];
        } else {
            rbac.allowed_permissions = vec!["platform:read".into()];
        }
        route = route.with_rbac(rbac);
    } else {
        route = route
            .with_access(AccessLevel::Internal)
            .with_auth(AuthConfig::Hmac(
                HmacAuth::new().with_header_name("authorization"),
            ));
    }
    route
        .with_response(400, Some(text()))
        .with_response(401, Some(text()))
        .with_response(403, Some(text()))
        .with_response(404, Some(text()))
        .with_response(409, Some(text()))
        .with_response(500, Some(text()))
}
fn family<C: JsonSchema, U: JsonSchema, R: JsonSchema>(
    registry: &mut ContractRegistry,
    name: &str,
    base: &str,
    batches: bool,
) {
    let create = RouteContract::post(
        format!("Create {name}"),
        format!("Creates a new {base}"),
        format!("/{base}"),
    )
    .with_request_body(body::<C>(&format!("Create{name}Dto"), true))
    .with_response(
        201,
        Some(if name == "Organization" {
            body::<R>(&format!("{name}Dto"), false)
        } else {
            text()
        }),
    );
    registry.register(protect(create, name, false));
    registry.register(protect(
        RouteContract::get(
            format!("Get {name}"),
            format!("Gets a {base} by ID"),
            format!("/{base}/:id"),
        )
        .with_param(id_field())
        .with_response(200, Some(body::<R>(&format!("{name}Dto"), false))),
        name,
        true,
    ));
    registry.register(protect(
        RouteContract::put(
            format!("Update {name}"),
            format!("Updates a {base}"),
            format!("/{base}"),
        )
        .with_request_body(body::<U>(&format!("Update{name}Dto"), false))
        .with_response(
            200,
            Some(if name == "Organization" {
                body::<R>(&format!("{name}Dto"), false)
            } else {
                text()
            }),
        ),
        name,
        false,
    ));
    registry.register(protect(
        RouteContract::delete(
            format!("Delete {name}"),
            format!("Deletes a {base}"),
            format!("/{base}/:id"),
        )
        .with_param(id_field())
        .with_response(200, Some(text())),
        name,
        false,
    ));
    if batches {
        let plural = format!("{name}s");
        for route in [
            RouteContract::post(
                format!("Create Batch {plural}"),
                "Creates multiple records",
                format!("/{base}/batch"),
            )
            .with_request_body(body::<Vec<C>>(&format!("Create{plural}"), true))
            .with_response(201, Some(text())),
            RouteContract::put(
                format!("Update Batch {plural}"),
                "Updates multiple records",
                format!("/{base}/batch"),
            )
            .with_request_body(body::<Vec<U>>(&format!("Update{plural}"), false))
            .with_response(200, Some(text())),
            RouteContract::delete(
                format!("Delete Batch {plural}"),
                "Deletes multiple records",
                format!("/{base}/batch"),
            )
            .with_query(ids_field())
            .with_response(200, Some(text())),
        ] {
            registry.register(protect(route, name, false));
        }
        registry.register(protect(
            RouteContract::get(
                format!("Get Batch {plural}"),
                "Gets multiple records",
                format!("/{base}/batch"),
            )
            .with_query(ids_field())
            .with_response(200, Some(body::<Vec<R>>(&plural, false))),
            name,
            true,
        ));
    }
}
fn id_field() -> FieldContract {
    let mut field = FieldContract::required("id", "String");
    field.schema.json_schema = Some(json!({"type":"string","format":"uuid"}).to_string());
    field
}
fn ids_field() -> FieldContract {
    let mut field = FieldContract::required("ids", "StringArray");
    field.schema.json_schema =
        Some(json!({"type":"array","items":{"type":"string","format":"uuid"}}).to_string());
    field
}
/// # Panics
/// Only invalid package-owned schema definitions can panic.
#[must_use]
pub fn registry() -> ContractRegistry {
    let mut registry = ContractRegistry::for_service(ServiceContract::new("iam", "0.1.0"));
    family::<CreateUserDto, UpdateUserDto, UserDto>(&mut registry, "User", "user", true);
    family::<CreateRoleDto, UpdateRoleDto, RoleDto>(&mut registry, "Role", "role", true);
    family::<CreatePermissionDto, UpdatePermissionDto, PermissionDto>(
        &mut registry,
        "Permission",
        "permission",
        true,
    );
    family::<CreateOrganizationDto, UpdateOrganizationDto, OrganizationDto>(
        &mut registry,
        "Organization",
        "organization",
        false,
    );
    for (name, path, body) in [
        (
            "Surface User Roles",
            "surface-roles",
            body::<Vec<RoleDto>>("Roles", false),
        ),
        (
            "Surface User Permissions",
            "surface-permissions",
            body::<Vec<PermissionDto>>("Permissions", false),
        ),
    ] {
        registry.register(protect(
            RouteContract::get(name, name, format!("/user/:id/{path}"))
                .with_param(id_field())
                .with_response(200, Some(body)),
            "Surface",
            false,
        ));
    }
    registry.register(
        RouteContract::get(
            "GetAuthMethods",
            "Get configured authentication methods",
            "/discovery/auth-methods",
        )
        .with_response(204, Some(text())),
    );
    for (name, path, method, schema) in [
        (
            "ExportUserData",
            "/compliance/export/:userId",
            forklaunch_core::contract::HttpMethod::Get,
            json!({"type":"object","required":["userId","entities"],"properties":{"userId":{"type":"string"},"entities":{"type":"object","additionalProperties":{}}}}),
        ),
        (
            "EraseUserData",
            "/compliance/erase/:userId",
            forklaunch_core::contract::HttpMethod::Delete,
            json!({"type":"object","required":["entitiesAffected","recordsDeleted"],"properties":{"entitiesAffected":{"type":"array","items":{"type":"string"}},"recordsDeleted":{"type":"number"}}}),
        ),
    ] {
        let mut response = BodyContract::json(name);
        response.schema.json_schema = Some(schema.to_string());
        let route = if method == forklaunch_core::contract::HttpMethod::Get {
            RouteContract::get(name, name, path)
        } else {
            RouteContract::delete(name, name, path)
        };
        registry.register(
            route
                .with_access(AccessLevel::Protected)
                .with_auth(AuthConfig::jwt(JwtAuthConfig::JwksUrl {
                    url: "${JWKS_PUBLIC_KEY_URL}".into(),
                }))
                .with_rbac(RbacConfig {
                    allowed_roles: vec!["system".into()],
                    ..RbacConfig::default()
                })
                .with_param(FieldContract::required("userId", "String"))
                .with_response(200, Some(response))
                .with_response(404, Some(text())),
        );
    }
    registry
}

/// Combined session and CRUD operations. The legacy session user endpoint has
/// its own name so the configured directory can retain `getUser` in SDKs/MCP.
/// # Panics
/// Only invalid package-owned schema definitions can panic.
#[must_use]
pub fn combined_registry() -> ContractRegistry {
    let mut result = crate::contracts::session_registry();
    let mut combined = ContractRegistry::for_service(ServiceContract::new("iam", "0.1.0"));
    for mut route in result.routes().iter().cloned() {
        if route.operation_name == "getUser" {
            route.operation_name = "getSessionUser".into();
        }
        combined.register(route);
    }
    result = registry();
    for route in result.routes() {
        combined.register(route.clone());
    }
    combined
}
