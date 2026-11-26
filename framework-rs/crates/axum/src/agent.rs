//! Agent routes under `/api/v1/agent`, bound as contract endpoints so they
//! appear in `OpenAPI`, SDKs and (for the finite routes) the service's MCP tools.
//!
//! Register [`forklaunch_core::agent::contract::routes`] in the service's
//! contract registry, then bind them with [`mount_agent`](crate::agent::mount_agent) and the app's normal
//! authentication policy:
//!
//! ```ignore
//! let router = mount_agent(ContractRouter::new(registry), agent, policy, stream_options)?;
//! // or, with the agent held in application state:
//! let router = mount_agent(router, |state: &AppState| state.agent.clone(), policy, options)?;
//! ```
use crate::native::{
    ContractRouter, EndpointError, EndpointPolicy, PolicyCapabilities, ServerEvent, StreamContext,
    StreamOptions,
};
pub use crate::native::{jwt_policy, jwt_policy_from_env};
use crate::session::AuthenticatedSession;
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, request::Parts};
use forklaunch_core::agent::contract::{tasks_endpoint, tasks_stream_endpoint, tools_endpoint};
use forklaunch_core::agent::{Agent, AgentCaller, AgentError, AgentEvent};
use forklaunch_core::contract::RouteContract;
use futures_util::stream;

/// The id an agent binds approvals to, read from the policy's verified context.
pub trait AgentIdentity {
    /// The verified user id; `None` rejects the request as unauthenticated.
    fn agent_user_id(&self) -> Option<String>;
}
impl AgentIdentity for AuthenticatedSession {
    fn agent_user_id(&self) -> Option<String> {
        Some(self.principal.subject.clone()).filter(|subject| !subject.is_empty())
    }
}

/// The app's policy context plus the agent caller derived from it.
pub struct AgentContext<C> {
    pub inner: C,
    pub caller: AgentCaller,
}

/// Wraps the app's policy: it authenticates and authorizes as usual, then the
/// verified user id and the caller's own `Authorization` header become the
/// agent caller (the header is forwarded to the app's services).
#[derive(Clone)]
pub struct AgentPolicy<P>(pub P);

impl<S, P> EndpointPolicy<S> for AgentPolicy<P>
where
    S: Send + Sync + 'static,
    P: EndpointPolicy<S>,
    P::Context: AgentIdentity,
{
    const CAPABILITIES: PolicyCapabilities = P::CAPABILITIES;
    type Context = AgentContext<P::Context>;

    async fn authorize(
        &self,
        parts: &mut Parts,
        state: &S,
        route: &RouteContract,
    ) -> Result<Self::Context, EndpointError> {
        let inner = self.0.authorize(parts, state, route).await?;
        let user_id = inner.agent_user_id().ok_or(EndpointError::Unauthorized)?;
        let mut values = parts.headers.get_all(AUTHORIZATION).iter();
        let authorization = match (values.next(), values.next()) {
            (Some(value), None) => Some(
                value
                    .to_str()
                    .map_err(|_| EndpointError::Unauthorized)?
                    .to_owned(),
            ),
            (None, _) => None,
            (Some(_), Some(_)) => return Err(EndpointError::Unauthorized),
        };
        Ok(AgentContext {
            inner,
            caller: AgentCaller::new(user_id).with_authorization(authorization),
        })
    }

    async fn authorize_body(
        &self,
        context: Self::Context,
        parts: &mut Parts,
        bytes: &[u8],
        state: &S,
        route: &RouteContract,
    ) -> Result<Self::Context, EndpointError> {
        let inner = self
            .0
            .authorize_body(context.inner, parts, bytes, state, route)
            .await?;
        Ok(AgentContext {
            inner,
            caller: context.caller,
        })
    }
}

fn endpoint_error(error: &AgentError) -> EndpointError {
    tracing::warn!(status = error.status, "agent request failed");
    match error.status {
        400 => EndpointError::InvalidRequest,
        401 => EndpointError::Unauthorized,
        403 => EndpointError::Forbidden,
        429 => EndpointError::TooManyRequests,
        502 => EndpointError::BadGateway,
        _ => EndpointError::Unavailable,
    }
}

/// Where the routes find the agent: a fixed [`Agent`], or a function of the
/// application state (so tests and startup can inject one).
pub trait AgentSource<S>: Clone + Send + Sync + 'static {
    fn agent(&self, state: &S) -> Agent;
}
impl<S> AgentSource<S> for Agent {
    fn agent(&self, _: &S) -> Agent {
        self.clone()
    }
}
impl<S, F> AgentSource<S> for F
where
    F: Fn(&S) -> Agent + Clone + Send + Sync + 'static,
{
    fn agent(&self, state: &S) -> Agent {
        self(state)
    }
}

/// Binds `GET /api/v1/agent/tools`, `POST /api/v1/agent/tasks` and
/// `POST /api/v1/agent/tasks/stream` with the app's `policy`. The stream route
/// answers 503 before streaming when the agent has no model; later failures
/// arrive as `error` events after a 200. `POST /tasks` answers 400 (invalid
/// task, expired or foreign continuation), 401/403 (policy), 429 (model
/// allowance), 502 (model call failed or interrupted) and 503 (not connected
/// to AI or a missing setting), matching the TypeScript runtime.
/// # Errors
/// Rejects registries that do not publish the agent contracts, or policies
/// that cannot admit them.
pub fn mount_agent<S, A, P>(
    router: ContractRouter<S>,
    agent: A,
    policy: P,
    stream_options: StreamOptions,
) -> Result<ContractRouter<S>, String>
where
    S: Clone + Send + Sync + 'static,
    A: AgentSource<S>,
    P: EndpointPolicy<S>,
    P::Context: AgentIdentity,
{
    let policy = AgentPolicy(policy);
    let tools = agent.clone();
    let tasks = agent.clone();
    router
        .bind_authorized(
            tools_endpoint(),
            policy.clone(),
            move |State(state): State<S>, context: AgentContext<P::Context>, ()| {
                let agent = tools.agent(&state);
                async move {
                    agent
                        .list_tools(&context.caller)
                        .await
                        .map_err(|error| endpoint_error(&error))
                }
            },
        )?
        .bind_authorized(
            tasks_endpoint(),
            policy.clone(),
            move |State(state): State<S>, context: AgentContext<P::Context>, request| {
                let agent = tasks.agent(&state);
                async move {
                    agent
                        .run(request, &context.caller)
                        .await
                        .map_err(|error| endpoint_error(&error))
                }
            },
        )?
        .bind_sse_authorized(
            tasks_stream_endpoint(),
            policy,
            stream_options,
            move |State(state): State<S>,
                  context: StreamContext<AgentContext<P::Context>>,
                  request| {
                let agent = agent.agent(&state);
                // Answer 503 up front, like `POST /tasks`, instead of a 200
                // followed by an `error` event.
                let receiver = agent
                    .check_ready()
                    .map(|()| agent.spawn_stream(request, context.initial().caller.clone()));
                async move {
                    let receiver = receiver.map_err(|error| endpoint_error(&error))?;
                    Ok(stream::unfold(receiver, |mut receiver| async move {
                        let event: AgentEvent = receiver.recv().await?;
                        Some((Ok(ServerEvent::new(event)), receiver))
                    }))
                }
            },
        )
}
