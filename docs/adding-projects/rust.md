---
title: Adding Projects - Rust Backends
category: Guides
description: Generate Rust services, contracts, and TypeScript or Rust SDKs.
---

# Rust backend generation

The CLI generates Rust services, workers, libraries and routers alongside TypeScript projects. Application initialization is language neutral; choose the language when adding each project. Framework crates are published under `forklaunch-*`, while generated application folders keep ordinary names such as `core`, `accounts` and `jobs`.

The [framework guide](../../framework-rs/GUIDE.md) and
[blueprint guide](../../blueprint-rs/GUIDE.md) provide crate/feature catalogs,
API reference build commands, and operational boundaries.

## Create and run a service

```sh
forklaunch init application my-app -D "My app" -A "My team" -L MIT
cd my-app
forklaunch init service accounts --language rust -D "Accounts service"
forklaunch init router users --language rust --path src/accounts
cargo test --workspace
cargo run -p accounts
```

The application command prompts for shared tooling settings; it does not require a
language or SDK target. In scripts, pass `--database postgresql --validator zod
--formatter prettier --linter oxlint --http-framework express --runtime node
--test-framework vitest` to supply those settings explicitly.

The root `Cargo.toml` is the workspace. Each service or worker owns its Cargo
package, migrations, tests and scripts under the configured modules directory
(`src` by default), beside TypeScript projects. Controllers, routes, domain
schemas/services, persistence, registrations and startup have separate files;
`lib.rs` and `main.rs` expose and start them. `--mappers` adds optional mapping
functions. A service without `--database` uses the in-memory starter. Selecting
`--database postgresql` generates its PostgreSQL repository and service-local
migrations; configure the database and encryption settings before running it.

The service serves `/health`, `/users`, `/openapi.json`, and a Scalar documentation UI at `/docs`. `HOST`, `PORT`, `PROTOCOL`, `VERSION`, `DOCS_PATH`, `RUST_LOG` and `MCP_ENABLED` are required; optional `PUBLIC_ORIGIN`, `DOCS_SERVER_URLS` and `DOCS_SERVER_DESCRIPTIONS` default to the listener's own address. Generated configuration has no in-code defaults (see [Configuration](#configuration)). Ctrl-C and SIGTERM stop the HTTP server gracefully.

Network requests use Axum and the same contract handlers as `local_router`. Path parameters, decoded URL queries, headers, binary bodies, body limits, CORS, and request IDs pass through the framework adapter.

When running in a terminal, you may omit the application, service, worker, library
or router name. Native initialization prompts for it and offers `my-<kind>` as the
default. For service/worker/library generation, this name is also the Cargo package
and project directory name. Pass the positional name explicitly in scripts; missing
names fail immediately when input or error output is not a terminal.

## Add Rust to an existing TypeScript application

Run native initialization from the application root containing `.forklaunch`:

```sh
forklaunch init service inventory --language rust -D "Inventory API"
forklaunch init worker inventory-jobs --language rust -t redis -D "Inventory jobs"
forklaunch init library inventory-models --language rust -D "Inventory models"
cargo check --workspace
cargo run -p inventory
```

The first native project creates a Cargo workspace and marks the application as
mixed. Node/Bun runtime settings, TypeScript modules, package workspaces and
existing source files are preserved. Native module initialization works too, for
example `forklaunch init module catalog --language rust -m cac-base`.
`--dryrun` leaves the application untouched.

An existing Cargo workspace retains its members, comments and configuration;
missing inherited package settings are filled from the application. Generated
native crates require `workspace.package.edition = "2024"` and a root
`Cargo.toml`. Incompatible configuration is reported before attachment writes.
Commit the Cargo files, generated SDK sources, and any `.forklaunch/blueprint-rs`
sources used by selected modules or workers.
The default framework dependencies resolve exact published versions; there is
no framework source copy in a default new application.

Compose selects adjacent root `Dockerfile.rust` and `Dockerfile.typescript`
files by project language. Each image builds the projects for that language;
Compose selects the service or worker command. Existing custom Dockerfiles are
preserved or reported for reconciliation before an owned file is replaced.
Use `docker compose build inventory` and `docker compose up inventory` to build
and start the native service. Its HTTP and MCP settings are the same as in a
Rust-first application. Node and Bun migration fixtures compile relocated native
services, workers, libraries and CAC modules; native HTTP/MCP startup and the
separate native container build are validated.

Install the TypeScript application's dependencies before exporting all services,
then run `forklaunch openapi export` and `forklaunch sdk generate --target all
--from contract` to refresh the project-local HTTP SDKs described below. Adding TypeScript
scaffolding to a Rust-first app and mixed development with `forklaunch dev` are
described below.

## Add TypeScript to an existing Rust application

```sh
forklaunch init service legacy-api --language typescript -d postgresql -D "Legacy API"
pnpm install
pnpm build
```

The first TypeScript service, worker, library or module uses the configured
modules directory (`src` by default), beside native projects, with Node, Express,
Zod, Prettier, oxlint and Vitest. Application settings switch
to those JavaScript tools and mark the app mixed; native projects retain their
Cargo paths and sources. The root package workspace includes both TypeScript
projects and existing SDK packages. Existing scripts and dependency versions are
preserved; missing root build/tool commands are added. `pnpm build` builds Cargo
and JavaScript packages when no custom root build script already exists.

Initialization stages the workspace and requested project before updating the
application. `--dryrun` leaves it unchanged, and failed project generation does
not publish the staged workspace. The foundational project names (`core`, `monitoring`, `client-sdk`) must be
available; unrelated existing native projects retain their directories. External
application symlinks are rejected during staging; internal links are preserved.
Local application scaffolding does not require a platform login.

The migration preserves native Dockerfiles, Compose entries and SDK target
selection, including Rust-only SDK selection. `rust_typescript_attach.sh` checks
all four initial project kinds, actual package installation and compilation,
unmodified TypeScript HTTP/MCP startup, and the native HTTP/MCP listener matrix.
Mixed development is available through `forklaunch dev`; platform deployment
validation remains separate work.

## Captured storefronts

```sh
forklaunch init module storefront shop --from /path/to/capture/manifest.json --path .
forklaunch sync library shop --path .
forklaunch change library -N boutique -D "Captured shop" --path src/shop --confirm
```

Captured storefronts work in Rust, Node, Bun and mixed applications. Initialization
copies the sibling `site/` assets and registers a library with the `storefront`
variant and a contained `metadata.staticPath`. Rename and description changes use
that registration and preserve asset bytes, application tooling and manifest
comments. `--dryrun` previews initialization or changes without writes.
`forklaunch delete library boutique --continue` removes the registered capture and
its topology references. Missing captures fail sync with an explicit error.

Adding a TypeScript workspace to a Rust application preserves the capture's
original location, including captures created before `staticPath` was recorded.
Static captures do not join Cargo or package workspaces and do not generate API
or SDK routes. Hosting the captured frontend remains a separate deployment step.

## Native Better Auth

```sh
forklaunch init application my-auth-app -D "Auth app" -A "My team" -L MIT
cd my-auth-app
forklaunch init module iam --language rust -m iam-better-auth -d postgresql -D "Authentication"
```

The `iam` service includes the configured Better Auth 1.7.6 authentication API,
organizations, teams, Google OAuth and encrypted tenant-bound PostgreSQL storage.
Use its generated README and `.env.example` for owner migrations, restricted
runtime grants and secrets. HTTP and MCP share the listener by default; separate
and disabled MCP modes use the same settings as other native services.

Raw authentication defaults to `/api/auth`. Set `BETTER_AUTH_BASE_PATH` for a
custom namespace or `/` for root. A path already present in `BETTER_AUTH_URL`
takes precedence. Default JWKS verification follows that namespace; JWT issuer
and audience use the configured URL's origin. Scalar `/docs` displays application
contracts and the raw authentication API as separate sources. The raw routes
and popup helpers retain cookie/CSRF behavior and do not become MCP tools.
Application discovery, HMAC role/permission surfacing and system-role compliance
operations use registered HTTP/MCP contracts and generated SDKs.

The shared TypeScript `clientIamBetterAuthSdkClient` factory and generated
`iamSdkClient` accept `authBasePath`, so their Better Auth client can connect to
native default/custom/root authentication. For example, pass `authBasePath: '/'`
with the public service origin for root auth. A path in `host` overrides this
option. Live tests execute both factories with the actual Better Auth client
against native PostgreSQL services, including additional fields, sessions,
profile updates, JWT/JWKS, signout and signin.

Enable the fixed password-login TOTP profile with
`BETTER_AUTH_TWO_FACTOR_ENABLED=true`. Generated Compose accepts
`IAM_BETTER_AUTH_TWO_FACTOR_ENABLED=true` from the application environment;
both default to false. Enroll with the current password, verify the initial
code, and use the actual Better Auth `twoFactorClient` for subsequent challenges,
backup codes and rotating trusted devices. Codes use SHA-1, six digits and a
thirty-second period. Factor state is tenant encrypted. HTTP/MCP discovery
reports the capability; raw authentication endpoints remain HTTP routes.
Existing sessions and social callbacks retain their independent policy.
Library applications can additionally select
`AuthHttp::with_two_factor_otp(Arc<dyn OtpDelivery>)` for a trusted server-owned
delivery implementation. This fixed profile uses six digits, 180 seconds and
five invalid attempts, and awaits delivery after encrypted storage commits.
The hook receives a public user projection and the short-lived code, without a
session token. Delivery targets never come from authentication input. See
[the delivery API and failure semantics](../../blueprint-rs/crates/implementations/iam/iam-better-auth/README.md#optional-delivered-otp).
Generated modules do not select a delivery channel. Trusted applications can use
`OtpOptions::new(digits, validity, allowed_attempts)` and
`AuthHttp::with_two_factor_otp_options` to configure 1–32 digits, positive whole
milliseconds and 1–100 invalid attempts. Existing codes retain their original
expiry and digit length; verification uses the current attempt limit. Custom
storage, background dispatch and broader plugins remain parity work. Trusted
`TotpOptions` additionally selects six/eight digits and a positive whole-millisecond
period from 1ms through 24h through `AuthHttp::with_two_factor_totp_options`.
It retains SHA-1 and the one-step skew window, preserves installed OTP settings,
and applies to existing secrets without rewriting them. Nonnegative clock bounds
and the source's floating-point period arithmetic are explicit; see the native
README for unsupported historical-clock and lifecycle-option boundaries.

This module serves one configured tenant. `AuthTransaction::find_unique` performs
exact email/subscription and organization-slug lookups using the existing
PostgreSQL blind indexes across current and previous encryption keys. It checks
the decrypted field before returning a match. `get_many` loads up to 4,096 IDs in
record insertion order, deduplicating requested IDs and skipping missing rows;
organization user joins use it instead of scanning the user model. Custom
`AuthTransaction` adapters must implement both methods. Neither lookup
requires the entire model to fit in the 4,096-row `list` bound.

Other account/session/member scans and general pagination still retain that bound.
Broader custom hooks/adapters and complete indexed storage pagination remain parity
work.

## Local cache and object storage

```sh
forklaunch infra add accounts redis --path .
forklaunch infra add accounts s3 --path .
# Also supported during initialization:
forklaunch init service storage --language rust -d postgresql --infrastructure redis s3
```

The CLI creates editable factories in `src/infrastructure/redis.rs` and `src/infrastructure/s3.rs`, exports them as `redis_infrastructure` and `s3_infrastructure`, and wires Cargo dependencies, local settings, manifest resources and Compose backends. Redis uses `REDIS_URL` with a one-hour default TTL; its synchronous operations belong on a blocking executor. The synchronous S3 factory uses explicit `S3_URL`, `S3_BUCKET`, `S3_REGION`, `S3_ACCESS_KEY_ID` and `S3_SECRET_ACCESS_KEY` settings for local MinIO. The async `configured_object_store()` factory uses the official AWS default credential chain, including container roles and configured profiles. Set `S3_BUCKET`; region and credentials can come from standard AWS configuration. An optional `S3_URL` selects a custom endpoint with path-style addressing, and a complete S3 key pair overrides the default chain. Construction never provisions a bucket.

HTTP and MCP startup do not connect these factories automatically. Call `redis_infrastructure::connect()`, `s3_infrastructure::object_store()`, or `s3_infrastructure::configured_object_store().await` where your application needs storage. Both S3 factories honor `S3_PREFIX`, `S3_PRESIGN_MAX_UPLOAD_SECONDS` and `S3_PRESIGN_MAX_DOWNLOAD_SECONDS`; default presign caps are 900 and 300 seconds. These factories expose the raw adapters; application encryption/compliance policies must be supplied separately.

Use `infra remove accounts redis` or `infra remove accounts s3` to undo owned settings. `change service --path src/accounts --infrastructure redis` replaces the enabled local infrastructure list; an empty `--infrastructure` removes both. Removal takes back the factory, dependency, settings and Compose backend while they still have their generated form; custom factories and settings survive sync, rename and removal, and a shared backend remains while another service uses it. No local state is kept, so removal behaves the same after a Git clone.

`forklaunch_infrastructure_redis::client::RedisQueue::connect` uses the same `LPUSH`/`RPOP` FIFO layout as TypeScript, including peeks in dequeue order. For a list written by the earlier native `RPUSH`/`LPOP` implementation, use `connect_with_layout(url, key, RedisQueueLayout::LegacyNative)` consistently for its producers and consumers. Drain an existing backlog using its original layout before switching it.

Generated Redis and S3 factories also expose `encrypted_cache()` and async `encrypted_object_store()`. They load `ENCRYPTION_KEY`, the optional `ENCRYPTION_FORMAT` and previous-key settings through `FieldEncryptor::from_env()` only when called. Pass a validated `TenantContext` to each JSON operation to encrypt/authenticate it; omitting context writes ordinary JSON. Reads accept legacy plaintext JSON, but encrypted records require the correct tenant and key. JSON, key, authentication and backend errors propagate rather than becoming cache misses or plaintext writes. Cache batch writes serialize every entry before sending the batch. These wrappers retain false, zero and null records; object reads retain content type and tags. Raw byte/file APIs remain available through the original factories or `into_inner()`.

`JsonObjectStore` also provides `put_batch_objects`, `get_batch_objects`, `delete_object` and `delete_batch_objects`. Batch reads retain input order, missing objects, tags and content types. JSON writes default an absent content type to `application/json`. Batch writes encode all values before sending any object; backend failures can still leave earlier writes committed. The TypeScript JSON stream-upload helpers use ordinary object writes, so the native JSON methods cover that behavior; use the underlying store's streaming/file APIs for actual byte streams.

The shared `forklaunch_core::persistence::json::JsonRecordCodec` uses TypeScript-compatible v2/v3 field-encryption envelopes and the configured key ring. JSON object member ordering may differ across languages; decrypted JSON values interoperate. This is application encryption for JSON records, separate from bucket-side encryption. `encrypted_queue(name)` adds the same tenant-bound JSON codec to the TypeScript-compatible FIFO layout, including batch writes and non-destructive peeks. Removal precedes decoding, as in TypeScript RPOP; failed authentication does not requeue a consumed item, so use peek first when context needs validation. `read_record_with_ttl` reads GET/PTTL together and returns millisecond-precision remaining expiry, with `None` for externally written persistent keys. `put_records` accepts a TTL per entry and validates all keys/TTLs and serializes every value before writing. `read_records` reads every value/expiry pair in one Redis transaction, preserving key order and misses; `read_records_by_prefix` uses a literal SCAN prefix and returns live decoded records with their expiry. Concurrent writes can change which keys a SCAN observes. Complete object-store helper and worker coverage remains tracked separately.


## Managed capabilities

Managed capabilities can be added to a native service or worker in either a Rust or mixed application:

```sh
forklaunch infra add accounts email --path .
forklaunch infra add accounts sms --path .
forklaunch infra add accounts whatsapp --path .
forklaunch infra add accounts voice --path .
forklaunch infra add accounts payments --path .
forklaunch infra add accounts models --path .
```

The CLI creates typed factories in `src/capabilities/mod.rs` (`crate::capabilities::email_client()`, `sms_client()`, `whatsapp_client()`, `voice_client()`, `payments_client()` and `stripe_client()`, `models_client()`), editable handlers in `src/capabilities/<feature>.rs`, and the signed `POST /platform-events/:feature` route. `models_client()` returns the framework's `ModelGatewayClient` (`models()`, `chat_completion`, `chat_completion_stream`): platform-hosted models without a provider key, under the product's model allowlist, token budget and rate limit. Models have no events, so `models` alone writes the factory without the route. On a worker, the worker process and its `-server` process both get the gateway settings, and events go to the server. HTTP and MCP use the same callback authentication. An existing billing payment-event handler keeps its receipt processing and business effects. Handler stubs acknowledge verified events; implement the required business effects and deduplicate event IDs in those handlers.

Client factories resolve trusted `PLATFORM_GATEWAY_URL`, `INSTANCE_ID`, and `INSTANCE_HMAC_KEY` settings and reuse a connection pool. SMS construction defers refusal when managed settings are missing; other factories return a configuration error. Compose receives the local gateway mock and a fixed-URL development opt-in. Production configuration should supply the managed gateway settings.

`src/capabilities/nonce_store.rs` supplies one replay store shared by every feature in the service process. Its default is bounded process-local memory. For multiple replicas, return an implementation of `PlatformEventNonceStore` backed by shared atomic storage. Changes to this factory and event handlers survive capability additions, sync and rename.

`forklaunch infra add accounts cache` and `infra add accounts object-store` (on a service or a Rust worker) write `infrastructure/redis.rs` (`crate::redis_infrastructure::connect()`, `encrypted_cache()`, `encrypted_queue(name)`) and `infrastructure/s3.rs` (`crate::s3_infrastructure::object_store()`, `configured_object_store()`, `encrypted_object_store()`). Compose gets Redis (the project's own database index) or the project's bucket in `minio-init`, every process of the project waits for them, and `.env.local` mirrors the settings for host runs. The encrypted helpers read `ENCRYPTION_KEY`, which `.env` holds; removal keeps it there so values encrypted under it stay readable if a store is added again.

Use `forklaunch infra remove accounts email --path .` to remove a capability. Removal keeps no records: it takes back what `infra add` writes for the service (generated files, the marked `FORKLAUNCH MANAGED CAPABILITIES` sections, hooks, dependency features and `KEY=value` settings) wherever they still have their generated form, so it works the same in a fresh clone. Anything edited since stays, with a note naming it. The local mock has one callback destination per feature; removing its active owner promotes another registered service.

## Export contracts and SDKs

Controllers own their inline contracts. Domain schemas provide the reusable
request/response structures, validation and compliance annotations. A private
contract build host compiles these definitions without starting the service.
Ordinary `cargo check`, `cargo build` and `cargo test` regenerate the service's
OpenAPI document, named TypeScript declarations and Rust client:

```text
src/accounts/
  api/controllers/
  domain/schemas/
  sdk/typescript/{contract.json,openapi.json,sdk.ts,index.ts,package.json}
  sdk/rust/{Cargo.toml,build.rs,src/lib.rs}
```

The generated TypeScript package exports discrete types and uses the existing
universal SDK transport. It does not widen every model to
`Record<string, unknown>`. Client packages take type-only dependencies on these
service packages. For example, after creating the `users` router:

```ts
import { universalSdk } from '@forklaunch/universal-sdk';
import type { AccountsSdkClient } from './src/accounts/sdk/typescript/sdk';

const openapi = await fetch('http://localhost:8000/openapi.json').then(r => r.json());
const accounts = await universalSdk<AccountsSdkClient>({
  host: 'http://localhost:8000',
  registryOptions: { raw: openapi }
});
const users = await accounts.users.listUsers({});
```

The local Rust SDK package reexports named models and a `Client` backed by
`forklaunch_universal_sdk`. Its dependency graph does not include the server's
bootstrapper or persistence implementation. To aggregate local Rust clients:

```sh
forklaunch init library clients --language rust --sdk-services accounts -D "Service clients"
```

The aggregate depends on the selected local SDK packages and reexports them;
it has no separate contract build host. Renaming a service preserves aggregate
import aliases while updating its dependency path.

Explicit export, imported OpenAPI and freshness checks remain available:

```sh
forklaunch openapi export --path .
forklaunch sdk generate --path . --target all --from contract
forklaunch sdk generate --path . --target all --from contract --check
forklaunch sdk generate --path . --target all --from openapi
```

CLI export includes the portable operation descriptors consumed by the shared
SDK runtime, including for custom output paths. With framework v0.1.0, use this
command or the ordinary compilation assets when a consumer needs those
descriptors; the low-level native offline writer emits standard OpenAPI only.

The public `universalSdk` options accept an injected `fetch`,
`maxSseEventBytes` and `maxMultipartResponseBytes`. Portable contracts also accept
default `headers`, an asynchronous `beforeRequest` hook and `serviceBaseUrls`.
Request headers override defaults. Remote plaintext transport requires the
explicit development-only `allowPlaintextDevelopment` option. Fetch is shared by registry
discovery/refresh and API requests; the size limits bound portable response
decoders. `forkLaunchQueryOptions`, `forkLaunchMutationOptions` and
`ForkLaunchSdkHttpError` are exported from `@forklaunch/universal-sdk`.

The root PNPM/Bun workspace includes native SDK packages at the configured
modules path. Existing scripts and dependency versions are preserved. Generated
TypeScript services use their SDK compiler during build to emit both targets;
existing services with custom build hooks require those hooks to export their
artifacts. `sdk watch` preserves the previous SDK if an artifact is missing or
malformed and resumes after it becomes valid. `--service` scopes regeneration,
and `--check` reports stale outputs without writing them.

### Legacy artifact and importer compatibility

Existing manifests can retain shared `.forklaunch/contracts`,
`.forklaunch/openapi` and `sdks/` output paths. The CLI still reads those paths;
new native scaffolds use the project-local layout above. The following importer
and legacy client details describe that compatibility path.

`--from openapi` reads the actual OpenAPI 3.0/3.1 JSON artifact at each project's `openapiPath`; export it first. It uses operation IDs and JSON parameter schemas, with JSON, text, URL-encoded form, multipart or raw binary request bodies, resolving document-local schema graphs with expansion limits. Missing artifacts, external/missing references, unguarded recursive aliases, unsupported identifiers and unsupported body formats fail before SDK files are written. The importer preserves every explicit 200–599 status with a JSON, text, binary, or absent body, including declared business-error payloads. Each status currently supports one media type. TypeScript narrows each status to its payload, and Rust returns a separate enum variant; the lowest status retains the original response model name, with `StatusNNN` model suffixes for other variants. Conflicting generated names and duplicate statuses fail before output writes. Empty responses (including 204/205 and HEAD) resolve to `undefined` in TypeScript and `()` in Rust; explicit JSON `null` remains distinct. Text responses decode as strings; binary downloads decode as `Uint8Array` in TypeScript and `Vec<u8>` in Rust. Declared JSON errors override default error types, including `application/problem+json`. Schema-free JSON remains `unknown`/`serde_json::Value`. Nullable schemas support OpenAPI 3.0 `nullable`, null-containing type arrays, and single nullable `anyOf`/`oneOf` alternatives. Dictionaries retain typed values, and open objects preserve additional fields through Serde flattening. Rust `oneOf`, `anyOf`, and multiple-type schemas generate untagged enums with ordered `Variant1`, `Variant2`, etc. branches, including nested arrays and objects. Serde chooses the first deserializable branch; full schema-constraint validation remains the server’s responsibility. Closed structured objects reject unknown fields during decoding. Nullable root objects use a distinct inner model. Text uploads use `body: string`; raw binary uploads use `body: Uint8Array` in TypeScript and `body: Option<Vec<u8>>` in Rust. JSON, text and URL-encoded Rust requests retain `Option<String>`; form strings must already be URL-encoded. TypeScript URL-encoded form requests accept the schema-derived object or `URLSearchParams`; field records encode composite values with the shared TypeScript `safeStringify` implementation. Clients send the contract media type and preserve binary view offsets. Multipart responses, complete schema-constraint conformance and equivalent framework-native code generator behavior remain parity tasks. Artifact paths must stay inside the application.

Rust model fields retain wire presence: optional non-nullable fields use `Option<T>` and reject explicit null, optional nullable fields use `Option<Option<T>>` (`None` is omitted, `Some(None)` is JSON null), and required nullable fields reject omission. The same rules apply inside arrays, unions and recursive records. JSON responses with generated models are boxed inside status variants to keep response enums small; field access works through dereferencing, while matching the inner union requires `*value`. `rust_sdk_presence_runtime.sh` compares 95 cases against the actual TypeScript validator, both artifact sources, both live SDKs and HTTP/MCP surfaces.

CLI SDKs preserve guarded recursive schemas from contract and OpenAPI artifacts, including self/mutual references, optional and nullable fields, arrays, dictionaries and object/array unions. TypeScript emits named recursive aliases; Rust emits named structs/enums and transparent wrappers for recursive collection types. Rust boxes direct reference edges while vectors provide their own indirection. Acyclic references retain inline types. Generated `Default` implementations are omitted when required recursive edges would make them recurse forever. External references, resource IDs/dynamic references, unsupported validation siblings and unguarded alias cycles fail before writes; reference processing retains node, depth and byte limits. OpenAPI export rebases standalone schema pointers into their component location without rewriting examples, constants or properties named `$ref`. Run `rust_recursive_sdk_runtime.sh` for strict compilation, live recursive payloads through both SDKs/artifact formats and HTTP/MCP startup/shutdown checks. Both framework-native HTTP generators use the same bounded recursive model compiler and run against these live fixtures. Broader schema constraints, resource IDs, anchors and dynamic references remain open.

`forklaunch_core::rust_sdk::generate_rust_http_sdk_source(&plan, &options)` generates a transport-capable reqwest client directly from `SdkGenerationPlan`. The plan retains request/field JSON schemas and response body kind, MIME and JSON schemas from the registry. Native clients share the CLI model compiler, Serde presence rules, client/auth/hooks template and bounded SSE implementation, including typed business-error status variants and separate empty versus JSON-null responses. Their generated crates need reqwest features `json` and `query` (plus `multipart` when used), serde with `derive`, and serde_json. The existing `generate_rust_sdk_source` remains a dependency-free request builder for custom transports. Presence, recursive-model and SSE runtime scripts now also compile and invoke native clients against the same HTTP/MCP services.

Mixed applications select sync, change and deletion behavior from each project's
language and registered path. `forklaunch sync all --confirm` discovers native
and TypeScript projects, restores Cargo members at custom paths, and removes
missing registrations plus their owned local dependencies. Aliased Cargo
dependencies retain their public alias and features. Missing or malformed
exports are reported instead of silently inventing service contracts.

`sync`, `change` and `delete` reconcile Cargo/package membership, Compose,
manifest entries and SDK references while preserving custom application code.
Consumer-only leased workers can change between supported PostgreSQL and Redis
backends while retaining their handler and event schema. Producer-backed and
Apalis queue migrations need an explicit migration design; the CLI rejects an
unsupported conversion before replacing application files.

### Mixed compliance clients

`src/client-sdk/compliance.ts` (or the configured modules path) retains TypeScript client types and accepts
native compliance adapters without importing nonexistent TypeScript factories:

```ts
const compliance = createComplianceClient({
  token: systemToken,
  iam: iamClient,
  catalog: { compliance: sharedSdk.catalog.compliance }
});
const results = await compliance.erase(userId);
```

Provide every registered database-backed service or worker required by the
generated signature. Native adapter responses retain their generated success and
business-error types. TypeScript Better Auth clients keep their `core.compliance`
wrapper; native Better Auth adapters use `compliance` directly. IAM services are
recognized by their variant as well as the conventional `iam` name.

Services created before IAM may lack compliance routes. They require declared
handlers or an explicit adapter implementation; their ordinary SDK client alone
is rejected by the generated TypeScript signature. The factory checks that every
adapter supplies both erase and export methods before dispatching requests.
No registered database service is silently omitted. Fan-out remains concurrent,
and callers must inspect each service's result. The mixed compliance fixture
checks both IAM variants, real legacy/generated SDK HTTP calls, bearer propagation,
response typing, business errors and missing adapters; persistence semantics are
validated separately by the module suites.

## Mixed development

```sh
forklaunch dev
forklaunch dev native-api legacy-api --no-deps
forklaunch dev --dryrun
forklaunch dev native-api --sdk-target typescript --sdk-service native-api
```

The command resolves the application's Compose file and optional `--compose-file`
overrides, then starts the selected services and their dependencies. Generated root package workspaces expose this through `pnpm dev` or `bun run dev`;
existing custom scripts are preserved. If JavaScript tooling lives only under
`modules/`, invoke `forklaunch dev` from the application root. No account login is required.

Native services compile debug binaries inside a Rust development image, with the
application mounted at `/app` and Cargo output/registry/git caches in separate
project-scoped Docker volumes. Development containers are never shipped; deployments
build from the generated Dockerfiles.

Rust reloads happen inside the running containers, without restarting them:

- One `forklaunch-native-build` container builds the whole selection once (the
  warm-up), then keeps watching the mounted sources with a pinned
  [watchexec](https://github.com/watchexec/watchexec) (2.7.4, checksum-verified and
  cached in the image). Registered native crate paths, libraries, a local
  `--framework-path` checkout, native SDKs and Cargo manifests are watched; `target/`,
  `node_modules/` and generated SDK artifacts are not.
- On a change it runs one incremental `cargo build` (dev profile) for every selected
  package and binary. Only this container runs Cargo, so service containers never
  wait on Cargo's build lock, and features resolve the same way as in the warm-up.
- After a successful build it marks each binary Cargo relinked. Each service runs its
  binary under `forklaunch-native-run --watch`, which restarts only that process when
  its binary changes. Edit a handler and only that service restarts; edit a shared
  crate and every dependent restarts. The log reports `forklaunch dev: built in Ns;
  restarting: ...`.
- A failed build changes nothing: every process keeps serving its last good build.
  The builder container counts as healthy after its first successful build. Authored
  service healthchecks apply unchanged, because the running process keeps serving
  while a rebuild runs.
- Migration and seed jobs run once per `forklaunch dev`, after the warm-up.
- Native services get no Compose restart rules; authored `develop.watch` rules are kept.
  Restart `forklaunch dev` after changing `rust-toolchain.toml` (the image installs the
  toolchain) or adding a crate (the watch list is fixed at start).

Host edits do not reach containers as inotify events on Docker Desktop or OrbStack, so
on macOS and Windows the builder polls the mounted sources every 500 ms. Set
`FORKLAUNCH_DEV_POLL_MS` to change the interval, or to `off` (Linux hosts use native
events by default). Each edit is picked up within one poll interval.

`cargo dev` is not a built-in Cargo command, and an alias can only run other Cargo
subcommands. The development loop is the dev profile under the in-container watcher.
To get the same loop on the host, run `watchexec -r -- cargo run -p <crate>`.

Native auxiliary workers keep their launch arguments. TypeScript services keep their
existing Compose development commands and reload behavior. Ctrl-C or SIGTERM
stops Compose before the CLI removes its private temporary configuration; the
application's Compose files and production Dockerfiles are preserved.

Use `--export-contracts` to export native contracts and native/TypeScript OpenAPI
after each successful build and before the service starts. Exports use registered artifact
paths, stage both files before publication, and preserve unchanged outputs. An export
failure keeps prior artifacts and prevents that startup from serving stale contracts.

`--sdk-target typescript|rust|all` also enables these exports and starts HTTP SDK
watch as a child of development. `--sdk-service NAME` optionally scopes generation;
without it, SDK watch includes all registered HTTP services. The CLI stops its SDK
watcher when development exits. A standalone `forklaunch sdk watch` also waits for
missing initial artifacts; `--once` still reports missing or invalid artifacts as
an error. Watch compares content, so identical exports do not trigger regeneration.

Automatic export covers selected native services, their auxiliary workers, and
TypeScript services. TypeScript exports use the service image’s installed Node or
Bun runtime after rebuilding its workspace, including shared libraries. Generated
outputs and dependencies use private container volumes. Shared source edits trigger
rebuild, export and restart. Combined SDKs read the explicit default document in
TypeScript version bundles; named-version selection remains unsupported. Dotted
operation IDs retain their namespace in generated method names. This option also
refreshes registered native socket artifacts as their HTTP contract export changes.
Use `--sdk-source websocket` or `--sdk-source all` with `--sdk-target` to watch
WebSocket SDKs alone or alongside HTTP SDKs. Both surfaces regenerate sequentially
in one owned watcher. `sdk generate/watch --from all` provides the same combined
artifact selection outside development. TypeScript socket artifacts still need
their existing explicit export workflow.

Node and Bun mixed-service fixtures validate HTTP/MCP before and after source edits,
shared-library contract/SDK refresh and owned watcher shutdown. The Bun fixture also
checks native socket artifact/SDK creation, edits, removal and restoration. Separate
TypeScript-only replays validate shared-library reloads; Bun covers `--no-build` image
reuse with Compose watching. See the parity inventory for the retained evidence.

Compose watch (TypeScript restarts and authored rules) requires
[Docker Compose 2.32 or later](https://docs.docker.com/reference/compose-file/develop/).
Use `--no-watch` to start without reloads (one build, then each process starts once), `--no-build` to reuse images,
and `--project-name` to select the Compose project. With reloads enabled,
`--no-build` requires existing local images for build-backed services; it removes
image build definitions from the private development plan and retains restart
watches. Custom `rebuild` watch rules are rejected for this combination.
`--no-deps` starts only the
named services, plus the native builder when any of them is native; configure required databases or providers through the existing
Compose/environment settings. Automatic native startup supports generated binary
commands; custom entrypoints require an explicit development setup. Dry-run shows
native and TypeScript changes without starting containers or writing application files.

## Portable source and containers

New applications use exact published framework versions by default. The 13
framework packages are listed in [the release guide](rust-releases.md).
`--framework-snapshot` explicitly embeds framework source under
`vendor/framework`; `--framework-path` selects a development checkout. These
source modes are alternatives to `--framework-version`. Local folders omit the
published crate prefix.

Selected modules/workers still use local `.forklaunch/blueprint-rs` sources. Those 28 blueprint
crates are excluded from the framework release. Commit these required local
sources and `Cargo.lock` with an application.

```sh
cargo generate-lockfile
docker compose build accounts
docker compose up accounts
```

The adjacent root `Dockerfile.rust` and `Dockerfile.typescript` each build the
relevant projects and required local dependencies. Compose selects the process
command. The Rust runtime image runs as UID 65532; compiler concurrency defaults
to two and is configurable through the `CARGO_BUILD_JOBS` build argument.
A custom project Dockerfile takes precedence in release metadata with its
project-local build context. Container deployment to ECS is separately tracked.

## Current boundaries

The published framework and current CLI generation/lifecycle checks have
separate receipts in [the release tracker](../../plan/RUST-CRATE-RELEASE-PLAN.html).
Publication does not imply complete historical feature parity: broader Better
Auth hooks/plugins, provider crash reconciliation and production deployment
remain tracked in `plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md`. Older runtime
fixtures are being aligned to the current scaffold layout; consult the tracker
for tests actually executed, rather than treating every retained script as a
passing current CI result.

## Validate the port

```sh
cd framework-rs
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ../blueprint-rs
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ../cli
cargo test
cargo build --release
cd tests
export FORKLAUNCH_CLI="$(cd ../target/release && pwd)/forklaunch"
bash init_rust_projects.sh
for script in rust*.sh; do bash "$script"; done
```

The Rust E2E job runs generation and lifecycle scripts, checks generated TypeScript SDKs, and calls a relocated Rust service through both generated TypeScript and Rust SDKs. The Rust client checks typed JSON models, concurrent requests, HTTP error responses, and transport failures. Both SDKs also call a compiled TypeScript HTTP wire fixture and compare its responses with the Rust service; full TypeScript framework conformance remains a separate task. Generated Rust fields use idiomatic names with explicit Serde mappings to the original JSON property names. Normalized field and nested-model collisions receive deterministic numeric suffixes; punctuation and reserved Rust keywords remain valid identifiers without losing wire values. Both SDK targets substitute whole path parameters and percent-encode their UTF-8 values, including slashes and query/fragment delimiters. SDK checks require TypeScript (`TSC_BIN` can point to an installed compiler); the live HTTP check also requires Node.js with built-in `fetch`.

## Authentication and verified sessions

Rust contracts and Axum support all 27 methods exported by the TypeScript router: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE, CHECKOUT, COPY, LINK, LOCK, M-SEARCH, MERGE, MKACTIVITY, MKCOL, MOVE, NOTIFY, PROPFIND, PROPPATCH, PURGE, REPORT, SEARCH, SUBSCRIBE, UNLINK, UNLOCK and UNSUBSCRIBE. Use a typed registration method or `ForkLaunchRouter::route(contract, handler)`; mismatched typed methods and duplicate registrations fail at construction. Rust uses `m_search` and `move_resource` for those method names. Extended methods share request validation, authentication, nested paths, telemetry and SDK generation. OpenAPI exports them under the path's `x-forklaunch-http-methods` extension; the CLI importer understands this extension and rejects unknown method tokens. Browser and Node Fetch forbid TRACE, so TypeScript callers need an injected transport for that method. Ordinary OPTIONS requests reach their handler and auth policy; browser CORS preflights retain their configured CORS behavior.

New IAM, CAC and messaging modules expose `routes::contracts()` and `routes::register_router(...)` hooks. `forklaunch init router custom --path src/cac --language rust` creates an Axum extension whose contracts appear in offline exports and SDKs. Each extension exports `contracts()` and `register(...)`; update both when adding handlers. Rename, sync and delete retain this module structure, and rename preserves edited handler code. Existing modules without these hooks reject router generation before writing files; add the hooks to their library registry and runtime router first. Custom routes begin public; declare their authentication/access policy in the contract as required.

Axum enforces route `auth`, RBAC, and non-public access requirements before running handlers. A protected route without a configured verifier returns 401. Disabling request schema validation does not disable authentication; implicit HEAD requests use their GET route's policy while authentication sees the actual HEAD method. Explicit HEAD routes take precedence.

Configure `RouterConfig::with_authenticator` with an async credential verifier. It receives the framework request and the route contract, including the declared auth method and custom header settings. Return `AuthenticatedSession` only after verification; Axum then evaluates the route's roles, permissions, scope hierarchy, features, and subscription requirements. Native handlers can extract `Session<MyClaims>`; local framework handlers receive verified claims in `request.session`.

`forklaunch_core::auth::jwt::verify_jwk` and `verify_jwks` accept typed claims and an explicit `jsonwebtoken::Validation` policy. Set the allowed algorithms, issuer, and audience in that policy. The optional `remote-jwks` feature adds `RemoteJwks`, with one cache per configured endpoint, bounded response sizes/timeouts, Cache-Control and Age handling, refresh deduplication, and a refresh cooldown for unknown key IDs. Remote verification rejects symmetric algorithms and never follows token-provided key URLs. Share one verifier across requests rather than constructing a cache for each request.

`forklaunch_auth::request_hmac` signs and verifies TypeScript-compatible `HMAC keyId=… ts=… nonce=… signature=…` request headers. It binds the exact method, path without query, transmitted body, normalized millisecond ISO timestamp and nonce. Serialize JSON once and sign/send the same bytes. `SignedBody` preserves the existing TypeScript distinction between absent/empty string bodies and raw buffers. `HmacAuthorization::verify_at` also bounds clock skew; atomic nonce replay claims across replicas remain application-owned. Actual TypeScript signer and header-generator fixtures cover all of these wire forms.

Basic login and credential verification callbacks remain application-configured. `parse_basic`, `hmac_verify`, and JWT verification helpers provide credential primitives; provider integrations and the remaining IAM/session lifecycle are tracked separately.

## Configuration

Generated Rust code reads settings with `forklaunch_core::config`, never with
in-code defaults. `config::required::<T>(NAME)` reads and parses a variable and
fails startup when it is unset, empty or malformed; `config::optional::<T>(NAME)`
is used only where absence is a documented choice (for example JWT issuer and
audience constraints, `PUBLIC_ORIGIN`, PayPal, Stripe Connect, worker HTTP
controls, retention ages). Errors name the variable and the expected type and
never include its value:

```rust
use forklaunch_core::config;

let port: u16 = config::required("PORT")?;
// environment variable PORT is not set; it is required at startup
```

`ConfigError` converts into `FrameworkError`, `BlueprintError`, `io::Error` and
generated `AppError`/`WorkerError` values. The values themselves come from the
generated `docker-compose.yaml` environment (used by `forklaunch dev`), each
service's git-ignored `.env.local` (HTTP listener settings for `cargo run`) and
the `.env.example`/`.env.database.example` files. Framework runtime readers follow
the same rule: `RUST_LOG`, `MCP_ENABLED`, `DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT`,
`ENCRYPTION_KEY_ID` and `ENCRYPTION_LEGACY_KEY_HEX` are required, and contract
export requires both `FORKLAUNCH_CONTRACT_OUTPUT` and `FORKLAUNCH_OPENAPI_OUTPUT`
(the CLI always sets them). OpenTelemetry `OTEL_*` variables keep their
specification-defined meaning.

## Telemetry

`forklaunch_core::telemetry::OpenTelemetryCollector` records counters, gauges, histograms, and up/down counters, and exposes its meter for observable instruments. It supplies structured trace/debug/info/warn/error/fatal logging through `tracing`.

Inject a collector through `RouterConfig.telemetry` to use application-owned OpenTelemetry providers. `with_meter` and `with_tracer` accept configured SDK instruments without replacing process-global providers. Generated services, IAM/CAC/messaging modules and workers initialize `TelemetryRuntime` from the optional `forklaunch-core/otlp` feature. It installs structured JSON stderr logging, W3C trace-context/baggage propagation, and application-owned providers. With no endpoint or explicit signal exporter, telemetry export stays disabled. Offline contract/OpenAPI export runs before telemetry initialization.

HTTP middleware emits server spans, request counts/duration, active requests, actual response-body bytes, auth failures, and error counts. Streaming responses retain their span and active-request measurement until completion, body failure, or cancellation, without buffering. Body failures and client cancellation preserve the HTTP status already sent while recording a failed span. HEAD and bodyless statuses finish without awaiting a body. Metric dimensions use matched route templates instead of user identifiers. Correlation IDs are attached to spans/logs rather than metric dimensions. Active requests and spans also close when handler futures are cancelled. Set `OTEL_EXPORTER_OTLP_ENDPOINT` to enable traces, metrics and logs using OTLP HTTP/protobuf, or set `OTEL_EXPORTER_OTLP_PROTOCOL=http/json`. Signal-specific endpoints/protocols and standard OTLP headers, resource attributes, service name and export intervals are supported. `OTEL_TRACES_EXPORTER`, `OTEL_METRICS_EXPORTER` and `OTEL_LOGS_EXPORTER` accept `otlp` or `none`; `OTEL_SDK_DISABLED=true` disables export. `OTEL_PROPAGATORS` accepts `tracecontext`, `baggage`, their comma-separated combination, or `none`. Unsupported configurations fail with sanitized errors.

Generated executables drain work and then flush every provider on both success and failure, using Tokio’s blocking pool for the blocking HTTP exporter. A runtime is installed once per process; reusable libraries continue to accept injected collectors. Receiver tests cover both encodings, authenticated headers, resource metadata, actual HTTP request spans and metrics, structured logs, incoming parent context and SIGTERM flushing. gRPC exporters are not enabled by this feature.

## Redis adapter

Enable the `redis-client` feature of `forklaunch-infrastructure-redis` to use `client::RedisTtlCache` and `client::RedisQueue`. The cache uses Redis expiration, binary-safe reads/writes, atomic batch writes, and SCAN-based literal prefix listing. The queue uses atomic Redis list operations and preserves FIFO order across clients. Connections have bounded connect/read/write timeouts and support TLS URLs.

These adapters implement synchronous operations; call them from a blocking executor in async services. The basic FIFO queue removes items on dequeue. It does not provide worker leases, acknowledgment, retry scheduling, or dead-letter handling; use the async Redis worker adapter below for those operations.

## PostgreSQL IAM

Enable the `postgres` feature on `forklaunch-iam-base` to use `postgres::PostgresIamService`. Apply `PostgresIamService::migrate(&owner_pool)` with a schema-owner connection, then create the service with an application pool and a validated `forklaunch_core::persistence::TenantContext`. Runtime connections must use a role without SUPERUSER or BYPASSRLS. Bind the tenant from verified application context.

The asynchronous service persists users, organizations, roles, and sessions. Passwords use salted Argon2id; hashing runs on blocking tasks. Session tokens are random and stored as SHA-256 digests, expire after one hour, and are checked against current user status. Password changes revoke existing sessions atomically. Every query includes its tenant and uses transaction-local context; migrations enable and force row-level security for reads and writes. The container test exercises a restricted role and connection-pool reuse.

`AsyncIamService`, `AsyncIamSessions`, and `AsyncIamRepository` expose asynchronous operations independently of the synchronous reference interface. `forklaunch-iam-base`'s `http` feature serves these traits with opaque bearer authentication and typed contract schemas. Runtime construction rejects PostgreSQL SUPERUSER and BYPASSRLS roles.

Generate a persistent IAM module in an existing Rust or mixed application:

```sh
forklaunch init module --language rust -m iam-base -d postgresql --path .
DATABASE_URL=postgres://schema_owner:password@localhost/app cargo run -p iam -- --migrate
# Export ENCRYPTION_KEY, HMAC_SECRET_KEY and JWKS_PUBLIC_KEY_URL first.
DATABASE_URL=postgres://restricted_app:password@localhost/app TENANT_ID=acme cargo run -p iam
```

Replace the example URLs with your own credentials. Migration and runtime connections use separate database roles. Generated modules bundle blueprint source in `.forklaunch/blueprint-rs`; commit it alongside the required parser patch. Contract/OpenAPI export works without a database connection.

The scaffold binds one trusted `TENANT_ID` per process. Login, logout, paginated users and organizations, and permission lookup use PostgreSQL. Provision users through the administrative repository API; the generator creates no default account. The configured `/user`, `/role`, `/permission`, and `/organization` families add thirty directory CRUD and grant-surfacing routes, shared by HTTP and MCP. Internal operations verify request HMAC signatures; protected operations verify JWKS JWTs and declared roles/permissions. User reads and mutations resolve organization membership from storage and enforce it transactionally. Directory users remain separate from credential/session users; the configured TypeScript CRUD mapper ignores passwords, and the native mapper follows that behavior. Sensitive directory fields use identity-bound encryption, rotated-key uniqueness claims and atomic actor audits. Grant the runtime role access to `fl_iam_directory`, `fl_iam_directory_unique_claims`, `fl_iam_directory_audit` and USAGE on `fl_iam_directory_sequence_seq` in addition to the session tables. The legacy `/users/:id` SDK/MCP operation is named `getSessionUser`; `/user/:id` uses `getUser`. Tenant selection from verified identity, cookie/CSRF sessions, email/reset/verification flows, JWT issuance, and OAuth/OIDC providers remain pending.

Base IAM also exposes public `/discovery/auth-methods` (204) and system-role JWT compliance export/erase routes. Its configured entities classify every scalar as `none`, so compliance returns 404 without reading or deleting directory data. A direct comparison executes the actual TypeScript compliance service against these source classifications. Application-specific compliance policies remain separate work.

`cli/tests/rust_iam_runtime.sh` checks a relocated generated module against PostgreSQL through its TypeScript SDK, including all thirty directory routes, HMAC and JWT authorization, organization scoping, atomic mixed-organization rejection, authenticated MCP CRUD, cross-tenant rejection, session persistence across restart, revocation, and SIGTERM shutdown. It accepts `FORKLAUNCH_TEST_POSTGRES_URL` or starts its own isolated PostgreSQL container.

## CAC module

Generate `forklaunch init module cac --language rust -m cac-base --path .`.
IAM and CAC can coexist in a language-neutral application.
Set `HMAC_SECRET_KEY` and trusted `JWKS_PUBLIC_KEY_URL`, then run `cargo run -p cac`.
Offline exports require neither key configuration nor network access.

`GET /codeSet` describes the free mock provider; `GET /codeSet/:code` looks up the
three upstream placeholder codes. Both use TypeScript-compatible request HMAC,
key ID `default`, and five-minute timestamp skew. The module includes no licensed
CPT data. `GET /compliance/export/:userId` and `DELETE /compliance/erase/:userId`
verify asymmetric JWTs and require the `system` role. The current TypeScript
skeleton registers no entities, so authorized compliance requests return its
matching text 404. There is no invented clinical schema or database migration.

Roles come from verified issuer claims by default. Role resolvers receive the complete verified payload, including organization context. Applications can configure
`HmacJwksAuth::with_role_resolver` (the module's `config.rs` builds it) to surface roles through trusted IAM instead
of the TypeScript template's placeholder admin-role function. Optional
`JWT_ISSUER` and `JWT_AUDIENCE` add issuer/audience constraints. Expiry and `nbf`
are validated. JWKS keys are cached per endpoint and may use RSA, EC or EdDSA.

The mounted registry drives both SDKs and OpenAPI. Runtime fixtures exercise HMAC,
JWT/JWKS/RBAC, both generated SDKs, relocation, embedded-workspace upgrades,
module coexistence, source preservation, rename/sync/delete and graceful shutdown.

## Email and SMS

Messaging is not a module in either language. Add the managed gateway to a
service with `forklaunch infra add <service> email` or
`forklaunch infra add <service> sms`; see
[managed capabilities](#managed-capabilities). `messaging-base` and
`messaging-twilio` are retired and `init module` rejects them.

## Field encryption compatibility

Enable `forklaunch-core`'s `encryption` feature for `persistence::encryption::FieldEncryptor`. It supports the TypeScript field encryptor's tenant-derived HKDF keys, v1–v4 envelopes, previous-key rings, rotation, and blind indexes. Deterministic v2/v3 ciphertext and blind indexes match TypeScript byte for byte. Sealed v4 values use random nonces. Use a high-entropy master key and preserve previous keys while rotating stored data.

The existing v4 format contains clear, unauthenticated anonymous metadata. Treat it as display metadata and never as an authorization claim. Deterministic encryption and blind indexes reveal equality within their key/tenant scope.

`framework-rs/parity/rust_field_encryption_parity.mjs` runs the actual TypeScript implementation alongside a Rust harness, testing both directions with empty, Unicode, binary-containing, current-key, previous-key, and legacy values. It needs a TypeScript compiler that exposes the compiler API; CI uses TypeScript 5.9.3 via `TSC_BIN`.

## PostgreSQL worker queue

Enable `forklaunch-worker-database`'s `postgres` feature and import `PostgresWorkerQueue` from its `postgres` module. It implements `forklaunch_worker_interface::AsyncWorkerQueue`; the synchronous `DatabaseWorkerQueue` remains an in-memory reference implementation.

Apply `PostgresWorkerQueue::migrate(&owner_pool)` before starting consumers. Worker tables and the checksummed migration ledger live in the `fl_worker` schema, independently of IAM migrations. Concurrent migration runners serialize through a transaction-scoped advisory lock, and migration transactions restore the connection's search path.

Runtime roles need `USAGE` on `fl_worker`, `SELECT, INSERT, UPDATE, DELETE` on `fl_worker.jobs`, and `USAGE` on its sequences. The constructor rejects SUPERUSER and BYPASSRLS roles. Construct the queue with a validated, trusted `TenantContext` and a `RetryPolicy`; every operation includes its tenant and runs with transaction-local RLS context.

`enqueue_batch` is atomic and preserves insertion order. IDs are unique within a tenant and queue; duplicates return an error. Enqueued jobs must start with zero attempts and valid JSON payloads of at most 1 MiB. Batches and peek limits are bounded at 1,000 jobs.

`claim` uses `FOR UPDATE SKIP LOCKED` and returns a `JobLease` with a fresh opaque token. Claims increment attempts. `ack`, `fail`, and `renew` check the token and the database deadline; an expired or replaced consumer cannot change the delivery. Leases range from 1 millisecond to 24 hours. Renewal never shortens the current deadline; inspect the job to read an updated deadline.

Call `ack` only after successful processing. On failure, `fail` schedules the configured fixed backoff or retains an exhausted job with `JobStatus::Failed` and its failure reason. Retry policy is stored when enqueuing, so another consumer cannot change an existing job's budget. Expired leases become eligible for redelivery; a claim also moves expired final attempts to failed state. `inspect` reads completed and failed jobs without exposing the lease token. Failure reasons are persisted; use application-safe text.

Delivery is at least once. Make handlers idempotent because a consumer can crash after performing a side effect and before acknowledging it. Renew leases during long-running handlers. Queued statistics include delayed retries; processing statistics include expired leases until the next claim recovers or fails them. `peek` returns currently eligible jobs without claiming them.

The adapter is available to application code. New PostgreSQL worker binaries use the runner below. Dead-letter administration and explicit retention are described below; Kafka durable adapters and distributed control signals remain tracked separately. CI runs `postgres_worker_leases_retries_isolation_and_concurrency` against PostgreSQL, covering concurrent consumers, tenant RLS, retries, crash recovery, stale tokens, batch rollback, and compatibility with IAM migrations.

## Generated PostgreSQL workers

Worker generation normally includes an accompanying producer service and
consumer binary. The examples below explicitly select `--consumer-only` for
queue/runtime operations. This mode emits typed event assets without a
fabricated HTTP SDK. Select `--runtime apalis` for the native Apalis worker
runtime; its producer and consumer are distinct commands in the shared image.


```sh
forklaunch init worker jobs --language rust -t database -d postgresql --consumer-only --path . -D "Jobs"
```

New workers with this backend bundle blueprint source and run a continuous polling loop through `forklaunch-worker-database`'s `runtime` feature. Apply migrations with `cargo run -p jobs -- --migrate` using the owner connection. Start the worker with a restricted `DATABASE_URL` and trusted `TENANT_ID`. Implement application work in `src/jobs/domain/services/handler.rs`; the initial handler fails with `worker_handler_not_implemented` and leaves jobs subject to their retry budget instead of discarding them.

The runner processes up to `WORKER_CONCURRENCY` jobs at once (1–256) and renews each active lease while its handler runs. Multiple processes can consume the same queue named by `WORKER_QUEUE`. Every runner setting is required (`.env.example` and Compose list the generated values). Retry policy uses `WORKER_MAX_ATTEMPTS` and `WORKER_BACKOFF_SECONDS`; polling, leases, renewal, and shutdown drain use `WORKER_POLL_MS`, `WORKER_LEASE_MS`, `WORKER_RENEW_MS`, and `WORKER_DRAIN_MS`. Invalid settings fail startup. Queue operations have bounded timeouts. Renewal must run at least three times per lease duration.

Ctrl-C or SIGTERM stops new claims and drains the active job while renewing its lease. If draining times out, the runner cancels the handler and leaves the delivery recoverable after lease expiry. It also cancels a handler when lease renewal reports an obsolete token. Keep all handler work inside its returned future and make external side effects idempotent. Application error codes are retained; detailed handler errors are not automatically persisted.

With runtime environment variables set, use `cargo run -p jobs -- --enqueue ID JSON` to submit a job and `--inspect ID` to read its state. Inspection omits payloads and lease tokens. Contract/OpenAPI export works without a database connection. Reinitialization, sync, and worker rename preserve the application handler and Cargo metadata; rename updates generated SDKs and contract artifacts.

Existing demonstration workers are preserved when reinitialized. To upgrade explicitly, run `forklaunch change worker --path src/jobs -t database -d postgresql --confirm`; backend conversion installs the runner while retaining an existing handler and unrelated Cargo metadata. Batch handlers and optional worker HTTP health/control endpoints are described below. Optional scoped distributed signals and durable Kafka workers are described below. `cli/tests/rust_worker_runtime.sh` tests a relocated generated worker against PostgreSQL, including renewal, drain, retries, forced cancellation, process crashes, tenant isolation, source preservation, rename, and delete.

## Redis leased worker queue

Enable `forklaunch-worker-redis`'s `redis-client` feature and import `leased::RedisLeasedQueue`. Call `RedisLeasedQueue::connect(url, namespace, trusted_tenant, retry_policy).await` and use `AsyncWorkerQueue` operations. Enable `forklaunch-worker-database`'s `runtime` feature to run the shared async runner with this adapter.

Redis scripts atomically claim jobs, fence acknowledgments and renewal with delivery tokens, schedule retries, and retain completed and failed jobs. A job's retry policy is persisted at enqueue time. Mixed-queue batches validate duplicate identities and Redis key types before writing; identities are unique within a tenant, namespace, and queue. Each claim or peek recovers up to 128 expired leases to bound script execution; larger expired backlogs recover over successive polls. Delivery is at least once, so handlers must be idempotent.

Tenant and namespace names are encoded into Redis keys. This provides logical isolation through trusted application code; configure Redis ACLs separately. Persistence and replication settings determine durability across Redis server restarts. The adapter uses a standalone Redis connection manager with bounded waits and TLS URL support. Keys share a cluster hash slot, but Redis Cluster redirection support is not implemented. Keep payloads small; the adapter caps each payload at 1 MiB and batches at 1,000 jobs.

Real Redis integration tests cover client restart, mixed-queue batches, collision rollback, FIFO ordering, ten concurrent consumers, retry budgets, stale leases, tenant isolation, and shared runner renewal during shutdown drain. Generated Redis workers use this adapter and the shared async runner.

## Generated Redis workers

```sh
forklaunch init worker jobs --language rust -t redis --consumer-only --path . -D "Jobs"
REDIS_URL=redis://localhost:6379/ WORKER_NAMESPACE=my-app TENANT_ID=acme cargo run -p jobs
```

The worker uses published framework dependencies and local blueprint sources, requires a trusted tenant and namespace, and runs continuously until Ctrl-C or SIGTERM. Implement idempotent application work in `src/jobs/domain/services/handler.rs`; the initial handler fails explicitly until implemented. Timing, retry settings, `--enqueue ID JSON`, and `--inspect ID` match the PostgreSQL worker. Redis requires no migration command. OpenAPI and contract export work without Redis credentials.

Reinitialization and sync preserve application code. Rename updates imports, artifacts, SDK names, and Compose entries. Explicit backend changes support both Redis and PostgreSQL while preserving existing handlers and unrelated Cargo metadata. Upgrade an existing demonstration worker with `forklaunch change worker --path src/jobs -t redis --confirm`. Existing documentation and example env files are preserved during conversion; configure the new backend's environment variables before starting it. `bullmq` selects the official BullMQ protocol adapter described below; it uses a separate storage protocol from native leased Redis queues.

`cli/tests/rust_redis_worker_runtime.sh` validates a relocated generated worker against Redis, including process crashes, lease recovery, renewal, shutdown drain, retry exhaustion, tenant isolation, rename, backend conversion, and delete.

## Generated BullMQ workers

```sh
forklaunch init worker jobs --language rust -t bullmq --consumer-only --path . -D "Jobs"
REDIS_URL=redis://localhost:6379/ REDIS_KEY_PREFIX=my-app cargo run -p jobs
```

The optional `forklaunch-worker-redis/bullmq` feature uses the official BullMQ Rust client and exchanges jobs with the pinned TypeScript BullMQ protocol. Implement the preserved `handler::ApplicationHandler`; its default fails explicitly. Event IDs become job names, and protocol job IDs are auto-generated, so duplicate event IDs remain separate jobs. Handlers receive the full JSON event and current attempt count. Default retries are three attempts with exponential 5000 ms backoff; configure `WORKER_MAX_ATTEMPTS`, `WORKER_BACKOFF_TYPE`, `WORKER_BACKOFF_MS`, `WORKER_CONCURRENCY` (1–1024), operation/handler timeouts and `WORKER_DRAIN_MS` in `.env.example`.

Use `--enqueue ID JSON`, `--peek LIMIT`, `--dead-letters LIMIT` and `--migrate` (connectivity check; BullMQ creates structures lazily). Peeks contain event payloads; failed-job inspection includes payloads and attempt counts. Completed/failed jobs remain retained by default. Library users can pass official queue/worker options for retention and advanced behavior; count-based retention is covered by actual TS/native fixtures. `BullMqConsumer::peek_events` works before starting, and `run_until_with_failure_handler` reports every failed attempt with its event, including removed jobs and graceful drain; diagnostic errors are redacted.

Optional HTTP health/readiness and authenticated pause/resume/drain, scoped Pub/Sub controls and graceful active-job shutdown are supported. Init/reinit/sync/backend conversion/rename/delete preserve application handlers and registered nested crate paths. Conversion does not migrate a native leased Redis backlog into BullMQ. Actual TS/native fixtures validate both producer/consumer directions, FIFO batches, fixed/exponential retries, default/count-based retention, failure callbacks, waiting/active inspection and restricted Redis ACL rejection. Age-based retention, stalled jobs and complete advanced-option conformance remain tracked separately.

## Worker concurrency

Both generated PostgreSQL and Redis workers accept `WORKER_CONCURRENCY` (1–256). Each active job has its own lease; batches renew their leases together. On shutdown every consumer stops claiming jobs and drains its active handler within `WORKER_DRAIN_MS`; work still queued remains available for the next process. A storage failure stops the other consumers and drains their active work before the runner returns the first error.

For direct library use, set `RunnerConfig::concurrency`. `RunReport` combines counts from all consumers. Handlers share the same `JobHandler` instance and must support concurrent calls, keep asynchronous work inside the returned future, and make external side effects idempotent. This setting controls concurrent async handler batches; use separate processes or a blocking executor for CPU-heavy work. Batch handlers are described below. Optional distributed controls are described below.

## Dead letters and retention

Both durable queue adapters implement `forklaunch_worker_interface::AsyncWorkerAdmin`. `dead_letters(queue, limit)` returns retained failures in terminal-time order; ordering within a timestamp tie is backend-specific. `retry_dead_letter(queue, id, policy)` atomically requeues a failed job with an explicit new budget, resets attempts and failure text, and fences old lease capabilities. Concurrent retries of the same failure have one winner.

`purge_terminal(queue, TerminalStatus::Completed | TerminalStatus::Failed, minimum_age, limit)` permanently deletes at most the requested number of terminal jobs older than the given duration. It preserves queued and processing jobs. Limits must be 1–1000 and ages must be at least 1 ms and at most 3650 days. Deletion frees identities for resubmission. Invoke bounded calls from an operator or scheduler, or opt into the periodic policy below. Automatic retention is disabled by default.

Generated workers expose these commands using their configured tenant and queue:

```sh
cargo run -p jobs -- --dead-letters 100
WORKER_MAX_ATTEMPTS=3 WORKER_BACKOFF_SECONDS=30 cargo run -p jobs -- --retry JOB_ID
cargo run -p jobs -- --purge-completed 604800 100
cargo run -p jobs -- --purge-failed 2592000 100
```

Listing and inspection print summaries without payloads or lease tokens. Retry uses `WORKER_MAX_ATTEMPTS` and `WORKER_BACKOFF_SECONDS`. Purge command ages are positive integer seconds; output reports the number removed. Run PostgreSQL migrations with the owner connection before upgrading runtime workers; the new migration adds the terminal-retention index without changing existing terminal timestamps.

Redis adds terminal indexes and incrementally upgrades legacy retained records on first administration use. Legacy terminal timestamps were not recorded, so their retention age starts when first observed; existing jobs are not prematurely deleted. Each indexing script scans one page and the client bounds the entire operation by its timeout. An interrupted upgrade resumes from its stored cursor on the next call. Upgrade all consumers before using administration: older consumers do not maintain the new terminal indexes. Redis isolation continues to depend on trusted tenant binding and deployment ACLs.

Integration tests cover stale capabilities after retry, changed retry budgets, concurrent retry/purge races, tenant isolation, bounded purges, protection of active and queued jobs, identity reuse, and legacy Redis indexing. Generated-process tests exercise the commands against both Redis and PostgreSQL.

## Automatic worker retention

Generated Redis and PostgreSQL workers opt into periodic cleanup when `WORKER_COMPLETED_RETENTION_SECONDS` and/or `WORKER_FAILED_RETENTION_SECONDS` is set to a positive age. Each unset status is retained indefinitely; with neither set there is no automatic deletion. Enabled retention requires `WORKER_RETENTION_INTERVAL_MS`, `WORKER_RETENTION_BATCH_LIMIT` (per status per tick) and `WORKER_RETENTION_TIMEOUT_MS`; `.env.example` lists 60000, 100 and 5000. Invalid ages, limits, intervals, and timeouts fail startup.

For direct library use, enable the database worker's `runtime` feature and call `retention::run_worker` with a `RetentionPolicy`, or use `retention::run` for standalone maintenance. Cleanup starts after the first interval and remains scoped to the queue's tenant. Reports count completed and failed deletions separately. Shutdown cancels maintenance and drains current handlers with lease renewal; a cleanup error or timeout also stops new claims and drains active handlers before returning the first error. Newly completed jobs during shutdown drain are retained because maintenance has already stopped.

Real backend tests check default preservation, independent completed/failed ages, tenant isolation, bounded cleanup, and active/queued job protection. Runtime tests check maintenance timeouts and cancellation; a Redis fault test verifies handler drain after maintenance failure. Generated-process suites validate opt-in cleanup and coordinated shutdown against both backends.

## Process-local worker controls

With the worker database crate's `runtime` feature, create a `control::WorkerControl` and pass it to `runner::run_controlled` or `retention::run_worker_controlled`. Clones share state across the process's consumers. `pause()` stops future claims while active handlers and claims already submitted to storage continue. Leases renew throughout processing. `resume()` restarts polling. `drain()` stops new claims and finishes active handlers within the configured drain timeout; it cannot be reversed. Pause/resume return false once draining starts. A new run needs a new control.

Retention continues while paused and stops as soon as draining starts. Shutdown signals and storage failures also set the control to draining. These control primitives are local to one runner process; generated workers can opt into distributed signaling as described below. Real Redis and PostgreSQL integration tests verify paused startup, active lease renewal, pause without cancellation, resume, sticky drain, and preservation of queued work.

## Worker HTTP health and controls

Set `WORKER_HTTP_ADDR` to an explicit socket address, for example `127.0.0.1:8081`, to enable the internal worker listener. HTTP is disabled by default. `GET /health` returns 200 with the process state (`running`, `paused`, or `draining`). `GET /ready` returns 200 while running and 503 while paused or draining. Readiness describes consumer state; it does not probe storage connectivity. Responses contain no job payloads, credentials, or tenant identifiers.

Health and readiness need no token. Controls are disabled (404) unless `WORKER_CONTROL_TOKEN` is configured with 32–4096 printable ASCII characters without whitespace; use a randomly generated secret. Send `Authorization: Bearer <secret>` to `POST /control/pause`, `/control/resume`, or `/control/drain`. Missing, wrong, or duplicate credentials return 401. Accepted commands return 202 and the process state. Pause and resume return 409 after drain begins. A token without a listener address and invalid tokens fail startup. These internal endpoints use plain HTTP; bind them to a trusted interface and use deployment access controls or a TLS proxy when remote access is needed.

The listener stays available during handler drain with readiness 503, then shuts down with a five-second connection deadline. Bind failure prevents claims. A server failure stops new claims and drains handlers. Automatic retention continues during pause and stops at drain. Controls affect only the bound tenant/queue in this process; other replicas continue independently. These management routes are separate from application contracts and SDKs. Newly generated durable workers export an empty application HTTP contract and OpenAPI paths; they do not advertise the demonstration worker’s `/jobs/process` or `/health` methods. Existing application sources are preserved by sync and reinitialization.

For library use, enable `forklaunch-worker-database`'s `http-control` feature and use `http::run` with an optional `HttpConfig` and retention policy, or mount `http::router` with your own `WorkerControl` in an existing server. Generated-process tests cover listener conflicts, health-only operation, bearer authentication, pause/resume, readiness during drain, and coordination with retention against both durable backends.

### Separate control-service role

Generated PostgreSQL, Redis, BullMQ and Kafka workers accept `--control-service`
with `WORKER_HTTP_ADDR`. This mode serves health/readiness without opening job
storage, claiming jobs or running retention. Without a control token, commands
return 404. A token requires `WORKER_SIGNAL_REDIS_URL`,
`WORKER_SIGNAL_NAMESPACE`, `TENANT_ID` and the same queue scope as the consumers.
Commands publish to that group without adding a subscriber; subscriber counts
are not acknowledgements. Readiness describes the control process, and draining
remote consumers does not stop it. SIGTERM shuts it down cleanly.

Scaffolding and sync record `workerControlService` only when installed worker
sources expose the generated entry point. Releases then carry explicit
`nativeWorkerRole: "control"` and `"consumer"` metadata; the companion platform
starts the service component with `--control-service`. Legacy manifests keep the
previous command. These management endpoints do not expose MCP or application
SDK methods. Actual ECS scaling and load-balancer acceptance remain unverified.

## Atomic batch delivery claims

Both durable adapters implement `AsyncWorkerQueue::claim_batch(queue, lease_duration, limit)` for limits 1–1000. Each atomic operation returns currently eligible jobs in delivery order, with a fresh independent token per job. Concurrent claimers receive disjoint live deliveries. Each lease is acknowledged, renewed, or failed independently, and the existing retry budget and tenant fences apply. An empty queue returns an empty vector. PostgreSQL uses a locked selection and transactional update; Redis validates the entire selected batch before acquiring its deliveries. PostgreSQL requires the built-in `gen_random_uuid` function available in PostgreSQL 13 and later; integration tests use PostgreSQL 16.

The trait's compatibility default supports a limit of one and rejects larger batches before acquiring any delivery. Custom adapters must override it to provide atomic batches. The runner and generated settings below support batch handler execution.

## Worker batch handlers

Set `WORKER_BATCH_SIZE` (generated as 1) to claim and process up to that many ready jobs in each handler call. `WORKER_CONCURRENCY` now limits simultaneous handler batches; their product must be at most 1000, and batch size must be 1–1000. Batches contain currently eligible jobs and can be smaller than the limit; the runner does not wait to fill a batch. With size 1, existing `JobHandler::process` behavior is preserved.

Override `JobHandler::process_batch(Vec<Job>) -> BlueprintResult<Vec<JobFailure>>` for bulk application work. Return only failed job IDs and errors; omitted jobs are acknowledged. A whole-batch error retries every job. An unknown or duplicate failure ID invalidates the entire result before any acknowledgments and fails all deliveries with `worker_invalid_batch_result`. Each job follows its own persisted retry budget. The compatibility implementation calls `process` sequentially for each job, so existing generated handlers remain usable. Only bounded, sanitized error codes are stored.

Every claimed lease renews while the batch future runs. If any delivery loses its lease, the runner cancels the entire batch future, counts lost leases and abandons its other deliveries for expiry recovery. Stop all work when the future is dropped and keep external effects idempotent. Shutdown drains active batches with renewal; exceeding the drain timeout cancels the batch and leaves unacknowledged jobs recoverable. Acknowledgments and failures are independently fenced and are not an atomic transaction across the batch. Storage operations, including parallel lease renewal and settlement, remain bounded by the renewal interval; choose timing and batch sizes suitable for the backend connection pool.

Redis and PostgreSQL tests cover partial failures, whole-batch errors, invalid failure identities, concurrency, renewal, and drain. Generated-process suites verify grouped handler calls, per-job retries, long-batch renewal, cancellation/recovery, queued-job protection, and invalid settings.

## Release builds

Current shared language images are supported by local Docker Compose. Application
release creation deliberately rejects them: the platform manifest and deployment
processor do not yet consume native process commands or migration dependencies.
Adding fields only in the CLI would silently lose that configuration. This is an
open platform integration requirement, separate from the published framework
crates. The generated-release CI gate tests successful builds/exports followed by
that rejection and verifies no unsupported release manifest is produced.


Release creation builds Rust workspaces with `cargo build --workspace --release`. When a Rust application also has a JavaScript workspace at its application or modules root, release creation installs and builds that workspace too. Mixed applications must provide its `package.json`. Existing root build scripts are honored; generated SDK workspaces without one build their packages recursively. PNPM and Bun use frozen installation when their lockfile is present. Build/install failure stops before export and release manifest creation.

Mixed applications using Tokio export their TypeScript services through Node; Bun and Node applications retain their configured JavaScript runtime. Rust components use the framework contract exporter. `--prebuilt-openapi` continues to skip installation, builds, and application execution. It honors Rust `cratePath` locations separately from the TypeScript modules directory and rejects paths or symlinks that leave the application snapshot. `rust_release_runtime.sh` exercises these paths through the actual CLI with owned command/platform fixtures; it validates orchestration rather than compiling the fixture applications. Generated service runtime and export suites validate real compiler/runtime behavior separately. `rust_generated_release_runtime.sh` additionally uses real optimized Cargo builds, PNPM, the generated TypeScript SDK, and a minimal tsx service export hook. It verifies shared artifacts and prebuilt reuse with deliberately unbuildable source. Export mode/output variables cannot be overwritten by dummy environment values and are excluded from deployment requirements. Neither suite publishes a release.

## Packaged CLI sources

`cli/tests/rust_packaged_install.sh` creates and installs an actual Cargo source
archive outside the checkout, then uses raw init commands to generate and
relocate an application with a native service, checked mappers, Redis/S3
infrastructure, an Apalis producer/consumer, a library and router. It verifies
archive source inventory, Cargo compilation, service tests, both local SDKs,
HTTP/MCP startup and producer authorization. The CI build job runs this gate
before uploading its CLI binary. Additional module/backend checks live in their
own runtime suites.

The registry scaffold gate verifies the published framework versions, including a
shared Docker image. Source
archive installation and registry consumption have separate receipts; neither
publishes the CLI itself.

## Async S3 object storage

Enable `forklaunch-infrastructure-s3`'s `aws-sdk` feature and construct `aws::AwsS3ObjectStore` with an official `aws_sdk_s3::Client`, `S3ObjectStoreConfig`, and `AwsS3Options`. Import `AsyncObjectStore` to call `put`, `get`, `delete`, `list`, and their batch variants. Supply the client’s credential provider, region, endpoint, TLS, and retry policy in application configuration; the adapter reexports the SDK through its `aws` module. S3-compatible endpoints typically require the SDK’s `force_path_style(true)` configuration; see the [official endpoint configuration guide](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/endpoints.html). The official SDK handles request signing.

Construction makes no network requests. `init_bucket(false)` checks access; `init_bucket(true)` creates a missing bucket, including its region configuration. Access-denied and transport failures never trigger creation. Every operation applies the configured namespace prefix. Keys returned to callers remain relative to that namespace. Reads preserve bytes, content type, and actual S3 object tags; reading tags requires `GetObjectTagging` permission. Content type defaults to `application/octet-stream`. Provider-specific tag restrictions remain enforced by the backend; the MinIO integration fixture uses ASCII tag values.

Defaults bound each object to 16 MiB, a listing to 1,000 objects and 64 MiB, and each operation to 30 seconds. Change these with `AwsS3Options`. `list_page(prefix, token, limit)` supports opaque continuation tokens and page sizes 1–1000 within the configured count bound. `list` follows pages and fails explicitly when its count or byte limit is exceeded. Concurrent listings are not snapshot isolated; objects removed during a page read are omitted. Deletion is idempotent, and its existence boolean is advisory under concurrent changes. Batch operations preserve input order and misses; failure leaves earlier writes/deletes committed.

`put_encrypted_async` and `get_decrypted_async` apply an application-supplied `ObjectCodec` around this async boundary. Use `JsonObjectStore` with `JsonRecordCodec` for the built-in authenticated JSON path. Existing `StreamingUpload` and `download_chunks` helpers buffer data.

`download(key)` returns an owned `aws::transfer::StreamingDownload` with metadata, optional declared length, and `next_chunk()`. It reads transport-sized chunks without collecting the object, enforcing its byte bound and one deadline that includes caller idle time. A read error terminates the stream; later calls return `None`. Dropping the download cancels the remaining read. `put_stream` accepts an SDK `ByteStream` with an exact known length; `put_file` opens a retryable SDK file stream. Both return `UploadedObject` metadata, size, ETag and version ID without collecting the source. Keep file sources unchanged until completion so SDK retries read the same bytes. Unknown-length streams fail before storage access.

`put_multipart_stream(key, ByteStream, metadata)` accepts unknown-length sources. It buffers one 5 MiB part at a time, caps the configured object bound at 10,000 parts (about 48.8 GiB), and applies one deadline to reads and requests, and uses consecutive part numbers with CRC32 checksums through completion. Empty sources and exact part boundaries are supported. Source failures and overflows abort the upload before publishing an object. Cancellation and timeouts schedule a bounded abort while the Tokio runtime is alive; failed operations also await cleanup. Configure an incomplete-upload bucket lifecycle rule as a backstop for process shutdown, abort failure, or a lost creation response. Cancellation or a lost response during completion can leave a committed object without a returned receipt. MinIO tests cover multiple parts, metadata, empty/exact-boundary sources, source errors, overflow, timeout cleanup, and cancellation after a real uploaded part.

`presign_upload(key, &PresignUploadOptions { content_type, max_bytes, expires_in })` returns `PresignedUpload { url, fields, key, expires_at }`. Submit the returned fields unchanged as a multipart form and append the `file` part last. The [AWS POST policy](https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-HTTPPOSTConstructPolicy.html) binds the exact namespaced key and content type and accepts only 1 through `max_bytes` bytes. Uploads default to a 900-second ceiling, configured with `AwsS3Options::max_upload_lifetime`; the byte bound must fit `max_object_bytes`. Positive fractional lifetimes of at least one second are floored to whole seconds, matching TypeScript. The returned key retains the caller's original relative key spelling. Credential resolution uses the injected client's cache and provider; signing captures the exact selected credentials, includes temporary tokens and shortens expiration to credential expiry. Signing makes no S3 object request and remains bounded by `operation_timeout`. Ordinary SigV4 bucket endpoints are supported; ARN/directory-bucket grants and unsupported signing scopes fail explicitly. Invalid/empty keys, filename substitution and invalid content types are rejected before resolving credentials. Grants can be reused until expiration and their fields are omitted from `Debug` output. Bucket CORS and IAM policy remain application configuration.

`presign_download(key, &PresignDownloadOptions)` creates a signed GET URL without contacting S3. Its lifetime defaults to the configured `max_download_lifetime` of 300 seconds; requested lifetimes must be whole positive seconds within that cap, and the configured cap cannot exceed seven days. The operation deadline also bounds credential resolution. Temporary credentials can expire sooner. An optional attachment filename carries an ASCII fallback and RFC 5987 UTF-8 form. The returned URL grants download access until expiry; avoid logging it. MinIO tests verify binary downloads, attachment headers, expiration, and signature rejection after namespace tampering.

The framework CI job starts an isolated MinIO container and runs the ignored `s3_backend` test. Locally, set `FORKLAUNCH_TEST_S3_ENDPOINT`, optionally `FORKLAUNCH_TEST_S3_ACCESS_KEY` and `FORKLAUNCH_TEST_S3_SECRET_KEY`, then run `cargo test --manifest-path framework-rs/Cargo.toml -p forklaunch-infrastructure-s3 --all-features s3_backend -- --ignored`. The suite checks binary/empty objects, real tags, namespace fences, ordered batches, pages, codec hooks, misses, deletion, resource limits, and actual browser POST grants with size/type/key/signature/expiry enforcement. `framework-rs/parity/rust_s3_post_parity.mjs` compares the actual compiled TypeScript provider’s policy fields, prefixes and lifetime behavior with Rust, and unit tests include AWS’s published signature vector. Separate tests check operation deadlines and access-denied bucket initialization. No AWS account or cloud deployment is exercised by these tests.

Native WebSocket servers and clients are available through `forklaunch-ws`'s optional `transport` feature. `TypedWebSocketServer` accepts connections with an authorization callback; `transport::connect` supports caller headers/subprotocols and TLS. `SocketSchemas` and `PayloadCodec` validate both directions, with optional serde/garde integration through the `validation` feature. Frame/message/control byte limits, handshake/write deadlines, automatic pong validation, and typed close handshakes are covered by real socket tests and bidirectional actual TypeScript peers. These typed transports use raw UTF-8 string / JSON payloads, separate from the legacy tagged envelope described below. Enable `forklaunch-axum` feature `websocket` to compose authorized `websocket::upgrade` handlers with HTTP/MCP on one listener; all three surfaces are covered by a live integration test, including invalid/oversized frames and shutdown. Applications own connection tasks and propagate shutdown explicitly. `connect_with_response` retains negotiated subprotocol metadata, and cancelling a partial write releases the transport. Raw `SocketContract` metadata and `generate_raw_socket_asyncapi_json` now support TypeScript channel fan-out, singular/plural operation precedence and document options, with checked standalone-schema relocation. The optional `json-schema` feature adds compiled `PayloadCodec::json_schema` validation before serde can remove fields; it bounds schema compilation and disables external retrieval. The raw-schema profile has 4,104 actual TypeScript helper/AJV comparisons, including exact decimals and explicit email/UUID formats. Native Rust WebSocket SDK source generation now shares the typed transport and compiled codecs, including recursive message unions; the generated client passes actual TypeScript-server and malformed-peer fixtures. Node TypeScript WebSocket SDK generation now has strict ESM/CommonJS consumer and native/actual-TypeScript peer fixtures, cancellation, bounded queues and terminal cleanup. `forklaunch sdk generate --from socket-contract --target all` now reads versioned `SocketContractArtifact` files from the service's SDK folder (`<service>/sdk/typescript/socket-contract.json`), emits checked AsyncAPI (`<service>/sdk/typescript/asyncapi.json`) alongside both clients, and supports `--check`, `--service` and `sdk watch`. Socket packages live in `sdks/typescript-websocket/<service>` and `sdks/rust/<service>-websocket-sdk`. Project metadata can override `socketContractArtifactPath` and `asyncapiPath` within the application. Live relocated tests include both clients, official AsyncAPI parsing and HTTP/MCP startup. Register a raw socket in `contract_registry()` with `registry.register_socket("chat", &chat_contract).expect("register chat socket")`, then run `forklaunch openapi export` to produce the portable artifact automatically from service metadata. Export honors custom socket artifact paths and removes unchanged owned artifacts after path changes or last-socket removal while retaining edited/manual artifacts. Re-export after registry changes before generating/watching SDK artifacts. Regeneration removes unchanged obsolete socket modules using per-package content hashes; `--check` reports pending removals without writing files. Edited obsolete modules and untracked files are retained, and existing TypeScript compiler options remain user-controlled. Exporting an empty registry then regenerating retires owned SDK files, AsyncAPI output and Cargo members; watch performs the same cleanup and restores packages when sockets return. A missing artifact alone does not retire packages. Explicit service deletion also cleans owned socket outputs. Shared pnpm package globs remain for retained user packages. Service rename regenerates existing socket SDK targets with their new package names, carries TypeScript compiler options forward, and retains edited/untracked files at old SDK locations. Explicit default socket artifact paths follow the service name; custom paths retain their location. Occupied generated destinations reject the rename before source mutation. External AsyncAPI imports support the checked profile described below. See [the runtime API](../../framework-rs/README.md#typed-websocket-connections).

To import an external AsyncAPI 3.0 WebSocket server document, place it at
`<service>/sdk/typescript/asyncapi.json` (or configure `asyncapiArtifactPath`)
and run:

```sh
forklaunch sdk generate --from asyncapi --target all --service realtime
forklaunch sdk generate --from asyncapi --target all --service realtime --check
forklaunch sdk watch --from asyncapi --target all --service realtime
```

The importer preserves the input document. It supports JSON messages, including
quoted strings and null, and canonical binary messages with
`application/octet-stream` media. Multi Format JSON payloads support Draft 4/6/7,
2019-09 and 2020-12 with matching explicit `schemaFormat` and `$schema`
declarations. Server receive/send operations become client outgoing/incoming
messages. Inputs need explicit message selections, fixed channel paths and ws/wss
servers. Bounded message/operation traits and bundled payload resource references
are supported, along with fixed operation replies on the same or another declared
channel. Reply messages populate the opposite send/receive direction of their
reply channel. Different paths generate independent socket clients; applications
open both connections and own routing, correlation and reply deadlines.
Dynamic reply addresses, upgrade authentication/parameters,
missing external resources and other protocols remain unsupported. See the
[import profile and wire encoding details](../../framework-rs/README.md#typed-websocket-connections).

Rust event schemas can be exported with `forklaunch_asyncapi::AsyncApiDocument::to_json()`, which returns `FrameworkResult<String>`. It groups messages by channel, emits AsyncAPI 3.0 send/receive operations, preserves inline payload/header schemas supplied through `SchemaRef::with_json_schema`, and rejects conflicting definitions or malformed schema JSON. Named payloads without supplied schemas stay unconstrained and retain their name as a title. Protocol metadata is retained in `x-forklaunch-protocol`; server endpoints and protocol bindings remain application configuration.

For WebSocket contracts, use `generate_asyncapi_json_with_schemas(info, contract, schemas)` with a `BTreeMap<String, serde_json::Value>` of payload schemas. The checked renderer describes actual `{event, payload}` frames, uses the event name as a discriminator, preserves shared schemas, and includes declared error/close control frames. Missing schemas, duplicate/reserved events, invalid component names, and unresolved/external schema references fail explicitly. Recursive local schema references are supported. The older `generate_asyncapi_json` provides placeholder schemas and should not be used to assert payload validation parity. These tagged event envelopes are separate from the CLI raw socket clients described above. Run `ASYNCAPI_PARSER_PATH=/path/to/node_modules/@asyncapi/parser node framework-rs/parity/rust_asyncapi_parity.mjs` after installing the pinned `@asyncapi/parser@3.6.3` and `ajv@8.20.0` to validate generated documents with the official parser.

Generated TypeScript SSE clients support both native Web Streams and byte streams returned by Node fetch adapters. Streaming requests own an abort controller, forward the caller signal, detach its listener after completion, and cancel the upstream request on iterator return/throw, pending-read cancellation or terminal failure. `rust_sse_runtime.sh` runs the live suite with native fetch, injected cross-fetch and the native generator's imported-fetch option. Install the pinned `cross-fetch@4.1.0` next to the fixture TypeScript compiler, or set `CROSS_FETCH_PATH=/path/to/node_modules/cross-fetch`.

The checked `forklaunch_sdk_typescript::generate_typescript_http_sdk_source(plan, &TypeScriptHttpSdkOptions::new("service"))` generates the CLI-compatible `createForkLaunchSdk` factory, typed requests, status-specific success/business-error responses, guarded recursive models and owned bounded SSE iterators. It shares the CLI type compiler, fetch/SSE runtime and canonical form serializer; unsupported schemas/media/statuses and conflicting names fail before source is returned. Both native generators pass the same 95-case validator, recursive HTTP/MCP and streaming fixtures. The dependency-free `generate_typescript_sdk_source` raw-response API remains available.

The framework-native TypeScript generator sends text, binary/file bodies, `FormData`, and `URLSearchParams` directly. JSON retains vendor `+json` media types, byte-array views retain their bounds, and multipart encoders own content-type boundaries. Typed clients also accept Blob and ArrayBuffer uploads. Declared request MIME parameters, including charset and quoted profiles, survive generation; header control characters are rejected. The native Rust generator remains a transport-independent request builder: text/JSON/form bodies accept serialized strings, binary/file inputs use `Vec<u8>`, and multipart inputs use generated `MultipartForm`/`MultipartPart` builders. `SdkRequest.body_bytes` carries encoded uploads, while its owned `body_content_type` and headers include the selected boundary. Files preserve arbitrary bytes, repeated names and empty parts; text fields normalize line endings and MIME names are escaped. SSE request bodies remain unsupported; the typed native Rust HTTP generator is described below. Native fetch retains standard browser upload bodies; custom fetch adapters receive serialized Blob/FormData bytes and encoder-owned MIME boundaries. Run `TSC_BIN=/path/to/tsc CROSS_FETCH_PATH=/path/to/node_modules/cross-fetch node framework-rs/parity/rust_native_sdk_parity.mjs` with pinned `cross-fetch@4.1.0` for strict generated-code compilation and real MIME/body parsing across native, injected and imported transports.

The CLI and checked framework-native HTTP SDKs support multipart uploads through either contract or OpenAPI generation. TypeScript inputs accept `FormData` or a field record; blobs/files pass through, arrays repeat fields, and other values use the actual shared TypeScript `safeStringify` implementation. URL-encoded field records encode each composite value as one JSON string. Rust SDKs reexport reqwest's `MultipartForm` and `MultipartPart`, enabling text, binary/file and streaming parts. Forms are consumed during transmission; multipart request types and requests in SDKs with multipart operations therefore omit cloning/serialization derives. Encoders override conflicting content-type headers, and empty reqwest forms receive a valid closing delimiter. Multipart responses remain unsupported; CLI SSE responses are described below. `rust_sdk_body_parity.mjs` compares actual TypeScript universal SDK behavior; `rust_http_runtime.sh` exercises both generated SDKs against Rust and TypeScript services through both artifact formats.

Both Axum adapters preserve declared binary/file request bodies as bytes, including valid UTF-8 and empty files. Media validation compares the case-insensitive media type on both sides; charset and boundary parameters do not change that comparison. Empty declared text and URL-encoded bodies also retain their text body kind. Required content-type headers remain enforced; JSON and undeclared empty bodies retain their existing missing-body behavior.

Native Redis workers and their Pub/Sub controls honor `REDIS_KEY_PREFIX`, falling back to `FORKLAUNCH_REDIS_KEY_PREFIX`. Like TypeScript BullMQ workers, they trim ECMAScript whitespace and normalize trailing colons. The application prefix precedes the existing namespace/tenant/queue keys and channels; dedicated caches without either setting retain their legacy layout. `RedisKeyScope` and the `connect_scoped` constructors support explicitly injected configuration. Prefixes containing control characters, hash tags or more than 256 bytes fail configuration validation, preserving the native cluster hash tag. Redis key/channel ACLs enforce isolation; the prefix only places conforming clients inside that boundary. Denied operations return a sanitized `worker_tenant_isolation_error`, and the runner stops and drains rather than retrying them. Pub/Sub startup validates an actual subscription acknowledgment through a bounded command connection before accepting the long-lived listener.

With the `redis-client` feature, `forklaunch_worker_redis::signals::RedisSignalBus::connect(url, namespace, &tenant, queue, options)` subscribes to a channel scoped by namespace, tenant and queue. Import `forklaunch_worker_interface::AsyncSignalBus` for `broadcast`, `next_signal(wait)` and `drain_signals`. `publisher()` returns an independent clone for control applications; its publication count measures current subscribers, not completed control changes. The default buffer holds 256 signals and connection/publication operations have a five-second deadline. Subscription overflow, malformed publications and disconnects are terminal errors; reconnect explicitly with a fresh subscription. Dropping a bus aborts its listener.

`forklaunch_worker_database::control::forward_signals(&mut bus, &control, shutdown)` forwards pause/resume/drain into the same `WorkerControl` passed to `runner::run_controlled`. Drain stays irreversible, and bus errors or shutdown initiate graceful drain. This works with either durable queue backend. Generated Redis and PostgreSQL workers configure this helper when `WORKER_SIGNAL_REDIS_URL` and `WORKER_SIGNAL_NAMESPACE` are supplied. Pub/Sub messages are ephemeral: late or disconnected subscribers miss signals, and Redis database numbers do not isolate channels. Use distinct environment namespaces and Redis ACLs, and define desired-state restoration in application code. See [Redis Pub/Sub delivery and scoping](https://redis.io/docs/latest/develop/pubsub/). Real Redis tests exercise a separate child process, order, scope isolation, actual socket loss and overflow, and run in the existing blueprint CI Redis suite.

Administrative controllers can call `RedisSignalPublisher::connect(url, namespace, &tenant, queue, &options)` directly without creating a subscription. For a worker with HTTP management, pass the same `WorkerControl` to `http::run_with_control` and `control::forward_signals`. This keeps HTTP readiness, pause/resume/drain, retention and queue consumption on one state. The existing `http::run` convenience function creates its own process-local control. Real Redis integration verifies that paused jobs remain queued, a distributed resume completes the job and changes readiness, and distributed drain stops the runner.

Generated workers accept `--signal pause`, `--signal resume`, and `--signal drain`
with `WORKER_SIGNAL_REDIS_URL`, `WORKER_SIGNAL_NAMESPACE`, `TENANT_ID`, and
`WORKER_QUEUE`. Signal commands do not connect to queue
storage. When enabled, authenticated HTTP controls publish to the same scoped group,
apply through the ordered subscription, and return `{ "subscribers": N }`; this count is not an acknowledgement.
A publication failure returns 503; delivery can remain uncertain after a timeout. Subscriptions
fail closed: disconnect, overflow or malformed messages drain active work and exit
with an error. There is no automatic reconnect or durable desired-state restoration.
An explicit restart begins running and does not inherit earlier pause/drain commands.
Without signal configuration HTTP controls retain their process-local behavior.

## Kafka workers

`forklaunch init worker jobs --language rust -t kafka --consumer-only` generates a broker-backed
Kafka worker. It uses `forklaunch_worker_kafka::client::KafkaWorker`, not the original
in-memory `KafkaWorkerQueue` reference. Configure `KAFKA_BROKERS`, `WORKER_NAMESPACE`,
`TENANT_ID`, and a stable unique `KAFKA_TRANSACTIONAL_ID` per worker replica. Separate
publishers use `KAFKA_PUBLISHER_ID`. Create queue/dead-letter topics explicitly with
`--migrate`, enqueue with `--enqueue ID JSON`, and inspect with `--peek LIMIT` or
`--dead-letters LIMIT`. Topics and groups hash length-delimited namespace/tenant/queue
identities; broker ACLs remain the authorization boundary.

Retry/dead-letter publications and source offsets commit in a Kafka transaction.
External application effects remain at-least-once. Duplicate IDs are permitted;
attempt counts reflect handler dispositions, and a crash before commit can repeat
the same attempt. A producer identity fences an older replica on restart. Assignment
changes and broker/transaction errors fail closed without acknowledging the current
record. Handler/drain deadlines bound active application work; failed transactions
require an explicit restart. Queue administration follows Kafka log semantics, with
per-partition ordering and broker retention rather than per-job database deletion.

Generated workers support health/readiness, authenticated local HTTP controls, and
optional Redis distributed controls. TLS/SASL properties are accepted through
`KAFKA_CLIENT_PROPERTIES`; automatic commits, isolation and transaction settings
remain framework-controlled. The Docker builder includes CMake, C/C++ tools and
Perl for the bundled librdkafka/OpenSSL build. The isolated broker tests exercise
transactions, retry/dead-letter handling, non-mutating inspection, offset persistence,
replica fanout, pause/drain recovery and producer fencing.

### HTTP and MCP startup

Generated Rust services, IAM, CAC, messaging, billing and ecommerce modules expose Streamable HTTP
MCP at `/mcp` on their HTTP listener. The transport uses the official `rmcp`
server and dispatches eligible tools through the same HTTP router, preserving
validation, route authentication, tenant binding and auditing. Every request
presents its own credentials; tool arguments can supply route-specific signed
headers, and bearer credentials on the MCP request also reach JWT routes.

`MCP_ENABLED` (`true` or `false`) is required. `MCP_PATH` changes the path.
`MCP_ALLOWED_HOSTS` and `MCP_ALLOWED_ORIGINS` accept comma-separated allowlists;
Hosts default to localhost/loopback and browser Origins are rejected until
explicitly allowed. Configure the public host when deploying behind a proxy.
Native applications enable `forklaunch-axum`'s `mcp` feature and call
`into_router_with_mcp(McpServerOptions)` to configure a transport authentication
callback, request bounds and tool timeout. JSON/text/form bodies and binary
response resources, multipart text fields and typed additional tool execution
are supported. Multipart tool bodies are objects of strings, matching the actual
TypeScript generator; array/object/non-string parts fail before HTTP dispatch.
Names are escaped and line breaks normalize to CRLF using FormData semantics.
URL-encoded composite values become one JSON string per field. File/binary tools
follow TypeScript MCP encoding: strings are UTF-8 bytes and other JSON values
are JSON bytes. This differs from arbitrary-byte file upload parts. The native
JSON encoder retains valid JSON quoting for scalar strings.

MCP input schemas relocate recursive local pointers into each body/parameter property. The shared `forklaunch_contract::relocate_schema_json` helper (feature `json-schema`) also handles OpenAPI component embedding, escaping pointer segments and preserving literal examples/defaults/constants, tuple/conditional schemas and embedded resources with their own `$id`. Resource identifiers establish a separate reference base under the [JSON Schema rules](https://json-schema.org/understanding-json-schema/structuring). CLI SDK import currently rejects resource IDs and dynamic references explicitly; full resource/anchor resolution remains a parity task.

Additional runtime tools bind through `McpServerOptions.manual_tools` using `forklaunch_mcp::server::ManualTool::typed`. A `FrameworkSchema` input supplies serde parsing, garde validation and schemars discovery metadata. Invalid input fails before the handler runs; handlers receive fresh request headers and must apply operation/tenant authorization. Transport authentication also remains active. Explicit public handler errors become MCP tool errors, with invocation timeouts and response bounds. Duplicate manual/contract tool names fail at startup. See [the framework example](../../framework-rs/README.md#additional-mcp-tools).

Live CLI fixtures start generated services and exercise HTTP health, MCP
initialization, discovery and tool calls, including signed calls, rejected
credentials and Origins, and graceful shutdown. Generated services use
`serve_with_default_mcp(listener, shutdown)` to coordinate both surfaces.
`MCP_LISTENER=shared` (the Rust default) keeps MCP on the HTTP listener.
`MCP_LISTENER=separate` binds MCP on the HTTP port plus 2000, matching the
TypeScript fallback. `MCP_PORT` overrides that port (`0` selects an ephemeral
port); `MCP_HOST` accepts a hostname or IP and defaults to the HTTP listener's
bound IP. Either override also selects separate mode when `MCP_LISTENER` is
absent. Explicit shared mode rejects these listener overrides.

Separate MCP listeners expose only the configured MCP endpoint; HTTP handlers
remain on the HTTP listener and tools dispatch internally through their route
guards. Both listeners bind before either starts serving. Failure to configure
or bind MCP fails startup and releases HTTP as well. Shutdown drains both
surfaces, bounded to 30 seconds. `into_routers_with_mcp(options)` exposes the
HTTP and MCP-only routers for custom listener orchestration; the existing
`into_router_with_default_mcp()` mounting helper configures one shared router.

### Query encoding

Generated TypeScript SDKs and MCP serialize array/object query values as one JSON string per field, matching the TypeScript transport. For example, `{ ids: ['a,b', '水'] }` sends one `ids` value containing `["a,b","水"]`; an empty array sends `[]`. Strings remain strings, and URL components are percent encoded once. The TypeScript universal SDK and MCP generator also preserve already encoded parameters without a second `encodeURI` pass. Rust SDK string query maps accept caller-serialized JSON values. `rust_query_transport_parity.mjs` compares 40 URL cases against the actual TypeScript transports, and live HTTP/MCP checks verify array values arrive intact.

`forklaunch_http::sse::{SseEvent, SseDecoder}` provides transport-independent SSE framing. Construct a decoder with an explicit positive per-event byte limit, feed arbitrary chunks with `push`, and call `finish` at EOF. UTF-8, one leading BOM, LF/CRLF/bare-CR, comments, multiline data, IDs/resets, event names and integer retry metadata are handled incrementally. Overflow closes the decoder; encoder metadata rejects CR/LF/NUL injection. The [HTML framing rules](https://html.spec.whatwg.org/multipage/server-sent-events.html#parsing-an-event-stream) require a blank delimiter to dispatch an event, so incomplete EOF frames are discarded. Thirty-five comparisons exercise common terminated frames against the actual TypeScript universal SDK. Its existing per-line data/whitespace and unterminated-EOF behavior differs from standard framing; those differences remain explicit conformance work. CLI SDKs use this framing for streaming responses; fetch reconnection remains caller-owned.

CLI contract/OpenAPI SDK generation supports `serverSentEvent` responses with `text/event-stream`. TypeScript returns an `AsyncIterable` of typed `{ data, id? }` events; Rust returns an owned `Box<SdkEventStream<T>>` with `next().await` and `close()`. Neither buffers the entire response. Both preserve UTF-8 chunk boundaries, multiline data and IDs, discard incomplete EOF frames and close on transport/size failures. Set TypeScript `maxSseEventBytes` or Rust `with_sse_event_limit(bytes)` to change the default 1 MiB event bound. Iterator return/throw, abort, Rust close/drop and decoder failures release the connection. Frames use JSON decoding with a text fallback, matching the TypeScript universal SDK's normal data handling; Rust deserializes each envelope into its generated model. Response event names/retry metadata remain available in the lower-level codec. SSE requests remain unsupported. `rust_sse_runtime.sh` validates a relocated generated HTTP/MCP service and both clients through both contract formats, including delivery before EOF, standard framing, auth before work, cancellation/resource release and graceful shutdown.

Checked Rust SDK schema imports and WebSocket AsyncAPI payloads require literal `/` JSON-pointer separators. Escape a slash inside a property/definition name with `~1`; percent-encoded Unicode, spaces and literal percent names remain supported. Encoded separators fail validation before SDK/artifact writes. This restriction applies to schema references; OpenAPI reference objects retain general document-local URI fragment decoding.

Raw socket JSON Schema codecs use an isolated validator dependency with AJV
compatibility for URI fragments: percent decoding happens once within each
pointer token, so `#/definitions/Foo%2FBar` selects the literal `Foo/Bar` key.
Drafts 4–7 also apply constraints beside `$ref`, matching the TypeScript AJV
configuration. Schema literals and annotations remain unchanged; external
resource retrieval stays disabled. The checked SDK/export/import profile
continues rejecting encoded pointer separators before generation. Vendor
licenses, pinned archive checksums and the small compatibility patch are
included in generated framework sources.

SDK watch compares artifact contents, so an identical re-export does not regenerate
clients. Repeated HTTP and WebSocket SDK generation preserves timestamps on unchanged
outputs and workspace files. Source changes require contract export before
SDK watch can regenerate the clients; `dev --export-contracts` performs this step
for native and TypeScript services on startup and reload.

See the [Rust/TypeScript compatibility baseline](rust-compatibility.md) for source package versions, artifact formats and implementation choices.
