use forklaunch_core::http::platform_events::{MemoryPlatformEventNonces, PlatformEventNonceStore};
use std::sync::Arc;

/// One store is shared by every managed callback in this service process.
/// For replicas, return a shared implementation with an atomic nonce claim.
/// CLI capability additions and sync preserve edits to this factory.
pub fn store() -> Arc<dyn PlatformEventNonceStore> {
    Arc::new(MemoryPlatformEventNonces::default())
}
