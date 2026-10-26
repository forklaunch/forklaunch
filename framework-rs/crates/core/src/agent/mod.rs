//! Agents as ForkLaunch services: MCP-shaped tools from three kinds of source
//! (the agent's own code, the app's services, external MCP servers), explicit
//! user approval for anything not read-only, and an OpenAI-style tool loop over
//! the managed model gateway. Routes and JSON shapes match the TypeScript
//! framework's `@forklaunch/core/agent`.
//!
//! ```ignore
//! use forklaunch_core::agent::{Agent, McpServer, Tool, env, env_bearer, service_tools};
//!
//! let agent = Agent::builder("assistant")
//!     .instructions("Answer questions about records.")
//!     .tool(
//!         Tool::with_input::<TimeInput>("current_time", "Current time", "The current UTC time.")
//!             .read_only()
//!             .typed_handler(|input: TimeInput, _ctx| async move { Ok(json!({"now": now(input)})) }),
//!     )
//!     .service(service_tools("records"))
//!     .mcp_server(
//!         McpServer::new("linear", env("LINEAR_MCP_URL"))
//!             .header("authorization", env_bearer("LINEAR_MCP_TOKEN")),
//!     )
//!     .build()?;
//! ```
pub mod contract;
pub mod types;
pub use types::{
    AgentEvent, AgentTaskRequest, AgentTaskResult, AgentToolListing, ApprovalDecision,
    ApprovalRequired, ListedTool, PendingApproval, ToolAnnotations, ToolCallRecord, ToolSourceKind,
    ToolSourceRef,
};

#[cfg(feature = "agent")]
pub mod continuation;
#[cfg(feature = "agent")]
mod model;
#[cfg(feature = "agent")]
mod run;
#[cfg(feature = "agent")]
mod sources;
#[cfg(feature = "agent")]
mod tool;

#[cfg(feature = "agent")]
pub use model::{GatewayModel, ModelBackend, ModelChunks, ModelError};
#[cfg(feature = "agent")]
pub use run::{Agent, AgentBuilder, AgentError, AgentLimits};
#[cfg(feature = "agent")]
pub use sources::{
    LocalTools, McpServer, SOURCE_REFRESH, ServiceTools, ToolSource, env, env_bearer,
    method_annotations, service_tools, service_url_setting, skipped_path,
};
#[cfg(feature = "agent")]
pub use tool::{
    AgentCaller, BoxFuture, Cancellation, MAX_TOOL_NAME, Tool, ToolContext, ToolError, ToolHandler,
    ToolResult, input_schema, prefixed_tool_name, tool_name_segment, valid_tool_name,
};

#[cfg(all(test, feature = "agent"))]
mod tests;
