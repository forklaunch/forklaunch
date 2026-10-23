//! IAM directory routes share HTTP/MCP validation, authentication and audited storage.
use crate::directory::{
    compliance::{DirectoryCompliance, DirectoryComplianceService},
    postgres::PostgresDirectoryStore,
    service::BaseIamDirectory,
};
use axum::{
    Extension, Json,
    extract::{Path, RawQuery},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
};
use forklaunch_axum::{
    AuthErrorFormat, RequestContext, RouterConfig, session::AuthenticatedSession,
};
use forklaunch_core::auth::{AuthError, Principal, parse_bearer};
use forklaunch_core::contract::AuthConfig;
use forklaunch_core::contract::HttpMethod;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_iam_interface::{
    AsyncIamSessions, module_contracts,
    services::{OrganizationService, PermissionService, RoleService, UserService},
};
use serde_json::{Value, json};
use std::sync::Arc;

/// A directory error as its HTTP response: the status follows the error code
/// and the body is the error's message as text, as in the TypeScript module.
#[derive(Debug)]
pub struct DirectoryFailure(pub BlueprintError);

impl From<BlueprintError> for DirectoryFailure {
    fn from(error: BlueprintError) -> Self {
        Self(error)
    }
}

impl IntoResponse for DirectoryFailure {
    fn into_response(self) -> Response {
        failure(&self.0)
    }
}

/// Decodes a directory request body. Like the packaged handlers, a body whose
/// `ids` member is not a string array is rejected too.
///
/// # Errors
/// `iam_directory_invalid` (400) when the body does not match `T`.
pub fn decode<T: serde::de::DeserializeOwned>(input: Value) -> BlueprintResult<T> {
    if let Some(ids) = input.get("ids") {
        serde_json::from_value::<Vec<String>>(ids.clone()).map_err(|_| invalid())?;
    }
    serde_json::from_value(input).map_err(|_| invalid())
}

/// The required JSON `ids` array of a batch read or delete query.
///
/// # Errors
/// `iam_directory_invalid` (400) when `ids` is missing, repeated or malformed.
pub fn batch_ids(query: Option<&str>) -> BlueprintResult<Vec<String>> {
    let input = query_input(query)?;
    serde_json::from_value(input["ids"].clone()).map_err(|_| invalid())
}

impl<S> super::IamHttp<S> {
    /// The tenant-bound directory service, audited as the session's subject
    /// under the request's correlation ID.
    ///
    /// # Errors
    /// Fails when the store rejects the audit context.
    pub fn directory(
        &self,
        session: &AuthenticatedSession,
        context: &RequestContext,
    ) -> BlueprintResult<BaseIamDirectory<PostgresDirectoryStore>> {
        let store = self.directory.with_audit_context(AuditContext::actor(
            &session.principal.subject,
            context.correlation_id.clone(),
        ))?;
        Ok(BaseIamDirectory::new(Arc::new(store)))
    }

    /// [`Self::directory`] scoped to the caller's organization, for the
    /// JWT-protected `/user` routes. The organization comes from trusted
    /// storage (or the stored session), never from caller claims.
    ///
    /// # Errors
    /// `iam_directory_organization_required` (401) when the caller has none.
    pub async fn user_directory(
        &self,
        session: &AuthenticatedSession,
        context: &RequestContext,
    ) -> BlueprintResult<BaseIamDirectory<PostgresDirectoryStore>> {
        let service = self.directory(session, context)?;
        let organization = organization_scope(&service, session, self.mode).await?;
        Ok(service.with_organization_scope(organization))
    }

    /// The directory's export and erasure service with the configured
    /// compliance classifications, audited like [`Self::directory`].
    ///
    /// # Errors
    /// Fails when the store rejects the audit context.
    pub fn compliance(
        &self,
        session: &AuthenticatedSession,
        context: &RequestContext,
    ) -> BlueprintResult<DirectoryComplianceService<PostgresDirectoryStore>> {
        let store = self.directory.with_audit_context(AuditContext::actor(
            &session.principal.subject,
            context.correlation_id.clone(),
        ))?;
        Ok(DirectoryComplianceService::new(
            Arc::new(store),
            self.compliance.as_ref().clone(),
        ))
    }
}

async fn organization_scope(
    service: &BaseIamDirectory<PostgresDirectoryStore>,
    session: &AuthenticatedSession,
    mode: DirectoryAuthorizationMode,
) -> BlueprintResult<String> {
    if mode == DirectoryAuthorizationMode::StoredSessions {
        session
            .claims
            .get("directoryOrganizationId")
            .and_then(Value::as_str)
            .map(str::to_owned)
    } else {
        service
            .get_organization_id_by_user_id(&session.principal.subject)
            .await?
    }
    .ok_or_else(|| {
        BlueprintError::new(
            "iam_directory_organization_required",
            "Unauthorized: missing organization context",
        )
    })
}

/// Authentication grant source for directory JWT routes. Legacy bearer and
/// internal HMAC routes retain their existing credential policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirectoryAuthorizationMode {
    #[default]
    JwtClaims,
    /// Require both verified JWT and current persisted session/binding; resolve
    /// fresh directory grants. JWT/credential grants are never merged.
    StoredSessions,
}

/// Bearer session, directory JWT and internal HMAC authentication for IAM
/// routes. Framework RBAC runs only after stored-session grant replacement;
/// grants are not cached.
pub(super) fn router_config<S: AsyncIamSessions + 'static>(
    http: &super::IamHttp<S>,
) -> RouterConfig {
    let opaque = super::session_config(&http.sessions)
        .authenticator
        .expect("session verifier exists");
    let auth = Arc::clone(&http.auth);
    let mode = http.mode;
    let authorization_store = Arc::clone(&http.directory);
    let config = RouterConfig::default().with_authenticator(move |request, contract| {
        let opaque = opaque.clone();
        let auth = auth.clone();
        let store = authorization_store.clone();
        async move {
            if matches!(contract.auth, Some(AuthConfig::Bearer)) {
                opaque(request, contract).await
            } else if mode == DirectoryAuthorizationMode::StoredSessions && matches!(contract.auth, Some(AuthConfig::Jwt(_))) {
                let token = request.headers.get("authorization").and_then(|header| parse_bearer(header))
                    .filter(|token| token.len() <= 8192).ok_or(AuthError::MissingCredentials)?.to_owned();
                let store = store.with_audit_context(AuditContext::system(request.context.correlation_id.clone()))
                    .map_err(|error| super::auth_backend_error(&error))?;
                let verified = auth.authenticate(request, contract).await?;
                let snapshot = store.authorization_for_session(&token).await
                    .map_err(|error| super::auth_backend_error(&error))?
                    .ok_or(AuthError::InvalidCredentials)?;
                if verified.principal.subject != snapshot.auth_user_id { return Err(AuthError::InvalidCredentials); }
                let mut principal = Principal::new(&snapshot.directory_user_id);
                principal.roles = snapshot.roles;
                principal.permissions = snapshot.permissions;
                let expiry = verified.claims.get("exp").and_then(Value::as_u64)
                    .map_or(snapshot.expires_at_unix_seconds, |expiry| expiry.min(snapshot.expires_at_unix_seconds));
                let claims = json!({"sub":principal.subject,"roles":principal.roles,"permissions":principal.permissions,
                    "credentialUserId":snapshot.auth_user_id,"directoryOrganizationId":snapshot.organization_id,"exp":expiry});
                Ok(AuthenticatedSession::new(principal, claims))
            } else {
                auth.authenticate(request, contract).await
            }
        }
    });
    directory_auth_error_formats(config)
}

fn directory_auth_error_formats(mut config: RouterConfig) -> RouterConfig {
    // Directory declarations retain the TypeScript text error wire format.
    // Opaque session routes declare JSON errors and keep the default renderer.
    for contract in module_contracts::registry().routes() {
        config = config.with_auth_error_format_for_route(contract, AuthErrorFormat::TypeScriptText);
    }
    config
}

/// The handler for one directory or compliance operation.
pub(super) fn handler<S>(http: &super::IamHttp<S>, contract: &RouteContract) -> super::Handler {
    let store = Arc::clone(&http.directory);
    let mode = http.mode;
    if contract.operation_name == "getauthmethods" {
        return any(|| async { StatusCode::NO_CONTENT });
    }
    if matches!(
        contract.operation_name.as_str(),
        "exportuserdata" | "eraseuserdata"
    ) {
        let erase = contract.operation_name == "eraseuserdata";
        let compliance = Arc::clone(&http.compliance);
        return any(
            move |Extension(session): Extension<AuthenticatedSession>,
                  Extension(context): Extension<RequestContext>,
                  Path(subject): Path<String>| {
                let store = store.clone();
                let compliance = compliance.clone();
                async move {
                    compliance_response(&store, &compliance, &session, &context, &subject, erase)
                        .await
                        .unwrap_or_else(|error| failure(&error))
                }
            },
        );
    }
    let handler_contract = contract.clone();
    match contract.method {
        HttpMethod::Post | HttpMethod::Put => any(
            move |Extension(session): Extension<AuthenticatedSession>,
                  Extension(context): Extension<RequestContext>,
                  Json(input): Json<Value>| {
                dispatch(
                    store.clone(),
                    session,
                    context,
                    handler_contract.clone(),
                    input,
                    mode,
                )
            },
        ),
        HttpMethod::Get | HttpMethod::Delete if contract.request.params.is_empty() => any(
            move |Extension(session): Extension<AuthenticatedSession>,
                  Extension(context): Extension<RequestContext>,
                  RawQuery(query): RawQuery| {
                let store = store.clone();
                let contract = handler_contract.clone();
                async move {
                    match query_input(query.as_deref()) {
                        Ok(input) => dispatch(store, session, context, contract, input, mode).await,
                        Err(error) => failure(&error),
                    }
                }
            },
        ),
        HttpMethod::Get | HttpMethod::Delete => any(
            move |Extension(session): Extension<AuthenticatedSession>,
                  Extension(context): Extension<RequestContext>,
                  Path(id): Path<String>| {
                dispatch(
                    store.clone(),
                    session,
                    context,
                    handler_contract.clone(),
                    json!({"id":id}),
                    mode,
                )
            },
        ),
        _ => panic!("Unsupported package-owned IAM method"),
    }
}
async fn compliance_response(
    store: &PostgresDirectoryStore,
    config: &DirectoryCompliance,
    session: &AuthenticatedSession,
    context: &RequestContext,
    subject: &str,
    erase: bool,
) -> BlueprintResult<Response> {
    let store = store.with_audit_context(AuditContext::actor(
        &session.principal.subject,
        context.correlation_id.clone(),
    ))?;
    let service = DirectoryComplianceService::new(Arc::new(store), config.clone());
    let incomplete = || {
        BlueprintError::new(
            "iam_directory_storage_error",
            "IAM directory compliance operation incomplete",
        )
    };
    if erase {
        let result = service.erase(subject).await?;
        if !result.failures.is_empty() {
            return Err(incomplete());
        }
        if result.records_deleted == 0 {
            return Ok((
                StatusCode::NOT_FOUND,
                "User not found or no PII data to erase",
            )
                .into_response());
        }
        Ok(Json(result).into_response())
    } else {
        let result = service.export(subject).await?;
        if !result.failures.is_empty() {
            return Err(incomplete());
        }
        if result.entities.is_empty() {
            return Ok((
                StatusCode::NOT_FOUND,
                "User not found or no PII data to export",
            )
                .into_response());
        }
        Ok(Json(result).into_response())
    }
}
fn query_input(query: Option<&str>) -> BlueprintResult<Value> {
    let mut result = json!({});
    for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        if key == "ids" {
            if result.get("ids").is_some() {
                return Err(invalid());
            }
            let ids: Vec<String> = serde_json::from_str(&value).map_err(|_| invalid())?;
            result["ids"] = json!(ids);
        }
    }
    if result.get("ids").is_none() {
        return Err(invalid());
    }
    Ok(result)
}
async fn dispatch(
    store: Arc<PostgresDirectoryStore>,
    session: AuthenticatedSession,
    context: RequestContext,
    contract: RouteContract,
    input: Value,
    mode: DirectoryAuthorizationMode,
) -> Response {
    match call(&store, &session, &context, &contract, input, mode).await {
        Ok(response) => response,
        Err(error) => failure(&error),
    }
}
async fn call(
    store: &PostgresDirectoryStore,
    session: &AuthenticatedSession,
    context: &RequestContext,
    contract: &RouteContract,
    input: Value,
    mode: DirectoryAuthorizationMode,
) -> BlueprintResult<Response> {
    let store = store.with_audit_context(AuditContext::actor(
        &session.principal.subject,
        context.correlation_id.clone(),
    ))?;
    let mut service = BaseIamDirectory::new(Arc::new(store));
    let operation = contract.operation_name.as_str();
    if matches!(contract.auth, Some(AuthConfig::Jwt(_))) && contract.path.starts_with("/user") {
        let organization = organization_scope(&service, session, mode).await?;
        // Resolve organization membership from trusted storage rather than caller claims.
        service = service.with_organization_scope(organization);
    }
    let value = execute(&service, operation, input).await?;
    if contract.method == HttpMethod::Get
        || operation == "createOrganization"
        || operation == "updateOrganization"
    {
        let status = if contract.method == HttpMethod::Post {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        return Ok((status, Json(value)).into_response());
    }
    let (verb, ending, status) = if operation.starts_with("create") {
        ("created", "successfully", StatusCode::CREATED)
    } else if operation.starts_with("update") {
        ("updated", "successfully", StatusCode::OK)
    } else {
        ("deleted", "successfully", StatusCode::OK)
    };
    let family = if operation.contains("Permissions") {
        "permissions"
    } else if operation.contains("Roles") {
        "roles"
    } else if operation.contains("Users") {
        "users"
    } else if operation.contains("Permission") {
        "Permission"
    } else if operation.contains("Role") {
        "Role"
    } else if operation.contains("User") {
        "User"
    } else {
        "Organization"
    };
    let message = if operation.contains("Batch") {
        format!("Batch {family} {verb} {ending}")
    } else {
        format!("{family} {verb} {ending}")
    };
    Ok((status, message).into_response())
}
#[allow(clippy::too_many_lines)]
async fn execute<S: UserService + RoleService + PermissionService + OrganizationService>(
    service: &S,
    operation: &str,
    input: Value,
) -> BlueprintResult<Value> {
    let id = input["id"].as_str().unwrap_or("");
    let ids: Vec<String> = input
        .get("ids")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    macro_rules! request {
        ($method:ident) => {
            serde_json::to_value(
                service
                    .$method(serde_json::from_value(input.clone()).map_err(|_| invalid())?)
                    .await?,
            )
            .map_err(|_| invalid())
        };
    }
    macro_rules! get {
        ($method:ident) => {
            serde_json::to_value(service.$method(id).await?).map_err(|_| invalid())
        };
    }
    macro_rules! batch {
        ($method:ident) => {
            serde_json::to_value(service.$method(&ids).await?).map_err(|_| invalid())
        };
    }
    match operation {
        "createPermission" => request!(create_permission),
        "getPermission" => get!(get_permission),
        "updatePermission" => request!(update_permission),
        "deletePermission" => get!(delete_permission),
        "createBatchPermissions" => request!(create_batch_permissions),
        "getBatchPermissions" => batch!(get_batch_permissions),
        "updateBatchPermissions" => request!(update_batch_permissions),
        "deleteBatchPermissions" => batch!(delete_batch_permissions),
        "createRole" => request!(create_role),
        "getRole" => get!(get_role),
        "updateRole" => request!(update_role),
        "deleteRole" => get!(delete_role),
        "createBatchRoles" => request!(create_batch_roles),
        "getBatchRoles" => batch!(get_batch_roles),
        "updateBatchRoles" => request!(update_batch_roles),
        "deleteBatchRoles" => batch!(delete_batch_roles),
        "createUser" => request!(create_user),
        "getUser" => get!(get_user),
        "updateUser" => request!(update_user),
        "deleteUser" => {
            serde_json::to_value(service.delete_user(id, None).await?).map_err(|_| invalid())
        }
        "createBatchUsers" => request!(create_batch_users),
        "getBatchUsers" => {
            serde_json::to_value(service.get_batch_users(&ids, None).await?).map_err(|_| invalid())
        }
        "updateBatchUsers" => request!(update_batch_users),
        "deleteBatchUsers" => serde_json::to_value(service.delete_batch_users(&ids, None).await?)
            .map_err(|_| invalid()),
        "createOrganization" => request!(create_organization),
        "getOrganization" => get!(get_organization),
        "updateOrganization" => request!(update_organization),
        "deleteOrganization" => get!(delete_organization),
        "surfaceUserRoles" => get!(surface_roles),
        "surfaceUserPermissions" => get!(surface_permissions),
        _ => Err(invalid()),
    }
}
fn invalid() -> BlueprintError {
    BlueprintError::new("iam_directory_invalid", "Invalid IAM directory input")
}
fn failure(error: &BlueprintError) -> Response {
    let status = match error.code.as_str() {
        "iam_directory_invalid" | "iam_directory_scan_limit" | "iam_directory_filter_limit" => {
            StatusCode::BAD_REQUEST
        }
        "iam_directory_not_found" => StatusCode::NOT_FOUND,
        "iam_directory_conflict" => StatusCode::CONFLICT,
        "iam_directory_forbidden" => StatusCode::FORBIDDEN,
        "iam_directory_organization_required" => StatusCode::UNAUTHORIZED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.message.clone()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use forklaunch_axum::ForkLaunchRouter;
    use forklaunch_core::ServiceInfo;
    use forklaunch_iam_interface::contracts;
    use tower::ServiceExt;

    #[test]
    fn request_mappers_reject_what_the_packaged_handlers_reject() {
        assert!(decode::<Value>(json!({"ids": 5})).is_err());
        assert!(decode::<Value>(json!({"ids": ["a"], "name": "x"})).is_ok());
        assert_eq!(batch_ids(Some("ids=%5B%22a%22%5D")).unwrap(), vec!["a"]);
        for query in [None, Some("ids=a"), Some("ids=%5B%5D&ids=%5B%5D")] {
            assert_eq!(batch_ids(query).unwrap_err().code, "iam_directory_invalid");
        }
        let response = DirectoryFailure(invalid()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn directory_and_session_auth_errors_follow_their_declared_content_types() {
        let directory = module_contracts::registry()
            .routes()
            .iter()
            .find(|route| route.operation_name == "getUser")
            .unwrap()
            .clone();
        let session = contracts::session_registry()
            .routes()
            .iter()
            .find(|route| route.operation_name == "listUsers")
            .unwrap()
            .clone();
        let config = directory_auth_error_formats(RouterConfig::default().with_authenticator(
            |request, _| async move {
                Err(if request.headers.contains_key("authorization") {
                    AuthError::InvalidCredentials
                } else {
                    AuthError::MissingCredentials
                })
            },
        ));
        let app = ForkLaunchRouter::new(ServiceInfo::new("iam-errors", "0.1.0"))
            .get(directory.clone(), || async { StatusCode::NO_CONTENT })
            .get(session.clone(), || async { StatusCode::NO_CONTENT })
            .with_config(config)
            .into_router();
        for (route, uri, text) in [
            (
                &directory,
                "/user/00000000-0000-4000-8000-000000000001",
                true,
            ),
            (&session, "/users", false),
        ] {
            for invalid in [false, true] {
                let mut request = Request::builder().uri(uri);
                if invalid {
                    request = request.header("authorization", "Bearer rejected-test-credential");
                }
                let response = app
                    .clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                let expected_status = if text && invalid { 403 } else { 401 };
                assert_eq!(response.status().as_u16(), expected_status);
                let contract = route
                    .responses
                    .iter()
                    .find(|response| response.status_code == expected_status)
                    .unwrap();
                let declared = contract
                    .body
                    .as_ref()
                    .unwrap()
                    .content_type
                    .as_deref()
                    .unwrap();
                assert_eq!(
                    declared,
                    if text {
                        "text/plain"
                    } else {
                        "application/json"
                    }
                );
                assert_eq!(
                    response.headers()["content-type"],
                    if text {
                        "text/plain; charset=utf-8"
                    } else {
                        "application/json"
                    }
                );
                let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                let message = if invalid {
                    "invalid credentials"
                } else {
                    "missing credentials"
                };
                if text {
                    assert_eq!(bytes.as_ref(), message.as_bytes());
                } else {
                    let error: Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(error["code"], "unauthorized");
                    assert_eq!(error["message"], message);
                    assert!(error["correlationId"].is_string());
                    assert_eq!(error.as_object().unwrap().len(), 3);
                }
            }
        }
    }
}
