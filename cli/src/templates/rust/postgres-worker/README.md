# {{{name}}}

This PostgreSQL worker polls durable jobs, renews leases, retries failures, and drains its current delivery on Ctrl-C or SIGTERM. Implement idempotent application work in `src/handler.rs` before consuming jobs. The initial handler returns `worker_handler_not_implemented`; it never silently acknowledges unimplemented work.

```sh
DATABASE_URL=postgres://schema_owner:password@localhost/app cargo run -p {{{name}}} -- --migrate
DATABASE_URL=postgres://restricted_app:password@localhost/app TENANT_ID=11111111-1111-4111-8111-111111111111 cargo run -p {{{name}}}
```

Use your own credentials. Runtime roles require schema USAGE, table DML, and sequence USAGE in `fl_worker`; SUPERUSER/BYPASSRLS roles are rejected. Bind `TENANT_ID` from trusted application configuration. The example env file is not loaded automatically.

With runtime configuration set, `--enqueue ID JSON` submits a job and `--inspect ID` reports its state without the payload or lease token. `WORKER_QUEUE`, timing and retry settings are required and listed in `.env.example`; a missing or invalid value fails startup with its name. Contract/OpenAPI export requires no database connection.

The runner allows `WORKER_CONCURRENCY` simultaneous handler batches per process (1–256). `WORKER_BATCH_SIZE` sets the batch size; their product must not exceed 1000. On shutdown it stops claiming jobs, renews each active lease, and waits up to `WORKER_DRAIN_MS` for all active handlers. A timeout cancels the handler and leaves the job recoverable after lease expiry. Keep all handler work inside its returned future. Delivery is at least once; external side effects must be idempotent. Application error codes are retained as failure reasons; detailed error reporting belongs in the handler.

Administration commands use the configured tenant and queue:

```sh
cargo run -p {{{name}}} -- --dead-letters 100
WORKER_MAX_ATTEMPTS=3 WORKER_BACKOFF_SECONDS=30 cargo run -p {{{name}}} -- --retry JOB_ID
cargo run -p {{{name}}} -- --purge-completed 604800 100
cargo run -p {{{name}}} -- --purge-failed 2592000 100
```

Listing omits payloads and lease tokens. Retry acts only on a failed job and resets its attempts and failure reason using the configured retry budget. Purge permanently removes at most the requested number of completed or failed jobs older than the given age in seconds; queued and processing jobs are preserved. Limits must be 1–1000 and ages must be positive and at most 3650 days. Purged identities may be submitted again. Automatic retention is disabled by default; explicit purge commands remain available.

To enable periodic retention, set `WORKER_COMPLETED_RETENTION_SECONDS` and/or `WORKER_FAILED_RETENTION_SECONDS` to a positive age. An unset age keeps that status indefinitely. Enabled retention also requires `WORKER_RETENTION_INTERVAL_MS`, `WORKER_RETENTION_BATCH_LIMIT` (per status per tick) and `WORKER_RETENTION_TIMEOUT_MS`; `.env.example` lists 60000, 100 and 5000. Missing or invalid settings fail startup. Cleanup begins after the first interval, stops on shutdown, and never purges queued or processing jobs. A cleanup failure stops new claims and drains active handlers before the worker exits with an error.

Set `WORKER_HTTP_ADDR=127.0.0.1:8081` to opt into internal `GET /health` and `/ready`. Health returns process state; readiness is 200 when running and 503 while paused or draining, without probing storage. Controls are disabled unless `WORKER_CONTROL_TOKEN` contains a random secret of 32–4096 printable ASCII characters without whitespace. With `Authorization: Bearer <secret>`, POST to `/control/pause`, `/control/resume`, or `/control/drain`. Accepted commands return 202; pause/resume after drain return 409. Pause preserves active work; drain is irreversible and stops retention immediately. Health remains available during drain. HTTP is disabled by default; a token without an address fails startup. Bind this plain HTTP listener to a trusted interface and protect remote access through deployment controls or a TLS proxy. Controls affect this process only, not other replicas. Internal management endpoints are separate from application contracts and SDKs.

Set `WORKER_BATCH_SIZE` to 1–1000 to claim currently eligible jobs in batches; the runner does not wait to fill them. Override `JobHandler::process_batch(Vec<Job>) -> BlueprintResult<Vec<JobFailure>>` for bulk work, returning only failed job IDs and errors. Omitted jobs are acknowledged; a whole-batch error or unknown/duplicate failure IDs fails every member. With size 1 the runner calls `process`; the default batch implementation calls it sequentially for compatibility. Each job keeps its own retry budget and lease token. All leases renew while the batch runs. Loss of any lease cancels the batch and leaves other deliveries recoverable after expiry. Handler drain timeouts cancel the full batch. Keep effects idempotent and all work inside the returned future. Batch acknowledgments are independently fenced rather than atomic across jobs.

## Distributed controls

Set `WORKER_SIGNAL_REDIS_URL` and `WORKER_SIGNAL_NAMESPACE` on every participating
worker to enable Redis Pub/Sub controls. The group is scoped by signal namespace,
trusted `TENANT_ID`, and `WORKER_QUEUE` (this worker's name in `.env.example`). PostgreSQL
workers may use a separate Redis deployment for controls. Without these variables,
HTTP controls remain process-local and no signal connection is opened.

Run `cargo run -- --signal pause`, `--signal resume`, or `--signal drain` with the
same scope and signal environment. These commands do not connect to queue storage.
When enabled, bearer-authenticated HTTP controls publish to the same group and
apply through the ordered subscription; their JSON response reports `subscribers`. This count means Redis
subscribers, not application acknowledgements. Health/readiness reflects local state.
Publication failure returns HTTP 503; delivery can remain uncertain after a timeout. Drain is
irreversible for each running process.

Delivery is ephemeral: disconnected or late subscribers miss commands. Subscription
disconnect, overflow, or malformed messages cause graceful drain and an error exit.
There is no automatic reconnect or durable desired-state restoration: restart the
worker explicitly; a restarted worker begins running. Use a unique namespace for
each environment and Redis ACLs; Redis database numbers do not isolate Pub/Sub.


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
