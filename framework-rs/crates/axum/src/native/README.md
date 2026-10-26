# Native Axum composition

Enable `forklaunch-axum` with `default-features = false, features = ["native"]`.
Applications keep Axum `Router`, typed extractors, `State`, handlers and responses.
ForkLaunch installs reusable transport controls after routes, documentation and
MCP have been composed:

```rust,ignore
use framework::native::{NativeHttpPolicy, secure};

let policy = NativeHttpPolicy::new("https://notes.example.test")?;
let router = secure(router, policy);
```

`secure` returns an ordinary router with these defaults:

- A single exact configured HTTP(S) origin; malformed startup configuration fails.
- Hostile or duplicate Origin headers and cross-site/same-site fetch metadata are
  rejected before application authentication or handlers execute. Native SDKs can
  omit Origin. These checks do not authenticate callers or grant tenant access.
- A 2 MiB request-body limit, including bodies without Content-Length.
- A 30-second deadline to produce response headers. This does not terminate an
  established SSE stream or WebSocket session.
- Fixed JSON transport errors and private/no-store cache headers, nosniff,
  frame denial and no-referrer headers on responses, including rejections.
- Traces limited to method, matched route template, status, timing and validated
  trace context. No bodies, cookies or query strings are recorded by this layer.

Use `with_max_body_bytes` and `with_response_timeout` for explicit positive
limits. Add routes before applying `secure`; Axum layers only apply to routes
already present. The boundary is intended for private APIs, not publicly cached
static files. Ingress owns TLS/HSTS. Application extractors still verify sessions,
tenant membership and resource permissions; stream handlers revalidate ongoing
access. Timeouts cancel local futures and cannot undo remote side effects.

`Shutdown` supplies a latched signal for HTTP shutdown and long-lived work.
`run_until_cancelled` drops in-flight futures when cancellation is triggered;
application transaction and publication idempotency rules remain necessary.

## Export without starting the service

Call `export_contracts_from_env(&registry)?` before configuration, secrets,
database connections or listener startup, and return when it yields true.
It handles `FORKLAUNCH_MODE=contract|openapi` and `--export-contracts`, writing
`FORKLAUNCH_CONTRACT_OUTPUT` and `FORKLAUNCH_OPENAPI_OUTPUT` from one registry.
`write_contract_artifacts` exposes the same operation with explicit paths.
Export errors propagate; the service must not continue startup after a failed
requested export. Authentication is not executed to produce static contracts.

Pure workers or third-party sidecars without this protocol declare
`contractExport = false` in their ForkLaunch project metadata. Absence preserves
the normal exporter behavior; export failures are not silently ignored.

## OTLP runtime integration

When an application starts `TelemetryRuntime`, construct its collector after
runtime startup and wrap the completed router with
`with_telemetry(secure(router, policy), collector)`. The outer observer preserves
the original W3C parent, records status/duration/body-size/request counts and
releases the active-request measurement when a body completes or is cancelled.
It observes frames without buffering SSE bodies. Correlation IDs are bounded
ASCII strings and enter spans/logs, never metric dimensions. Method values and
route templates bound the metric dimensions; raw paths and queries are excluded.
The secure tracing middleware reuses the active server span, so handler-forwarded
`traceparent`, response headers and structured trace IDs agree with OTLP exports.
The normalized correlation header is also visible to handlers. Managed HMAC/JWKS
authentication emits fixed scheme/outcome child spans and counters, including
cancellation. Credentials, claims, subject identifiers and error text are excluded.
This is operational telemetry, not an acknowledged compliance access audit.

Keep the runtime alive through graceful HTTP shutdown, then await its shutdown
to flush exporters. Safe tracing alone does not start OTLP exporters.

## Contract-bound finite bodies

Use `forklaunch_core::contract::endpoint::Endpoint<I, O>` in the host-compilable contract
crate, and register `endpoint().contract()` in its registry. `NoBody`, `JsonBody<T>`,
`TextBody`, `HtmlBody`, `FormBody<T>`, `BinaryBody` and request-only `MultipartBody<T>`
bind the Rust value and wire representation together. Multipart extraction bounds
total bytes, field bytes and field count; duplicate names are rejected. Typed file
bytes and metadata are returned in `MultipartData<T>`.

At runtime, `ContractRouter::new(registry).bind(endpoint, handler)?.finish()?`
requires every published route to have an identical binding. Public handlers take
`State<S>` and the decoded input value and return `Result<O::Value, EndpointError>`;
they do not choose independent response status or content type. JSON request and
response constraints are validated, in addition to Rust typing. Standard framework
rejections are sanitized and declared in the contract.

Private endpoints require `bind_authorized` and an `EndpointPolicy<S>`. The policy
resolves trusted session/tenant context before parsing the body; it receives the
contract access declaration and passes its typed context to the handler. Origin
checks remain provided by `secure`, not a replacement for authentication. Mount
MCP over the resulting router so invocations traverse the same policy.

This binding surface covers finite, single-success responses and declared UUID/string
path fields. Query/header request schemas and typed multi-status response enums still
require adapters; raw Axum registration does not gain these guarantees merely by
being merged into a contract router. `JsonMessage<T>` validates SSE/WebSocket
payloads against the exact published schema and enforces byte limits, while existing
authenticated stream lifecycles retain responsibility for tenant checks, reauthorization,
backpressure, replay/cursors and shutdown. This payload adapter does not encrypt messages. Policy implementations remain responsible for
verifying credentials and enforcing tenant/role scope.

### Access-specific policy admission

`EndpointPolicy::CAPABILITIES` defaults to `PolicyCapabilities::Authenticated`.
Internal endpoints (and any HMAC-auth declaration) require `SignedInternal`;
protected endpoints or RBAC declarations require `Authorized`. Policies implementing
both use `AuthorizedSignedInternal`. Every finite, versioned, SSE and WebSocket
mount checks these requirements before serving. Streaming wrappers retain the
underlying policy capabilities.

The capability is an explicit implementation obligation, not verification of
application policy code. A signed policy must authenticate original bytes and
claim replay protection in `authorize_body`; an authorized policy must enforce
the contract's permissions/roles/scope. Session authentication alone is insufficient.
Static protected operation tags additionally prevent the ordinary public binding
constructor. Runtime declarations remain checked at mount time.

For static `#[access(Internal)]` operations, the policy also implements
`forklaunch_core::contract::bindings::OriginalBodySignedPolicy`. Static
`#[access(Protected)]` operations require `AuthorizationPolicy`. These markers
make wrong-policy binding construction a compiler error, including direct
`AuthorizedBinding::new` and `SseBinding::new` calls. The matching runtime
`CAPABILITIES` declaration is still required: dynamic registrations do not have
static tags. Neither marker implements authentication or replaces runtime checks.

A wrapper policy that delegates to a `SessionPolicy` held in application state
implements the marker itself:

```rust,ignore
#[derive(Clone)]
pub struct UserPolicy;
impl forklaunch_core::contract::bindings::AuthorizationPolicy for UserPolicy {}
impl EndpointPolicy<AppState> for UserPolicy {
    const CAPABILITIES: PolicyCapabilities = PolicyCapabilities::Authorized;
    type Context = AuthenticatedSession;
    async fn authorize(&self, parts: &mut Parts, state: &AppState, route: &RouteContract)
        -> Result<AuthenticatedSession, EndpointError> {
        state.auth.authorize(parts, state, route).await
    }
}
```

Binding with a policy that lacks the marker fails with a message naming the
missing marker and the `CAPABILITIES` value to declare.

### IAM session JWTs (`native-jwt`)

`jwt_policy_from_env()` builds a `SessionPolicy` that verifies bearer JWTs
against `JWKS_PUBLIC_KEY_URL` (asymmetric algorithms only, optional
`JWT_ISSUER`/`JWT_AUDIENCE`). `optional_jwt_policy_from_env()` does the same
but, when `JWKS_PUBLIC_KEY_URL` is unset, rejects every protected request with
401 so a service without an identity provider still starts. Both need only the
`native-jwt` feature; `agent` re-exports them.

### Cross-origin browsers (`CORS_ORIGINS`)

The boundary admits browser requests from `PUBLIC_ORIGIN` only. Add exact
frontend origins with `NativeHttpPolicy::with_cors_origins` or
`NativeApplication::with_cors_origins` (generated services read the
comma-separated `CORS_ORIGINS`). Preflights from a listed origin get `204` with
the requested headers allowed; responses echo the origin with credentials and
`Vary: Origin`. `*` is rejected, and any other origin still gets `403`.

### Reusing managed session authentication

`SessionPolicy::new(authenticator, authorizer)` accepts the same trusted
`session::Authenticator` and `session::Authorizer` used by the existing router.
`SessionPolicy::with_typescript_rules(authenticator, rules)` applies the shared
`core::auth::typescript::TypeScriptRbacPolicy`, preserving permission precedence,
roles, scope, features and subscription semantics. The typed handler receives an
`AuthenticatedSession`; the verified session is also installed for existing typed
session extractors. Invalid or duplicate credentials fail before the handler.

The credential callback must verify signatures or resolve a trusted session;
this adapter never treats decoded client claims as authentication. It receives
original pathname/query and transport headers, decoded path params and query
map (the raw query preserves repeats). Body-dependent signed callbacks use their
own original-body/replay policy: `SessionPolicy` cannot mount internal/HMAC routes
and does not implement `OriginalBodySignedPolicy`. On streams, the existing
managed refresh loop invokes the same credential and authorization hooks again.
