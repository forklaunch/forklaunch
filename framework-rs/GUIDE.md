# framework-rs guide

Framework development and operations reference for the current source workspace.

## Crate catalog

The source links expose the public API. Build the searchable Rust API reference
with the commands below. Feature names come from each crate's `Cargo.toml`; enabling
a feature makes its API available but does not configure credentials or start a service.

| Crate | Responsibility | Optional features and defaults |
| --- | --- | --- |
| [forklaunch-common](crates/common/src/lib.rs) | Path, naming, UTF-16 string hash, JSON formatting and error utilities. | —; default: none |
| [forklaunch-core](crates/core/src/lib.rs) | Contracts/schema macros, HTTP/auth, OpenAPI/MCP, encryption, persistence, telemetry and optional SDK/build compilers. | `json-schema`, `typed-schema`, `schema-validation`, `http`, `auth`, `openapi`, `mcp`, `rmcp-server`, `build`, `asyncapi-import`; default: none |
| [forklaunch-core-macros](crates/core-macros/src/lib.rs) | Internal procedural-macro implementation for core contracts and compliance. Consume macros through core. | — |
| [forklaunch-validator](crates/validator/src/lib.rs) | Serde/garde and JSON Schema validation with bounded diagnostics. | —; default: none |
| [forklaunch-axum](crates/axum/src/lib.rs) | HTTP routing, middleware, docs, authenticated sessions, MCP and optional media adapters. | `cookie-session`, `multipart`, `sse`, `websocket`, `mcp`, `remote-jwks`, `adapter`; default: `adapter` |
| [core::asyncapi](crates/core/src/asyncapi/mod.rs) | AsyncAPI documents and optional external-document import. | core `asyncapi-import`; default: none |
| [core::mcp](crates/core/src/mcp/mod.rs) | Contract-to-tool conversion and optional Streamable HTTP server. | core `mcp` / `rmcp-server`; default: none |
| [core::codegen](crates/core/src/codegen/mod.rs) | Shared bounded schema/model compiler and SDK generation types. | —; default: none |
| [core::rust_sdk](crates/core/src/rust_sdk/mod.rs) | Rust HTTP/stream/socket client generation. | —; default: none |
| [core::typescript](crates/core/src/typescript/mod.rs) | TypeScript HTTP/stream/socket client generation. | —; default: none |
| [core::ws](crates/core/src/ws/mod.rs) | Socket contracts, artifacts, typed descriptors and bounded payload codecs. | core `ws`, `typed-schema`, `schema-validation`, `ws-validation` |
| [forklaunch-universal-sdk](crates/universal-sdk/src/lib.rs) | Shared Rust HTTP client transport, checked responses, SSE and multipart runtime used by generated clients. | Native runtime; source extraction regression checks pending |
| [forklaunch-ws](crates/ws/src/lib.rs) | Native connection/session transport using core-owned contracts and codecs. | `transport`; forwards schema/validation features; default: none |
| [forklaunch-infrastructure-redis](crates/redis/src/lib.rs) | Cache interfaces and optional Redis/encryption adapters. | `redis-client`, `encryption`; default: none |
| [forklaunch-infrastructure-s3](crates/s3/src/lib.rs) | Object storage interfaces and optional AWS/encryption adapters. | `encryption`, `aws-sdk`; default: none |
| [forklaunch-testing](crates/testing/src/lib.rs) | Contract/test utilities and optional performance measurements. | `benchmarks`; default: none |

## Start with a generated application

Use the [backend generation guide](../docs/adding-projects/rust.md#create-and-run-a-service)
for the CLI commands and environment setup. Generated applications carry source
under `.forklaunch/framework-rs`; edit application code under `crates/<service>`.
A source checkout can use path dependencies directly. Registry publication and
installation are separate open release gates; the local version `0.1.0` is not
proof of a published artifact.

Define each operation once with `RouteContract` and register it with
`ForkLaunchRouter`. Contracts carry paths, methods, request schemas, declared
status/body types and authorization metadata. Register the same routes in the
exported `ContractRegistry`: code generation consumes that registry and does not
infer unregistered Axum handlers. The generated `src/routes/mod.rs` is the
application extension point. The [canonical service example](../blueprint-rs/crates/implementations/iam/iam-base/examples/canonical_iam.rs)
shows authenticated routing with request validation and contract metadata.

Use internal mounts for application-owned raw protocols (for example, Better
Auth): `nest_internal` reserves a prefix; `merge_internal` requires declared
namespaces. They retain shared middleware and cannot overlap contract/docs/MCP
namespaces. They deliberately have no generated operation in application SDKs.

## HTTP, MCP and media

The [listener guide](../docs/adding-projects/rust.md#http-and-mcp-startup) documents
shared, separate and disabled MCP listeners and shutdown. MCP discovery uses the
registered operation set. Authentication and authorization still apply when a
tool invokes an operation. Browser handoffs, raw auth APIs and worker control
listeners have separate surfaces; do not assume every HTTP route is an MCP tool.

Enable only needed Axum features. See the runnable API examples and policy in
[the README](README.md) for typed `Multipart<T>`, SSE emitters and clients, typed
WebSockets, and cookie sessions. JSON/text/binary/form bodies and media-specific
SDK response decoding are supported. Request/response limits and stream
cancellation are part of the protocol: preserve them when adding custom raw
handlers. The remaining request-stream/media conformance gate is tracked in
[the implementation checklist](../plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md).

HTTP and authentication are modules of the core project, matching TypeScript core ownership. Enable `forklaunch-core` feature `http` for `forklaunch_core::http`, or `auth` (which includes HTTP) for `forklaunch_core::auth`. Session and OIDC/JWKS network clients remain opt-in. The former HTTP/auth standalone crates are removed.

`forklaunch-core::http` separates transport data from Axum. `forklaunch-validator`
validates schema/model inputs and produces bounded diagnostics; schema metadata
alone does not validate application-owned internal handlers. HTTP errors use a
structured native envelope and correlation ID. Declared business errors retain
their declared status and JSON/text/binary shape in generated clients. A 2xx
response, a declared business failure, and a transport/decode failure are
separate SDK outcomes; use the generated status discriminant before reading the
response. Full TypeScript error-envelope equivalence remains an explicit gate.

## Authentication and session ownership

Keep verification policy server-owned. JWT algorithms, issuer, audience and
trusted JWKS location belong to application configuration. Remote JWKS does not
trust key URLs from a token. The [JWT section](README.md#jwt-authentication)
shows native issuance/verification and the actual-JOSE conformance boundary.
Roles/permissions/scopes come from verified identity and trusted store lookup,
not unsigned request fields. `AuthConfig` declares a requirement; configure the
router's authenticator to implement it.

Request HMAC covers the exact method, path and serialized body using the
ForkLaunch format. Timestamp verification and a business nonce/receipt serve
different purposes: HMAC verification does not itself make a financial or relay
operation idempotent. Route-specific HMAC key policies can replace the default
policy for one method/path without granting those keys access elsewhere.

Generic cookie sessions use the optional `cookie-session` adapter and explicit
CSRF/origin policy described in [the README](README.md#optional-browser-sessions).
Better Auth uses its own cookie/session adapter in the blueprint workspace.
Applications choose production cookie security, storage and revocation policy.

## Infrastructure and operations

Read settings with [`forklaunch_core::config`](../docs/adding-projects/rust.md#configuration):
`config::required::<T>("NAME")?` fails startup with an error naming a missing or
malformed variable, and `config::optional` is reserved for settings whose absence
is a documented choice. Configuration values never have in-code defaults.

[PostgreSQL](../docs/adding-projects/rust.md#postgresql-iam),
[Redis](../docs/adding-projects/rust.md#redis-adapter),
[S3](../docs/adding-projects/rust.md#async-s3-object-storage),
[field encryption](../docs/adding-projects/rust.md#field-encryption-compatibility)
and [telemetry](../docs/adding-projects/rust.md#telemetry) have concrete environment
and runtime examples. A trusted tenant context scopes adapter operations; run
application connections without superuser/RLS-bypass permissions. Encryption,
compliance classification, audited disclosure and retention are separate
capabilities. The compliance derive supplies metadata, not automatic storage
protection or erasure. Implement atomic tenant-scoped retention batches as
specified in the README and explicit store traits.

Start OTLP providers in the application lifecycle and flush them during shutdown.
Avoid logging credentials, provider tokens, complete cookies or classified data.
For a failed operation, retain its correlation ID and sanitized error category;
check environment, migration state, tenant scope and authorization independently.
Use the exact reviewed dependency exceptions in the
[security review](../plan/RUST-SECURITY-REVIEW.md), not blanket audit suppression.

## Contracts, clients and upgrades

`forklaunch_common` provides the twelve string-input naming/hash helpers exported
by TypeScript's common package, including transformation diagnostics. Camel-case
conversion uses the exact separator set, removes other punctuation, strips leading
numbers, and falls back to the original input when conversion is empty. This also
corrects generated operation names: `foo!bar` becomes `foobar`, and `123alpha`
becomes `alpha`. Regenerate clients when names use these edge cases.
`capitalize`/`uncapitalize` follow JavaScript's first UTF-16 code-unit behavior;
`hash_string` uses wrapping signed 32-bit arithmetic over UTF-16 units and must not
be used for security decisions. Inputs are Rust UTF-8 strings; JavaScript non-string
coercion and isolated surrogate code units are outside this typed boundary.

The public `remove_trailing_slash`, `remove_double_leading_slash` and
`sanitize_path_slashes` helpers match TypeScript's single-step replacements.
For example, `/` becomes empty after trailing-slash removal, `///` loses only
one leading slash, and a missing leading slash is not inserted. Native contract,
request and router normalization uses `normalize_route_path` instead: it preserves
root `/`, adds missing leading slashes and removes repeated boundary slashes.
Callers that relied on the earlier Rust helpers' full normalization should use
`normalize_route_path`.

`open_api_compliant_path` now matches TypeScript's global ASCII-word replacement:
`/files/:name.pdf` becomes `/files/{name}.pdf`, and `/prefix:id` becomes
`/prefix{id}`. Unicode outside the ASCII parameter name remains literal. Native
contracts and route decorators use `route_open_api_path` to preserve their
whole-segment matching behavior. Use that function if you relied on the former
Rust helper's output. Converting an embedded parameter to an OpenAPI string does
not add embedded-parameter matching to the native router.


[Export and SDK commands](../docs/adding-projects/rust.md#export-contracts-and-sdks)
cover contract/OpenAPI imports, both client languages and mixed applications.
The shared compiler handles recursive schemas with bounded graph processing;
unsupported references fail rather than silently widening a type. Bundled static
resource IDs and scoped anchors are supported by the HTTP SDK compiler; see the
[resource profile](README.md#bundled-schema-resource-ids) for explicit boundaries.
Dynamic references, remaining schema constraints, versioned dispatch and richer
external AsyncAPI imports are still open compatibility work. Version metadata
must not be interpreted as implemented runtime version selection.

Review generated differences using the [compatibility baseline](../docs/adding-projects/rust-compatibility.md).
Keep generated SDKs reproducible, preserve application-owned code during sync,
and regenerate artifacts after contract changes. Source archives and local
containers have fixtures; registry publishing and native ECS planning remain
separate release gates.

## Build API docs and verify changes

From the repository root:

```sh
CARGO_INCREMENTAL=0 RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path framework-rs/Cargo.toml --workspace --all-features --no-deps
cargo test --manifest-path framework-rs/Cargo.toml --workspace --all-features
cargo clippy --manifest-path framework-rs/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

Open `framework-rs/target/doc/forklaunch_axum/index.html` to browse the generated
reference; the crate selector links every workspace API. Infrastructure tests
that require credentials or containers are documented by their fixture scripts;
an ignored test is not evidence that an integration passed. The
[validation guide](../docs/adding-projects/rust.md#validate-the-port) lists end-to-end
commands and [benchmarks](../docs/adding-projects/rust-benchmarks.md) explain what
the performance reports measure. Use task-owned services and scratch directories
for integration tests and preserve unrelated running applications.
