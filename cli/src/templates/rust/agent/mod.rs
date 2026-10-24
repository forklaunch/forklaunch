//! The agent: instructions, tool sources and the authenticated `/api/v1/agent`
//! routes. Add local tools in `tools/`, app services with `service_tools`, and
//! external MCP servers with `McpServer`. Credentials are settings, never source.
pub mod tools;

use crate::error::AppError;
use crate::registrations::AppState;
use axum::http::request::Parts;
use forklaunch_axum::agent::{jwt_policy_from_env, mount_agent};
use forklaunch_axum::native::{
    ContractRouter, EndpointError, EndpointPolicy, PolicyCapabilities, RouteContract,
    SessionPolicy, Shutdown, StreamOptions,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_core::agent::{Agent, AgentBuilder{{#has_servers}}, McpServer, env, env_bearer{{/has_servers}}{{#has_services}}, service_tools{{/has_services}}};
use std::error::Error;
use std::time::Duration;

/// System instructions the model reads. Never put settings or secrets here.
pub const INSTRUCTIONS: &str = {{{instructions}}};

/// The agent definition. Tests add a model backend or extra tools to it.
#[must_use]
pub fn builder() -> AgentBuilder {
    tools::all()
        .into_iter()
        .fold(
            Agent::builder("{{name}}").instructions(INSTRUCTIONS),
            AgentBuilder::tool,
        )
{{#services}}
        // Routes of the {{service}} service, called with the user's own token.
        .service(service_tools("{{service}}"))
{{/services}}
{{#servers}}
        // External MCP server; set {{url_setting}} and {{token_setting}}.
        .mcp_server(
            McpServer::new("{{server}}", env("{{url_setting}}"))
                .header("authorization", env_bearer("{{token_setting}}")),
        )
{{/servers}}
}

/// The agent and the JWT policy that guards its routes.
#[derive(Clone)]
pub struct AgentRuntime {
    pub agent: Agent,
    pub auth: SessionPolicy,
}

impl AgentRuntime {
    #[must_use]
    pub fn new(agent: Agent, auth: SessionPolicy) -> Self {
        Self { agent, auth }
    }

    /// The agent with JWT auth from the required `JWKS_PUBLIC_KEY_URL`.
    /// # Errors
    /// Names a missing or invalid JWKS setting and rejects invalid tool definitions.
    pub fn from_env() -> Result<Self, AppError> {
        let agent = builder()
            .build()
            .map_err(|error| AppError::InvalidConfig(error.to_string()))?;
        let auth = jwt_policy_from_env().map_err(|error| AppError::InvalidConfig(error.to_string()))?;
        Ok(Self::new(agent, auth))
    }

    /// Startup check: no tool names collide.
    /// # Errors
    /// Returns the collision.
    pub async fn check(&self) -> Result<(), Box<dyn Error>> {
        self.agent.check().await?;
        Ok(())
    }
}

/// The application's JWT session policy, read from state.
#[derive(Clone)]
pub struct AgentAuth;

impl EndpointPolicy<AppState> for AgentAuth {
    const CAPABILITIES: PolicyCapabilities = PolicyCapabilities::Authorized;
    type Context = AuthenticatedSession;

    async fn authorize(
        &self,
        parts: &mut Parts,
        state: &AppState,
        route: &RouteContract,
    ) -> Result<AuthenticatedSession, EndpointError> {
        state.agent.auth.authorize(parts, state, route).await
    }
}

/// Binds `GET /api/v1/agent/tools`, `POST /api/v1/agent/tasks` and
/// `POST /api/v1/agent/tasks/stream`.
/// # Errors
/// Rejects a registry without the agent contracts.
pub fn mount(router: ContractRouter<AppState>) -> Result<ContractRouter<AppState>, String> {
    // Stream frames carry approval tokens of up to 256 KiB.
    let options = StreamOptions::new(
        Shutdown::new(),
        512 * 1024,
        Duration::from_secs(30),
        Duration::from_secs(10),
    )?;
    mount_agent(router, |state: &AppState| state.agent.agent.clone(), AgentAuth, options)
}
