//! Explicit local access revocation. No provider effects or creation authority.
use super::{BillingBackend, bound_billing::configuration, failure, invalid};
use crate::{
    bound_portal_creation::BoundPortalCreationProfile,
    bound_portal_reads::PortalLocalExpirationOutcome, services::StripeBillingService,
};
use axum::{
    Extension, Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use forklaunch_axum::{ForkLaunchRouter, session::AuthenticatedSession};
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_core::contract::AccessLevel;
use forklaunch_core::contract::AuthConfig;
use forklaunch_core::contract::BodyContract;
use forklaunch_core::contract::BodyKind;
use forklaunch_core::contract::FieldContract;
use forklaunch_core::contract::HmacAuth;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::contract::SchemaRef;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;
/// Explicit expiration and historical local-status contracts. Provider URLs may remain usable.
#[must_use]
pub fn bound_portal_expiration_contracts() -> Vec<RouteContract> {
    let mut id = FieldContract::required("publicId", "String");
    id.schema.json_schema = Some(json!({"type":"string","format":"uuid"}).to_string());
    [RouteContract::post("Expire Stored Bound Portal Session","Append local access expiration only; does not invalidate Stripe URL. Repeat or inspect after uncertain commit.","/bound-portal/sessions/:publicId/expire"),RouteContract::get("Inspect Stored Bound Portal Expiration","Historical local expiration only. notRecorded is not rollback proof after an uncertain transaction.","/bound-portal/sessions/:publicId/expiration")].into_iter().map(|route|{
        let is_post=route.operation_name=="expireStoredBoundPortalSession";
        let mut body=BodyContract::json("BoundPortalLocalExpiration");
        body.schema.json_schema=Some(json!({"type":"object","additionalProperties":false,"required":["publicId","state"],"properties":{"publicId":{"type":"string","format":"uuid"},"state":{"type":"string","enum":if is_post {vec!["localExpired"]} else {vec!["localExpired","notRecorded"]}}}}).to_string());
        let mut route=route.with_param(id.clone()).with_access(AccessLevel::Internal).with_auth(AuthConfig::Hmac(HmacAuth::new().with_header_name("authorization"))).with_response(200,Some(body));
        if is_post { let mut unknown=BodyContract::json("BoundPortalLocalExpirationUnknown");unknown.schema.json_schema=Some(json!({"type":"object","additionalProperties":false,"required":["publicId","state"],"properties":{"publicId":{"type":"string","format":"uuid"},"state":{"type":"string","enum":["localExpirationUnknown"]}}}).to_string());route=route.with_response(202,Some(unknown)); }
        for status in [400,409,500,503] {route=route.with_response(status,Some(BodyContract{kind:BodyKind::Text,schema:SchemaRef::named("String"),content_type:Some("text/plain".into())}));} route
    }).collect()
}
impl BillingBackend {
    pub(super) async fn with_bound_portal_expiration(
        mut self,
        profile: BoundPortalCreationProfile,
    ) -> BlueprintResult<Self> {
        if self.backup || self.bound_portal_expiration.is_some() {
            return Err(configuration());
        }
        let service = StripeBillingService::new(
            BaseBillingService::new(Arc::new(self.store.clone())),
            self.client.as_ref().ok_or_else(configuration)?.clone(),
        );
        service
            .validate_portal_local_expiration_configuration()
            .await?;
        self.bound_portal_expiration = Some(Arc::new(profile));
        Ok(self)
    }
}
async fn execute(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    id: &str,
    expire: bool,
) -> BlueprintResult<(StatusCode, Value)> {
    if backend.backup {
        return Err(configuration());
    }
    let profile = backend
        .bound_portal_expiration
        .as_ref()
        .ok_or_else(configuration)?;
    let id = Uuid::parse_str(id)
        .ok()
        .filter(|id| !id.is_nil())
        .ok_or_else(invalid)?;
    let store = backend.store.with_audit_context(AuditContext::actor(
        &session.principal.subject,
        Uuid::new_v4().to_string(),
    ))?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend.client.as_ref().ok_or_else(configuration)?.clone(),
    );
    let (status, state) = if expire {
        match service
            .expire_stored_bound_portal_session(profile, id)
            .await?
        {
            PortalLocalExpirationOutcome::Expired => (StatusCode::OK, "localExpired"),
            PortalLocalExpirationOutcome::CommitUnknown => {
                (StatusCode::ACCEPTED, "localExpirationUnknown")
            }
        }
    } else {
        (
            StatusCode::OK,
            if service
                .inspect_stored_bound_portal_expiration(profile, id)
                .await?
            {
                "localExpired"
            } else {
                "notRecorded"
            },
        )
    };
    Ok((status, json!({"publicId":id.to_string(),"state":state})))
}
async fn dispatch(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    id: &str,
    expire: bool,
) -> Response {
    match execute(backend, session, id, expire).await {
        Ok((status, body)) => (status, Json(body)).into_response(),
        Err(error) => match error.code.as_str() {
            "billing_provider_binding_conflict" | "billing_portal_local_session_expired" => (
                StatusCode::CONFLICT,
                "Stored Portal evidence is unavailable",
            )
                .into_response(),
            _ => failure(&error),
        },
    }
}
pub(super) fn register(
    mut router: ForkLaunchRouter,
    backend: &Arc<BillingBackend>,
) -> ForkLaunchRouter {
    if backend.bound_portal_expiration.is_none() {
        return router;
    }
    for contract in bound_portal_expiration_contracts() {
        let backend = Arc::clone(backend);
        let expire = contract.operation_name == "expireStoredBoundPortalSession";
        if expire {
            router = router.post(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(backend, session, &id, true).await }
                },
            );
        } else {
            router = router.get(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(backend, session, &id, false).await }
                },
            );
        }
    }
    router
}
