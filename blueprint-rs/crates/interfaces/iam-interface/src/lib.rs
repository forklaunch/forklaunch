#![forbid(unsafe_code)]

pub mod async_service;
pub mod module_contracts;
pub mod services;
pub use async_service::{AsyncIamRepository, AsyncIamService, AsyncIamSessions};

use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;

/// An organization (tenant boundary) a user can belong to.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
}

impl Organization {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, slug: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            slug: slug.into(),
        }
    }
}

/// A named role bundling a set of permissions.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Role {
    pub name: String,
    pub permissions: Vec<Permission>,
}

impl Role {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            permissions: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permissions.push(permission);
        self
    }
}

/// An issued bearer token bound to a user.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Token {
    pub value: String,
    pub user_id: String,
    /// Numeric UNIX seconds must remain exact in JavaScript clients.
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    pub expires_at_unix_seconds: u64,
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub disabled: bool,
    pub roles: Vec<String>,
}

impl User {
    #[must_use]
    pub fn new(id: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            email: email.into(),
            display_name: None,
            disabled: false,
            roles: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    #[must_use]
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    /// Numeric UNIX seconds must remain exact in JavaScript clients.
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    pub expires_at_unix_seconds: u64,
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Permission {
    pub resource: String,
    pub action: String,
}

impl Permission {
    #[must_use]
    pub fn new(resource: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            action: action.into(),
        }
    }
}

pub trait IamService {
    /// # Errors
    ///
    /// Returns an error when the user backend cannot complete the lookup.
    fn get_user(&self, id: &str) -> BlueprintResult<Option<User>>;

    /// # Errors
    ///
    /// Returns an error when the user backend cannot complete the lookup.
    fn find_user_by_email(&self, email: &str) -> BlueprintResult<Option<User>>;

    /// # Errors
    ///
    /// Returns an error when the user backend cannot list users.
    fn list_users(&self, request: PageRequest) -> BlueprintResult<Page<User>>;

    /// # Errors
    ///
    /// Returns an error when credentials are invalid or the backend cannot create a session.
    fn authenticate(&self, credentials: Credentials) -> BlueprintResult<Session>;

    /// # Errors
    ///
    /// Returns an error when permission lookup fails.
    fn permissions_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Permission>>;

    /// # Errors
    ///
    /// Returns an error when the organization backend cannot complete the lookup.
    fn get_organization(&self, _id: &str) -> BlueprintResult<Option<Organization>> {
        Ok(None)
    }

    /// # Errors
    ///
    /// Returns an error when the organization backend cannot list organizations.
    fn list_organizations(&self, _request: PageRequest) -> BlueprintResult<Page<Organization>> {
        Ok(Page::new(Vec::new()))
    }

    /// # Errors
    ///
    /// Returns an error when role resolution fails.
    fn roles_for_user(&self, _user_id: &str) -> BlueprintResult<Vec<Role>> {
        Ok(Vec::new())
    }
}

#[must_use]
pub fn invalid_credentials() -> BlueprintError {
    BlueprintError::new("iam_invalid_credentials", "invalid credentials")
}

/// IAM HTTP route contracts, the source of truth for `OpenAPI` and SDK generation.
pub mod contracts {
    use forklaunch_core::contract::{
        AccessLevel, AuthConfig, BodyContract, ContractRegistry, FieldContract, JwtAuthConfig,
        RouteContract, ServiceContract,
    };

    use super::{Credentials, Organization, Permission, Session, User};
    use forklaunch_core::domain::Page;

    #[derive(serde::Serialize, schemars::JsonSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SessionErrorResponse {
        code: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        correlation_id: Option<String>,
    }

    fn session_error_body() -> BodyContract {
        json_body::<SessionErrorResponse>("IamSessionErrorResponse")
    }

    fn json_body<T: schemars::JsonSchema>(name: &str) -> BodyContract {
        let mut body = BodyContract::json(name);
        let mut settings = schemars::generate::SchemaSettings::draft07();
        settings.inline_subschemas = true;
        let schema = settings.into_generator().into_root_schema_for::<T>();
        body.schema.json_schema =
            Some(serde_json::to_string(&schema).expect("JSON Schema serializes"));
        body
    }

    /// Builds the IAM service contract registry.
    #[must_use]
    pub fn registry() -> ContractRegistry {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("iam", "0.1.0"));

        registry.register(
            RouteContract::post(
                "Authenticate",
                "Exchange credentials for a session",
                "/auth/login",
            )
            .with_request_body(json_body::<Credentials>("Credentials"))
            .with_response(200, Some(json_body::<Session>("Session")))
            .with_response(401, None),
        );

        registry.register(
            RouteContract::get("Get User", "Get a user by id", "/users/:id")
                .with_access(AccessLevel::Authenticated)
                .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
                .with_param(FieldContract::required("id", "String"))
                .with_response(200, Some(json_body::<User>("User")))
                .with_response(404, None),
        );

        registry.register(
            RouteContract::get("List Users", "List users", "/users")
                .with_access(AccessLevel::Authenticated)
                .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
                .with_query(FieldContract::optional("limit", "Integer"))
                .with_query(FieldContract::optional("cursor", "String"))
                .with_response(200, Some(json_body::<Page<User>>("UserPage"))),
        );

        registry.register(
            RouteContract::get(
                "List User Permissions",
                "List the permissions granted to a user",
                "/users/:id/permissions",
            )
            .with_access(AccessLevel::Authenticated)
            .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
            .with_param(FieldContract::required("id", "String"))
            .with_response(200, Some(json_body::<Vec<Permission>>("PermissionList"))),
        );

        registry.register(
            RouteContract::get("List Organizations", "List organizations", "/organizations")
                .with_query(FieldContract::optional("limit", "Integer"))
                .with_query(FieldContract::optional("cursor", "String"))
                .with_access(AccessLevel::Authenticated)
                .with_auth(AuthConfig::jwt(JwtAuthConfig::SignatureKey))
                .with_response(
                    200,
                    Some(json_body::<Page<Organization>>("OrganizationPage")),
                ),
        );

        registry
    }

    /// IAM routes backed by persistent bearer sessions, including logout.
    #[must_use]
    pub fn session_registry() -> ContractRegistry {
        let mut sessions = ContractRegistry::for_service(ServiceContract::new("iam", "0.1.0"));
        for mut route in registry().routes().iter().cloned() {
            if route.auth.is_some() {
                route.auth = Some(AuthConfig::bearer());
                // Reads of another user, and listings, need `platform:read`.
                route = route
                    .with_response(401, Some(session_error_body()))
                    .with_response(403, Some(session_error_body()));
            }
            // The session adapter and its authenticator return the framework's
            // JSON error envelope, including for errors inherited from registry().
            for response in &mut route.responses {
                if response.status_code >= 400 && response.body.is_none() {
                    response.body = Some(session_error_body());
                }
            }
            route = route
                .with_response(400, Some(session_error_body()))
                .with_response(500, Some(session_error_body()));
            sessions.register(route);
        }
        sessions.register(
            RouteContract::get("Get Health", "IAM liveness", "/health")
                .with_response(200, Some(BodyContract::json("HealthResponse"))),
        );
        sessions.register(
            RouteContract::post("Logout", "Revoke the current session", "/auth/logout")
                .with_access(AccessLevel::Authenticated)
                .with_auth(AuthConfig::bearer())
                .with_response(204, None)
                .with_response(401, Some(session_error_body()))
                .with_response(500, Some(session_error_body())),
        );
        sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_error_contracts_validate_actual_framework_wire_bodies() {
        use forklaunch_core::contract::BodyKind;
        use forklaunch_core::contract::schema_validation::compile_checked;
        use forklaunch_core::http::ErrorKind;
        use forklaunch_core::http::ErrorResponse;

        let registry = contracts::session_registry();
        for route in registry.routes() {
            for response in &route.responses {
                if response.status_code < 400 {
                    continue;
                }
                let body = response
                    .body
                    .as_ref()
                    .expect("session errors have JSON bodies");
                assert_eq!(body.kind, BodyKind::Json);
                assert_eq!(body.content_type.as_deref(), Some("application/json"));
                let schema =
                    serde_json::from_str(body.schema.json_schema.as_ref().unwrap()).unwrap();
                let validate = compile_checked(&schema).unwrap();
                let kind = match response.status_code {
                    400 => ErrorKind::Validation,
                    401 => ErrorKind::Unauthorized,
                    403 => ErrorKind::Forbidden,
                    404 => ErrorKind::NotFound,
                    500 => ErrorKind::Internal,
                    status => panic!("unexpected session error status {status}"),
                };
                for correlation in [None, Some("request-id")] {
                    let mut error = ErrorResponse::new(kind, "safe message");
                    if let Some(correlation) = correlation {
                        error = error.with_correlation_id(correlation);
                    }
                    let wire = error.to_response();
                    assert_eq!(wire.status_code, response.status_code);
                    let payload = serde_json::from_str(wire.body.as_text().unwrap()).unwrap();
                    validate(&payload).unwrap();
                }
                for invalid in [
                    serde_json::json!({"code": 401, "message": "invalid"}),
                    serde_json::json!({"code": "unauthorized"}),
                    serde_json::json!({"code": "unauthorized", "message": "safe", "private": "detail"}),
                ] {
                    assert!(validate(&invalid).is_err());
                }
            }
        }
        let logout = registry
            .routes()
            .iter()
            .find(|route| route.operation_name == "logout")
            .unwrap();
        assert!(
            logout
                .responses
                .iter()
                .find(|response| response.status_code == 204)
                .unwrap()
                .body
                .is_none()
        );
        let combined = module_contracts::combined_registry();
        let directory = combined
            .routes()
            .iter()
            .find(|route| route.operation_name == "getUser")
            .unwrap();
        assert_eq!(
            directory
                .responses
                .iter()
                .find(|response| response.status_code == 401)
                .unwrap()
                .body
                .as_ref()
                .unwrap()
                .kind,
            BodyKind::Text
        );
    }

    #[test]
    fn portable_session_and_token_expiry_preserves_numeric_wire_values() {
        const SAFE: u64 = 9_007_199_254_740_991;
        for schema in [
            serde_json::to_value(schemars::schema_for!(Session)).unwrap(),
            serde_json::to_value(schemars::schema_for!(Token)).unwrap(),
        ] {
            let expiry = &schema["properties"]["expires_at_unix_seconds"];
            assert_eq!(expiry["type"], "integer");
            let validate =
                forklaunch_core::contract::schema_validation::compile_checked(expiry).unwrap();
            for seconds in [0, 1_800_000_000, SAFE] {
                validate(&serde_json::json!(seconds)).unwrap();
            }
            for invalid in [
                serde_json::json!(-1),
                serde_json::json!(SAFE + 1),
                serde_json::json!("1800000000"),
            ] {
                assert!(validate(&invalid).is_err());
            }
        }
        let wire = serde_json::to_value(Session {
            id: "session".into(),
            user_id: "user".into(),
            expires_at_unix_seconds: 1_800_000_000,
        })
        .unwrap();
        assert_eq!(
            wire["expires_at_unix_seconds"],
            serde_json::json!(1_800_000_000)
        );
    }

    #[test]
    fn user_builder_records_roles() {
        let user = User::new("user_1", "test@example.com")
            .with_display_name("Test User")
            .with_role("admin");

        assert_eq!(user.display_name.as_deref(), Some("Test User"));
        assert_eq!(user.roles, vec!["admin"]);
    }

    #[test]
    fn role_bundles_permissions() {
        let role = Role::new("editor").with_permission(Permission::new("articles", "write"));
        assert_eq!(role.permissions.len(), 1);
    }

    #[test]
    fn iam_contracts_cover_core_routes() {
        let registry = contracts::registry();
        let operation_names: Vec<&str> = registry
            .routes()
            .iter()
            .map(|route| route.operation_name.as_str())
            .collect();

        assert!(operation_names.contains(&"authenticate"));
        assert!(operation_names.contains(&"getUser"));
        assert!(operation_names.contains(&"listUsers"));
        assert!(operation_names.contains(&"listOrganizations"));

        // JWT-only routes require authentication without declaring RBAC
        let get_user = registry
            .routes()
            .iter()
            .find(|route| route.operation_name == "getUser")
            .expect("getUser route should exist");
        assert!(get_user.auth.is_some());
    }
}
