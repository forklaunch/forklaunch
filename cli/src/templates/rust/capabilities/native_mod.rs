// BEGIN FORKLAUNCH MANAGED CAPABILITIES
pub mod nonce_store;
__EVENT_MODULES__


use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::platform_events::PlatformEventVerifier;
use axum::{extract::{FromRequestParts, Path}, http::request::Parts};
use forklaunch_axum::native::{ContractRouter, EndpointError, EndpointPolicy, PolicyCapabilities};
use forklaunch_core::http::platform_events::VerifiedPlatformEvent;
use forklaunch_core::contract::RouteContract;
use crate::registrations::AppState;
use forklaunch_core::config;
use std::sync::{Arc, OnceLock};
__CLIENT_IMPORTS__
fn instance_gateway() -> BlueprintResult<InstanceGatewayClient> {
    // Reuse the connection pool after the first successful resolution. A
    // configuration error is not cached, so it can be corrected by the caller.
    static GATEWAY: OnceLock<InstanceGatewayClient> = OnceLock::new();
    if let Some(gateway) = GATEWAY.get() { return Ok(gateway.clone()); }
    let gateway = InstanceGatewayClient::from_env()?;
    Ok(GATEWAY.get_or_init(|| gateway).clone())
}

__CLIENT_FACTORIES__

pub use contracts::platform_events::{contracts, register_contracts};
#[derive(Clone)]
struct CallbackPolicy(Arc<Option<PlatformEventVerifier>>);
struct CallbackContext { feature: String, event: Option<VerifiedPlatformEvent> }
impl EndpointPolicy<AppState> for CallbackPolicy {
    const CAPABILITIES: PolicyCapabilities = PolicyCapabilities::SignedInternal;
    type Context = CallbackContext;
    async fn authorize(&self, parts: &mut Parts, state: &AppState, _: &RouteContract) -> Result<CallbackContext, EndpointError> {
        let Path(feature) = Path::<String>::from_request_parts(parts, state).await.map_err(|_| EndpointError::InvalidRequest)?;
        if !matches!(feature.as_str(), __KNOWN_FEATURES__) { return Err(EndpointError::NotFound); }
        if self.0.is_none() || !parts.headers.contains_key("authorization") { return Err(EndpointError::Unauthorized); }
        Ok(CallbackContext { feature, event: None })
    }
    async fn authorize_body(&self, mut context: CallbackContext, parts: &mut Parts, bytes: &[u8], _: &AppState, _: &RouteContract) -> Result<CallbackContext, EndpointError> {
        if bytes.len() > 1024*1024 { return Err(EndpointError::PayloadTooLarge); }
        let verifier = self.0.as_ref().as_ref().ok_or(EndpointError::Unauthorized)?;
        let authorization = parts.headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();
        context.event = Some(verifier.verify("POST", parts.uri.path(), authorization, bytes).await.map_err(|error|
            if matches!(error.code.as_str(), "platform_event_nonce_capacity" | "platform_event_nonce_store") { EndpointError::Unavailable } else { EndpointError::Unauthorized })?);
        Ok(context)
    }
}
pub trait ManagedCapabilitiesRouterExt { fn with_managed_capabilities(self) -> Result<Self, String> where Self: Sized; }
impl ManagedCapabilitiesRouterExt for ContractRouter<AppState> {
    fn with_managed_capabilities(self) -> Result<Self, String> {
        // Platform callbacks are rejected until the managed INSTANCE_HMAC_KEY is set.
        let key: Option<String> = config::optional("INSTANCE_HMAC_KEY").map_err(|error| error.to_string())?;
        let verifier = Arc::new(key.map(|key| PlatformEventVerifier::new(key, nonce_store::store()))
            .transpose().map_err(|error| error.to_string())?);
        self.bind_authorized(contracts::platform_events::endpoint(), CallbackPolicy(verifier), |_, context, _body| async move {
            let event = context.event.ok_or(EndpointError::Unauthorized)?;
            let result: BlueprintResult<()> = match context.feature.as_str() {
__HANDLER_ARMS__
                _ => return Err(EndpointError::NotFound),
            };
            result.map(|()| "ok".to_owned()).map_err(|_| EndpointError::InvalidResponse)
        })
    }
}
// END FORKLAUNCH MANAGED CAPABILITIES
