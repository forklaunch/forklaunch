
Finite endpoints can bind request metadata and multiple successful representations
without repeating a schema in the router:

```rust,ignore
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Search { limit: u32, tags: Vec<String> }
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct Headers { #[serde(rename = "x-revision")] revision: u64 }
#[derive(contract::ForklaunchResponses)]
enum SearchResponse {
    #[response(status = 200)] Found(Vec<Note>),
    #[response(status = 202, content_type = Text)] Pending(String),
    #[response(status = 204)] Empty,
}
type SearchEndpoint = contract::endpoint::Endpoint<
    contract::endpoint::Fields<contract::endpoint::NoBody, Search, Headers>,
    contract::endpoint::Responses<SearchResponse>,
>;
let endpoint = SearchEndpoint::responses("search", "Search notes", HttpMethod::Get, "/notes")?;
// bind_authorized receives request.body, request.query.limit, request.headers.revision.
```

The same field schemas are published to OpenAPI and both SDKs. Native decoding
validates required fields and constraints after authorization. Strings remain
strings, typed scalar fields decode from their wire text, and arrays accept either
repeated query keys or JSON arrays. Structured fields use JSON wire values. Duplicate
scalar values fail validation. Request field structs must have fixed object shapes;
cross-field JSON Schema constraints are rejected during descriptor construction.
Use `serde(deny_unknown_fields)` to reject undeclared query parameters. Transport
headers remain available to authentication middleware and only declared headers
are passed into the typed handler value.

Each response enum variant owns a unique successful status/content-type pair and its JSON, text,
HTML, binary, or empty representation. The adapter verifies the selected body's
schema before emitting bytes. A single status may offer JSON, text, and HTML variants; generated clients discriminate by both status and Content-Type. Streaming remains a dedicated binding. Rust SDK
operations with declared query/header fields expose typed structs and return a
serialization result when constructing transport requests; raw transport hooks
remain explicit escape hatches.

Path parameters use the fourth `Fields` type argument and arrive as `request.path`.
For example, `Fields<NoBody, NoFields, NoFields, NotePath>` derives path schemas
from `NotePath`; its required field names must exactly match the route placeholders.
Numeric and boolean parameters are parsed and validated before handler invocation.
Both generated SDKs use these same parameter types.

`MediaType` selects a MIME override while preserving the Rust codec:

```rust,ignore
let export = Endpoint::<NoBody, TextBody>::new("export", "Export notes", HttpMethod::Get, "/export", 200)?
    .with_response_media(MediaType::Csv)?;
// Handler returns String; generated response contentType is text/csv.
#[derive(contract::ForklaunchResponses)]
enum Export {
    #[response(status = 200, content_type = Text, media_type = Csv)] Csv(String),
    #[response(status = 200, content_type = Binary, media_type = Pdf)] Pdf(Vec<u8>),
}
```

Text overrides include XML, CSS, JavaScript, CSV, Markdown, RTF and YAML. Binary
media include PDF, Office documents, ZIP, images, audio and video. An explicit
`MediaType::Custom { representation, value }` supports application MIME types on
single-response/request descriptors. Invalid MIME values and codec changes fail
at construction. Multipart remains `multipart/form-data`; SSE remains
`text/event-stream`, matching their current wire processors.

Schema-selected versions use the same declarations for builds and handlers. Create
`VersionedEndpoint::new("v1", &v1)?.with_version("v2", &v2)?`, register its
`contract()`, and bind each typed descriptor through
`VersionedHandlers::new(versions).bind("v1", v1, handler_v1)?.bind("v2", v2, handler_v2)?`.
`ContractRouter::bind_versioned_authorized(handlers, policy)` verifies shared
security metadata and every version binding before serving. The shared policy
(including exact-wire body verification) runs once before schema matching. The
last matching declaration wins, including JavaScript numeric-key ordering;
versioned DTOs do not require an invented version header. Handlers receive
`State<S>`, the policy context, and their version's typed request value.

For versioned SSE, use `VersionedSseHandlers::new(versions, stream_options)` with
`.bind(version, endpoint, producer)` or `.bind_with_headers(...)`, then
`bind_versioned_sse_authorized(handlers, policy)`. Producers receive the same typed
input and `StreamContext` as ordinary SSE. Selection shares the finite dispatcher;
initial/body authorization runs once and stream authorization refreshes periodically
and before frames. Headers and each payload may satisfy independent matched-version
alternatives. Invalid frames close the stream before exposure. No version is selected
solely by a caller-supplied version label.

A finite endpoint can declare dynamic response headers with
`.with_typed_response_headers::<Headers>()?`. Its handler returns
`ResponseWithHeaders { body, headers }`. Header structs derive the ordinary
Forklaunch schema/serialization traits; optional members may be omitted. Header
schemas are published alongside each output representation and enforced before
sending. The generated TypeScript envelope exposes typed `headers`; generated
Rust variants with headers contain `SdkResponseWithHeaders { body, headers }`.
Transport-managed headers such as Content-Type and Set-Cookie remain reserved.

Response enums also accept custom MIME literals while retaining their typed codec:
`#[response(status = 200, content_type = Json, media_type = "application/vnd.example.note+json")]`.
Standard MIME overrides continue to use named `MediaType` variants such as `Csv`
or `Pdf`; malformed custom MIME values fail the derive.

With `native-streams`, register `Endpoint<I, SseBody<Event>>` using
`bind_sse_authorized(endpoint, policy, options, handler)`. The handler takes
`State<S>`, `StreamContext<Policy::Context>`, and its typed request, and returns a
stream of `Result<ServerEvent<Event>, EndpointError>`. `context.refresh()` refreshes
identity before storage work; `context.operation(work)` bounds that work. The
framework also independently checks authorization while idle and before every
outbound event. SSE descriptors are excluded from finite MCP tools automatically.

For WebSockets, define a `forklaunch_ws::TypedSocket<Request, Incoming, Outgoing>`
from a GET handshake contract and register its `contract()` under a socket name.
`bind_socket_authorized(name, descriptor, policy, options, handler)` checks that
registration exactly. Handlers receive `State<S>`, initial policy context, typed
handshake input and `SocketSession<Context, Incoming, Outgoing>`. `session.recv()`
returns each message with fresh authorization context; `session.send(value)`
validates the outgoing schema and awaits bounded delivery. JSON messages arriving
in either text or binary frames use the same schema. Opaque binary socket codecs
remain separate explicit transports.

`StreamOptions` requires bounded message sizes, an authorization interval, a
per-operation timeout, and the application shutdown signal. Framework-owned
one-message queues bound backpressure; disconnect, shutdown, timeout, schema
failure or revoked access ends the stream. These mechanisms enforce contracts and
lifecycle; transport confidentiality still requires TLS or the separately
authenticated encrypted-channel protocol.

### Native parser and response-validation profiles

Native typed HTTP and SSE requests use the same bounded JSON, text, URL form and
binary parser as version selection. Declared byte limits, strict JSON, character
sets, inflation and form options are applied after original-byte policy
verification. The independent 2 MiB transport limit remains in force. Multipart
keeps its typed file extractor and separate part limits.

`RouteOptions.response_validation` supports `Error`, `Warning` and `None` for
finite responses, matched-version response unions and SSE frames/headers.
`Warning` observes schema assertion failures through redacted tracing metadata;
`None` skips these assertions. Both retain valid status/media selection, JSON and
HTTP framing, stream bounds, and typed serialization. A generated field codec
may reject an invalid Rust value during serialization (for example a bounded
string or Date); validation modes cannot bypass that codec. Generated clients
continue to validate remote responses strictly regardless of server mode.

Native request `Warning`/`None` still validate first. On a schema assertion
failure they retry argument construction from the original parser values and
raw field strings/arrays; warning mode emits redacted metadata. Rust
argument deserialization and field codecs remain mandatory. In particular, a
failed numeric query constraint cannot invent a numeric coercion from the raw
string fallback. Valid requests keep ordinary coercion, and version selection
remains strict. Parser failures and operational validator errors never use the
fallback. This is the representable portion of TypeScript's raw-request fallback;
it does not permit untyped invalid values to enter a typed handler.

### No-body static handlers

Public `Endpoint<NoBody, Response>` operations support state-only handlers:

```rust,ignore
async fn health(State(state): State<AppState>) -> Result<HealthResponse, EndpointError> {
    state.health().await
}
let bindings = contracts::Bindings { health: contracts::Health::bind(health) };
let router = bindings.mount(ContractRouter::new(contracts::registry()))?;
```

Authorized no-body bindings similarly accept `fn(State<S>, P::Context)`; their
existing three-argument form with trailing `()` remains supported, and the same
policy pipeline runs before the handler.

Existing `fn(State<S>, ())` public handlers remain supported. A body-bearing endpoint
cannot bind a state-only handler. The generated `mount` method infers a handler
shape for each operation; callers that explicitly wrote `mount::<RouterType>`
must use normal inference or add one `_` generic placeholder per operation.
`PublicBinding` constructors and existing custom `MountBinding<B>` adapters keep
their signatures (the adapter's shape parameter defaults to `()`).
