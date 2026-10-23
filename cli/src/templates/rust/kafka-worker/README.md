# {{name}} Kafka worker

Implement idempotent application work in `src/handler.rs`. The generated handler
fails until you implement it. Set the connection and trusted tenant environment
from `.env.example`, then explicitly create queue/dead-letter topics with
`cargo run -- --migrate`. Topic creation requires broker administrative permissions;
normal operation disables automatic topic creation. Provision production replication
and retention with your broker administrator.

Publish with `cargo run -- --enqueue ID JSON`, inspect pending group records with
`--peek LIMIT`, or inspect retained failures with `--dead-letters LIMIT`. Summaries
omit payloads and credentials. Kafka permits duplicate job IDs. Topics and consumer
groups are deterministically scoped by namespace, trusted tenant and queue. Different
group IDs consume independently. Use broker ACLs to enforce tenant authorization.

Each process consumes sequentially; scale across replicas and topic partitions.
Give each worker a stable, unique `KAFKA_TRANSACTIONAL_ID`, and each independent
publisher a separate `KAFKA_PUBLISHER_ID`. Reusing an identity fences an older
producer. Consumer offsets and retry/dead-letter publications commit atomically.
A failed or uncertain transaction stops the worker; restart explicitly. Handler
side effects remain at-least-once and must be idempotent. Attempt counts track
handler dispositions; a crash before commit can repeat the same attempt.

Shutdown drains the active handler within `WORKER_DRAIN_MS`. Cancellation leaves
its offset uncommitted so another replica/restart can recover it. Handler execution
is bounded by `WORKER_HANDLER_TIMEOUT_MS`. Assignment changes fail closed rather
than committing a record from an older consumer generation. Pending inspection is
bounded, does not advance offsets, and can return fewer records during open
transactions. Ordering is per partition. Kafka log retention governs history;
per-job database inspection and deletion are not Kafka operations.

Optional `WORKER_HTTP_ADDR` enables health/readiness. Set a random 32+ character
`WORKER_CONTROL_TOKEN` for bearer-authenticated `/control/pause`, `/control/resume`,
and `/control/drain`. These affect the local process unless optional Redis signaling
is configured with `WORKER_SIGNAL_REDIS_URL` and `WORKER_SIGNAL_NAMESPACE`. Then
HTTP commands and `--signal pause/resume/drain` publish to the same scoped group;
responses report subscriber counts, not acknowledgements. Delivery is ephemeral.
Subscription failures drain active work and exit with an error; restarts begin
running without replay or automatic desired-state restoration.

`KAFKA_CLIENT_PROPERTIES` accepts librdkafka TLS/SASL options as a JSON object of
strings. Keep it out of logs. Delivery-critical offset, transaction and isolation
settings cannot be overridden. OpenAPI and SDK exports contain no application
routes until application HTTP contracts are added separately.


## Separate control service

Run this executable with `--control-service` and `WORKER_HTTP_ADDR` to serve
health/readiness without connecting to job storage, claiming jobs or running
retention. Without `WORKER_CONTROL_TOKEN`, controls return 404. Authenticated
controls additionally require the distributed signal configuration above; they
publish to consumers without becoming a consumer or signal subscriber. Readiness
describes this control process, not remote queue health. Draining consumers does
not stop the control service; SIGTERM stops it cleanly.

New release manifests advertise distinct control and consumer roles when the
installed generated sources support this entry point. Older/custom sources keep
their existing command. The health/control server does not expose MCP tools.


## Encrypted event contract

Processing and enqueue require a trusted non-nil UUID `TENANT_ID` and
`ENCRYPTION_KEY_HEX` (a securely generated 32-byte key encoded as 64 hex digits).
Use the key ID/previous-key variables in `.env.example` for rotation. No default
key is provided. Keep credentials outside source control.

`--enqueue` accepts a UUID operation ID and this typed JSON input:

```sh
cargo run -- --enqueue 22222222-2222-4222-8222-222222222222 '{"resourceId":"33333333-3333-4333-8333-333333333333","revision":1,"message":"example work"}'
```

The event boundary encrypts the input before the broker stores it. The worker
authenticates tenant, operation, resource, revision and queue before exposing the
typed event to the handler. Retry/dead-letter state retains the sealed event;
inspection prints metadata only. Plaintext legacy jobs are rejected. Application
handlers remain fail-closed until implemented; enforce idempotency and current
resource permissions before acknowledging success. This does not establish
wire compatibility with existing TypeScript queues; migrate their event contracts
explicitly rather than accepting unauthenticated legacy JSON.


Broker transport is a separate security boundary from encrypted job contents.
Remote Redis/BullMQ requires `rediss://` with certificate verification; do not use
an insecure fragment. Remote PostgreSQL requires `sslmode=verify-full`. Remote
Kafka requires `security.protocol=SSL` or `SASL_SSL` with certificate and hostname
verification enabled. The generated defaults permit loopback plaintext for local
checks. For isolated Docker development brokers only, explicitly set
`WORKER_ALLOW_PLAINTEXT_DEVELOPMENT=true`; never carry that override into production.
The Redis/BullMQ adapters include Rustls roots. Kafka uses librdkafka TLS settings;
its `KAFKA_CLIENT_PROPERTIES` configuration is also checked before connecting.
Control-channel Redis uses the same transport policy. TLS does not replace the
encrypted payload boundary, and encrypted payloads do not protect broker credentials.

## Shared event producers

`domain/schemas/event_contract.rs` defines `WorkInput` and `WorkEvent` once. Every Cargo build runs `build.rs` and updates `events/event-contract.json`, `events/typescript/events.ts`, and `events/rust/events.rs`; unchanged assets keep their timestamps. Deleting an output regenerates it. These are worker event contracts, separate from HTTP OpenAPI, and are not an AsyncAPI document.

The generated TypeScript `WorkProducer` uses `createEncryptedWorkPublisher` from the bundled shared Node runtime. Supply the trusted tenant, worker namespace, active key ID/key, and a broker adapter whose `publish` receives only `SealedWorkEvent`. The runtime performs HKDF-SHA256 tenant key derivation and AES-256-GCM v3 encryption before that adapter is called. Call `close()` to wipe the derived key. JavaScript strings/objects cannot be reliably wiped; retain plaintext inputs only as needed. The Rust producer module exposes `seal_work` with the same authenticated event contract. Configure broker TLS independently. Existing historical TypeScript ciphertext formats are not implicitly compatible with this protocol.

The generated `WorkConsumer(options, queue)` exposes asynchronous `consume(expectedContext, envelope)`. It authenticates the trusted tenant, expected resource/operation/revision, namespace, channel and queue, validates the complete decrypted event against the Rust-generated JSON schema, then requires a bounded, acknowledged metadata-only audit before returning `WorkEvent`. Configure `activeKey` and up to four `previousKeys` for rotation, and call `close()` when finished. `WorkProducer` validates the exact serialized input before encryption, including routing identity. Install the dependencies in `events/typescript/package.json`; errors omit plaintext and detailed schema diagnostics. The runtime file retains `createEncryptedWorkConsumer().open()` only as a low-level compatibility primitive without DTO validation or disclosure auditing; the generated typed entrypoint does not export it.
