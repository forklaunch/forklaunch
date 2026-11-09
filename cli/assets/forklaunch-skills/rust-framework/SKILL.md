---
name: rust-framework
description: "Rust Forklaunch services, contract artifacts, encrypted workers and service-owned persistence."
user-invokable: true
---

# Rust Forklaunch applications

Use this guidance for projects whose `.forklaunch/manifest.toml` selects Rust or mixed languages. TypeScript framework examples do not establish Rust API availability. Inspect the project's Cargo dependencies and exported APIs before implementing an abstraction.

## Structure and contracts

Keep services and workers under `src/<domain>` as independent Cargo packages. Keep controllers and route bindings in `api`, schemas and business logic in `domain`, and storage in `persistence`. Migrations belong to their service or worker. `main.rs` and `lib.rs` are composition/export entry points. Shared application policy lives in `src/core`; browser code lives in `web`.

Use Rust schema structs and declared content types as the source for contracts. Bind handlers using available Forklaunch routing abstractions and preserve security processing. Axum is the underlying runtime; raw routes must not bypass authentication, tenant checks, validation or contract publication. Do not assume a separately declared contract proves the handler matches it.

New built-in modules share `api/contracts/registry.rs` and pure extension descriptors between runtime and build scripts; adding, renaming or deleting a generated extension updates those descriptors. Billing profile selection must be consistent between build and runtime. Service `build.rs` generates contract/OpenAPI, TypeScript SDK artifacts and an include-ready Rust SDK module; client build hooks can also generate Rust SDKs. Current portable contracts identify the shared HTTP SDK compiler, so `forklaunch sdk generate --from contract` uses the same framework compiler for TypeScript and Rust. `--check` compares staged output without rewriting client files; legacy artifacts retain their compatible legacy compiler. The CLI caches its compiler host under `.forklaunch/sdk-host`, honoring the application’s framework dependency mode. Edit schema/contract sources, never generated SDK output. Cargo incremental builds rerun hooks only when tracked inputs change. Validate generated clients after changing inputs, status codes, content types, streaming or socket events. MCP must exclude internal and unsupported streaming operations.

## Ordinary generation and application composition

`init service <name> --language rust -d postgresql` generates service-owned migrations and a checked PostgreSQL repository; omit `-d` for a lightweight in-memory starter. The default SQLx driver is 0.8; `--postgres-driver sqlx-09` selects the compatible framework policy facade explicitly. Unsupported Rust databases fail rather than silently ignoring the option. Runtime connections require restricted roles and admitted transport; migration owners are separate. Generated repositories require a trusted tenant and configured envelope keys.

`init worker <name> --language rust --type database --runtime apalis` selects the native Apalis PostgreSQL profile. Inspect `init worker --help` for the backend argument accepted by the installed CLI. Keep application processing in `worker_hooks.rs` and domain modules; generated bootstrap, supervision, compliance and build hooks remain authoritative. A generated placeholder deliberately refuses work until authorized business processing is implemented.

`init library <name> --language rust --sdk-services <service...>` creates a Cargo SDK consumer under `src/client-sdk/rust` from registered service contract crates. It can build before the services themselves. Put extra client exports in `extensions.rs` and extra generated assets in `build_assets.rs`. Service renames update host dependencies; remove or update a bound consumer before deleting its contract source. Every Cargo package has its own Makefile tasks.

The Notes preset composes ordinary application, service, worker, infrastructure, library and SDK generators, then applies an allowlisted business overlay. `.forklaunch/composition.json` records the recipe. The overlay does not replace generated Cargo policy, bootstrap/server wrappers, mandatory build hooks or local provisioning. Reconcile new dependencies structurally, preserve framework source configuration, and use app-owned build/lifecycle extension hooks. Do not copy an entire reviewed application over the generators or mistake preset-only behavior for general framework support.

## Encrypted processing and compliance

Use framework encryption and worker processors instead of app-owned cryptography. Queue backends, retry state, outbox records, dead letters, Redis streams/pubsub and object storage are persistence boundaries. Encrypt sensitive payloads before each boundary; decrypt only within the authorized processor. TLS does not replace payload encryption. Tenant-bound cache/queue/object JSON writes use random-nonce v4 field envelopes and retain authenticated legacy readers; generic worker channels use a separate v3 envelope protocol. These version numbers describe different formats and must not be interchanged. Bind ciphertext to the intended tenant, channel, job and purpose when supported. Never copy plaintext payloads into logs, errors, telemetry or retry metadata.

Authenticate stream subscribers and enforce workspace membership on HTTP, SSE, WebSocket and MCP paths. Do not expose internal worker callbacks publicly. Test wrong-tenant access, tampering, key rotation, retries, dead letters and backend contents in addition to successful processing. Document unsupported encryption guarantees rather than calling them parity.

Declare entity-member compliance classifications separately from encryption policy. Encryption alone does not implement auditing, retention, subject export/erasure or authorized plaintext access. Use framework persistence policy where available; do not silently replace missing policy with unscoped SQL.

For protected persistence, keep default anonymous redaction; opt into authenticated
partial projections only under explicit application policy. Use audited
rotation/reclassification for existing rows. Durable subject exports require
absolute snapshot/delivery deadlines and a scheduled, separately authorized
cleanup role. Keep operation tombstones for their full replay lifetime. External
erasure must enqueue encrypted versioned targets in the same transaction as source
deletion and reconcile through an idempotent destination; a database-only deletion
or declared compliance artifact is not complete cross-store erasure. See the
framework repository's `crates/core/PERSISTENCE.md` for APIs and real-database
receipts; do not invent subject ownership, legal holds or retention deadlines.

## Dependencies and validation

`vendor/framework` is a development snapshot, not evidence of published crates. Keep local aliases such as `framework` and `worker`. Switch to registry dependencies only after exact versions and required parser patches are published and a clean consumer build passes. Cargo does not inherit a dependency's workspace patches. Plain application initialization accepts `--framework-version X.Y.Z` for an already published release or `--framework-path /checkout/framework-rs` for host development without an application language flag. It saves this preference without creating Rust projects or a Cargo workspace; the first Rust project consumes it and retains the required parser patch. Generate and commit a new Cargo.lock before locked builds. A host-local checkout is not a portable Docker input: mixed workspaces require an application-owned `Dockerfile.rust` that integrates the local source before adding Rust projects. Without that explicit integration, initialization rejects the project before modifying application files. Use the default snapshot or a published version for managed shared images.

Use each project's Makefile/Cargo commands and the root Docker Compose configuration. Do not deploy as part of local generation. Run contract/SDK checks, HTTP/MCP startup, authenticated streaming and real worker delivery. Inspect stored ciphertext and verify security failures. Passing a demo does not establish framework parity.

Worker event contracts live in `domain/schemas/event_contract.rs`; `build.rs` compiles the same source to emit `events/event-contract.json`, TypeScript producer/types, and a reusable Rust producer module. Keep the event manifest separate from empty worker HTTP contracts; do not label it AsyncAPI. The bundled Node producer uses Rust-compatible v3 HKDF-SHA256/AES-256-GCM encryption before broker callbacks; configure trusted tenant/namespace/key ID and broker TLS explicitly. Use `seal_work` in Rust, and `WorkProducer` plus `createEncryptedWorkPublisher` in Node. Historical TS ciphertext compatibility must be proven separately.

For Node consumers use generated `WorkConsumer` with `createEncryptedWorkConsumer`; authentication alone does not validate domain schema. Generated Ajv validators enforce the Rust schema before typed return and before producer encryption. The lower-level transport returns unknown input intentionally. Install the generated TypeScript package dependencies; never replace validation with an unchecked TypeScript cast.
