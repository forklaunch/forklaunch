//! Typed declarations for schema-selected API versions.
use super::{Endpoint, EndpointBody};
use crate::contract::{RouteContract, VersionedContract};

/// Versions share route/security metadata and common path parameters. Request
/// schemas determine the version; the last matching declaration wins.
#[derive(Clone)]
pub struct VersionedEndpoint {
    route: RouteContract,
}
impl VersionedEndpoint {
    /// # Errors
    /// Rejects empty version names or divergent shared route metadata.
    pub fn new<I: EndpointBody, O: EndpointBody>(
        version: &str,
        endpoint: &Endpoint<I, O>,
    ) -> Result<Self, &'static str> {
        Self {
            route: endpoint.contract(),
        }
        .with_version(version, endpoint)
    }
    /// Add a version in JavaScript declaration order (numeric keys sort first).
    /// # Errors
    /// Rejects duplicate names and divergent route/security/path declarations.
    pub fn with_version<I: EndpointBody, O: EndpointBody>(
        mut self,
        version: &str,
        endpoint: &Endpoint<I, O>,
    ) -> Result<Self, &'static str> {
        if version.is_empty() || self.route.versions.contains_key(version) {
            return Err("version names must be nonempty and unique");
        }
        let route = endpoint.contract();
        let mut shared = route.clone();
        shared.request = self.route.request.clone();
        shared.responses.clone_from(&self.route.responses);
        shared.versions.clone_from(&self.route.versions);
        if shared != self.route || route.request.params != self.route.request.params {
            return Err("versions must share route, security and path parameter declarations");
        }
        self.route = self.route.with_version(VersionedContract {
            version: version.into(),
            request: route.request,
            responses: route.responses,
        });
        Ok(self)
    }
    #[must_use]
    pub fn into_contract(self) -> RouteContract {
        self.route
    }
    #[must_use]
    pub fn contract(&self) -> RouteContract {
        self.route.clone()
    }
}
