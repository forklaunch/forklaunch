//! Portable raw socket contracts consumed by offline SDK and `AsyncAPI` generation.
use crate::ws::SocketContract;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versioned service metadata; socket keys become generated module names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocketContractArtifact {
    pub format_version: u32,
    pub title: String,
    pub version: String,
    pub sockets: BTreeMap<String, SocketContract>,
}
impl SocketContractArtifact {
    #[must_use]
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            format_version: 1,
            title: title.into(),
            version: version.into(),
            sockets: BTreeMap::new(),
        }
    }
    #[must_use]
    pub fn with_socket(mut self, name: impl Into<String>, socket: SocketContract) -> Self {
        self.sockets.insert(name.into(), socket);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_socket_contracts_preserve_all_metadata() {
        let artifact = SocketContractArtifact::new("Realtime", "1.0.0")
            .with_socket("chat", SocketContract::new("/chat"));
        let value = serde_json::to_value(&artifact).unwrap();
        assert_eq!(value["formatVersion"], 1);
        assert_eq!(
            serde_json::from_value::<SocketContractArtifact>(value).unwrap(),
            artifact
        );
        assert!(
            serde_json::from_str::<SocketContractArtifact>(
                r#"{"formatVersion":1,"title":"x","version":"1","sockets":{},"unknown":true}"#
            )
            .is_err()
        );
    }
}
