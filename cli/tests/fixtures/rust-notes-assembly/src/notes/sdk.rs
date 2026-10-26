//! Service-owned SDK contracts, compiled independently for host build scripts.
#[path = "api/contracts/mod.rs"]
mod contracts;
#[path = "domain/schemas/mod.rs"]
pub mod schemas;
pub use contracts::*;
pub use schemas::*;

#[path = "domain/events/mod.rs"]
pub mod events;
