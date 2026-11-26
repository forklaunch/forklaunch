# ForkLaunch worker

Security boundaries for native worker runtimes. The optional `apalis` feature
provides a native retry-policy adapter; applications still configure Apalis
storage, concurrency, heartbeat, dispatch and graceful shutdown.

`JobPayload` declares authenticated job identity and zeroizes sensitive decoded
fields. `seal_job` authenticates tenant, operation and purpose with the encryption
crate's wire profile. `decode_job` also compares the decoded resource and revision
against the queue routing metadata. Ciphertext and decrypted bytes are bounded by
the envelope library. Applications retain their schema-specific size constraints.

`process` rejects invalid jobs before invoking the handler, applies a bounded
timeout, erases handler error details and returns only unit. This prevents a
handler's plaintext result or error response from being persisted as a queue
result. `process_attempt` provides the same lifecycle boundary for a domain
service that already calls `decode_job`; applications map errors to fixed
`ProcessorError` variants so permanent validation failures stay non-retryable.
Deserialized types must implement correct `Zeroize` behavior; parsing
failures and application-created copies are not a general secure-memory guarantee.
The wrapper cannot prevent application code from logging or separately storing
plaintext and does not catch panics. Process panics must not embed private values.

`apalis::retry_policy` applies bounded exponential backoff with jitter and rejects
permanent errors even after native runtime boxing. Use `queue_error` at the
handler boundary. Only fixed `Failed`/`Timeout` errors retry; unknown errors fail
closed. Task metadata cannot increase the configured retry count. The runtime
must persist attempt counts across restart and keep its queue lease valid for
the processing and retry duration.

## Application responsibilities

- Queue delivery is at least once, not exactly once. Publish using the original
  operation ID, with transactional deduplication and revision checks on the owner
  service. Validate publication acknowledgements before returning success.
- A timeout cancels the local future, not a remotely committed request. Retry
  publication idempotently, including after crash recovery.
- Encryption authenticates context, not current authorization. Owner services
  enforce tenant membership, resource access and caller authorization.
- Never place application plaintext in queue metadata, error messages or logs.
- Keep database transactions out of backoff waits and use least-privilege queue
  credentials. A worker need not gain direct access to owner-service tables.

The Fieldnotes preset demonstrates this boundary with encrypted outbox jobs and
typed SDK publication. Its Notes algorithm, workspace policy, schema and SQL stay
inside the generated application.

## Typed encrypted channels

`EncryptedChannel<T>` binds one versioned event contract to its authenticated
channel name. `seal` produces a `SealedEvent` suitable for broker/outbox/retry/DLQ
storage; only `open` or `process` releases typed plaintext. Consumers supply the
expected tenant from trusted configuration or authorization, not from an
unauthenticated envelope. Swapping channel, tenant, resource, operation or revision
fails authentication. Diagnostics redact even malformed envelope contents.

New configured keyrings use per-tenant HKDF-SHA256/AES-256-GCM v3 envelopes,
retaining explicit v1/v2 readers during rotation. This is a new Rust-native wire
profile, not implicit compatibility with TypeScript's persistence ciphertext.
Cross-language producers must select the same documented profile. TLS remains
necessary for broker credentials and routing metadata; encrypted event bodies
alone do not encrypt the whole connection.

The Notes API and worker now share `domain/events/insight.rs`. The callback result
is encrypted under a distinct purpose and sent using the generated SDK with an
HMAC signature over the exact wire body. The receiving API verifies freshness
and claims the nonce atomically in PostgreSQL before decoding the result. The
existing operation ledger and revision compare-and-swap still own idempotency.

### Broker transport admission

`BrokerTransportPolicy` requires verified TLS for remote Redis (`rediss`), PostgreSQL (`sslmode=verify-full`), and Kafka (`SSL`/`SASL_SSL`) endpoints. Loopback is allowed for local development; container-network plaintext requires the explicit `PlaintextDevelopment` policy. Certificate-verification bypasses are rejected. Errors never include connection strings or credentials. This checks configured endpoints; Kafka advertised broker addresses and caller-supplied clients remain deployment responsibilities. Message encryption does not encrypt broker credentials or traffic metadata.

## Native application hooks

The `native` feature exposes `WorkerApplication` and `run_application`. Ordinary
CLI `--runtime apalis` workers use generated entrypoints that delegate database
registration, migrations and typed backend assembly to `worker_hooks.rs`. Telemetry
startup/flush and migration-command admission remain framework-owned. The native
feature also enables attempt telemetry: fixed outcome counters, active attempts,
duration and processing spans inherit the active in-process trace context. Queue
transport does not implicitly carry that context; cross-process trace continuation
requires an explicitly defined event/transport carrier. Trace IDs do not authorize
jobs or replace authenticated tenant/resource/revision checks. The generated
`build.rs` always exports compliance metadata and delegates event assets to
`build_assets.rs`; custom channels keep their actual schemas and wire purposes.

`supervise` drives an application-supplied native backend future alongside bounded
dispatch and maintenance callbacks. SIGINT/SIGTERM share the drain path. A dispatch
failure stops admission and drains active jobs before returning a fixed error. The
helper does not replace native queue leases, retries, encrypted `JobPayload`
validation, acknowledged access audit, or application transaction/idempotency checks.

The initial Apalis handler fails closed until authorized application processing is
implemented. Selecting a native runtime does not opt into the leased adapter's
separate HTTP management surface.

### Authenticated cross-process trace continuation

`TracedJob<T>` is an explicit new encrypted payload shape (`payload` plus optional
bounded W3C version-00 `trace`). Use a new channel/purpose version and generate its
schema for all language consumers; existing `work-v1` wire data is unchanged.
`TracedJob::new(payload)` captures an active parent with the telemetry feature.
No baggage or vendor tracestate is carried. Domain context maps now point into
`payload`, for example `payload.context.tenantId`.

`process_traced_audited` takes the trusted expected tenant, `JobAccess`, ciphertext,
timeout, durable audit sink and handler. It verifies ciphertext and all job context,
requires the audit acknowledgment, and only then imports the parent and starts the
consumer span. Invalid ciphertext, tenant/context substitution and failed audits
never activate the transported parent or release plaintext to the handler. Audit
and handler share the timeout budget. The wrapper is schema-exportable without a
telemetry runtime; trace continuation itself requires the telemetry feature.
