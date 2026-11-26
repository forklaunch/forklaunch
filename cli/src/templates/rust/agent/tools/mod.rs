//! The agent's own tools. Name them `snake_case`, describe when to call them,
//! and mark only tools that never change data as read-only: every other tool
//! waits for the user's approval.
pub mod current_time;

use forklaunch_core::agent::Tool;

/// Every local tool, offered to the model unprefixed.
#[must_use]
pub fn all() -> Vec<Tool> {
    vec![current_time::tool()]
}
