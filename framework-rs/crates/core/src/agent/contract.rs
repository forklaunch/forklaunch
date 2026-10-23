//! Published contracts of the agent routes. Register them in the service's
//! contract registry so they appear in `OpenAPI`, SDKs and the service's MCP
//! tools; `forklaunch-axum`'s `agent` feature binds them.
use super::types::{AgentEvent, AgentTaskRequest, AgentTaskResult, AgentToolListing};
use crate::contract::assembly::BindingDeclaration;
use crate::contract::endpoint::{Endpoint, EndpointBody, JsonBody, NoBody, SseBody};
use crate::contract::{AccessLevel, BodyContract, ContractRegistry, HttpMethod, RouteContract};
use schemars::JsonSchema;

pub const TOOLS_PATH: &str = "/api/v1/agent/tools";
pub const TASKS_PATH: &str = "/api/v1/agent/tasks";
pub const TASKS_STREAM_PATH: &str = "/api/v1/agent/tasks/stream";

/// Body of the agent's upstream failures (429 model allowance, 502 model call).
#[derive(JsonSchema)]
#[allow(dead_code)]
struct AgentFailure {
    error: String,
}

fn endpoint<I: EndpointBody, O: EndpointBody>(
    operation: &str,
    summary: &str,
    method: HttpMethod,
    path: &str,
    upstream_failures: bool,
) -> Endpoint<I, O> {
    let mut route = Endpoint::<I, O>::new(operation, summary, method, path, 200)
        .expect("fixed agent endpoint")
        .with_access(AccessLevel::Authenticated)
        .into_contract();
    // The shared contract fixes these operationIds for both runtimes.
    operation.clone_into(&mut route.operation_name);
    if upstream_failures {
        // Upstream failures carry the agent's own body, replacing the framework default.
        for status in [429, 502] {
            let body = Some(BodyContract::json_type::<AgentFailure>());
            if let Some(response) = route.responses.iter_mut().find(|r| r.status_code == status) {
                response.body = body;
            } else {
                route = route.with_response(status, body);
            }
        }
    }
    Endpoint::from_contract(route).expect("fixed agent endpoint contract")
}

/// `GET /api/v1/agent/tools` (`ListAgentTools`).
/// # Panics
/// Never: the descriptor is fixed and covered by tests.
#[must_use]
pub fn tools_endpoint() -> Endpoint<NoBody, JsonBody<AgentToolListing>> {
    endpoint(
        "ListAgentTools",
        "List the tools this agent can call",
        HttpMethod::Get,
        TOOLS_PATH,
        false,
    )
}

/// `POST /api/v1/agent/tasks` (`RunAgentTask`).
/// # Panics
/// Never: the descriptor is fixed and covered by tests.
#[must_use]
pub fn tasks_endpoint() -> Endpoint<JsonBody<AgentTaskRequest>, JsonBody<AgentTaskResult>> {
    endpoint(
        "RunAgentTask",
        "Run a task, or resume one after an approval",
        HttpMethod::Post,
        TASKS_PATH,
        true,
    )
}

/// `POST /api/v1/agent/tasks/stream` (`StreamAgentTask`).
/// # Panics
/// Never: the descriptor is fixed and covered by tests.
#[must_use]
pub fn tasks_stream_endpoint() -> Endpoint<JsonBody<AgentTaskRequest>, SseBody<AgentEvent>> {
    endpoint(
        "StreamAgentTask",
        "Run a task, or resume one, streaming its events",
        HttpMethod::Post,
        TASKS_STREAM_PATH,
        false,
    )
}

/// The three agent route contracts.
#[must_use]
pub fn routes() -> Vec<RouteContract> {
    vec![
        tools_endpoint().into_contract(),
        tasks_endpoint().into_contract(),
        tasks_stream_endpoint().into_contract(),
    ]
}

/// Static binding declarations for the agent routes, for a service's
/// `binding_manifest()`. `source` is the file that mounts them, relative to the
/// service crate (for example `agent/mod.rs`).
/// # Errors
/// Returns the contract serializer's error.
pub fn bindings(source: &str) -> Result<Vec<BindingDeclaration>, String> {
    routes()
        .iter()
        .zip(["list_agent_tools", "run_agent_task", "stream_agent_task"])
        .map(|(route, member)| BindingDeclaration::http(source, "agent", member, route))
        .collect()
}

/// Adds the agent routes to a service registry.
pub fn register(registry: &mut ContractRegistry) {
    for route in routes() {
        registry.register(route);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_carry_the_shared_operation_ids_and_statuses() {
        let routes = routes();
        let ids: Vec<_> = routes.iter().map(|r| r.operation_name.as_str()).collect();
        assert_eq!(ids, ["ListAgentTools", "RunAgentTask", "StreamAgentTask"]);
        let statuses: Vec<u16> = routes[1].responses.iter().map(|r| r.status_code).collect();
        for status in [200, 400, 401, 403, 429, 502, 503] {
            assert!(statuses.contains(&status), "{status}");
        }
        assert!(!routes[2].options.mcp);
        let bindings = bindings("agent/mod.rs").unwrap();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[1].member, "run_agent_task");
    }
}
