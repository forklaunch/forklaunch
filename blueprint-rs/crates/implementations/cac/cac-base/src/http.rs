//! Contract-bound HTTP support for the current CAC skeleton. No clinical
//! entities are registered upstream yet: authorized compliance requests
//! therefore return 404. Generated services own their controllers, which call
//! [`CacHttp::provider`] and mount through [`CacRoutes`]; the packaged
//! [`CacHttp::handler`] remains for services generated before 0.2.5.
use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{MethodRouter, any},
};
use forklaunch_axum::{ForkLaunchRouter, RouterConfig, module_auth::HmacJwksAuth};
use forklaunch_cac_interface::{CodeSetProvider, ProcedureCodeLookupDto, contracts};
use forklaunch_core::ServiceInfo;
use std::{collections::HashMap, sync::Arc};

/// One operation's request handler. Its contract supplies the method and path.
pub type Handler = MethodRouter;

/// Provider, compliance and health handlers sharing request HMAC and JWKS
/// authentication.
pub struct CacHttp<P> {
    provider: Arc<P>,
    auth: Arc<HmacJwksAuth>,
    service: ServiceInfo,
}

impl<P> Clone for CacHttp<P> {
    fn clone(&self) -> Self {
        Self {
            provider: Arc::clone(&self.provider),
            auth: Arc::clone(&self.auth),
            service: self.service.clone(),
        }
    }
}

impl<P: CodeSetProvider + 'static> CacHttp<P> {
    #[must_use]
    pub fn new(provider: Arc<P>, auth: HmacJwksAuth, service: ServiceInfo) -> Self {
        Self {
            provider,
            auth: Arc::new(auth),
            service,
        }
    }

    /// The code-set provider that generated controllers call.
    #[must_use]
    pub fn provider(&self) -> &Arc<P> {
        &self.provider
    }

    /// The configured service identity, as reported by the health route.
    #[must_use]
    pub fn service(&self) -> &ServiceInfo {
        &self.service
    }

    /// The packaged handler for one CAC operation.
    ///
    /// Deprecated since 0.2.5: generated services now own their controllers.
    /// It stays available, unchanged, for services generated earlier.
    ///
    /// # Panics
    /// Panics if `operation` is not a CAC operation.
    pub fn handler(&self, operation: &str) -> Handler {
        let provider = Arc::clone(&self.provider);
        match operation {
            "describeCodeSet" => any(move || {
                let provider = Arc::clone(&provider);
                async move { Json(provider.describe()) }
            }),
            "lookupProcedureCode" => any(move |Path(code): Path<String>| {
                let provider = Arc::clone(&provider);
                async move { lookup(&provider, code).await }
            }),
            "exportUserData" => any(|| async {
                (
                    StatusCode::NOT_FOUND,
                    "User not found or no PII data to export",
                )
            }),
            "eraseUserData" => any(|| async {
                (
                    StatusCode::NOT_FOUND,
                    "User not found or no PII data to erase",
                )
            }),
            "getHealth" => {
                let service = self.service.name.clone();
                any(move || {
                    let service = service.clone();
                    async move { Json(serde_json::json!({"service":service,"status":"ok"})) }
                })
            }
            _ => panic!("CAC operation {operation} has no contract"),
        }
    }

    /// Collects handlers for the service's CAC routes.
    #[must_use]
    pub fn routes(&self) -> CacRoutes<P> {
        CacRoutes {
            http: self.clone(),
            handlers: HashMap::new(),
        }
    }
}

/// The service's CAC routes. Each operation's contract comes from the CAC
/// interface, so routes, SDKs, `OpenAPI` and MCP tools share one declaration.
pub struct CacRoutes<P> {
    http: CacHttp<P>,
    handlers: HashMap<String, Handler>,
}

impl<P: CodeSetProvider + 'static> CacRoutes<P> {
    /// Serve `operation` with `handler`. Operations without a handler are not mounted.
    ///
    /// # Panics
    /// Panics if `operation` is not a CAC operation or is already routed.
    #[must_use]
    pub fn route(mut self, operation: &str, handler: Handler) -> Self {
        assert!(
            contracts::module_registry()
                .routes()
                .iter()
                .any(|route| route.operation_name == operation),
            "CAC operation {operation} has no contract"
        );
        assert!(
            self.handlers
                .insert(operation.to_owned(), handler)
                .is_none(),
            "CAC operation {operation} is already routed"
        );
        self
    }

    /// Mount the routed operations in contract order with request HMAC and
    /// JWKS authentication.
    #[must_use]
    pub fn finish(mut self) -> ForkLaunchRouter {
        let auth = Arc::clone(&self.http.auth);
        let config = RouterConfig::default().with_authenticator(move |request, contract| {
            let auth = Arc::clone(&auth);
            async move { auth.authenticate(request, contract).await }
        });
        let mut router = ForkLaunchRouter::new(self.http.service.clone());
        for route in contracts::module_registry().routes() {
            if let Some(handler) = self.handlers.remove(&route.operation_name) {
                router = router.route(route.clone(), handler);
            }
        }
        router.with_config(config)
    }
}

async fn lookup<P: CodeSetProvider>(provider: &Arc<P>, code: String) -> Response {
    match provider
        .lookup_procedure_code(ProcedureCodeLookupDto { code: code.clone() })
        .await
    {
        Ok(Some(result)) => Json(result).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            format!("Procedure code '{code}' not found"),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Code-set provider unavailable",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests;
