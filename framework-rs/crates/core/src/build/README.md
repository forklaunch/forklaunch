# Compile-time contract assets

`service_sdks` emits HTTP SDKs from the host-compiled route registry. `worker_events`
is specifically the ordinary `work-v1` contract; use `channel_events` for application
DTOs with different encryption purposes or envelopes.

The default HTTP output is a usable `src/client-sdk/typescript` package: compiling
services updates its root `createForkLaunchSdk` factory, service namespaces such as
`NotesSdk`, and package subpaths such as `@your/sdk/notes`. Namespace exports keep
same-named DTOs from different services separate. The CLI uses the same renderer
for native `sdk generate`, `sdk check`, and service removal. Concurrent service
builds serialize discovery and package updates. An explicit output override remains
standalone.

Only unchanged generated root files and export entries are replaced. Custom package
fields and edited root barrels are preserved; new service subpaths remain available
even when the application owns its root barrel. `.forklaunch-sdk-package.json`
records that ownership. Per-service `.forklaunch-generated` markers restrict
aggregation and deletion to generated children. To verify real two-service package
imports, set `FORKLAUNCH_TYPESCRIPT_COMPILER` to a `tsc` executable and run
`cargo test -p forklaunch-core --features build,asyncapi-import --test service_package`; it compiles and runs a
consumer using both namespaced clients and the aggregate factory.

`ChannelEvent` declares a Rust-derived JSON Schema, its purpose, four context field
names, and `ChannelWire::SealedEvent` or an explicitly mapped flat envelope. The
payload itself is encrypted directly using the native v3 HKDF-SHA256/AES-256-GCM
protocol. The build emits `event-contract.json`, typed TypeScript producer/consumer
factories, the shared Node runtime, and the original Rust source module. The Rust
module retains application `JobPayload` and `Zeroize` implementations and lists its
required dependencies; it is not presented as an independently publishable crate.

The generated producer requires trusted tenant/resource/operation/revision context
and sends only ciphertext to its adapter. The consumer verifies that same expected
context against both authenticated payload and routing metadata, validates the DTO
schema, then awaits an audit callback returning `true` before releasing plaintext.
The callback must acknowledge durable metadata-only storage. It receives no payload.
Audit failure, missing acknowledgment or timeout rejects access. The consumer is not
a replay ledger: transactional operation/revision checks still belong to the app.

Key configuration includes `activeKey` and optional `previousKeys`. Producers use the
active key; consumers authenticate the embedded key identifier against that bounded
keyring. `close()` clears derived key buffers. Native envelopes up to 1 MiB are
supported; callers may choose a lower `maxPayloadBytes`. JavaScript integer fields
are constrained to the exact safe-integer range. Plaintext byte buffers are cleared;
JavaScript object/string values cannot offer Rust's deterministic zeroization.

Notes exports its flat `SealedNoteJob` under `note-insight-job-v1` and its result
`SealedEvent` under `note-insight-result-v1`. Its worker build uses the actual
`NoteJob`/`InsightEvent` structs, not a parallel work-v1 approximation.

Validation: `cargo test --manifest-path blueprint-rs/Cargo.toml -p forklaunch-worker-interface --features schema,encrypted --test node_channel` uses the actual
Notes DTO source and performs Node/Rust roundtrips, rotation, tamper and context
rejection, bounded payload checks and audit rejection/timeout. Set
`FORKLAUNCH_CHANNEL_ASSET_FIXTURE` to retain generated TypeScript assets, install
that generated package's dependencies, and run
`node crates/core/compiler-tests/build/channel-typescript.mjs <fixture-directory>` to check strict
TypeScript consumer types and actual Ajv schema enforcement on both channels.

The ordinary `work-v1` exporter also uses the shared encrypted-channel boundary.
Its generated `WorkConsumer` is asynchronous: construct it with `ConsumerOptions`
(including active/previous keys, actor and a durable audit callback), then call
`consume(expectedContext, envelope)`. The queue passed to its constructor is trusted
configuration. Queue, DTO schema, resource, revision, tenant and operation checks
all precede the audit callback. It exposes `close()` to dispose derived keys.
`WorkProducer` validates the exact serialized input and rejects serialization hooks
that alter its routing context; only authenticated ciphertext reaches the publisher.

The direct runtime `createEncryptedWorkConsumer().open()` function is retained only
as a low-level compatibility primitive. It has no DTO validation or access audit and
is no longer exported by the generated typed entrypoint. Applications should use the
audited `WorkConsumer`; retaining the raw primitive is not a disclosure-policy guarantee.
