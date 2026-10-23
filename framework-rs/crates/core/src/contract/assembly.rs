//! Compiler-produced declarations for bounded source assembly verification.
//! These records describe requirements; they never attest that a router ran.
use super::RouteContract;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindingDeclaration {
    pub source: String,
    pub group: String,
    pub member: String,
    pub transport: String,
    pub contract: serde_json::Value,
}
impl BindingDeclaration {
    /// Capture the exact compiled HTTP/SSE declaration, including policy metadata.
    /// # Errors
    /// Returns an error if the framework contract serializer produces invalid JSON.
    pub fn http(
        source: &str,
        group: &str,
        member: &str,
        route: &RouteContract,
    ) -> Result<Self, String> {
        Ok(Self {
            source: source.into(),
            group: group.into(),
            member: member.into(),
            transport: "http".into(),
            contract: serde_json::from_str(&route.to_json()).map_err(|error| error.to_string())?,
        })
    }
    /// Capture the exact compiled socket declaration. Runtime admission is separate.
    /// # Errors
    /// Returns the descriptor serialization error.
    pub fn socket<T: Serialize>(
        source: &str,
        group: &str,
        member: &str,
        contract: &T,
    ) -> Result<Self, String> {
        Ok(Self {
            source: source.into(),
            group: group.into(),
            member: member.into(),
            transport: "websocket".into(),
            contract: serde_json::to_value(contract).map_err(|error| error.to_string())?,
        })
    }
}
