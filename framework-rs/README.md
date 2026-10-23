# framework-rs

See the [workspace guide](GUIDE.md) for the complete crate/feature catalog, development and operations workflows, API documentation commands, and compatibility boundaries.

Rust-native ForkLaunch framework crates.

This workspace implements contract registration, validation primitives, an Axum HTTP adapter, OpenAPI and AsyncAPI generation, MCP tool generation and an optional Streamable HTTP server, SDK code generation, and infrastructure interfaces. Optional runtime integrations are feature-gated: PostgreSQL tenant transactions, TypeScript-compatible field encryption, Redis storage, and remote JWKS verification. Axum supports verified sessions, RBAC, and OpenTelemetry request instruments. Production integration and cross-language parity work remains tracked in [the implementation plan](../plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md).

The CLI can generate runnable Rust backends with bundled framework source. See [Rust backend generation](../docs/adding-projects/rust.md) for setup, contract exports, SDKs, and current boundaries.

New native services and worker producer services separate reusable composition
in `application.rs` from process configuration, resource registration and listener
startup in `bootstrapper.rs`. `error.rs` holds service/domain errors; controllers
translate them into public endpoint errors. The existing
`forklaunch_axum::native::run_application` handles offline contract export before
startup and telemetry flushing afterward. PostgreSQL services use
`forklaunch_axum::native::with_cleanup` to close their repository after serving
returns either success or an error. Asynchronous cleanup is not guaranteed if the
entire bootstrap future is cancelled or panics. Independent worker consumers keep
their worker runtime lifecycle.

Controller build hosts record source provenance relative to the owning service,
so same-named generated applications can share Cargo caches across locations.
Generated build scripts resolve output directories from Cargo's execution-time
package environment, keeping compliance and SDK assets inside that application.
See the [startup validation receipt](../plan/RUST-NATIVE-BOOTSTRAP-RECEIPTS.json)
and [crate release plan](../plan/RUST-CRATE-RELEASE-PLAN.html) for current checks
and remaining work before registry-only generated applications are available.

Scalar docs accept additional documents through `RouterConfig.docs_sources`.
Use `ScalarSource::url("External API", "https://example.com/openapi.json")` or
`ScalarSource::inline("Authentication", schema)`; optional `slug` and `default`
fields control source navigation. The generated application API remains first.

Application-owned HTTP APIs can use `nest_internal(prefix, router)` or
`merge_internal(router, namespaces)` for a root mount. Declare every raw namespace
when merging. Contracts, documentation and MCP endpoints cannot claim reserved
namespaces; parent router paths and shared middleware still apply. Internal
routes remain outside generated application contracts and SDK/MCP operations.

Persistence adapters can derive `forklaunch_core::persistence::compliance::ComplianceMetadata`
on named structs. Field attributes such as `#[compliance(level = "pii", nullable,
name = "email_address")]` declare classification, storage nullability and storage
name. Supported levels are `none`, `pii`, `phi`, and `pci`; all fields
must declare a level explicitly. Only explicitly classified nullable fields
qualify for anonymization. Metadata does not turn on encryption or perform
export, erasure, or retention writes: adapters must verify their storage schema
and enforce tenant boundaries. Rust `Option`, serde renames and encryption
configuration do not implicitly change these declarations. The derive supports
generics and `#[compliance(crate_path = "renamed_core")]` for renamed dependencies.

The `persistence::retention` module provides bounded async enforcement hooks for
application-owned stores. Build a `RetentionPolicy::for_entity::<T>` from explicit
compliance metadata and an already resolved exclusive cutoff. `enforce` requires
validated tenant identities, supports dry-run counts and entity selection, and
reports failed/incomplete passes without losing earlier committed counts. Stores
must implement atomic tenant-scoped batches, oldest-first selection and storage
metadata validation; anonymization clears only classified nullable fields and
records its timestamp even when no field qualifies. Batch sizes and pass lengths
are bounded. Duplicate scopes, future cutoffs and counter overflow fail explicitly.
Calendar-duration parsing and timezone selection remain application responsibilities.
The opt-in `retention::postgres` adapter provides bounded PostgreSQL enforcement
for explicit service-owned column mappings. The [in-memory conformance adapter](crates/core/examples/retention_hooks.rs)
illustrates the hook boundary; 24 cases compare it with the actual TypeScript
retention service. This does not claim generic SQL/ORM retention parity.

See [protected persistence and subject lifecycle](crates/core/PERSISTENCE.md) for
audited fields, explicit partial disclosures, durable exports with physical
cleanup, SQL erasure and transactional external-erasure reconciliation.

## JWT authentication

`forklaunch_core::auth::issue_jwt_hs256(&claims, secret)` issues typed `JwtClaims`
through `jsonwebtoken`. Supply a server-owned subject, expiry in UNIX seconds,
roles, permissions, scopes, features and subscription state. The function rejects
empty subjects/keys; applications own expiry policy, secret generation, rotation
and key distribution. `verify_jwt_hs256` checks the signature, strict expiry and optional not-before
time before returning a principal. It rejects empty keys and malformed optional
`nbf`/`iat` claims; it does not add implicit clock tolerance. For asymmetric keys and issuer/audience/algorithm policy,
use `jwt::verify_jwk`, `verify_jwks`, or the optional remote-JWKS adapter with an
explicit `Validation`. Token-provided key URLs never select trusted keys.

The [canonical IAM example](../blueprint-rs/crates/implementations/iam/iam-base/examples/canonical_iam.rs)
shows issuance and router authentication with a test-only key. The actual
TypeScript/JOSE fixture verifies Rust-issued claims and both servers accept the
token; JOSE-issued tokens also authenticate against Rust. Declared JWT routes
return 401 for missing credentials and 403 for invalid tokens. Opaque bearer
session rejection remains 401. Native HTTP error bodies retain their structured
JSON envelope; complete TypeScript error-envelope parity is still tracked.
Better Auth JWT formats and provider/cookie/OIDC flows use their own adapter.

Enable the `forklaunch-core` feature `oidc` for the reusable
`oidc::OidcProvider` authorization-code adapter built on `oauth2` 5 and
`openidconnect` 4. Configure a trusted issuer, registered client credentials and
callback URL with `OidcProvider::discover`. Discovery checks the issuer and
fetches provider keys using HTTPS (loopback HTTP is available for fixtures),
without redirects, with a 15-second timeout and a 1 MiB response limit.
The adapter supports RS256 ID tokens, S256 PKCE, public clients and
`client_secret_basic` authentication. Advertised incompatible flows/auth methods
are rejected; authorization-code and refresh grants use RS256 ID tokens.

`provider.begin(&scopes)` returns `PendingAuthorization`. Redirect the browser to
`pending.authorization_url()` and keep the non-cloneable pending value in the
initiating browser's server-side session. On callback, retrieve and consume that
browser-bound value, then call `provider.exchange(pending, returned_state, code)`.
Do not use the callback state alone to select another browser's pending login.
State expires after ten minutes and is bound to issuer, client and callback.
Successful exchange returns verified standard claims and typed secret tokens;
applications own encrypted token persistence and account/session creation.
Verification checks signature, issuer, audience, expiry, nonce, a present
`azp`, nonempty subject, future issue time (60-second tolerance), and `at_hash`
when present. Errors and Debug output omit credentials, tokens and provider
response details. Each login or refresh response containing an ID token fetches
fresh verification keys from the discovery-pinned JWKS URL before validation.
This adds one bounded public-key GET per ID-token response and never retries a
code exchange or refresh grant. Key requests carry no client/token credentials;
invalid, unavailable or revoked keys fail closed without a discovery-key fallback.
Refresh responses without ID tokens do not fetch keys. Rediscover explicitly when
provider endpoints/metadata change. This generic
adapter is separate from the configured Better Auth Google/account flow.


`provider.refresh(&refresh_token, &initial_claims)` accepts a server-owned refresh
token and the original verified login claims from trusted encrypted storage.
Keep those original claims across rotations. Refresh checks unchanged identity,
audience, optional nonce and authentication time; an omitted ID token yields
`claims: None`. A missing replacement refresh token retains the supplied token.
Applications own atomic rotation persistence and concurrent refresh coordination.
Never accept the original claims from a request.

`forklaunch_core::auth::typescript::TypeScriptRbacPolicy` is an explicit compatibility
policy for verified, server-enriched JWT/Basic sessions. It preserves omitted
versus empty rule sets, permission-before-role precedence, access-level bypass
of those checks, exact scope membership, and subscription-before-feature checks.
The `scopeHeirarchy` wire spelling matches TypeScript; `scopeHierarchy` is also
accepted when deserializing native configuration. A protected policy with no
configured permission/role checks returns 401. Its errors retain the upstream
status and public message; the default native HTTP response is a JSON envelope.
Applications resolve grants before invoking this policy. HMAC routes have no
session and bypass the session policy in TypeScript.

For request-scoped grant lookups, `TypeScriptRbacPolicy::authorize_session` accepts
a `typescript::session::TypeScriptSessionResolver`. It checks application-owned
session validation, then awaits scopes, permissions or roles, subscription and
features in middleware order. Earlier rejection prevents later lookups; missing
required grant callbacks return the matching 500. Missing scope callbacks skip
scope checks in this compatibility profile. Validation checks original claims
without replacing them with defaults or transformed output. Implementations can
hold trusted tenant/request context and must return only public callback errors.
Call this after credential verification and before returning an authenticated
session; ensure the subsequent router authorizer uses the same complete policy.
The helper does not issue credentials, create principals or implement a schema
validator. Callback cancellation does not retry work or begin subsequent lookups.

`forklaunch_validator::ParseError` retains the TypeScript validator's ordered
string path segments and public message. `pretty_print_parse_errors` matches
`prettyPrintParseErrors`, including absent/empty lists, empty versus absent
prefixes, root paths and multiline numbered messages. A session resolver can
format `Invalid session: {pretty_print_parse_errors(Some(&errors), Some("Session"))}`
in a validation response. Only supply public messages. This helper formats
validator output; it does not equate native/Garde schema rules with Zod/TypeBox
or change the default JSON error envelope. Use `AuthErrorFormat::TypeScriptText`
when the application also needs TypeScript's auth wire representation.

`forklaunch_core::auth::typescript::basic::verify_basic` explicitly enables the
TypeScript synchronous Basic-login parsing profile. It matches case-sensitive
prefixes, first-space token selection, Node base64 decoding, replacement UTF-8
and the first two colon-delimited fields. Thus `user:pass:extra` checks password
`pass`; the strict native `parse_basic` retains `pass:extra`. Compatibility is
opt-in and does not change that default. A trusted synchronous callback must
verify the parsed username/password before the function returns a principal.
Missing/malformed credentials return 401 and denied login returns 403, with the
TypeScript public messages. `basic::verify_basic_resource` instead awaits a
trusted async decoder with the unchanged first token, including an empty token.
It bypasses base64/login parsing, rejects null claims and preserves other JSON
claims. The decoder must actually verify credentials; applications validate the
session and bind its principal before returning an `AuthenticatedSession`.
Explicit decoder errors must contain public text. Cancellation drops the decoder
future without retrying it. Session-schema validation remains application-owned.

`typescript::jwt::verify_jwt_resource` awaits a trusted JWT verifier or custom
resource decoder with the same prefix/first-token rules (default `Bearer`). It
rejects all falsy JSON results and converts callback failures to the TypeScript
403 without exposing their details. It does not verify signatures itself: supply
a real credential verifier and validate/bind the returned session before use.

`typescript::hmac::verify_hmac` provides opt-in TypeScript header/error ordering
and trusted async verifier dispatch. It preserves the first four components and
embedded equals, ignores extra components, and selects a configured key. The
callback receives raw timestamp text (not a JavaScript Date), optional secret,
nonce and signature. It must reject unknown keys and verify the request method,
path and raw body; freshness and atomic replay protection remain explicit policy.
The existing strict `request_hmac::HmacAuthorization` can verify the normal
RFC3339 signing profile. Legacy JavaScript date parsing is not implemented by
this adapter. Successful HMAC verification returns no session or principal.

Use `RouterConfig::with_response_authenticator` when credential verification
needs explicit `ErrorResponse` statuses/messages, including this Basic verifier.
It returns an `AuthenticatedSession` only after verification; normal RBAC or a
configured authorizer still runs afterward. It replaces `with_authenticator`
and vice versa; cookie routes retain their dedicated session verifier. Callback
errors must contain public text only. Native responses default to structured JSON,
including when forwarded through MCP. Set `RouterConfig::auth_error_format` to
`AuthErrorFormat::TypeScriptText` to send auth failures as the public message with
`text/plain; charset=utf-8`, matching the TypeScript middleware. Combine it with
the TypeScript credential/session policy for matching status and message rules.
The option applies to both credential and authorization failures; request parsing,
validation and handler errors retain their own behavior. The served OpenAPI document
also supplies missing 401/403 plain-text schemas on routes with declared auth. Explicit
response contracts take precedence (the TypeScript generator instead overwrites
explicit auth-error declarations). Standalone exporters can select the same projection
with `OpenApiOptions::with_typescript_auth_errors`; native exports remain unchanged.
MCP forwards the same HTTP rejection text; successful responses are unaffected.
Actual middleware comparisons and live
HTTP/MCP tests cover parsing, rejection, verified subjects and subsequent RBAC.

Set `RouterConfig::handler_error_format` to `HandlerErrorFormat::TypeScriptText`
for `LocalRouter` handlers that return `Err`: the HTTP adapter emits the same
500 plain-text exception message and request correlation ID as the TypeScript
Express error handler. Internal error codes and details stay out of the response.
The default retains the native JSON envelope. This option does not catch panics,
rewrite ordinary Axum handler responses, or change request validation errors.

When an application validator supplies public `ParseError` diagnostics,
`forklaunch_core::http::typescript_request_validation_error_response` emits the
TypeScript request validation 400 response, formatted paths/messages and
correlation suffix as `text/plain; charset=utf-8`. Although upstream request
middleware initially selects JSON media, its enriched send applies the default
400 string contract and produces plain text. Explicit response media contracts
can override that default; apply the corresponding header override when needed.
This helper preserves the native validation envelope. Callers own their schema
messages; it does not make native JSON Schema diagnostics equivalent to Zod or
TypeBox.

For finite, unversioned default-error responses, register selected missing
statuses with `config.default_error_responses.insert(&route, &[404, 500],
65536, Duration::from_secs(1))?`. The supported selections are 400, 401, 403,
404 and 500. Registration rejects explicit response conflicts, duplicates,
versioned routes and zero bounds. In response-validation Error mode, the
profile accepts plain text, JSON-encoded strings and absent bodies; JSON
strings become plain text. Other values receive redacted text diagnostics.
Axum enforces byte/time bounds and normalizes selected statuses at the
validation middleware boundary, including early failures and response callbacks.
Configure the corresponding parser/auth error profiles as required; incompatible
known combinations are rejected before export or serving. Direct `LocalRouter`
execution uses `execute_with_response_profiles`; handler `Err` remains its
separate result channel.

The effective registry includes only active, selected declarations, so SDK and
OpenAPI exports share the runtime profile. Resetting the configuration removes
its contributions. `ForkLaunchRouter::registry()` now validates completed profile
registrations and is no longer a `const fn`; callers using it from a const
function must move that export to runtime. Selected zero-byte wire bodies are
admitted as absent bodies: Axum cannot recover whether they originated as an
empty byte buffer or an absent value. That boundary differs from TypeScript's
original-value validation. Versioned and streaming default-error responses
remain outside this finite profile.

Set `RouterConfig.router_origin_format` to `RouterOriginFormat::TypeScriptHtml`
for Express-compatible framework fallbacks: unknown paths and unsupported methods
return escaped HTML 404 responses, with security headers and HEAD representation
length. Pair it with `ParserErrorFormat::TypeScriptText` for malformed matched
parameters to return the TypeScript 400 text response before method dispatch.
Native defaults, application handler responses and internal-router fallbacks are
preserved. A root internal mount retains ownership of its root fallback. CORS,
explicit OPTIONS routes and correlation propagation remain separate policies.
Registered methods also produce automatic OPTIONS responses (including implicit
HEAD for GET), unless an explicit handler owns that method.

Set `RouterConfig.correlation_id_format` to `CorrelationIdFormat::TypeScript`
to select supplied correlation values with Node-compatible repeated-header and
Latin-1 handling, or generate a UUIDv4 when absent/empty. Request contexts,
framework errors and telemetry share that selection. The response propagates it
unless an application explicitly sets its own correlation header. Native
`x-request-id` remains independent, and generated values do not satisfy a
required incoming-header contract. Combined with the HTML fallback profile,
non-HEAD fallbacks reproduce Express's UTF-8 serialization of Latin-1 values;
HEAD and normal responses preserve the original wire bytes. Native defaults
remain available. This profile does not change CORS or JSON parser selection.

Set `RouterConfig.typescript_cors` to `Some(TypeScriptCors::default())` for
the static TypeScript CORS profile. Configure fixed, reflected or exact allowlisted
origins, credentials, headers, max-age and preflight continuation/status there.
This option is mutually exclusive with `cors`; unsupported configuration fails
when constructing the router. Dynamic origin callbacks/regexes and arbitrary
JavaScript coercion are outside this profile. Hyper omits Content-Length on 204
responses, where Express sends zero. Handlers porting incremental `res.vary`
updates can seed headers from `TypeScriptCorsHeaders`; returned response headers
override prepared defaults. Native CORS behavior remains the default.

Register declared unversioned JSON routes in `JsonRequestParsers` and set
`RouterConfig.json_request_parsers` to parse framed matching-media requests before
authentication and request-schema callbacks. Parsing remains active when schema
validation is disabled. Handlers can extract `ParsedJsonBody`; authenticators and
framework handlers can inspect `FrameworkRequest.parsed_json`. Original wire
bytes remain available independently from decoded bytes and parsed values.
Unframed or nonmatching media skips parsing; framed empty JSON becomes `{}`.
Parser options and verifier/reviver hooks reuse the versioned parser engine.
The bounded value domain admits finite decimals/exponents and normalizes large
source integers through JavaScript-compatible IEEE-754 rounding, including before
reviver callbacks. Overflow and lone UTF-16 surrogates remain unsupported.
Large parsed numbers require `as_f64`; exact integer deserialization and
caller-owned schema enum/const literals are not automatically normalized.
This does not implement generic JSON-schema validation or JavaScript response
serialization (including signed-zero spelling). Native behavior is unchanged
unless explicitly registered. LocalRouter offers `execute_with_json_parsers`
with the same framing rules; MCP argument handling remains separate. Construct
framework requests through `FrameworkRequest::new` when migrating callers that
previously enumerated every public struct field.

Explicit `RequestValidators` callbacks normally run after authentication.
Use `set_stage(route, RequestValidationStage::BeforeAuthentication)` to move
a registered callback after parsing and before authentication. Only Error-mode
callbacks run, once per request; None/Warning behavior is unchanged. Rejection
metadata remains enforced, and diagnostics must be safe to expose without
credentials. This controls the explicit callback, not native schema checks or
request transformations. Authentication and authorization still run after a
successful callback.

`RequestValidators::insert_json_transform` registers a schema-bound JSON
transform alongside matching parser registration and declared 400 rejection
metadata. It runs once before authentication in all validation modes. Successful
output replaces the parsed value atomically, including absent defaults or root
removal; None/Warning failures preserve the complete original value, while Error
rejects before authentication. Warning logs omit callback/input contents.
Only native body presence/media rechecks are suppressed after schema success;
other native field checks remain. Handlers consume `ParsedJsonBody` to see the
result; raw Axum extractors still read original bytes. Obtain parsed bodies from
the parser rather than struct literals; the transform-success marker is private.
The callback owns schema semantics; generic Zod, query/header transformation and
MCP handling remain separate.

Use `insert_json_transform_checked` when the callback can fail operationally.
Return `JsonTransformFailure::Validation(response)` for public schema diagnostics
and `JsonTransformFailure::Operational` for evaluator/resource failures. Operational
failures always return a safe correlated 500, including None/Warning modes.
The existing `insert_json_transform` API treats its errors as validation failures.

The bounded `typescript_coerce::zod_json` evaluator supports typed JSON scalar
rules, strict typed literals, optional/nullable values, static defaults, ordered unions, arrays and ordered object fields
with unknown-field stripping. Map `ZodJsonOutcome::Invalid` to validation failure
and evaluator errors to operational failure. The [live composition example](crates/axum/examples/json_composition_runtime.rs)
shows this mapping with `ParsedJsonBody`; its [runtime fixture](../framework-rs/parity/rust_json_composition_runtime.mjs)
checks HTTP, registered MCP forwarding and both generated SDKs. Define supported
schemas explicitly; this API does not load arbitrary Zod callbacks or interpret
OpenAPI schemas. Input/schema/evaluation budgets are enforced before projection. For request
schema diagnostics, call `zod_json_at_path(input, &schema, &["body"])` so the raw
issue paths receive their root before union formatting. Adding a root to already
formatted errors would produce different nested-union diagnostics. Ordered unions
select the first valid result, otherwise the first dirty branch; all-aborted
branches use the actual adapter's single-level variant formatting. The trusted
path shares admission/work budgets with schema/input diagnostics. Use
`ZodJsonLiteral::{String, Number, Boolean}` for strict literal branches; ordered
enums lower to `Union` of those literals in source `Object.values` order. Numeric
signed zeros compare equal and preserve the input sign. This API accepts an
explicit ordered schema; it does not reflect arbitrary JavaScript enum objects.
Repeated input metric scans also debit the shared evaluation node budget.

`ZodJsonSchema::{Null, Undefined, Void, Never}` distinguish supplied null from
absent input (`None`). Undefined/Void accept absence and preserve object-field
omission; Never rejects every input. Lower nullish through ordered
`Union(vec![Void, Null, Undefined])` to retain source diagnostics. Explicit
JavaScript undefined-valued properties and sparse array holes are outside JSON.

`ZodJsonSchema::Record { key, value }` evaluates each own string key and its
value, retaining source key-before-value diagnostics and transformed-key collision
behavior. Use `OrderedJsonDocument` when input insertion order affects diagnostics
or collisions. Plain `serde_json::Value` returns an unsupported result when that
order is required. Successful output comparisons cover semantic values; serialized
property order is not guaranteed.

`ZodJsonSchema::ObjectWithPolicy` adds `ZodJsonUnknownKeys::{Strip, Strict,
Passthrough}` and an optional catchall. An active catchall overrides the unknown-key
policy; only unwrapped `Never` restores that policy. Extra values are validated
in own-key order after declared fields, including `__proto__`, whose output is
omitted. Use an ordered document when multiple extra diagnostics need insertion
order. Passthrough/catchall preserve semantic values without a serialized-key-order
promise. Existing `Object` and `StrictObject` use the same evaluator.

Choose `ZodJsonSchema::StrictObject` to reject unknown keys with source-ordered
diagnostics. Parse `OrderedJsonDocument` from bounded JSON and evaluate it with
`zod_json_ordered_at_path(&document, &schema, &["body"])`. The immutable document
retains original own-key order, including duplicate-key position and numeric
indices. Its value has read-only access. Ordinary `Value` inputs work where order
is unambiguous; selected diagnostics needing unavailable order return an explicit
unsupported result. Later valid union branches still win. Never rebuild ordering
from stale raw bytes after a reviver or transformation; supply a fresh ordered
document at that boundary. The live example uses an explicit parser without
those hooks and handles its synthetic empty object separately. Evaluator outputs
retain semantic JSON scope; serialized property order and record transforms are
separate capabilities.

`RequestValidators::insert_request_transform` accepts one whole-schema callback
for atomic parameter/query/header projections and optional JSON-body output.
It runs before authentication in every validation mode, preserving all original
sections on ignored failure. Body routes require matching parser registration;
bodyless routes need none. `ParsedRequestFields` and `ParsedParams`, `ParsedQuery`,
`ParsedHeaders` expose successful projections. Use optional projection access and
original wire values when permissive validation fails; required typed extractors
require success. `FrameworkRequest.raw_query` retains repeated encoded inputs.
Headers retain every original key and replace only truthy exact-key values;
absent-header defaults are not inserted. Native credential extraction uses
original authorization/cookie headers. Successful schema output skips only native
request-schema checks; authentication and authorization still run. Generic Zod,
complex query interpretation, credential-header rewriting and MCP remain separate.

`RouterConfig::with_authorizer` replaces native RBAC with an application-owned
callback over the verified session and declared contract. Authentication still
runs first. Async grant resolution belongs in `with_authenticator`; the returned
principal must contain trusted grants. The authorizer owns policy for all
routes requiring authentication, including mounted routes. For example, select
an application policy by operation and call
`policy.authorize(&session.principal, Some(contract.access))`. Configure route
access/auth explicitly. Public routing/credential behavior is still determined
by the router's contracts; the session policy does not change credential parsing
or session validation. Without an authorizer, existing native RBAC behavior stays
in effect. The 1,495-case actual TypeScript fixture covers session-policy results;
credential transports and full error-envelope conformance remain separate gates.

`AuthHttp::with_rate_limit_options` accepts `AuthRateLimitOptions` with a positive
`AuthRateLimitRule::new(window_seconds, max)` default and `with_exact_path`
overrides relative to the auth mount. An override of `None` disables that endpoint's
limit. Built-in sensitive-endpoint limits take precedence over the default;
custom overrides take precedence over both. `with_path_pattern` adds wildcard
rules (`*` within a path segment, whole `**` across segments). The first registered
matching custom rule wins, including when exact and wildcard rules overlap;
replacing a rule retains its position. Patterns compile at configuration time.
`with_rate_limiting` controls enablement separately (production by default).
`with_dynamic_path` registers an asynchronous rule receiving request method,
original URI, normalized auth-relative path, headers, and the effective plugin,
default or sensitive-endpoint rule. Return `UseDefault`, `Disable`, or `Limit(rule)`.
Only the first matching custom rule runs. Callbacks execute before the atomic
counter lock; errors fail closed with a generic authentication error. Request
bodies remain available to downstream handlers. `with_plugin_rule` registers a
positive limit and a path predicate; register rules in plugin order. The first
matching plugin rule overrides built-in limits before custom rules run.
`with_custom_storage` accepts an asynchronous atomic counter backend receiving the
normalized `IP|auth-path` key and effective rule. Return `Allow` or
`Deny { retry_after }`; omitted retry values use the effective window, and zero is
preserved. Disabled limits skip storage. Backend failures fail closed without
exposing private details or falling back to memory. Applications own counter
atomicity and expiry; built-in Redis/PostgreSQL counter backends and body-reading
rule callbacks remain unsupported.

The [engineering security review](../plan/RUST-SECURITY-REVIEW.md) records verified
auth/crypto boundaries, fixed findings and the exact remaining dependency notices.
The audit gate scans all three lockfiles and rejects unreviewed findings.

Runtime performance checks cover HTTP, typed validation, generated SDK calls,
Redis, S3 and typed WebSockets. See [benchmark commands and measurement boundaries](../docs/adding-projects/rust-benchmarks.md)
for optimized runs, optional infrastructure and JSON reports.

`module_auth::HmacJwksAuth::with_hmac_keys_for_route` configures key IDs and
secrets for one declared HMAC route's method and path. Its keys replace the
router's default key policy only on that route. Empty policies, secrets and key
IDs, duplicate IDs, and non-HMAC contracts are rejected. Keys remain server-side
and are absent from contract artifacts. The generated Better Auth relay uses
this mechanism for instance-ingest and platform-claim keys.

## Commands

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps
```

## Package Policy

- Workspace package metadata lives in the root `Cargo.toml`.
- Optional integrations should be feature-gated at the crate that owns the integration.
- Core contract types should stay independent of any specific HTTP framework.
- `forklaunch-axum` is the first adapter, not the framework core.

## Additional MCP tools

Enable `forklaunch-core`'s `rmcp-server` feature and use `forklaunch_core::mcp` and append a `server::ManualTool::typed` binding to `McpServerOptions.manual_tools` before calling `ForkLaunchRouter::into_router_with_mcp(options)` (or `server::mount_http`). Input types implement `forklaunch_validator::FrameworkSchema`: serde parses arguments, garde validates them and schemars describes discovery metadata. Declare matching schemars constraints when a garde rule should appear in the schema. Unknown-field behavior follows the input type's serde configuration.

```rust,ignore
let tool = ManualTool::typed::<LookupInput, serde_json::Value, _, _>(
    "lookup", "Look up application data", |input, headers| async move {
        // Authorize this operation with fresh request headers and trusted state.
        authorize_lookup(&headers, &input)?;
        Ok(serde_json::json!({"result": lookup(input).await?}))
    },
)?;
options.manual_tools.push(tool);
let router = application.into_router_with_mcp(options)?;
```

Transport authentication still gates every request. Manually bound handlers must apply their own operation/tenant authorization; HTTP route guards apply to contract-derived tools. Every call receives fresh headers, and tool arguments cannot replace them. Invalid typed input fails before application work; handler errors use explicitly public `ManualToolError` messages. Timeouts cancel the invocation future and response size limits apply. Duplicate manual/HTTP names fail at startup. Object results expose structured content; scalars and arrays remain JSON text.

## HTTP and MCP listeners

Generated services call `ForkLaunchRouter::serve_with_default_mcp(listener, shutdown)`. MCP shares the HTTP listener by default. Set `MCP_LISTENER=separate` to use a second listener with the TypeScript HTTP-port-plus-2000 fallback, or set `MCP_PORT`/`MCP_HOST` to select a separate listener explicitly. Port `0` supports ephemeral binds. Only MCP is exposed on the second listener; tool calls still invoke authenticated HTTP handlers internally. Both listeners bind before serving, then drain together on shutdown with a 30-second bound. `into_routers_with_mcp(options)` supplies separate routers for custom orchestration.

## Server-sent events

Enable `forklaunch-axum`'s `sse` feature and pass a stream of
`Result<SseFrame<T>, E>` to `sse::respond(stream, SseOptions::default())`.
`T` implements `FrameworkSchema`; its garde rules validate each payload.
Use `respond_with(stream, options, validator)` for whole-envelope validation.
The application supplies the typed model or validator matching its route contract;
the helper does not infer validation rules from arbitrary contract JSON Schema.

The default `SseValidation::EveryEvent` validates every event. Explicit
`FirstEvent` matches the TypeScript emitter's first-event-only schema validation.
The helper awaits one event before returning headers, allowing first-event errors
to become sanitized HTTP 500 responses. Later failures terminate the body.
An empty source returns an empty successful SSE response. No producer task or
unbounded queue is created, and cancellation drops the source even while awaiting
the first or next event.

`max_event_bytes` defaults to 1 MiB and bounds each encoded wire frame in either
validation mode. String data is emitted as text; other serializable data uses
JSON. Multiline text receives an SSE `data:` prefix on each line. IDs containing
CR, LF or NUL are rejected in either mode. These framing safeguards intentionally
remain stricter than TypeScript's raw field interpolation.

`rust_sse_emitter_parity.mjs` executes the actual TypeScript response parser and
emitter with Zod schemas, compares 16 frames, and verifies first-event validation
and empty-source behavior. `rust_sse_runtime.sh` uses this helper in a relocated
generated service and exercises both generated SDKs, stream failures, cancellation,
resource release, and HTTP/MCP startup and shutdown.

## Typed WebSocket connections

Enable `forklaunch-ws` features `transport` and optionally `validation`. `transport::TypedWebSocketServer::bind(address, schemas, options)` owns a TCP listener; `accept_with(callback)` authorizes upgrade headers before accepting a connection. `transport::connect(request, schemas.reversed(), options)` accepts a URL or tungstenite request with caller headers/subprotocols and supports TLS with WebPKI roots. Applications own connection tasks and shutdown.

`SocketSchemas::new(incoming, outgoing)` accepts `PayloadCodec::serde()` or `PayloadCodec::framework()` for serde/garde validation; builders add typed ping, pong and close reason schemas. Messages use raw UTF-8 strings or JSON, matching the TypeScript wrappers. Explicit `PayloadCodec::bytes()` bypasses text/JSON interpretation for native binary protocols. The older tagged `WebSocketEnvelope` is a separate wire format.

Checked contracts can declare raw bytes with `SocketSchemaEntry::binary()` (or
`"binary": true` plus `"shape": {"type":"string","format":"binary"}` in the
portable artifact). Both generated clients preserve those bytes as `Vec<u8>` or
`Uint8Array`, including invalid UTF-8 and JSON-looking data. Byte limits still
apply to messages and ping/pong controls. Each binary direction must have one
entry with the canonical shape; mixed JSON/binary unions, extra assertions and
binary close reasons are rejected. A JSON Schema `format: binary` alone remains
an annotation and does not select the binary codec. AsyncAPI export marks binary
messages `application/octet-stream`; the external AsyncAPI importer accepts
canonical binary schemas with that media type.


`SocketSchemaEntry::json(shape)` (portable metadata `jsonEncoding: true`) selects
explicit JSON wire encoding. Generated clients quote strings, preserve `null`,
and reject non-JSON input and invalid UTF-8. Native runtime codecs opt in with
`PayloadCodec::json_schema(&shape)?.with_json_encoding()` or
`PayloadCodec::serde().with_json_encoding()`. Limits count actual encoded bytes,
including string quotes and escapes. Mixed raw/JSON alternatives in one direction
and simultaneous binary/JSON flags are rejected. External AsyncAPI JSON messages
select this mode automatically; native entries created with `new` retain the
TypeScript-compatible raw-string behavior. Exported explicit JSON messages always
carry `application/json` media.

`SocketOptions` bounds frame/message/write buffers and handshake/write deadlines. Invalid incoming payloads close with 1008 before application delivery; invalid outgoing payloads fail locally. Automatic pong replies retain exact bytes and satisfy the outgoing pong schema. `recv()` returns typed message/control events; after `close(code, reason)`, continue receiving to finish the handshake. Dropping a connection releases it; cancelling a pending receive preserves subsequent messages. With `forklaunch-axum` feature `websocket`, `websocket::upgrade(upgrade, schemas, options, handler)` adapts an authorized Axum `WebSocketUpgrade` into `AxumTypedSocket`, using the same validation/control runtime. Compose that route with the HTTP/MCP router on one listener. Select subprotocols or an upgrade-failure callback on the extractor before passing it in; `transport::connect_with_response` retains the negotiated client response. Apply authorization in the HTTP handler before upgrading and propagate application shutdown to active connection tasks. HTTP server handshake timeouts apply before upgrade; socket write deadlines apply afterward. Cancelling an in-progress write releases a partial connection. `SocketContract`/`SocketSchemaEntry` retain raw payload schemas, channel/operation enrichment, controls and context/user-data metadata. `SocketSchemaEntry::framework::<T>()` derives framework schema metadata. `forklaunch_core::asyncapi::generate_raw_socket_asyncapi_json` exports raw payloads using TypeScript channel fan-out and singular/plural precedence; `_with_options` accepts description, ID and content type. Errors are documented without operations; ping/pong/close controls are omitted, matching TypeScript. JSON-schema document-local pointers relocate to each embedded payload while literal `$ref` data remains intact. Conflicting definitions, invalid addresses, unresolved/external references and unsupported resource scopes fail explicitly. Bundled IDs, static anchors and declared dynamic/recursive scopes are resolved before payload relocation. Schema recursion is bounded to 64 levels, and boolean schemas use AsyncAPI's object form. This raw exporter is separate from tagged-envelope exports. CLI socket-contract generation below uses this checked exporter.

Enable the optional `json-schema` feature for `PayloadCodec::<T>::json_schema(&schema)`. It compiles constraints once, checks incoming JSON before serde can discard unknown fields, and checks serialized outgoing values before raw string encoding. Missing dialect declarations use Draft 7; declared 2020-12 schemas, recursive pointers and in-document resource IDs are supported. File and network retrieval always fail, even when another dependency enables resolver features. Schema compilation bounds depth, nodes, bytes and regex execution. Email uses the AJV full-format profile (dotted domains), and UUID assertions apply in Draft 7 too. Compiled codecs accept safe integral decimal/exponent spellings for schema checks and typed integers, while float/JSON-value targets preserve negative zero. Strings and fractional values are not coerced; larger numbers retain Rust representation/range limits. Decimal `multipleOf` checks are exact; a TypeScript AJV consumer needs the equivalent exact decimal keyword rather than floating-point tolerance. Valid JSON beyond serde's numeric/depth representation limits is rejected instead of becoming plain text. The raw string ambiguity remains: sending a schema-valid string `"42"` emits `42`, which a receiving JSON-first decoder interprets as a number. This codec does not add binary schemas or change the checked exporter's reference restrictions.

The actual TypeScript wrappers also validate automatic control replies before typed delivery. A rejected reply reports one error and closes with 1008, releasing an unresponsive peer after five seconds. Manual invalid control payloads remain local failures, and `autoPong: false` is preserved. Typed close listeners report abnormal 1006 disconnects as transport errors rather than validating an empty reason against the close schema. `rust_ws_runtime.mjs` exercises these paths against real sockets in CI.

`rust_ws_jsonschema_parity.mjs` checks 4,104 encode/decode cases against actual TypeScript WebSocket helpers with AJV and an exact decimal keyword. The separate Zod fixture checks 168 cases, including JSON representation-limit rejection.

`forklaunch_core::codegen::websocket::SocketSdkPlan::from_contract(&contract)` prepares checked client directions, protocol controls and recursive message unions. `forklaunch_core::rust_sdk::generate_rust_websocket_sdk_source(&plan, &RustSdkOptions::new("chat-sdk"))` emits a native client with typed `send`, `recv`, `ping`, `pong` and `close`. `connect(request, options)` returns the client and HTTP upgrade response; caller requests retain authorization headers and subprotocols. The generated source needs `forklaunch-ws` features `transport,json-schema`, serde `derive`, `serde_json`, tokio `net` and tokio-tungstenite 0.30. `SOCKET_PATH` exposes the declared path; callers supply the complete request URL. `schemas()` prepares reusable validated codecs, and socket accessors allow composition with the native runtime. Error/context/user-data metadata describe framework events, rather than extra wire messages. Unions retain each standalone schema's recursive references and require a common dialect; bundled resource IDs and declared dynamic/recursive references are resolved independently before union relocation. Messages may reuse resource IDs without sharing bindings. CLI socket-contract generation below reuses this generator.

`forklaunch_core::typescript::generate_typescript_websocket_sdk_source(&plan, &TypeScriptWebSocketSdkOptions::new("ChatClient"))` emits a Node client using ws 8.22, AJV 8, ajv-draft-04 1 and ajv-formats 3, with Node/ws type declarations for TypeScript consumers. `await ChatClient.connect(url, options)` retains upgrade headers and negotiated protocol in `response`/`protocol`. `send`, `ping` and `pong` return promises; `close(code, reason)` starts a handshake, and `recv()` yields typed `{ type, value }` message/control events or `{ type: 'close', code, reason }`. After a terminal error is reported, receiving ends with `null`. Consume close events to finish a graceful handshake, or call `terminate()` to release the connection immediately. Breaking an async iterator releases it too.

`build::typescript_websocket` also emits a sibling `<name>.browser.ts`. Generated service SDK packages expose `./<service>/<name>` with a browser condition and the explicit `./<service>/<name>/browser` subpath. The browser client uses native WebSocket and cookies, validates the same canonical payload schemas, bounds queues and buffers, and supports `send`, `recv`, typed `close` and cancellation. Browser control frames and upgrade headers are unavailable; it does not expose Node `ping`, `pong`, custom headers or TLS overrides. Browser `send` resolves after queuing in the browser, not a peer acknowledgment. Both transports apply explicit UTC Date codec metadata; ordinary timestamp-looking strings remain strings.


The Node client bounds message bytes (1 MiB), queued events (128), queued wire payload bytes (4 MiB), handshake time (10 seconds), write/automatic-pong time (5 seconds) and close time (5 seconds); options override these positive limits within Node's timer range. Receive abort/timeouts preserve the next event, while cancelling a partial write releases the connection. One explicit receive and one explicit write may be pending at a time. The raw JSON profile uses exact decimal multiples, own-property checks and reference-safe normalization for AJV's excluded `__proto__` properties. Literal schema data and existing pattern/reference targets remain unchanged. These generated clients use JSON/raw UTF-8 strings; explicit raw binary SDK metadata is still tracked separately.

Generated services also export sockets registered in `contract_registry()`. With `forklaunch-core/json-schema` enabled (already enabled through generated OpenAPI dependencies), register a `forklaunch_ws::SocketContract`:

```rust
registry.register_socket("chat", &chat_contract).expect("register chat socket");
```

`forklaunch openapi export` writes registered socket metadata into the service contract and the service's SDK folder (`<service>/sdk/typescript/socket-contract.json`), using the service name/version. It honors project metadata `socketContractArtifactPath`. A content ownership record lets later exports remove an unchanged generated artifact when its path moves or the last socket is removed; manual and edited artifacts are retained. Run export after Rust registry changes, then generate/watch the artifacts. Runtime upgrade handlers are composed by the application.

`SocketContractArtifact::new(title, version).with_socket("chat", contract)` produces a versioned, portable collection of named socket contracts. Serialize it to `<service>/sdk/typescript/socket-contract.json`, then run:

```sh
forklaunch sdk generate --from socket-contract --target all --service accounts
forklaunch sdk generate --from socket-contract --target all --check
forklaunch sdk watch --from socket-contract --target all
```

The CLI runs the checked generators from the application's bundled Rust framework, with a five-minute compiler deadline and bounded input/output. It emits `<service>/sdk/typescript/asyncapi.json`, Node packages under `sdks/typescript-websocket/<service>` and Rust crates under `sdks/rust/<service>-websocket-sdk`. Each named socket has a `socket_<name>` module and `SocketClient`; root TypeScript exports use namespaces. Socket keys contain lowercase ASCII letters, digits or underscores. Cargo and PNPM workspace registration is automatic, and existing TypeScript compiler configuration is retained. Optional project metadata `socketContractArtifactPath` and `asyncapiPath` select custom paths inside the application; watch follows configured contract paths and manifest changes. After startup it reports invalid edits, retains the last generated outputs and retries when artifacts change; repeated manifest-read errors are reported once. All requested services are checked before any SDK files are written. `rust_ws_cli_runtime.sh` validates relocated generation, drift checks, multi-service failure staging, path containment, custom-path watching, both live clients, the official AsyncAPI parser and generated HTTP/MCP startup. Registry export lifecycle, custom paths and ownership are exercised through the relocated CLI fixture and archive installer. Regeneration prunes unchanged obsolete modules. Exported empty registries retire owned SDK files/AsyncAPI/Cargo members; transient missing artifacts and edited/untracked files are preserved, and live watch restores packages when sockets return. Explicit service deletion also cleans owned socket outputs. Shared pnpm package globs remain for retained user packages. Service rename updates socket artifact/package names and Cargo membership, preserves custom compiler options and retains edited/untracked old files. The relocated fixture compiles both clients and validates HTTP/MCP startup after default/custom-path renames. External AsyncAPI documents use the explicit server-view import profile below.

`rust_ws_typescript_sdk_runtime.mjs` tests both module formats and declarations, normal/recursive/empty/Draft-4/Draft-6 clients, actual TypeScript/native peers, all 4,104 codec cases, protected keys and scoped references, malformed-peer 1008 rejection, queue limits, iterator cleanup, and stalled/cancelled handshakes/writes/pongs/closes. Generated TypeScript clients support Draft 4, Draft 6, Draft 7, 2019-09 and 2020-12. The official Draft 6 meta-schema is embedded in emitted source for strict ESM/CommonJS consumers; no runtime meta-schema imports or retrieval are required. Draft 4 uses `ajv-draft-04` with newer validation keywords disabled; shared model generation also ignores those keywords, preserving declared field types. Legacy Draft 4 `id` resource scopes use the same bounded offline resolver as modern IDs. Ignored older-dialect annotations retain their exported literal data. Generated validators clean inactive resource IDs on a private copy because AJV discovers them independently of its dialect vocabulary; explicitly referenced schemas and literal IDs in active assertions remain intact. Shared SDK models and OpenAPI reference objects decode URI fragments once, including Unicode, escaped pointer tokens and literal percent names; equivalent recursive edges reuse one model. Shared relocation emits URI-safe fragments for nested union schemas and metadata exports. The archive installer also generates and compiles both languages' WebSocket consumers using its relocated bundled sources and pinned npm dependencies.

`rust_ws_sdk_runtime.mjs` compiles normal, recursive and empty native clients with warnings denied, exercises the normal client against the actual TypeScript server, and verifies malformed peer payloads close with 1008 before delivery.

`rust_ws_metadata_parity.mjs` compares 34 documents with the actual TypeScript generator and validates all against the official AsyncAPI JSON Schema. Parser 3.6.3 also accepts 63 documents; eleven cases expose its percent/prototype map-key and literal `$ref` limitations, so those retain direct specification-schema and pointer/AJV checks. Recursive structural schemas and explicitly referenced legacy annotation storage additionally pass the parser without changing unreferenced literal data. Schema traversal follows reachable local pointers exactly once, decodes percent escapes once, and enforces 64-step/100,000-location bounds. Checked exports now bundle schemas referenced from active const/enum/default/example data into collision-checked definitions, preserving the original literal values and recursive references. Copying and final output share 64-level, 100,000-node and 32 MiB bounds; repeated bundling is stable. The same relocation helper protects OpenAPI/MCP schemas and generated socket unions, preserving embedded resource bases. Checked unscoped socket export bundles references to entire schema dictionaries in separate schema/literal interpretation contexts; resource-preserving relocation retains its conservative alias restrictions. Declared Draft 4/6/2019-09/2020-12 payloads use AsyncAPI Multi Format Schema Objects; the parser accepts these custom formats but leaves their schema validation to the independently checked native/codecs. This preserves older boolean exclusive bounds and avoids imposing Draft 7 semantics.

`forklaunch_core::contract::bundle_document_schema(&document, "/components/schemas/Message")` extracts a standalone JSON Schema from a containing document. It bundles transitive local references, including recursive and literal-data targets, into collision-checked definitions. It preserves literal values and rejects missing/external references and active resource IDs/dynamic scope under shared copy/output limits. Dictionary references use separate interpretation contexts: a property named `const` can remain a schema in the original dictionary and literal data in a referenced copy. Recursive edges are rebased without rewriting annotation data. `forklaunch_core::codegen::bundle_document_schema_resources` adds offline resource IDs, anchors and dynamic/recursive roots to document extraction; resource-free documents retain the unscoped path. Ambiguous resource-aware dictionary aliases still fail explicitly. The checked AsyncAPI importer uses this shared resolver with explicit operation roles.

Import an AsyncAPI **3.0.0 WebSocket server** document with:

```sh
forklaunch sdk generate --from asyncapi --target all --service realtime
forklaunch sdk watch --from asyncapi --target all --service realtime
```

The input defaults to `<service>/sdk/typescript/asyncapi.json`; project metadata `asyncapiArtifactPath` selects another contained path. Generation/check/watch preserve the input document. SDK ownership records retain their source through service rename; standard input paths move with the service without changing document bytes, while custom paths remain custom. Regeneration removes obsolete unchanged modules and preserves edited files and compiler settings. All services validate before outputs are written, and outputs cannot overwrite input artifacts. Importing a formerly generated AsyncAPI file releases its export ownership without deleting it.

The native API is `forklaunch_core::asyncapi::import_server_websocket_asyncapi` with the `import` feature. Server `receive` operations become client outgoing messages, and server `send` operations become client incoming messages. Channels need unique absolute paths, explicit operation message subsets, JSON or `application/octet-stream` media and ws/wss servers; GET bindings without upgrade parameter schemas are supported. An offline, licensed official specification schema validates the input before bounded local-reference extraction. Declared payload constraints are preserved, multiple messages retain `oneOf`, and undeclared directions reject payloads. Channel message schemas must have provably disjoint root types or required string discriminators. Bundled component/payload resource IDs, URI aliases, static/dynamic anchors and recursive roots are resolved offline. Different messages may declare different supported dialects, but each message must retain one dialect across its reachable resource graph. Missing external resources, duplicate IDs, mixed runtime dialects, headers/security, dynamic reply addresses, parameterized paths and other protocols fail checked import. Binary messages require the exact payload `{ "type": "string", "format": "binary" }`, inline or through bounded local references, and `application/octet-stream` on the message or document default. Each binary direction selects exactly one message. Send and receive may use different wire types; mixed JSON/binary alternatives within one direction and extra byte-schema assertions fail explicitly. The generated clients use `Vec<u8>` and `Uint8Array`, preserving invalid UTF-8 and JSON-looking byte sequences. Binary schema annotations alone never select a binary codec for JSON media. Message and operation traits merge in order using JSON Merge Patch, with the target taking precedence. Local trait references are resolved offline; inherited content types and bindings pass the same wire checks as explicit fields. Arrays replace earlier arrays and nested null members remove inherited fields. Trait lists are bounded to 64 entries per object and share the import byte budget. Traits cannot supply payloads or bypass unsupported headers/security/bindings. Fixed operation replies are supported on the same channel or a different declared channel, inline or through bounded local reply references: a server receive operation's replies become client incoming messages on the reply channel, and a server send operation's replies become client outgoing messages there. Replies must explicitly name a root channel and a nonempty message subset within it. The same schema, message-disjointness and binary-direction checks apply. The SDK exposes send/receive streams; applications own request correlation, reply deadlines and sequencing. Different paths generate independent socket clients; applications must open their connections and route messages explicitly. Dynamic reply addresses remain rejected. This follows the [AsyncAPI 3 reply model](https://www.asyncapi.com/docs/reference/specification/v3.0.0#operationReplyObject) within the supported WebSocket profile. Broader event projects and profiles remain open. The relocated `rust_asyncapi_import_runtime.sh` fixture compiles both SDKs, runs live peers, exercises watch recovery and rename, and validates generated HTTP/MCP listener startup and shutdown.


Multi Format JSON payloads support Draft 4, Draft 6, Draft 7, 2019-09 and 2020-12.
Set `schemaFormat` to `application/schema+json;version=<dialect>` and include the
matching explicit `$schema` URI inside `schema`. For example, Draft 4 uses
`application/schema+json;version=draft-04` and
`http://json-schema.org/draft-04/schema#`. The importer preserves dialect-specific
constraints and document-local references; message unions retain their root
dialect. Every alternative in one direction must use the same dialect. Missing
or mismatched declarations, unknown formats and nested mixed dialects fail checked
import. Draft 4 message discrimination requires `enum` tags because its `const`
keyword is only an annotation. `rust_asyncapi_dialects_runtime.mjs` compiles both
SDKs for all five dialects, tests live roundtrips and rejects invalid numeric bounds.

`forklaunch_core::asyncapi::RuntimeExpression::parse` checks message header/payload
locations and RFC6901 URI-fragment pointers with4096-byte/64-segment limits.
`resolve(headers, payload)` borrows the referenced value with its original type,
including present null; a missing source/path returns None. URI percent escapes
are decoded once, then JSON Pointer escapes are applied. Applications retain
responsibility for matching correlation values and enforcing reply deadlines.
`MessageContract::with_correlation_id` exports a checked location in event
AsyncAPI documents. `import_server_websocket_asyncapi_with_metadata` retains
resolved message correlations alongside the unchanged socket contracts, including
local references and ordered trait precedence. The raw WebSocket profile accepts
JSON payload locations; application-header and binary-payload correlations are
rejected. Upgrade headers are separate from application message headers. The
ordinary importer validates the same profile and returns only socket contracts.
Dynamic reply addresses and broader event imports remain open. See the
[AsyncAPI runtime expression grammar](https://www.asyncapi.com/docs/reference/specification/v3.0.0#runtimeExpression).

Checked SDK schemas, raw socket AsyncAPI exports and imported AsyncAPI payloads require literal `/` JSON-pointer separators; use `~1` for a slash within a name. Percent-encoded names, escaped tokens and literal percent names remain supported. Encoded separators fail before artifact writes, matching the checked validator profile. Generic OpenAPI reference-object resolution and resource-preserving relocation retain URI decoding independently. Native raw JSON Schema codecs and actual AJV still differ for encoded interior separators outside this checked profile; broader resolver parity remains tracked.

See the [Rust/TypeScript compatibility baseline](../docs/adding-projects/rust-compatibility.md) for source package versions, artifact formats and implementation choices.

## Reviewing generated output changes

The TypeScript SDK crate's `snapshots::openapi_and_both_http_sdks_match_reviewed_snapshots`
test compares a shared catalog contract against committed OpenAPI, Rust HTTP SDK
and TypeScript HTTP SDK output. It covers path/query/header parameters, optional
nullable fields, typed success/error responses, response headers and empty 204s.
The normal workspace test job runs this check in CI. These are native output
regressions; the cross-language conformance fixtures remain separate.

For an intentional generator change, run from this workspace:

```bash
FORKLAUNCH_UPDATE_SNAPSHOTS=1 cargo test -p forklaunch-core --features build,asyncapi-import snapshots
cargo test -p forklaunch-core --features build,asyncapi-import snapshots
```

Review the source and snapshot diffs together before committing. CI refuses the
update flag. OpenAPI is compared as sorted, pretty-printed JSON; SDK source is
compared with trailing line whitespace normalized. No timestamps or absolute build paths are included.

### Optional TanStack Query helpers

Generated HTTP TypeScript SDKs export dependency-free `forkLaunchQueryOptions`
and `forkLaunchMutationOptions`. Import them when using TanStack Query v5; the
ordinary SDK keeps its existing call shape and status-union behavior.

```typescript
import { queryOptions, useQuery, useMutation } from '@tanstack/react-query';
import { createForkLaunchSdk, forkLaunchQueryOptions, forkLaunchMutationOptions } from './sdks/typescript/src/index';

const sdk = createForkLaunchSdk({ baseUrl: 'https://api.example.com' });
const users = useQuery(queryOptions(forkLaunchQueryOptions(
  ['tenant', tenantId, 'accounts', 'users'],
  sdk.accounts.users.listUsers,
  {},
)));
const remove = useMutation(forkLaunchMutationOptions(
  ['tenant', tenantId, 'accounts', 'deleteUser'],
  sdk.accounts.users.deleteUser,
));
```

Use your generated operation names. The helper retains the complete successful
HTTP envelope (`status`, `code`, `response`, content type and headers), narrowing
away non-2xx variants. Non-2xx responses throw `ForkLaunchSdkHttpError`, whose
`result` retains the returned envelope; transport errors propagate unchanged.
Request parameters and mutation variables retain the generated schema types.
Queries propagate TanStack's cancellation signal and combine it with a caller
signal, removing listeners after settlement. Mutations retain caller request
options and do not invent automatic cancellation or invalidation policies.

Keys are explicit: include the operation, resource arguments, endpoint identity
and tenant/user cache scope appropriate to your application. Do not put bearer
tokens or other secrets into keys. Clear scoped cached data when a session ends.
Spread TanStack options such as `staleTime`, `retry` or `select` at the call site;
wrap the result with TanStack's `queryOptions` when you need its tagged-key type
inference for `getQueryData`. Streaming iterators should be consumed explicitly
rather than cached as reusable results.

The fixture `framework-rs/parity/rust_tanstack_query_parity.mjs` checks actual Query Core and
React Query **5.104.2** types, cache reuse/invalidation and scope separation,
HTTP failures and retries, mutations, network failures, and both sources of
cancellation against generated fetch clients. It follows the upstream
[query-options](https://tanstack.com/query/latest/docs/framework/react/guides/query-options)
and [TypeScript integration](https://tanstack.com/query/latest/docs/framework/react/typescript)
interfaces. Configure `FORKLAUNCH_TEST_TANSTACK_DEPENDENCIES` to an isolated npm
prefix containing the pinned dependencies listed in the Rust parity workflow.

### JSON parser

The framework and generated applications use upstream `serde_json` without a
workspace patch. Axum and SQLx enable its `raw_value` feature, so, as in every
Axum or SQLx application, an object whose first key is exactly
`$serde_json::private::RawValue` is read as an embedded raw JSON value rather
than as object data. `arbitrary_precision` is not enabled, so the
`$serde_json::private::Number` key remains ordinary data.

The [JSON boundary audit](../plan/RUST-JSON-BOUNDARY-AUDIT.md) records coverage and
remaining limits, including unpaired surrogates, bounded nesting, and independent
JavaScript/ORM normalization semantics.

The [crate release guide](../docs/adding-projects/rust-releases.md) documents
packaging, registry installation checks and explicit publication. A passing local registry check does not mean the crates
have been published publicly.


### Typed multipart requests

Text fields use strict UTF-8 by default. For the pinned TypeScript/Busboy text
profile, install `Extension(forklaunch_core::http::BusboyTextOptions::default())` on
typed extractors and `VersionBodyHooksRegistry::enable_busboy_text` on versioned
contracts. `with_default_charset` changes the fallback charset; an explicit
part charset takes precedence. Byte limits apply before decoding. This profile
preserves malformed-UTF-8 replacement, Latin-1 aliases, UTF-16LE and base64
text while leaving file bytes unchanged. Unknown decoder results become absent
JSON fields. JavaScript own-undefined identity and isolated UTF-16 surrogates
are not Rust JSON values; the latter return a safe parser error. Mixed file/text
name collisions remain sensitive to upstream stream scheduling, so this profile
does not certify that ordering.


Enable `forklaunch-axum`'s `multipart` feature and use
`multipart::Multipart<Upload>` as the last handler extractor. `Upload` implements
`FrameworkSchema` (Serde, Schemars and garde); parsing and validation finish before
the handler runs. Text parts map to strings, file parts to `Vec<u8>`. Octet-stream
parts without filenames are files too. Repeated names use the last parsed part.
Mixed text/file parts sharing a name are not a portable cross-language contract:
Busboy assigns files on asynchronous stream completion. Use distinct field names.
Filenames are ignored and never used as filesystem paths. Numeric/composite text
coercion requires an explicit Serde model rule.

`Extension(MultipartLimits { parts: 128, field_bytes: 1024 * 1024,
total_bytes: 2 * 1024 * 1024 })` supplies the defaults; customize them per router.
Every part counts, including overwritten values. These are additional limits:
Axum's `DefaultBodyLimit` and `RouterConfig` body limits still apply. Invalid
models/forms return 400, extractor size limits return 413, and invalid zero
limits return 500. Error text omits uploaded values. The framework router's
earlier total-body limit retains its existing framework validation response.

Unlike Busboy's permissive text decoder, this extractor rejects invalid UTF-8;
it does not implement legacy charset conversion. It preserves file bytes,
including empty files, and does not JSON-parse text fields implicitly.
`framework-rs/parity/rust_multipart_extract_parity.mjs` compares seven cases against the
actual TypeScript discriminator/content-parser source with Busboy, including
repeated fields/files, Unicode, binary/empty files and filename-less octet streams.
Multipart SDK response decoding is described below; broader MCP conformance
remains tracked independently.


### Multipart SDK responses

Both native and CLI generators decode declared `multipart/form-data` responses.
TypeScript returns `FormData`; Rust returns `MultipartResponse { parts }`, an
ordered list of names, optional filenames/content types and byte vectors. Repeated
names, binary files and empty files are retained. No part is written to disk.
Multipart/mixed is rejected during generation. The transport result is not an
automatic JSON DTO conversion or validation of individual part schemas.

Responses are buffered with an **8 MiB** default wire-byte limit and **1,024**
parts. TypeScript config `maxMultipartResponseBytes` and Rust client builder
`with_multipart_response_limit(bytes)` customize the positive byte limit. Wrong
media, missing/invalid boundaries, truncation and limit violations reject the
call; early transport failures release the response. TypeScript supports native
fetch and injected cross-fetch byte streams and retains request abort signals.

Rust generated crates add `multer = "3.1"` and `futures-util = "0.3"` only when
responses require them. Consumers of standalone native-generated source must
provide those dependencies alongside reqwest, Serde and Serde JSON.

`cli/tests/rust_http_runtime.sh` compiles both artifact formats and both native
generators, runs multipart cases against Rust and TypeScript servers and checks
HTTP/MCP startup and graceful shutdown. The TypeScript native runtime uses the
platform FormData parser; Rust uses multer.

### Optional browser sessions

Enable `forklaunch-axum/cookie-session` and configure
`RouterConfig.cookie_sessions` with `CookieSessions::new(cookie_name,
"https://your-app.example", store, resolver)`. Declare protected routes with
`AuthConfig::cookie(cookie_name)`. OpenAPI exposes each named cookie as its own
cookie API-key security scheme. Without the feature/configuration these routes
fail closed. Bearer/JWT routes still require their own verifier.

The adapter uses tower-sessions 0.15, secure HttpOnly SameSite=Strict cookies,
path `/`, and a 30-minute expiry refreshed by session writes. Read-only requests do not
extend the lifetime. Supply a shared durable `SessionStore`
in production; MemoryStore is for tests/development. The async resolver receives
the stored subject and must load current authorization and reject revoked or
disabled identities. No roles or provider credentials are copied into the cookie.
Generic session-map updates are not atomic replay or global-revocation storage.

Use the `CookieSession` extractor on login, logout and CSRF bootstrap handlers.
`csrf_token()` issues a random synchronizer token; serve it with `Cache-Control:
no-store` and keep it in the trusted browser context. For login/logout and other
unsafe requests, browsers must send that token in `x-csrf-token`, the session
cookie, and the exact configured HTTPS `Origin`. Missing, duplicate and foreign
origins/tokens fail. Verify credentials before calling `login(subject)`; login
rotates both the session ID and token and returns the replacement token.
`logout()` flushes the session. Both reject safe methods.
`authenticated()` resolves the current server-side binding through the policy.
`identity()` exposes that binding only to trusted handler code; it is not an
authentication result and must never be logged or returned if it is a credential. Plain Axum handlers
can use `policy.wrap(router)` with the same policy/store as the framework router.
Configure credentialed CORS only for trusted origins when needed.

MCP cookie-authenticated tools forward the transport cookie, Origin and CSRF
headers to the same HTTP authentication path; tool arguments cannot replace
these headers. MCP transport authentication and Host/Origin policy remain
independent. This adapter does not replace Better Auth's compatibility cookie
format or implement OAuth/OIDC login providers.

HTTP SDK schema imports support document-local static `$anchor` references in
modern JSON Schema, including percent-encoded names, recursive models and
anchors in OpenAPI component schemas. Pointer and anchor references to the same
schema reuse a model. Indexing follows schema positions under shared depth,
node and byte limits; examples/defaults remain literal values. Duplicate,
invalid, missing and foreign-resource anchors fail before generation. Older
Draft 4/6/7 schemas do not gain modern `$anchor` semantics. Resource `$id`,
external retrieval and dynamic references remain unsupported in SDK imports;
checked AsyncAPI payload imports retain their separate conservative profile.

OpenAPI export keeps ordinary schema component names unchanged. Empty names,
names containing characters outside `[a-zA-Z0-9._-]`, and names beginning with
the reserved `_fl_encoded_` prefix become that prefix followed by the UTF-8
bytes in hexadecimal. Reserving the prefix prevents collisions with literal
names. Exported references and embedded recursive references use the same key;
contract names and schema annotations retain their original values.
`rust_component_names_runtime.sh` exercises these names with both artifact
imports, both live SDKs, native generators and HTTP/MCP startup.

`framework-rs/parity/rust_mcp_transport_parity.mjs` runs 32 calls through the real pinned
FastMCP server, repository Zod validator/generator and official MCP client, then
compares live HTTP body bytes with the native MCP encoder. Set
`FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES` to the installed canonical-iam fixture
prefix. The MCP link uses the official in-memory transport; forwarded HTTP is a
loopback TCP server. This complements native HTTP/MCP listener/startup tests and
the lower-level exact multipart-framing fixture.


### Bundled schema resource IDs

The shared SDK compiler resolves `$id` resources already bundled in a schema
or OpenAPI document. Relative IDs and references resolve against their lexical
base; pointer fragments address the selected resource, and `$anchor` names are
scoped to that resource. Legacy Draft-4/6/7 fragment-only IDs (`#Node`) also
define scoped anchors, including recursive aliases; IDs beside legacy `$ref`
retain AJV-compatible scope in both the compiler and native codecs. Legacy IDs
combining a URI/path and fragment (`nodes#Node`) resolve as exact bundled
aliases, including nested fragment-only aliases. Such an alias does not create
a bare `nodes` resource or an implicit JSON-pointer base. The compatibility
profile accepts legacy pointer-shaped IDs and digit/underscore names. Exact
registered IDs take precedence over document pointers. URI normalization decodes
unreserved characters and normalizes escape case while keeping encoded
delimiters distinct (`#%2Falias` differs from `#/alias`).
Draft-4 `id` and embedded resources using different
schema drafts retain their own keyword rules; legacy `const`/conditional
annotations do not become modern constraints. Recursive URI/pointer aliases
retain one model identity. Explicit 2020-12 resources support `$dynamicAnchor`
and `$dynamicRef`: outermost dynamic bindings specialize recursive models, while
ordinary `$ref` stays static. Dynamic references to plain anchors or pointers
retain static resolution. Missing dialect declarations do not enable dynamic
keywords. Explicit 2019-09 resources also support root `$recursiveAnchor`
booleans and `$recursiveRef: "#"`, preserving resource-root fallback when no
recursive anchor applies. Relative root IDs use the implementation's retrieval base
`https://forklaunch.invalid/__document__`.
No network fetch occurs during generation. Literal defaults/examples are not
interpreted as resource declarations. The same depth/node/byte budgets apply to
the resource index and model graph.

This profile rejects duplicate IDs, missing resources, malformed IDs, nonempty
modern ID fragments, non-root recursive anchors, non-`#` recursive references,
and resource-aware references into literal data/schema dictionaries. WebSocket
SDK unions and raw AsyncAPI export resolve each standalone resource graph before
relocation; resource-free schemas retain their original annotation/pointer layout.
`forklaunch_core::codegen::bundle_schema_resources` exposes this bounded operation.
Mixed reachable Draft 4/6/7/2019-09/2020-12 resources and mixed-draft message
unions are translated to 2020-12 after resolving original resource identities.
Draft 4 exclusive bounds and legacy tuple/tail rules retain their validation
meaning; inactive keywords remain annotations. Unreachable definitions do not
force translation, and unsupported reachable dialects fail explicitly.
`framework-rs/parity/rust_schema_mixed_parity.mjs` compares original/translated native
validation with independent AJV engines. Its receipt records AJV's existing
`contains`/`unevaluatedItems` discrepancy separately from resource conversion.
Run
`cli/tests/rust_schema_resources_runtime.sh` for live recursive Rust/TypeScript
SDK checks from contract and OpenAPI artifacts, including HTTP/MCP startup.

## Versioned HTTP request selection

`RouteContract::with_version` retains declaration order, including JavaScript's
canonical integer-key enumeration rules. For versioned JSON and text routes, the local
router and Axum match each declared request and select the last successful one.
They check response status against that version's responses. Common route
parameters and auth policy remain shared. Live routers compile embedded schemas
at construction through the same bounded, offline validator as socket codecs.
Invalid version schemas fail during router construction. Versions share the
first declaration's body parser and incompatible parser kinds are rejected,
matching TypeScript registration. Empty text remains an empty string; binary
inputs retain their bytes before version matching. The combined encoded version
schemas are capped at 32 MiB before decoding.

Versioned file/binary inputs accept the opaque OpenAPI schema
`{"type":"string","format":"binary"}` with descriptive annotations. Arbitrary
wire bytes, including empty and invalid UTF-8 inputs, are preserved. Native
byte-array schemas validate the actual bytes and their array constraints;
boolean schemas are also supported. Additional assertions on an opaque string
are rejected at registration instead of being ignored. This file profile matches
the TypeScript file validator's input acceptance; it does not reproduce arbitrary
Zod transformations or treat the separate base64-string binary validator as a
raw-byte validator.

URL-encoded version matching supports string fields, repeated arrays and nested
bracket paths, including sparse indexed arrays and object/array merges. It
follows the TypeScript parser's percent-decoding fallback, repeated-key grouping
and prototype-key handling. Parsing rejects more than 1,000 parameters or 32
input bracket levels; a 64-level bound also protects merged trees. Numeric form
indices use the exactly representable JavaScript integer range. Wire bytes remain
available to the handler; matching does not rewrite its body. Body decoding is
shared across candidate versions. `rust_versioned_form_parity.mjs` compares the
profile against the actual TypeScript content middleware. Configurable parser
options and additional legacy charsets remain incomplete.

Multipart version matching uses buffered wire bytes and shares `MultipartLimits`
with the typed Axum extractor, including caller-installed limit extensions.
Text fields become strings and file fields become byte arrays; repeated names
use the last part, and overwritten parts still count toward limits. The matcher
polls only ready memory and does not start an executor. Handlers retain the exact
original body. Native byte-array schemas and opaque-file annotations are supported, including
resource IDs, anchors and references stored in definitions or literal metadata. Opaque files accept binary and text parts, matching the TypeScript file
validator. The exact legacy `File` marker emitted for its instance guards is
recognized; other string assertions still fail registration. Source/export
schemas and literal values are preserved. Multipart assertions compile at their
original resource location, including evaluated-item tracking and mixed drafts.

Parser failures remain fatal when schema validation is disabled, including for
named body schemas without embedded JSON Schema. Form parameter and multipart
size limits return 413; malformed bodies and form nesting overflow return 400;
invalid multipart limits return 500. Streamed request-body limits also return
413, while ordinary body-read failures return 400 without exposing I/O details.

Local handlers read `FrameworkRequest.version`; Axum handlers can extract
`Extension<forklaunch_core::http::SelectedVersion>`. This contains `version` and
`matched_versions`. MCP exposes each version as `operation [v<version>]` when
there is more than one version, with its own input/output schema; calls still go
through normal HTTP selection and authentication. Calling an earlier tool with
an overlapping request can therefore select a later HTTP version, matching the
TypeScript implementation.

`framework-rs/parity/rust_versioned_api_parity.mjs` executes the actual TypeScript parser
and compares its selections with Rust. The standard OpenAPI document shows the last declared version and retains every
request/response in `x-forklaunch-version-contracts`. Use
`generate_versioned_openapi_json` for a named document collection with unversioned
routes under the empty key, matching TypeScript's exporter.

CLI contract/OpenAPI imports and both native SDK plans generate a typed method
for every version: `write` with `v1`/`v2` becomes `writeVersionV1`/`writeVersionV2`
in TypeScript and the corresponding snake-case Rust methods. Version labels
follow the existing generated identifier rules; ambiguous normalized names fail
explicitly. The methods send the declared request to the same HTTP path, where
schema matching determines the version. Run `rust_versioned_sdk_runtime.sh`
for both artifact formats, native generators and live HTTP/MCP checks.
Remaining non-JSON media/coercion cases are still tracked as incomplete.

Version matching follows the default TypeScript JSON/text wire parser: JSON roots
must be objects or arrays, an empty JSON body becomes an empty object, one UTF-8
BOM is stripped, and malformed UTF-8 becomes replacement characters. Original
handler bytes remain intact; standard Axum extractors retain their own decoding
rules. Additional legacy charsets remain separate work.

Versioned handlers can use `forklaunch_axum::VersionedBody<T>` to deserialize the
body already parsed and validated during version selection. It is a parts
extractor, so a subsequent `Bytes` extractor can still read the original wire
payload. JSON/text BOM handling, replacement characters, nested form objects,
multipart values and file byte arrays reach handlers without a second parser.
Local and metadata adapters expose the same shared value through
`request.version.parsed_body`. Missing version/body metadata rejects with 500;
a handler type incompatible with the selected schema rejects with 400. Arbitrary
schema transformations and unversioned parsed-body delivery remain separate work.

Versioned JSON/text/form parsing honors the request Content-Type charset. JSON
requires a `utf-` encoding; URL-encoded forms accept UTF-8 or ISO-8859-1, including
Latin-1 percent decoding. Text supports UTF-8, UTF-16/32 endian variants and
autodetection, Latin-1, ASCII, and encoding_rs legacy encodings. Unsupported
charsets reject with 415, including when schema rejection is disabled. The
comparison matrix covers Windows-1252, Shift-JIS, GB18030, Big5 and KOI8-R as well
as Unicode encodings; failed multibyte leads retry the following byte to match
iconv behavior. This does not claim every iconv alias/table or unpaired surrogate behavior.

Set `route.options.body_parser` to configure versioned parsing: JSON strict mode,
JSON/text/form default charsets, form extended mode, nesting depth and parameter
limit. Explicit request charsets override defaults. Non-strict JSON permits scalar
roots, while extended=false or depth=0 retains literal bracketed form keys. All
versions share these options. Registration rejects unsupported default charsets,
form depth above 32, or parameter limits outside 1..=100,000. Custom options are
included in contract artifacts; default options preserve the prior artifact
shape. These options currently apply to the versioned pipeline. Custom verify/reviver hooks remain separate work.

The `json_wire`, `text_wire`, `form_wire` and `raw_wire` options set a decompressed
byte limit and enable/disable inflation independently. Defaults match body-parser:
100 KiB and inflation enabled. Identity, gzip (including concatenated members),
zlib deflate and Brotli are supported. Unsupported or disabled encodings return
415, malformed compressed bodies return 400, and decoded limit violations return
413. Limits may be configured up to 32 MiB; zero permits only empty decoded bodies.
Decoding reads at most the limit plus one byte. Transport limits still bound
compressed bytes, and multipart retains its separate part/field/total limits.
Parsed values use decoded bytes while original request bytes remain available to
raw handlers; TypeScript body-parser's verify buffer contains decompressed bytes.

`form_charset_sentinel` recognizes the first `utf8=` field, switches percent
decoding for the standard UTF-8/Latin-1 sentinels, and removes that first field
from the parsed object. It still counts toward the parameter limit. With
`form_interpret_numeric_entities`, Latin-1 values decode decimal HTML entities
using JavaScript Number/UTF-16 code-unit behavior, including paired surrogates;
keys remain unchanged. Both options default to false. Unpaired surrogates retain
the documented native string representation limitation.

Native version bodies can attach `VersionBodyHooks::with_preprocess` before schema
validation and `with_transform` after successful validation. Register callbacks
with `VersionBodyHooksRegistry::insert(&route, version, hooks)` and install the
registry through `ForkLaunchRouter::with_version_body_hooks`,
`LocalRouter::register_with_body_hooks`, or `AxumApplication::version_body_hooks`.
Callbacks receive a JSON value and return `Result<Value, String>`. Each version
starts with the same original parsed input. A callback error rejects that version;
a later rejection preserves an earlier successful match. The selected transformed
value is available through `VersionedBody<T>` and `SelectedVersion::parsed_body`.
Raw request bytes remain intact. Callbacks execute synchronously and should not
perform blocking I/O.

Hook registries use the HTTP method and operation name, independently of mount
prefixes. Live and metadata adapters reject missing/ambiguous registrations;
nesting carries child hook registries and rejects duplicate hook operations.
For local handlers served through Axum, register hooks on the local router: it
owns selection and response validation, while Axum still enforces authentication
and its transport body limit. Callbacks execute once through this adapter.

Runtime callbacks are not serialized into contracts or SDKs. Export schemas that
describe accepted wire input and explicit response schemas; attach native
callbacks when constructing the server. This supports native equivalents of body
preprocessing/transformation, without interpreting JavaScript functions. Missing
body defaults and query/parameter/header transformations use the optional and
field hooks described below.


`BodyParserHooks::with_verify` adds verification for the shared JSON, text,
URL-encoded or raw/file parser. Register it with
`VersionBodyHooksRegistry::insert_parser(&route, hooks)` alongside version hooks.
The callback receives the original framework request, bounded decompressed bytes,
and the selected charset (`None` for raw/file bodies). It runs once before
character decoding and parsing, including for empty bodies. Encoding and size
checks run first. A verification error returns a fixed 403 without exposing the
callback's error text, even when schema validation is disabled. Multipart and
SSE are outside this body-parser verification API. The callback is read-only;
it does not expose Express's mutable request/response objects.


`BodyParserHooks::with_json_reviver` visits JSON children before their parent and
finally visits the root with key `""`. It runs once before per-version hooks.
Canonical uint32 object keys are visited numerically, then other keys in source
order; repeated JSON keys keep their original position and last value. Callbacks
return `Some(value)` to retain/replace a value and `None` to remove an object
property or the root. Empty decoded bodies bypass the reviver, matching
body-parser's special-case `{}` result. Callback failures return a fixed 400.

This native JSON callback does not expose mutable JavaScript holders or source
context. Sparse array removal returns 400 because `serde_json::Value` cannot
represent array holes; it is not silently converted to null. A removed root is
an absent parsed body, so callers must use an appropriate optional-body contract
and handler; `VersionedBody<T>` requires a present value.
`VersionBodyHooks::with_optional_preprocess` receives `None` for absence and
`Some(Value::Null)` for explicit null, allowing missing-value defaults before
schema checks. `with_optional_transform` can return `None` after successful
validation. These methods replace the corresponding required-value callback;
required-value callbacks continue to skip absent inputs. Each version sees its
own candidate value. `OptionalVersionedBody<T>` extracts the selected optional
result without consuming raw bytes; it still requires a selected version, and
present values must deserialize into `T`. Root deletion followed by defaults and
absent transform outputs work through HTTP/MCP. Sparse-array validation, mutable
holders/source context and remaining codec boundaries remain open.
Traversal is depth-bounded.


In a mixed application, `forklaunch sdk mode -t live` switches registered
TypeScript packages to source resolution. Rust services continue using generated
contract SDKs. `-t generated` restores the TypeScript declaration entry points.
Both modes preserve mixed-workspace Cargo, root TypeScript and editor task
configuration. Pure Rust applications require generated SDKs; declare a project's
`metadata.language = "typescript"` when attaching TypeScript source to a Rust
workspace. Existing TypeScript-only Node/Bun mode behavior is retained.


Versioned parameters, query fields and headers support `VersionFieldHooks`
(the same value callbacks as body hooks). Register a declared field with
`VersionBodyHooksRegistry::insert_field(&route, version, location, name, hooks)`;
locations are `PathParameter`, `Query` and `Header`. Optional preprocessors can
supply missing defaults before required/schema checks. Transforms run after
validation and can remove a successfully parsed field. A callback error rejects
that version; the last successful version wins. Unknown fields/versions and
unused or ambiguous route registrations are rejected during construction.

Read results through `SelectedVersion::parsed_fields` or Axum's
`VersionedParams<T>`, `VersionedQuery<T>` and `VersionedHeaders<T>`. Parameters
and query results contain declared fields. Headers follow the TypeScript parser:
only truthy transformed values replace existing wire keys; false, null, zero,
empty strings and absent outputs preserve the original value. Defaults do not
add absent header keys. Empty arrays/objects count as truthy.

Raw URI, query and header values remain available. Authentication uses the wire
headers before version callbacks. Path captures decode percent escapes exactly
once before validation; encoded slashes remain within the matched capture and
plus signs remain literal. Invalid escapes or UTF-8 produce 400 even when schema
validation is disabled. Callback execution is synchronous; errors omit callback
text. These hooks do not implement mutable JavaScript holders, sparse arrays or
cross-field callback side-effect ordering.

### Explicit PostgreSQL service generation

`forklaunch init service records --language rust -d postgresql` generates a
service-owned SQLx 0.8 repository, migrations, database configuration, protected
record schema, and `make migrate`. It uses the framework's restricted runtime
connection helper, transaction-local tenant context, forced row-level security,
and authenticated protected-field storage. Health checks query the repository.
The same build emits compliance metadata, OpenAPI, and both SDK languages.

Provision a separate database per service and distinct migration/runtime roles.
Set `MIGRATION_DATABASE_URL` for migration commands; runtime requires
`DATABASE_URL`, trusted `TENANT_ID`, stable `STORAGE_ENCRYPTION_NAMESPACE`, and
configured encryption keys. Remote connections require verified TLS unless the
explicit development transport override is enabled. The generated migration
README describes grants and key rotation. Compilation does not connect to the
database, and startup never performs migrations implicitly.

Omitting `-d` keeps the lightweight memory starter. Unsupported explicit database
choices fail before generation; existing memory services require deliberate
storage migration. This starter supplies a protected repository, not domain CRUD
HTTP endpoints or automatic production role provisioning. Its ignored integration
tests run against a migrated isolated database with the restricted runtime role.

### Test environment compatibility

`forklaunch_testing::setup_test_environment` builds the same environment values as
TypeScript `setupTestEnvironment` from already-started container endpoints. It
supports postgres/postgresql, mysql/mariadb, mongodb/mongo, mssql and sqlite/libsql,
plus Redis, Kafka and S3. SQLite selects `:memory:` without requiring a container.
Mapped ports are explicit; missing required mappings return an error. Test-only
credentials, telemetry, HMAC and service defaults match the TypeScript helper.
Custom variables override defaults, including empty strings. Unrelated baseline
variables remain; the helper does not clear stale database/infrastructure keys.

```rust
use forklaunch_testing::{TestEnvConfig, TestService};

let service = TestService::new("accounts")
    .with_env("PORT", "0")
    .with_test_environment(&TestEnvConfig::default())?;
let mut command = std::process::Command::new("./accounts");
command.envs(&service.env);
# Ok::<(), forklaunch_testing::TestEnvironmentError>(())
```

The native helper returns an owned environment map for a command/configuration
loader; it does not mutate the process-global environment. `TestContainerEndpoint`
only describes a host and mapped ports, with no container ownership or cleanup.
`TEST_TOKENS` supplies the same deliberately invalid mock Bearer/HMAC values as
TypeScript and cannot authenticate real requests. Container management, ORM setup,
database clearing and the complete blueprint test harness remain separate work.

### Typed Redis inspection and TypeScript scalar values

`JsonCache::peek_record` and `peek_batch_records` check existence without JSON
parsing or decryption. The Redis backend uses one MULTI/EXEC transaction for a
batch and preserves input order, duplicates and misses. Lists, corrupt ciphertext
and non-JSON strings still count as existing keys. `delete_batch_records` removes
keys without decoding and returns the number deleted; Redis validates every key
before the single DEL command. Default backend implementations may process keys
sequentially.

`list_keys_by_prefix` escapes glob characters; `list_keys_matching` accepts a Redis
glob, so TypeScript `listKeys(prefix)` maps to `list_keys_matching` with `*`
appended. Both return sorted unique SCAN results; concurrent changes can affect
which keys are observed. `peek_records_by_prefix` retains key/result pairs,
including a false result when a scanned key disappears before the existence
transaction. `backend`, `backend_mut`, `RedisTtlCache::config` and
`connection_mut` expose configuration and native client access. Raw commands
bypass the outer key/TTL/JSON/encryption wrappers.

`JsonRecordCodec` writes tenant-bound cache, queue and object values with fresh
random nonces in the TypeScript-readable v4 field envelope. Repeated values do
not produce identical ciphertext. Authenticated legacy v1/v2/v3 readers and key
rotation remain supported. These field envelopes are a separate protocol from
the worker channel v3 envelope; their version numbers are not interchangeable.
Passing no tenant still selects the explicitly unencrypted JSON path.

`JsonRecordCodec::with_typescript_safe_values()` explicitly enables TypeScript's
`safeStringify`/`safeParse` behavior for JSON-compatible values. It writes top-level
strings verbatim and treats non-JSON plaintext as a string. Strings containing
valid JSON change type on read: `"false"` becomes a boolean, and `"[1]"` becomes an
array. A requested Rust type may reject that result. Strict JSON remains the
codec default. Both profiles reject invalid UTF-8 and enforce authentication
before parsing encrypted values; missing tenant context, wrong keys and tampering
never become plaintext fallbacks. JavaScript class/BigInt serialization, number
rounding remain separate boundaries.

`JsonQueue::peek_records_typescript` and `dequeue_batch_records_typescript`
explicitly apply TypeScript's `.filter(Boolean)` after decoding. They omit null,
false, numeric zero and empty strings while retaining empty arrays and objects.
Counts apply before filtering. A zero dequeue count removes nothing; a zero peek
count reads the entire current queue, matching TypeScript's LRANGE behavior.
Ordinary native methods continue preserving falsy values and return empty batches
for zero counts. Both profiles authenticate before filtering, retain FIFO order
and propagate decoding errors. Dequeue failures do not requeue removed messages.
Select `with_typescript_safe_values` on the codec as well when TS producers write
unquoted string values. These typed APIs accept nonnegative integer counts.

The optional `zod-uri` feature on validation (or its Axum feature forwarder) adds
`ZodStringFormat::Uri`. It matches the WHATWG URL admission used by Node 24.2.0 /
Ada 3.2.4 and retains the original input string. Enable it for explicit Zod URL
checks; it does not redefine RFC JSON Schema `format: uri`. The pinned Ada dependency
requires a C++20 compiler; default builds exclude it. The
[URI differential fixture](../framework-rs/parity/rust_zod_uri_parity.mjs) checks 205,676
inputs plus ordered errors and composition against the actual TypeScript factory,
with pinned source hashes.

The marked TypeScript URI factory, with its single URL check and optional/nullable
wrappers, now exports `format: forklaunch-whatwg-url`
without the conflicting RFC regex. UniversalSdk and generated TypeScript sockets
register that format with the JavaScript URL parser. Native contract, HTTP, MCP,
WebSocket and SDK generator crates forward the optional `zod-uri` feature; without
it, compiling a schema containing this reserved format fails explicitly. Arbitrary
RFC `uri` schemas and explicit user overrides keep their existing meaning.

For CLI socket generation, set `FORKLAUNCH_SDK_ZOD_URI=1` to enable the pinned
parser in its compiler subprocess. Unset or `0` leaves the feature disabled;
other values fail. Watch/sync inherit the selection. Generated Rust socket crates
expose their own `zod-uri` feature, which consumers must enable. C++20 is required.
This profile targets Node24.2/Ada3.2.4; other JS engines or parser versions may
admit different Unicode hostnames. HTTP SDK clients retain their existing typed
transport behavior. The [schema fixture](../framework-rs/parity/rust_zod_uri_openapi.mjs)
checks 203,261 native/AJV admissions; the
[SDK fixture](../framework-rs/parity/rust_zod_uri_sdk.mjs) checks both socket clients.

TypeScript OpenAPI export preserves the strongest finite numeric min/max bound,
including exclusive ties, and the conjunction of exact/min/max array lengths.
Explicit `.openapi` overrides remain authoritative. The
[bounds fixture](../framework-rs/parity/rust_zod_bounds_openapi.mjs) compares 153,972 runtime
and schema admissions for OpenAPI 3.0 and 3.1. Nonfinite registrations, multipleOf,
UTF-16 string length and tuple schema semantics remain separate boundaries.

Ordinary Zod union exports use `anyOf`, preserving overlapping alternatives.
Finite homogeneous literal enums deduplicate values; mixed enums retain alternative
types and empty unions export an always-false schema. Explicit composition annotations
and discriminated unions retain existing behavior. The
[union fixture](../framework-rs/parity/rust_zod_union_openapi.mjs) checks 10,674 cases.
Nonfinite enum and OAS3.0 nullable-union emission remain separate gaps;
empty-union validation does not imply a generated precise `never` type.

Zod Never exports an always-false schema, including required object fields.
Optional Never properties may be omitted but reject every supplied JSON value.
The [Never fixture](../framework-rs/parity/rust_zod_never_openapi.mjs) checks both OpenAPI
versions. Generated exact static `never`/uninhabited types remain separate.

Finite structural string-key records preserve key restrictions: OAS3.1 uses
`propertyNames`, and OAS3.0 uses optional named properties with additional keys
rejected. Safe schema merging preserves own reserved property names without
prototype mutation. UniversalSdk's owned AJV engines validate own properties;
injected engines retain caller options. OAS3.0 AJV still filters `__proto__` schema
properties, an explicit consumer limitation. Transform/regex/UTF-16 key grammars
remain separate. See the [record fixture](../framework-rs/parity/rust_zod_record_openapi.mjs).

`ZodJsonSchema::Tuple { items, rest }` validates ordered dense JSON arrays.
Every declared slot counts toward minimum length, including optional/default
schemas. Short arrays abort before child evaluation; surplus values without rest
produce a length error followed by declared child errors. Rest validates trailing
values under shared depth/work/output budgets. Sparse/undefined JS elements remain
outside this JSON profile. The [tuple oracle](../framework-rs/parity/rust_zod_tuple_json_parity.mjs)
checks 19,050 cases in each serde mode; the live composition fixture covers HTTP,
MCP and both SDK transports. This does not add positional schema emission or
precise generated tuple models; those require coordinated dialect/SDK changes.


SDK validation preserves own schema properties such as `__proto__` through a
private bounded AJV compilation view, including exact pattern constraints and
resource-local references. Caller-injected engines retain their options and custom
keywords. A local reference into literal `default`, `const` or `enum` storage that
requires own-property repair fails explicitly: changing that storage could change
default data or equality. Public schema documents remain unchanged.


Universal SDK response validation accepts explicit JSON Schema2020 via a response
schema `$schema` or document `jsonSchemaDialect`. Owned engines select the matching
validator; caller factories receive `ResponseSchemaContext`. Injected engine options
and keywords remain intact, and mismatched/mixed resource dialects reject explicitly.
Unmarked OpenAPI3.1 documents retain legacy draft7 behavior. Positional tuple slots
coerce dates/binary through legacy `items[]` or explicit2020 `prefixItems`, including
references and rest items. This consumer support does not add tuple schema emission
or precise generated Rust tuple models.


The Zod adapter's optional `openapi2020(schema)` capability emits an explicit2020
input schema, including positional tuples with fixed or typed rest items. Existing
`openapi` and TypeBox defaults remain unchanged. Direct `generateSchema` callers
may pass fourth options `{ jsonSchemaDialect: 'https://json-schema.org/draft/2020-12/schema' }`
with OpenAPI3.1. Conflicting dialect metadata/OAS3.0 reject. Root input callbacks
are skipped by the adapter; nested callback/default export behavior retains its
existing limitations. Precise generated Rust tuple models remain separate work.


Generated Rust models now represent exact fixed-length JSON tuples as named
sequence structs with typed `item0`, `item1`, and subsequent fields. Custom Serde
requires every position and rejects extras, including correct `[]` handling for
empty tuples. Required recursive positions retain boxed edges without recursive
Default derivation. Shared CLI/native generation supports arities beyond Serde's
native tuple limit; tests include128 positions. Variable prefixes and typed rest
items use presence-aware sequence models: optional positions preserve absence
separately from JSON null, and serialization checks contiguity/length before writing.
Homogeneous arrays retain their existing Vec models; other schema constraints remain
the runtime codec’s responsibility.


TypeScript `openapi2020(schema, {pureInput:true})` opts into bounded structural
input export without evaluating Zod default factories or predicates. Unsupported
nodes and coercions reject; computed defaults are omitted. This supported profile
covers ordinary constructed schemas, preserves declared Date wire representation,
and does not sandbox arbitrary getters, Proxies or shape thunks. NativeEnum supports normalized finite string/number values, including numeric
reverse maps and mixed enums; non-JSON effective values reject. The default exporter remains unchanged.

Pure input also rejects built-in string trim/case transforms: ordered constraints
and Unicode expansion require a separate input projection. Default exports retain
their compatibility behavior.

The static TypeScript CORS profile accepts explicit preflight final statuses
200–599. Express sends zero Content-Length on204/304; Hyper omits it. Tests
assert this wire difference explicitly. Dynamic origin delegates remain separate.


### Explicit Zod constraint profile

TypeScript schema export can opt into `openapi2020(schema, { zodChecks: true })`
for exact UTF-16 string lengths and ordered Zod numeric remainder checks. Enable
`zod-checks` on native contract/HTTP/MCP/socket or SDK crates that consume these
schemas; socket CLI generation uses `FORKLAUNCH_SDK_ZOD_CHECKS=1`. Unsupported
native consumers reject the reserved `x-forklaunch-zod-checks` capability.
UniversalSdk and generated TypeScript socket clients register its AJV handler.
Separately bundled helper instances need separate AJV engines.

Ordinary explicit schema annotations keep their override semantics. This option
does not imply general Zod schema equivalence: regex uses the separate opt-in
profile below, native strings exclude lone UTF-16 surrogates, and transformations combined
with synthesized length checks are rejected. Default schema export is unchanged.


The explicit Zod-check profile also preserves ordered email/UUID/URI conjunctions.
URI assertions additionally require `zod-uri`; native compilation rejects URI
demand without that feature. Email/UUID use shared Zod predicates. Compatibility
fixtures pin Node 24.2.0 and Ada 3.2.4. Additional formats and the separate regex
profile are described below; explicit ordinary annotations retain precedence.


The same opt-in profile supports CUID, CUID2, ULID, Nano ID, Base64 and Base64url
using Zod's syntax rules, including noncanonical Base64 pad bits and optional
URL-safe padding. These assertions require `zod-checks` but do not require
`zod-uri`. They preserve ordered conjunctions and explicit format overrides.
Date/time, IP-related and other raw Zod formats remain under parity development.

The explicit `zodChecks` profile also preserves raw Zod `.date()` assertions: exact
four-digit Gregorian dates, including year zero and century leap-year rules. This
is string validation; date/time coercion and other time formats remain independent.

Raw Zod `.duration()` also survives explicit `zodChecks` export: signed ordered
components and terminal dot/comma fractions retain lexical acceptance. Native
validation scans bytes without numeric conversion or regex backtracking. No
normalization, elapsed-time arithmetic or general ISO duration claim is implied.

Raw `.time()` assertions use the same explicit profile with `timePrecisions`:
null permits optional seconds/fractions, zero permits optional integer seconds,
and positive integer precision (1..1000) requires exact fractional digits. Positive
precision deliberately preserves Zod's repeated seconds-group syntax. Negative,
fractional or larger precision configurations fail admission. No timezone conversion
or time normalization occurs.

Raw `.datetime()` options use ordered `dateTimes` rules in the explicit profile:
null or 0..1000 precision plus boolean `local`/`offset`. Gregorian date and time
checks preserve Zod syntax, including lexical two-digit timezone fields (even
`+99:99`), optional colon, and local timestamps when enabled. This does not parse
an instant or normalize offsets. Ordinary date-time export remains unchanged.

Raw `.ip()`/`.cidr()` assertions preserve explicit v4/v6/default selection through
the same profile. Native predicates reproduce Zod's legacy mixed IPv4/IPv6 and
zone syntax, including its lowercase mapped-prefix and two-digit embedded-octet
quirks. CIDR prefix widths are lexical/range checks; host bits are not normalized.
This validation does not establish routability, address ownership or network policy.

Raw `.emoji()` uses an 84-range Unicode16 union of Extended_Pictographic and
Emoji_Component in native and both TypeScript helpers, independent of host Unicode
tables. Individual components such as digits and joiners intentionally pass; this
is Zod property membership, not grapheme-sequence validation. Source data, Unicode
License V3 and reproducible generator are retained; the oracle pins Node24.2.

Raw `.jwt()` assertions preserve optional algorithm matching through ordered
`jwtAlgorithms` rules. Native header parsing matches JavaScript atob Latin1, JSON
escaped UTF16 units, duplicate keys and numeric truthiness; deep containers use an
iterative parser. This checks Zod structure/header policy only, including optional
empty signature and unparsed payload. It performs no signature or claim verification.


### Explicit Zod regex profile

Export raw Zod regex assertions with
`validator.openapi2020(schema, { zodRegex: true })`. This option is independent of
`zodChecks` and leaves default export unchanged. The reserved version1
`x-forklaunch-zod-regex` keyword contains ordered rules with exact UTF-16 source
units and flags. Stateful `g` and `y` checks start at offset zero for each
validation, matching Zod; malformed flags and unsupported profiles are rejected.

Enable `zod-regex` on the native contract, HTTP, Axum, MCP, WebSocket, validation
or SDK crate consuming the schema. Socket CLI generation accepts
`FORKLAUNCH_SDK_ZOD_REGEX=1` and forwards the capability to generated Rust clients.
Demand without the feature fails before generated output. Native callers use
`schema_validation::compile_checked` or `compile_multipart_checked`; the older
string-error compiler rejects regex demand even when the feature is enabled.
`compile_with_regex_profile` accepts explicit native quotas. Literal data containing
the reserved keyword does not itself demand this capability.

Checked native validation distinguishes ordinary invalid input from operational
resource exhaustion. Exhaustion in an evaluated regex stays operational across
negation, conditions and alternative branches. Checked HTTP version-schema and transform validation treats it as a safe500 in
all validation modes, MCP returns an internal error, and WebSocket codecs stop with
an operational payload error. Unvisited branches do not consume regex work.
Native limits bound source size, compilation storage/work and matching work;
aggregate allowances are debited before each rule. These are logical requested
storage/work limits, not a wall-clock deadline or a process-memory guarantee.

TypeScript execution requires its own explicit host policy:

```ts
const sdk = await UniversalSdk.create(host, registryOptions, undefined, 'host-native');
// Generated socket client options:
const options = { zodRegexExecution: 'host-native' as const };
```

Direct UniversalSdk construction accepts the policy after responseAjvFactory.
Owned response engines and generated socket codecs propagate it; an injected AJV
instance cannot implicitly enable the reserved keyword. Conflicting custom
handlers are rejected. Reusing an engine with the same helper and policy works;
separately emitted helper implementations require separate engines.
Host-native JavaScript RegExp executes synchronously and
cannot be interrupted by the native budgets, so select this policy only when that
execution model is acceptable. The compatibility oracle pins Node24.2/Unicode16;
other JavaScript engines may support different syntax or Unicode versions.
Native Rust strings cannot represent lone UTF-16 surrogates even though the
underlying kernel's explicit UTF-16 API can test them.

Validator package ownership: `forklaunch-validator` depends on common diagnostic
DTOs, not core HTTP or route contracts. Use `forklaunch_core::http::FrameworkSchema`
(with core's `validator` feature) for the `schema_ref()` contract adapter and
`forklaunch_core::http::zod_provenance::register_zod_version_parsers` for bounded,
atomic HTTP version-parser registration. Pure validators export JSON Schema and
validate values without linking the HTTP runtime.

WebSocket ownership follows the TypeScript project split: `core::ws` owns socket
contracts, artifacts, typed descriptors and bounded payload/schema validation;
`forklaunch-ws` owns connection and session transport and reexports those shared
types. Host-side AsyncAPI and SDK generation depend on core's optional `ws`
feature without importing the WebSocket transport runtime. Core's `ws-validation`
feature adds pure validator-backed payload checks; `schema-validation` enables
bounded JSON Schema evaluation and `typed-schema` enables typed descriptors.

### Typed URL forms

`FormBody<T>` decodes nested bracket fields and indexed arrays with the shared bounded form parser. The TypeScript client accepts typed object values; the Rust client generates a request DTO and encodes it through the universal SDK. String values remain strings, while native destination types request numeric/boolean coercion. Both encoders cap nesting at 32, parameters at 1,000, and encoded size at 100 KiB. Null and empty collections have no lossless default form representation and are rejected; use JSON for these shapes. Date codecs remain JSON-only for requests. Existing explicitly encoded compatibility methods remain available.
