#![forbid(unsafe_code)]
//! Native WebSocket transport sessions over core-owned contracts and codecs.
pub use forklaunch_core::ws::{
    AsyncApiEnrichment, EventContract, MessageDirection, SocketContract, SocketContractArtifact,
    SocketSchemaEntry, WebSocketContract, WebSocketEnvelope, WsMessageKind, WsValidationError,
    codec, contract,
};
#[cfg(feature = "typed-schema")]
pub use forklaunch_core::ws::{TypedSocket, bindings, socket_api};
#[cfg(feature = "transport")]
pub mod transport;
