# TypeScript package inventory for the Rust parity port

This inventory records every non-root framework/blueprint package in this checkout,
including private generated modules. A Rust crate or passing smoke test does not
mean the package has full parity. The implementation plan retains the unchecked
provider, route, DTO, storage, deployment and conformance work described below.

The current target has 31 published packages and 13 private package/module fixtures.
Root `framework/package.json` and `blueprint/package.json` are workspace tooling.
JavaScript runtime adapters map to equivalent Rust behavior rather than literal
Express/HyperExpress/Bun clones. Provider protocols and broker/storage formats
require explicit interoperability fixtures; sharing a Redis/Kafka server is not
proof that TypeScript and Rust producers/consumers can exchange jobs.

The checked source inventory is [rust-typescript-public-api.json](rust-typescript-public-api.json).
It records 635 resolved declaration entry points and 1,776 declaration/re-export
records across the 44 packages, with source locations, public service/member
signatures, type aliases, explicit external re-export boundaries, private module
sources and CLI bins. Counts include repeated declarations exposed by different
packages; they are not counts of independent features or completed parity items.
`scripts/rust_api_inventory.mjs` follows manifest type exports (including wildcard
subpaths), build output/root directories and local source/re-export graphs. It
checks the package table against tracked manifests. Regenerate with `TSC_BIN`
pointing to TypeScript 5.9.3's `tsc`, then run
`node scripts/test_rust_api_inventory.mjs`; CI checks for drift without executing
application code or installing application dependencies.

This is a syntactic source inventory, not a type checker or proof of equivalent
Rust behavior. Imported aliases, external exports, computed schema/DTO shapes,
class inheritance, compiler-generated declarations and dynamic runtime behavior
still need semantic review. Private application source lists and binary paths are
capability leads, not public npm exports. Detailed Rust symbol/fixture mappings
remain open under checklist item 849; this inventory does not close that item.

The [reviewed symbol map](rust-typescript-symbol-map.json) now records 16 common
string/naming/hash/path declarations with exact native symbols, actual-source
fixtures and explicit input-domain boundaries. `node scripts/check_rust_symbol_map.mjs`
checks declaration, public native function and fixture references in CI. This
checks evidence pointers; it does not execute conformance tests or certify
unlisted exports. The existing `rust_common_naming.mjs` fixture executes the
linked TypeScript/Rust comparisons. Detailed mapping remains open.

| Source package | Rust implementation | Remaining scope / evidence |
| --- | --- | --- |
| [`@forklaunch/bunrun`](../framework/bunrun/package.json) | CLI Cargo/watch and mixed orchestration | Native builds and mixed Node/Bun Compose orchestration pass live HTTP/MCP, source reload and shutdown fixtures; automatic contract/SDK refresh remains. |
| [`@forklaunch/common`](../framework/common/package.json) | forklaunch-common | Sixteen public naming/hash/path exports now have direct Rust equivalents and 10,455 actual-source comparisons; operation naming uses the TypeScript separator, cleanup, prefix and fallback semantics. Path normalization, other utilities and JavaScript-only value/type boundaries remain under symbol audit. |
| [`@forklaunch/core`](../framework/core/package.json) | forklaunch-core, forklaunch-core-macros, forklaunch-http, forklaunch-auth, forklaunch-contract, forklaunch-mcp | HTTP/contracts/config/DI/cache/object store/telemetry/encryption, explicit compliance metadata derives and an optional MCP HTTP server exist; Bounded tenant-scoped retention hooks exist; generic SQL entity/compliance/retention adapters and full auth/streaming conformance remain. |
| [`@forklaunch/e2e-tests`](../framework/e2e-tests/package.json) (generated/private) | Rust wire/runtime suites | Actual TypeScript framework service conformance remains beyond the plain Node wire fixtures. |
| [`@forklaunch/express`](../framework/express/package.json) | forklaunch-axum | Equivalent HTTP behavior is the target; multipart/SSE and full validation/auth/error fixtures remain. |
| [`@forklaunch/hyper-express`](../framework/hyper-express/package.json) | forklaunch-axum | All 27 exported HTTP methods dispatch through shared validation/auth and nested contracts; both SDK artifact sources and actual compiled TypeScript router token forwarding are covered. Runtime-specific adapter internals need no clone. |
| [`@forklaunch/infrastructure-s3`](../framework/infrastructure/S3/package.json) | forklaunch-infrastructure-s3 | Async store, bounded streaming/multipart transfers, codecs, signed downloads and browser POST policies are implemented with real MinIO fixtures. POST size/type/key/expiry enforcement, AWS signature vectors, SDK credential cache/rotation and actual TypeScript policy fixtures are covered; ordinary SigV4 bucket endpoints only. |
| [`@forklaunch/infrastructure-redis`](../framework/infrastructure/redis/package.json) | forklaunch-infrastructure-redis | Redis cache/queue/pubsub, typed EXISTS/delete batches, literal-prefix/glob scans and native client/config access have real fixtures. Opt-in safe-value codecs now use actual TS safeParse/safeStringify source for JSON-compatible string interoperability; Opt-in bulk queue methods also match falsy filtering and zero-count peeks. JavaScript-special values, number boundaries and remaining provider options remain. |
| [`@forklaunch/internal`](../framework/internal/package.json) | contract macros, CLI embedded templates and Rust tooling | Map remaining exported build/runtime helpers; JavaScript compiler utilities remain in the TypeScript path. |
| [`@forklaunch/testing`](../framework/testing/package.json) | forklaunch-testing and runtime fixtures | Test environment configuration, nine database aliases, Redis/Kafka/S3 endpoints and mock tokens match 970 actual TypeScript cases; child-process environment isolation passes. Container ownership/lifecycle, ORM setup/database clearing and the complete blueprint harness remain. |
| [`@forklaunch/universal-sdk`](../framework/universal-sdk/package.json) | forklaunch-sdk-rust, forklaunch-sdk-typescript, forklaunch-sdk-codegen and CLI SDKs | HTTP/multipart uploads, bounded CLI SSE responses and guarded recursive CLI models and omission/null presence exist through both artifact formats, with actual TypeScript conformance and live clients; native Rust and TypeScript typed HTTP/SSE responses and recursive models share the CLI compiler and pass strict consumers and live clients; one shared HTTP SDK now imports native contracts and TypeScript OpenAPI, with both live client languages, lifecycle preservation and custom-path watch recovery. Remaining schema conformance, multipart response scope and broader realtime profiles remain. |
| [`@forklaunch/validator`](../framework/validator/package.json) | forklaunch-validation and schema contracts | JSON Schema validation/coercion exists; audit Zod/TypeBox exported behaviors against shared fixtures. |
| [`@forklaunch/ws`](../framework/ws/package.json) | forklaunch-ws, forklaunch-asyncapi | Typed native server/client transports now cover authenticated upgrades, serde/garde payloads, bounded I/O, validated ping/pong/close and bidirectional actual TypeScript interoperability. Optional Axum upgrades share HTTP/MCP listeners, preserve negotiated subprotocols and use the same typed engine. Raw socket metadata/checked AsyncAPI exports match actual TypeScript channel/operation behavior in 34 conformance documents, with standalone recursive and reachable legacy annotation schema relocation, once-decoded local pointers and official schema validation. References into literal assertion/default/example data now use separately bundled schema copies, preserving the original values and recursive targets under copy/output bounds. Optional compiled raw JSON Schema codecs validate original payloads before serde, with bounded compilation and disabled external retrieval; 4,104 actual-helper/AJV and 168 Zod cases pass. Generated native Rust clients share the typed transport, with actual TS-server and invalid-peer fixtures plus recursive/empty consumer compilation. Generated Node TypeScript clients pass strict ESM/CommonJS consumers, 4,104 codec comparisons, actual TS/native peers, bounded queues and cancellation/close cleanup. CLI `--from socket-contract` now generates multi-socket Rust/Node packages and checked AsyncAPI, with service selection, check/watch, custom artifact/output paths and validation before writes. Relocated live clients, parser conformance and HTTP/MCP startup pass. Rust service registries now export portable socket artifacts automatically, with custom contained paths and content-owned move/removal cleanup preserving edited/manual artifacts. Owned socket SDK module/package pruning, empty-registry watch recovery and service rename/delete now pass with preserved user files/compiler options and updated Cargo membership. External AsyncAPI 3.0.0 JSON WebSocket server imports now support native/CLI generation/check/watch, local schema extraction, explicit directions, checked disjoint messages, input ownership and SDK rename/pruning. Relocated strict consumers/live peers and HTTP/MCP startup pass. Broader AsyncAPI event/protocol profiles and binary payload metadata remain. |
| [`@forklaunch/blueprint-billing-base`](../blueprint/billing-base/package.json) (generated/private) | forklaunch-billing-axum + Rust CLI | Generated base module: 33 HTTP/MCP routes, both live SDKs, five persistent service families, compliance authorization, tenant isolation, encryption/audits, restart and CLI lifecycle pass. Fresh archive installation passes. Sixty-five actual configured mapper cases now audit schema resolution, HTTP stripping, local IDs, ownership, read normalization and missing-row failures. Mixed deployment remains. |
| [`@forklaunch/blueprint-billing-stripe`](../blueprint/billing-stripe/package.json) (generated/private) | forklaunch-billing-axum + Rust CLI | Generated Stripe module: 35 HTTP/MCP routes, authenticated provider services, both live SDKs, restart and CLI lifecycle pass, including fresh archive installation. Stripe raw callbacks and signed platform payment events now synchronize business state with durable tenant receipts. Managed outbound Stripe transport passes 23 actual TS gateway comparisons; generated keyless HTTP/MCP startup, signed provider calls, ignored local keys, incomplete configuration rejection and lifecycle pass. Provider reconciliation and mixed deployment remain. |
| [`@forklaunch/blueprint-cac-base`](../blueprint/cac-base/package.json) (generated/private) | forklaunch-cac-interface/base/axum and Rust init module cac-base | Generated module mounts HMAC lookup and JWKS/system-role compliance; no entities are registered, so authorized export/erase return upstream 404s. Uses verified claims or a trusted role resolver in place of the TS admin-only stub. Mixed coexistence, both SDKs, upgrade, rename/sync/delete and installed source archives pass. |
| [`@forklaunch/blueprint-client-sdk`](../blueprint/client-sdk/package.json) (generated/private) | forklaunch-blueprint-client-sdk and CLI SDK generation | Native SDK prototypes and generated HTTP packages exist; all added modules/mixed surfaces remain. |
| [`@forklaunch/blueprint-core`](../blueprint/core/package.json) (generated/private) | forklaunch-blueprint-core and framework crates | Core DTOs/contracts exist; compliance/service registrations and shared role/provider behavior remain. |
| [`@forklaunch/blueprint-ecommerce-stripe`](../blueprint/ecommerce-stripe/package.json) (generated/private) | forklaunch-ecommerce-axum; Rust CLI ecommerce-stripe module | Core 27 catalog/cart/order/payment routes plus health start over HTTP/MCP with PostgreSQL, HMAC, provider selection and durable inventory events. Live generated HTTP/MCP, both SDKs, separate inventory worker, restart persistence and init/sync/rename/delete checks pass. Tax/shipping calculators pass 32 actual TS comparisons; bulk catalog import passes actual TS and generated PostgreSQL HTTP/MCP/SDK fixtures. Checkout HTTP/MCP/SDK wiring passes eight actual TS controller comparisons, cross-process payment races and ten parallel-cart checks. Verified Stripe/PayPal callbacks pass actual TS business comparisons and cross-process capture/replay/inventory checks; 31 application routes plus health start over HTTP/MCP. Generated Compose starts the separate inventory worker; Linux HTTP/MCP, paid/cancelled delivery and graceful shutdown pass. Persistence compliance and interrupted capture reconciliation remain. |
| [`@forklaunch/blueprint-iam-base`](../blueprint/iam-base/package.json) (generated/private) | Rust init module iam-base | Generated sessions plus thirty configured directory CRUD/grant routes, JWKS/HMAC/RBAC, transactional organization scope and HTTP/MCP startup are covered. Configured discovery/compliance also match the actual all-`none` policy; application-specific metadata and provider lifecycle remain. |
| [`@forklaunch/blueprint-iam-better-auth`](../blueprint/iam-better-auth/package.json) (generated/private) | forklaunch-iam-better-auth and native CLI preset | All 70 configured raw routes, four server-only helpers, seven application HTTP/MCP contracts, surfacing/compliance/retention, real encrypted ORM conformance, popup helpers, Scalar sources and default/custom/root namespaces pass actual comparisons. Generated restricted PostgreSQL, SDKs, shared/separate/disabled MCP, self-JWKS, restart and rename/delete pass. Broader custom options/hooks/client plugins, typed JWT/JWKS adapters and indexed storage remain. |
| [`@forklaunch/implementation-billing-base`](../blueprint/implementations/billing/base/package.json) | forklaunch-billing-base | Five asynchronous base services implement plan/subscription CRUD and checkout/payment-link/portal cache lifecycles, with typed DTOs, optional backups and tenant-RLS PostgreSQL persistence. 168 actual TS base-class operations and native concurrency/expiry/encryption/audit/rotation fixtures pass. Generated billing modules and HTTP/MCP startup now pass; 229 actual configured mapper cases now pass; Redis cache interop remains. |
| [`@forklaunch/implementation-billing-stripe`](../blueprint/implementations/billing/stripe/package.json) | forklaunch-billing-stripe | Version-pinned authenticated Stripe client and five persistent async provider services pass 21 official SDK transport cases and 144 actual TS wrapper operations with backup/cache lifecycle. Raw-body authentication is shared with ecommerce and rejects stale callbacks. Legacy synchronous adapter fails closed. Generated modules/contracts and HTTP/MCP startup now pass. Twenty-six actual TS webhook state/provider-effect comparisons pass, with paginated product updates and subscription reassignment protection. Native tenant-RLS webhook receipts and replica-safe platform nonces pass PostgreSQL gates. Managed outbound transport passes 23 actual TS gateway comparisons and native JSON capability signature checks. Configured mapper audit now covers 65 actual cases; reconciliation remains. |
| [`@forklaunch/implementation-cac-base`](../blueprint/implementations/cac/base/package.json) | forklaunch-cac-base | Mock codes and provider descriptor/lookup match actual TS fixtures; authenticated generated module, both SDKs and CLI lifecycle now pass. |
| [`@forklaunch/implementation-ecommerce-base`](../blueprint/implementations/ecommerce/base/package.json) | forklaunch-ecommerce-base | Six base services, PostgreSQL persistence, restricted-role tenant RLS and encrypted/audited order addresses. Actual TypeScript 208-operation base and 35-case catalog-controller conformance pass, alongside real PostgreSQL concurrency/security and bounded catalog reads. Generated module integration remains separate. |
| [`@forklaunch/implementation-ecommerce-paypal`](../blueprint/implementations/ecommerce/paypal/package.json) | forklaunch-ecommerce-paypal | Bounded OAuth/Orders/signature-verification client and shared-store payment wrapper. Actual TS conformance covers 22 client cases and 19 payment operations; live fixtures cover concurrent capture and rollback. Generated webhook/controller wiring passes live HTTP/MCP, provider verification and cross-process capture tests; interrupted-capture reconciliation remains. |
| [`@forklaunch/implementation-ecommerce-stripe`](../blueprint/implementations/ecommerce/stripe/package.json) | forklaunch-ecommerce-stripe | PaymentIntent creation/retrieval, resume, direct Connect fees and shared-store confirmation/failure. Fifteen actual TS scenarios and bounded live HTTP fixtures pass; secrets stay out of persistence. Generated raw-body webhook/controller wiring passes official SDK verification, actual TS controller comparisons and live HTTP/MCP/SDK checks. |
| [`@forklaunch/implementation-iam-base`](../blueprint/implementations/iam/base/package.json) | forklaunch-iam-base, forklaunch-iam-axum | Typed single/batch directory CRUD, configured mapper behavior, nested relationships, encrypted PostgreSQL/RLS, atomic audit/rollback, retained-key uniqueness and session persistence exist. Actual TS service/route/schema conformance covers 44 operations, all 33 configured routes and 22 schema layouts, plus actual configured compliance export/erase behavior. Provider/session integration and generic application compliance extensions remain. |
| [`@forklaunch/implementation-messaging-base`](../blueprint/implementations/messaging/base/package.json) | forklaunch-messaging-base | Hooks-only sent behavior, encrypted tenant Postgres persistence, audited reads, subject export/erasure and key rotation; actual TS service fixtures pass. |
| [`@forklaunch/implementation-messaging-twilio`](../blueprint/implementations/messaging/twilio/package.json) | forklaunch-messaging-twilio | Actual TS REST/status/callback/error conformance; bounded single-attempt dispatch, signed form callbacks and generated SDK/runtime fixtures. |
| [`@forklaunch/implementation-worker-bullmq`](../blueprint/implementations/worker/bullmq/package.json) | forklaunch-worker-redis optional bullmq feature | Official wire protocol passes actual TS/native producers/consumers, retry/backoff, default retention, waiting/active peeks and restricted ACL fail-stop. Generated lifecycle, per-attempt failure observers (including removed/draining jobs), HTTP controls and count-based retention pass. Lazy age retention, bidirectional abandoned-job recovery, stalled/start limits and retained/removed preprocessor failure callbacks now pass. A feature-gated outcome-data hook in the pinned vendored protocol preserves original events through deletion, without changing Redis scripts or existing failure notifications. Job defaults/metadata, size-limit rejection, bounded log retention, delayed/priority/LIFO ordering and concurrency also pass actual TS/native checks. Concurrent rate limiting and bounded age-cleanup limits also pass actual TS/native fixtures. Simple/TTL/delayed-replacement deduplication now passes six actual TS/native producer and consumer scenarios, including cross-producer keys, completion release and TTL expiry. Sixteen worker scenarios preserve failure reasons and bounded histories for recovered/terminal jobs under immediate/delayed retries. Four additional actual TS/native consumer scenarios cover active-job keep-last deduplication, cross-producer submission, latest-payload follow-up and release after either completion or terminal failure. Twelve actual-library scheduler scenarios now cover bidirectional interval/cron schedules, three-iteration limits, success/failure, inspection/removal, scheduling before processing, active cancellation, preprocessor rejection and zero disabling start limits. Scheduler templates and every iteration also retain log/history/size options, with real bounded-log and failure-history checks. Iteration creation timestamps and cron scheduled-time metadata also match actual TS workers. Scheduler date/other metadata, broader cron/update profiles and advanced-option conformance remain open. |
| [`@forklaunch/implementation-worker-database`](../blueprint/implementations/worker/database/package.json) | forklaunch-worker-database | Persistent lease/runner/retry/dead-letter/retention/control suites exist; public option and cross-language DB-wire audit remains. |
| [`@forklaunch/implementation-worker-kafka`](../blueprint/implementations/worker/kafka/package.json) | forklaunch-worker-kafka | Transactional native producer/consumer/admin and generated/Linux suites exist; TS Kafka event-array envelope interoperability remains. |
| [`@forklaunch/implementation-worker-redis`](../blueprint/implementations/worker/redis/package.json) | forklaunch-worker-redis | Persistent lease/runner/admin/control suites exist; native protocol differs from TS, so interoperability remains explicit. |
| [`@forklaunch/interfaces-billing`](../blueprint/interfaces/billing/package.json) | forklaunch-billing-interface | Typed DTOs and asynchronous traits cover all five TS service families, including CRUD/update, provider fields, dates, party lookups and session lifecycle. Thirty shared application contracts plus compliance per variant pass 66 actual TS route and 28 actual Zod runtime/OpenAPI DTO schema comparisons, including enums, required fields, UUID IDs and provider-specific `stripeFields`. Generated runtime, SDK and compliance checks pass; callback/platform contracts now match actual TS metadata. |
| [`@forklaunch/interfaces-cac`](../blueprint/interfaces/cac/package.json) | forklaunch-cac-interface | Provider interface/DTOs, code-set and compliance contracts, authenticated runtime, generated module and both SDKs are validated. |
| [`@forklaunch/interfaces-ecommerce`](../blueprint/interfaces/ecommerce/package.json) | forklaunch-ecommerce-interface | Catalog/inventory/cart/order/payment DTOs and six service traits implemented; native integer money/stock and absent-versus-null validation. Thirty-one application contracts match actual TS route metadata; generated HTTP/MCP, both SDKs, worker and lifecycle fixtures pass. |
| [`@forklaunch/interfaces-iam`](../blueprint/interfaces/iam/package.json) | forklaunch-iam-interface | Core/session DTOs plus typed asynchronous user/role/permission/organization CRUD interfaces and configured contracts exist; generic application mapper/filter extensions and provider interfaces remain. |
| [`@forklaunch/interfaces-messaging`](../blueprint/interfaces/messaging/package.json) | forklaunch-messaging-interface | Send/record DTOs, status enum, async service/store traits and module contracts; optional metadata matches omission and rejects null. |
| [`@forklaunch/interfaces-worker`](../blueprint/interfaces/worker/package.json) | forklaunch-worker-interface | Async queue/admin/lease interfaces exist; TS producer/consumer option/event DTO mapping remains. |
| [`@forklaunch/blueprint-messaging-base`](../blueprint/messaging-base/package.json) (generated/private) | generated Rust messaging-base + forklaunch-messaging-axum | HMAC routes, JWKS/system-role compliance, encrypted Postgres/RLS/auditing, both SDKs, restart and CLI lifecycle fixtures. Shared MikroORM schema and generic retention metadata remain separate work. |
| [`@forklaunch/blueprint-messaging-twilio`](../blueprint/messaging-twilio/package.json) (generated/private) | generated Rust messaging-twilio + forklaunch-messaging-axum | Trusted-URL callback signature verification, exact upstream delivery transitions, both SDKs including forms, mixed-app init/rename/sync/delete. Callback verification intentionally strengthens upstream public controller. |
| [`@forklaunch/blueprint-monitoring`](../blueprint/monitoring/package.json) (generated/private) | forklaunch-monitoring-interface and core telemetry | Monitoring DTOs and exporter bootstrap exist; audit all generated monitoring registrations/instruments. |
| [`@forklaunch/blueprint-sample-worker`](../blueprint/sample-worker/package.json) (generated/private) | sample-worker and generated durable workers | Database/Redis/Kafka lifecycles exist; audit producer/consumer options, interop and BullMQ. |

## CLI extension surfaces outside published packages

`cli/src/constants.rs` also exposes the `relay` module, which injects an OAuth
session-ingest endpoint into an existing IAM service, and `init module storefront`,
which extends an app with its frontend. Rust/mixed support for both needs an
explicit behavior and lifecycle audit. They must remain in scope even though
there is no corresponding published blueprint package.

## Completion criteria

For each row, map exported service methods, DTO fields/enums, route/status/auth
contracts and provider options to implementation and fixture names. Validate
init/change/sync/delete, offline artifacts, both generated SDKs, relocated builds,
actual provider/storage behavior and mixed applications where applicable. The
package-level inventory is complete; detailed symbol/DTO and conformance mapping
is still in progress and the implementation-plan inventory task stays open.

Configured billing audit: HTTP parsing strips unknown fields and allocates local UUIDs for Stripe plan/subscription creates; optional `stripeFields` now deserialize in generated SDKs, including verified-webhook-created portals. Two hundred twenty-nine actual configured mapper cases execute repository schema resolvers, Zod validators and mapper definitions over owned ORM boundaries. Intentional differences remain: native scalar coercions and provider-object validation are stricter; native cached Stripe resources retain provider IDs that configured TS mappers discard; native updates can use primary cache without an optional database backup. These are recorded differences, not literal full validator/mapper parity. Generated base/Stripe HTTP/MCP and both SDK regressions pass.

Managed payments: `forklaunch-blueprint-core::payments_client::PaymentsClient` implements instance-signed onboarding links and readiness status with typed DTOs, gateway status and positive finite numeric Retry-After hints. Sixteen actual TypeScript/native client cases cover routes, exact JSON-byte HMAC verification, fresh nonces, redirects, readiness, refusal/JSON failures and response bounds. Native redirect URL/response validation is stricter and remote error bodies remain redacted. This completes the payments helper; other managed capability helpers remain in scope.

Port managed SMS client, typed send/event DTOs and GSM/UCS accounting. Fifty-five actual TypeScript/native cases cover all GSM extensions, UTF-16 surrogate-pair and ten-segment boundaries, E.164, ECMAScript whitespace, default/promotional purpose, deferred refusal, signed JSON wire bytes, fresh nonces, opt-out/refusal statuses and retry hints. Native response DTOs are validated and remote errors redacted. CLI managed-capability registration/event stubs for native modules remain in scope; client coverage alone does not complete infra add/remove parity.

Port managed WhatsApp template/text/listing client and delivery DTOs. Twenty-one actual TypeScript/native comparisons cover template component pass-through, empty listings, recipient/text validation, Unicode whitespace, platform policy refusals (403/409/422/429), signed exact JSON bodies, nonce freshness and retry hints. Typed native responses reject malformed data. CLI capability registration/event routes remain open.

Port managed voice start/read/end and event DTOs. Forty-seven actual TypeScript/native comparisons cover catalog flow names, E.164, attribute key/value/count/UTF-16/byte limits, reserved prefixes, call-ID isolation, status, empty 200/204 hang-up responses, exact signed bodies, nonce freshness, refusal statuses and retry hints. Empty-response support is opt-in; existing provider JSON response validation remains strict. CLI capability wiring/event mounting remains open.

Port managed email send/suppression and delivery DTOs. Fifty-one actual TypeScript/native comparisons cover mailbox rules, scalar/array recipients and replies, 50-recipient bounds, UTF-16 subject length, UTF-8 body size, tag limits/reserved names, suppression lowercase and encodeURIComponent paths, signed body bytes, gateway refusals and retry hints. Test servers collect complete bytes before UTF-8 decoding, including a 512 KiB multibyte body. Native email responses remain typed; CLI capability/event wiring and model gateway remain open.

Add reusable platform-event verification with an asynchronous replaceable nonce-store interface and bounded atomic memory default. Thirty-four actual TypeScript/native comparisons cover exact raw bytes, method/path/key binding, minimal event shape, skew boundaries, replay, malformed payloads and same-event-ID redelivery. One hundred concurrent deliveries have one nonce winner. Native retains future-dated nonces through their full inclusive validity window, closing an actual TS replay gap, and bounds bodies/storage without evicting valid nonces. Memory is single-process; replica callers must supply shared storage, and event-ID business deduplication remains handler-owned. Generated capability routes/wiring remain open.

Add trusted managed-settings resolution and explicit fixed Docker gateway-mock transport. Fourteen actual TypeScript/native configuration cases cover missing/partial/complete settings, loopback, fixed mock opt-in, remote TLS, invalid opt-in and credential redaction; normal transport continues to reject remote plaintext. Generated Stripe billing uses the validated gateway constructor and passes HTTP/MCP, both SDKs, callbacks, restart, CLI lifecycle and invalid-opt-in startup rejection. Actual Docker capability add/remove/startup wiring remains open.

Add native managed-capability CLI wiring for email, SMS, WhatsApp, voice and payments. `infra add/remove` resolves the registered native crate path, stages typed client factories, handler stubs, callback contracts and shared HTTP/MCP middleware, and records source/dependency ownership. Clients share a gateway connection pool; SMS defers unmanaged refusal as the TypeScript registration does. Fresh relocated mixed applications compile both SDKs and pass HTTP/MCP startup, actual-TypeScript-signed callbacks for every feature, forgery/staleness/replay rejection, authenticated MCP boundaries, dry-run and stacked removal preservation. Untouched files restore byte for byte; human handler/Cargo/library/env edits survive. All 60 CLI infrastructure tests pass, including 31 existing TypeScript capability tests. Linux Compose verifies all five outbound clients over signed Docker-DNS calls and signed gateway-to-service callbacks, plus HTTP/MCP startup. Archive-installed CLI generation, service HTTP/MCP settings, callback validation and contract/SDK exports pass. Ownership snapshots are Git-ignored and private on Unix because restoration records may contain pre-existing local settings. Capability lifecycle rename/delete, safe removal without local ownership records, shared replica nonce storage, and in-place S3/Redis CLI wiring remain open.

Preserve native capability hooks and custom source through service sync, rename and deletion. Renames move private ownership records, update Cargo restoration snapshots and local callback targets, and update generated config/smoke-test names without rewriting customized files. Registered custom crate locations survive sync and rename. A fresh-clone recovery path removes generated hooks without a local ownership record while retaining dependencies/settings of unknown origin. Deletion removes ownership records and releases or retains the shared mock according to remaining manifest capabilities. Generated relocated mixed-app fixtures pass HTTP/MCP before/after rename, SDKs, six service smoke tests, custom nested crate paths, clone recovery and deletion; 62 infrastructure tests pass. Multi-service mock baseline/target preservation and human edits to managed dependency entries remain under review.

Track the local gateway mock at application scope so a custom pre-existing mock survives both service removal orders and deletion even when the first capability owner leaves early. Removing the active owner promotes another registered service; removing an inactive owner preserves the existing target. Rename handling retains exact host boundaries and URL suffixes. Real CLI/Compose configuration tests pass both orders, callback promotion, deletion and custom baseline restoration. Cargo ownership comparisons now use TOML values, avoiding false human overrides when sync changes inline dependencies into tables; custom features/paths/removals remain preserved. All 960 CLI tests and the expanded generated HTTP/MCP/SDK/lifecycle fixture pass. Edits made to the shared mock while capabilities are active still need ownership-aware merging; local-mock routing supports one target per feature.

Shared gateway restoration now merges active developer edits, preserves adopted mocks and honors explicit mock removal; real CLI/Compose ownership fixtures pass. Compose long-form network/port/volume parsing remains a separate CLI parity gap.

Generate a preserved `src/capabilities/nonce_store.rs` factory returning `Arc<dyn PlatformEventNonceStore>`. Callback dispatch creates one verifier with that store for every feature; application code can supply shared atomic storage without editing the owned dispatch section. A customized capacity-two store survives sync/rename, accepts email and SMS, then returns 503 for voice, proving shared capacity rather than a new store per callback. Default stub removal and custom factory retention pass alongside generated HTTP/MCP, SDK, source/dependency ownership, clone recovery and custom crate lifecycle checks. All 960 CLI tests pass; the default remains single-process memory and applications must configure shared storage for replicas.

Preserve standard structured Compose service fields through CLI edits. Native/TypeScript shared parsing accepts network maps, long ports/mounts and short dependencies, retaining aliases/IP assignments, port metadata, bind options, dependency required/restart flags and build arguments/secrets. Application rename retains those options and host port allocation understands addresses/protocols/ranges. All 964 CLI tests, normalized Compose capability ownership edits and generated HTTP/MCP, SDK and lifecycle fixtures pass. This closes the Compose parser gap; native Redis/S3 in-place CLI wiring remains open.

Wire native in-place Redis/S3 infrastructure commands and honor init/change infrastructure flags. Editable typed factories, relative Cargo dependencies, local settings, manifest resources and shared Compose backends support registered nested crate paths, sync, rename, removal, deletion and clone recovery. Customized factories/dependencies/settings and adopted backends remain application-owned. Generic service metadata edits preserve source. Fix relative directory paths and update ejection to use the source parent; capability removal no longer restores unrelated removed infrastructure settings or discards adopted/deleted gateway dependencies. All 967 CLI tests, real CLI/Compose ownership fixtures and relocated generated Redis/MinIO binary round-trips pass; HTTP/MCP enabled/disabled/custom-path startup and both SDK compilers pass. Factories currently expose raw adapters with explicit local S3 credentials; default production AWS credential-chain configuration and generated encryption/compliance wiring remain open.

Packaged CLI validation also passes native Redis/S3 generation alongside all five managed capabilities, relocated compiler checks, service HTTP/MCP settings, signed callbacks and contract/SDK exports. Fresh IAM, CAC and base/Twilio messaging runtime fixtures pass HTTP/MCP and lifecycle checks; the IAM fixture now rejects an actually unsupported module instead of the now-supported billing module.

Add production S3 environment resolution and an async generated factory using the official AWS default credential chain with profile/SSO/process/container-role support. Complete S3 key pairs override the chain; partial pairs follow the TypeScript fallback. Both factories honor namespace and presign caps. Thirty-one actual TypeScript/native configuration cases cover endpoint/region/key combinations, prefixes, numeric/radix/fractional values and ECMAScript whitespace. An owned ECS credential endpoint supplies session credentials used to sign actual SDK object/tag requests under the configured prefix. Generated relocated factories round-trip data through real Redis/MinIO using standard AWS environment credentials, pass HTTP/MCP settings and both SDK compilers. All 967 CLI tests, 18 S3 unit tests, the actual S3 backend test and strict all-target clippy pass. Preserve the synchronous explicit-local factory for compatibility; encrypted/compliance-aware cache and object-store wiring remains open.

The production S3 checkpoint also passes archive-installed CLI generation, portable framework compilation, HTTP/MCP settings, signed capabilities and contract/SDK exports. The package-source inventory now checks 268 embedded source/template assets, including the new S3 example directory and infrastructure/capability factories.

Align native Redis list queues with the actual TypeScript LPUSH/RPOP wire layout and add non-destructive batch peeks in dequeue order. Keep an explicit LegacyNative RPUSH/LPOP mode for existing native backlogs; producers/consumers must use one consistent layout. Actual TypeScript queue methods and native clients exchange data through the isolated Redis backend in both directions, covering batch/single FIFO, mixed producers, peeks and legacy backlog reads. Strict all-target clippy and all six Redis tests (including the actual backend fixture) pass. This closes list orientation interoperability; JSON/encryption/compliance, complete TTL/cache metadata and BullMQ worker wire conformance remain open.

Add shared fallible `JsonRecordCodec`, typed `JsonCache` and async `JsonObjectStore` wrappers plus explicit generated encrypted factories. Per-operation validated tenant context selects authenticated v2/v3 envelopes; encrypted reads without context, wrong tenants, malformed payloads and serialization failures fail closed. Plaintext JSON migration and key rotation work, including false/zero/null values. Pre-serialize cache batches before writing. Actual TypeScript Redis methods and native clients exchange encrypted JSON in both directions against Redis, covering rotation, expiry, corruption and tenant rejection. Relocated generated services use the factories against real Redis/MinIO with encrypted at-rest checks, metadata preservation and wrong-tenant rejection, then pass HTTP/MCP enabled/disabled/custom endpoint startup and both SDK compilers. All 967 CLI tests, five core tests and six non-ignored Redis tests pass; strict infrastructure all-target clippy and 273 packaged source assets pass. Direct TypeScript/native S3 storage comparisons, encrypted queue convenience methods and complete TTL/cache metadata remain open.

Add generated `encrypted_queue(name)` and typed enqueue/dequeue/batch/peek helpers, with TypeScript-compatible FIFO envelopes and explicit decode-after-removal semantics. Add transactional GET/PTTL JSON cache reads retaining subsecond expiry and persistent-key metadata. Actual TypeScript/native encrypted cache/queue exchanges pass v2/v3, both producer directions, batch order, non-destructive wrong-tenant peeks and false/zero/null preservation. Actual TypeScript S3 methods over their injected SDK boundary and signed native AWS SDK requests exchange v2/v3 JSON in both directions, including prefixes, content types, rotation, plaintext migration, misses and tenant/corruption rejection. Real MinIO encrypted factories remain covered by the generated runtime fixture. Fresh relocated encrypted-queue/service runtime passes HTTP/MCP settings, shutdown and both SDK compilers; strict infrastructure all-target clippy, six non-ignored Redis tests and 274 packaged source assets pass. The prior encrypted-factory checkpoint also passed archive-installed CLI generation, HTTP/MCP, managed callbacks and SDK exports. Per-record batch TTLs, complete S3 helper coverage and worker/BullMQ wire parity remain open.

Complete typed Redis per-record TTL batches with prevalidated keys/expirations and transactional value/expiry reads. `put_records` supports a separate TTL or configured default per entry; `read_records` keeps order, misses and persistent-key metadata; literal-prefix SCAN reads expose decoded records with expiry. Actual TypeScript/native batch writes and reads compare values and relative expirations in both directions, including encrypted envelopes. Invalid TTL/key batches leave the first valid entry unwritten, empty batches work, and glob characters are matched literally. The full Redis fixture, including the actual backend, passes all seven tests; strict all-target clippy and the extended encrypted cache/queue conformance harness pass. Retain the intentional native semantics of precise milliseconds, explicit persistent expiry, misses as Option and falsy-value preservation rather than TypeScript's rounding/filtering accidents. Worker/BullMQ interoperability remains open.

Add typed S3 JSON batch writes/reads and single/batch deletes, defaulting absent content types to application/json while preserving custom types/tags and ordered misses. Serialize/encrypt all JSON before starting batch writes; backend partial-write behavior remains explicit. Actual TS object-store methods (including JSON stream-upload aliases) and signed native AWS SDK calls pass encrypted single/batch exchange, false/zero/null payloads, deletes, missing objects and empty batches. A serialization-failure regression proves no S3 write occurs and serializer details stay redacted. Generated encrypted factories pass real MinIO single/batch writes, metadata/default-type reads, missing keys, deletes and wrong-tenant rejection, then HTTP/MCP settings/shutdown and both SDK compilers. Strict S3 all-target clippy, 19 unit tests and encryption-only feature tests pass; all 274 embedded source assets package correctly. Complete binary/stream/presign helper parity, worker/BullMQ interoperability and release gates remain tracked separately.

Wire platform Redis key/channel namespace conformance into native leased workers and Pub/Sub controls. Resolve REDIS_KEY_PREFIX before FORKLAUNCH_REDIS_KEY_PREFIX with actual TypeScript-compatible whitespace/separator behavior; preserve dedicated-cache keys and native cluster hash tags. Add injected RedisKeyScope/connect_scoped APIs and sanitized NOPERM errors. A real restricted ACL fixture compares actual TS prefix helpers, enqueues/claims/acks native jobs, fences tenants, delivers Pub/Sub controls and denies foreign keys/channels and FLUSHDB. The runner fails promptly on NOPERM rather than looping. The fixture exposed redis 0.32 PubSub subscribe silently discarding server-error payloads; add a bounded subscription-ack preflight without publishing control messages, and verify denied subscription startup fails. All seven Redis-worker unit tests and both actual Redis integration tests pass, as do strict all-target clippy, 276 packaged assets and generated worker runtime/control/retention/batches/relocation/rename/backend-change/delete regressions. This closes platform prefix/ACL startup behavior; BullMQ wire compatibility remains open and is being implemented separately.

Add an optional official BullMQ Rust protocol adapter and actual TypeScript/native interoperability fixture. Both producer/consumer directions pass single/batch jobs, fixed/exponential retries, terminal failures, duplicate event IDs, default retention, and waiting/active inspection against a real restricted Redis ACL. A denied native consumer fails promptly and drains. Strict all-feature blueprint workspace clippy, seven worker unit tests, both actual Redis integration tests, formatting and 278 packaged source assets pass. CI now runs infrastructure JSON/encryption and worker ACL/BullMQ conformance fixtures using pinned BullMQ 6.3.10. Generated BullMQ lifecycle support, native failure observers and complete option/retention coverage remain open.

Generate native BullMQ workers through init/change/sync/rename/delete with the official protocol adapter, preserved application handlers, retries/backoff/concurrency/timeouts, inspection and scoped optional Pub/Sub controls. Add HTTP health/readiness and authenticated pause/resume/drain with active-job shutdown, plus a watch-based control transition API. Per-attempt native failure observers retain original events through retries, automatic removal and graceful drain; consumer peeks work before start. Actual TS/native fixed/exponential retry and default/count-based retention fixtures pass both directions under restricted Redis ACLs. Fresh relocated generation passes explicit stub failures, real processing/retries, HTTP controls, service HTTP/MCP initialization/discovery/health/enabled/disabled/custom-path checks, contracts/SDKs and source-preserving lifecycle changes. A nested workers/team/jobs registration passes sync/reinit/backend conversion/rename/delete, closing hardcoded worker crate/dependency paths. All 967 CLI tests, strict Redis/database-worker clippy, three control-transition tests and 283 packaged assets pass. E2E CI includes the pinned BullMQ runtime fixture. Age-based retention, stalled-job/custom-option behavior and broader worker conformance remain separate gates.

Bind additional typed MCP tools to the real Streamable HTTP runtime. ManualTool::typed derives discovery metadata from FrameworkSchema, parses/validates before application work, supplies fresh headers for operation authorization and preserves transport authentication. Reject manual/contract name collisions at startup; normalize explicitly public handler failures, redact serializer details, bound invocation time/output and preserve object/scalar/array results. Eleven MCP tests exercise initialization/discovery/invocation, HTTP/manual coexistence, schema/type/range/unknown-field rejection before side effects, tenant credentials per call, timeout, output limits and serializer failures. Strict MCP/Axum and whole-blueprint all-feature clippy, formatting and 285 embedded assets pass. Fresh generated generic HTTP/MCP and SDK fixtures pass after the new dependency. Fresh IAM, CAC, base/Twilio messaging, billing/Stripe and ecommerce runtime/lifecycle suites also pass HTTP/MCP startup and authenticated dispatch. Separate listeners, multipart and complete actual-generator conformance remain open.

Add configurable separate MCP listener orchestration to generated generic, IAM, CAC, messaging, billing and ecommerce executables. MCP_LISTENER=separate uses the actual TypeScript HTTP-port-plus-2000 fallback; MCP_PORT/MCP_HOST select explicit/ephemeral ports and hostname/IP binding. Preserve the native shared-listener default explicitly. The second router exposes only MCP while tools retain internal HTTP validation/authentication; both listeners bind before serving, then drain together with a 30-second bound. Live two-listener tests verify route credentials, denied calls, concurrent active HTTP/MCP completion and closed ports. Fresh generated startup covers shared/separate/disabled/custom endpoints, explicit/ephemeral/default-offset ports, hostname binding, invalid settings and occupied-port rejection without a leftover HTTP listener. Generated BullMQ runtime and nested lifecycle regressions also pass. All 967 CLI tests, 31 Axum unit tests, two live MCP integration tests, eleven MCP tests, strict Axum/MCP clippy, formatting and 287 packaged assets pass. Multipart and remaining actual-generator conformance are still open.

Fix managed-capability wiring for the new serve_with_default_mcp entrypoint while retaining legacy into_router_with_default_mcp mounts. Insert the capability router exactly once and preserve byte-for-byte removal; reject ambiguous/missing mounts. A regression exercises all six current generated entrypoint templates plus the legacy call. All 968 CLI tests pass, and fresh billing/Stripe and ecommerce HTTP/MCP/SDK/capability/lifecycle fixtures pass after the correction. IAM, CAC and base/Twilio messaging fixtures also pass with coordinated listener startup.

Add MCP multipart text-field transport, File/Binary JSON-argument encoding and URL-encoded composite values matching the actual TypeScript generator. Generate collision-checked MIME boundaries and escape field names/line breaks; reject non-string multipart parts and SSE request bodies. Thirty-two actual-generator/body-discriminator comparisons and fifteen MCP unit/transport tests pass, including real Axum Multipart/Form/Bytes extractors behind route authorization. Strict full-framework all-feature/all-target clippy, formatting and 289 embedded assets pass. Expand the archive-installed CLI fixture with official BullMQ generation and assertions for typed, separate-listener and multipart MCP sources. Streaming, arbitrary multipart upload parts and broader MCP option conformance remain open.

Complete multipart uploads in both CLI SDK targets through contract and OpenAPI generation. TypeScript accepts FormData or typed field records and uses the actual shared safeStringify source for multipart/URL-encoded values; files, repeated names, JSON composites and special JavaScript values match thirteen actual universal SDK comparisons. Normalize header casing and let encoders own boundaries. Rust SDKs consume native reqwest forms, preserve authentication/hooks and repair reqwest zero-byte empty forms with a closing MIME delimiter. Preserve multipart bytes in both HTTP adapters. The framework Rust request builder now encodes owned multipart text/file parts and exposes dynamic content types; the framework TypeScript generator clears conflicting multipart headers. Strict standalone Rust/TypeScript compilation and standard MIME parsing pass; fresh relocated SDKs call Rust and TypeScript services successfully from both artifact formats. All 969 CLI tests, 31 Axum unit/two live MCP tests, fifteen MCP tests, both SDK unit suites, strict framework workspace clippy, formatting, 290 embedded assets and a fresh archive-installed CLI fixture pass. The archive fixture also typechecks its generated TypeScript SDK and verifies the bundled shared stringifier, all four workers, modules, capability callbacks and HTTP/MCP startup. Multipart responses, automatic typed extraction, SSE and remaining recursive/realtime SDK tasks remain open.

Add transport-independent bounded SSE encoding/decoding with arbitrary chunk boundaries, UTF-8/BOM, CR/LF/CRLF, multiline data, comments, IDs/resets, event names and retry metadata. Reject metadata injection, fail closed on event overflow and discard unterminated EOF frames according to the HTML standard. Eighteen HTTP tests, strict HTTP clippy, formatting and 292 embedded assets pass. Thirty-five actual TypeScript universal SDK comparisons pass for common terminated frames; CI now runs them. Document existing TS multiline/whitespace/EOF differences explicitly. Generated streaming clients, cancellation/reconnection and full typed runtime behavior remain open.

Generate bounded pull-driven SSE responses in CLI TypeScript/Rust SDKs from both contract and OpenAPI artifacts. Return typed async iterables or boxed owned Rust streams, preserve authentication/hooks, enforce configurable 1 MiB default event bounds and response MIME, deserialize Rust envelopes and release responses on iterator return/throw, abort, close/drop and terminal failures. Preserve complete events before later oversized events even when coalesced in one chunk. Live relocated service tests verify delivery before EOF, split UTF-8, BOM/multiline/CRLF/bare-CR framing, ID resets, auth before handler work, finite/truncated streams, wrong media, type/transport/size failures, unstarted cancellation, server stream cleanup and graceful HTTP/MCP shutdown. Both artifact formats pass, including strict generated-SDK clippy; the streams are boxed to keep response enums small, and request-hook type aliases close an existing clippy warning. All 970 CLI tests, nineteen HTTP codec tests, 31 Axum/two live MCP tests, fifteen MCP tests, both framework SDK suites, strict full-framework clippy, formatting and 298 embedded source/template checks pass. Thirty-five actual TS universal SDK/native codec/generated SDK SSE comparisons and thirteen form comparisons pass; ordinary HTTP/multipart/MCP SDK regressions pass against Rust/TypeScript servers from both artifacts. A fresh archive-installed CLI also passes portable generation, all four workers/modules, TypeScript SDK compilation, capability callbacks and HTTP/MCP startup; E2E CI now includes the dedicated streaming fixture. Framework-native typed response decoding, remaining emitter/schema conformance, recursive models and realtime/mixed SDK work stay open.

Support guarded recursive CLI SDK models through contract and OpenAPI artifacts: self/mutual records, optional/null fields, recursive arrays/dictionaries and object/array unions. Preserve named graph edges with bounded local reference processing; inline acyclic definitions to retain existing types. Reject external/missing references, validation siblings and unguarded aliases before writes. Bound cached wide-graph traversal as well as initial expansion, and omit Rust Default implementations across required recursive edges. Rust vectors avoid redundant boxes; collection aliases use transparent wrappers. Rebase standalone contract schema pointers when exporting OpenAPI components, escaping component names and retaining literal examples/defaults/constants. Fresh relocated clients pass strict TypeScript compilation (including a rejected invalid recursive request), strict Rust SDK clippy, recursive HTTP/MCP invocation and graceful shutdown through both artifact sources. All 977 CLI tests, 31 Axum/two live MCP tests, nineteen HTTP codec tests, fifteen MCP tests, six OpenAPI tests, both framework SDK suites, strict full-framework clippy/formatting and 299 embedded assets pass. Ordinary HTTP/multipart/SSE regressions and 40 query/thirteen form/35 SSE actual TypeScript comparisons pass. The expanded archive-installed CLI fixture compiles recursive clients through both artifact formats alongside all four worker types/modules and passes HTTP/MCP listener settings and managed callbacks. CI includes the recursive runtime fixture. Framework-native typed responses/recursive models, broader schema constraints, multipart responses and typed realtime/mixed SDKs remain open.

Share standalone-schema relocation between OpenAPI and MCP through the optional forklaunch-contract/json-schema feature. Rewrite schema-pointer edges only, retaining examples/defaults/constants/enums/extension data and properties named $ref; escape body/query/header/path field locations. Handle tuple/conditional and unevaluated schemas, and preserve embedded $id resource bases. CLI imports explicitly reject resource IDs and dynamic references until their resolver is implemented. Live recursive tool discovery preserves an example containing a literal $ref while HTTP/MCP invocations and both SDKs pass through both artifact formats. All 977 CLI tests, 31 Axum/two live MCP tests, sixteen MCP tests, five contract/five OpenAPI tests, nineteen HTTP tests, both SDK suites, strict full-framework clippy/formatting and 299 embedded assets pass. The contract crate also checks without default features. A fresh archive-installed CLI passes recursive client compilation through both artifacts, all worker/module compilation, HTTP/MCP shared/separate listeners and all five managed callbacks. Rust field omission/null distinctions, framework-native response/recursive models, resource/anchor resolution and broader realtime/mixed SDK work remain open.

Generated Rust SDKs now preserve field presence: absent optional fields are omitted, explicit null requires a nullable schema, optional nullable fields retain both states, and required nullable fields reject omission. Ninety-four actual TypeScript validator cases cover nested records, arrays and unions through both artifact formats, strict SDK compilation and Rust/TypeScript HTTP with MCP startup/invocation/shutdown. Correct the canonical TypeScript null schema enum from the string `"null"` to JSON null; all fourteen Zod tests pass. Box generated JSON model status payloads to bound enum size. All 977 CLI tests, HTTP/multipart/recursive/SSE regressions and 299 embedded assets pass; a fresh archive-installed CLI passes both SDK formats, all four workers/modules and HTTP/MCP listener settings/capability callbacks. Framework-native typed responses/recursive models, broader schema constraints and realtime/mixed SDKs remain open.

Share the CLI schema graph/type compiler, Serde model renderer and Rust client/SSE templates with framework-native generators. SdkGenerationPlan retains complete JSON schemas and response media kinds. The checked native Rust HTTP generator emits typed status/business-error responses, recursive models, exact omission/null presence, auth/hooks, multipart uploads and bounded owned SSE. Reject invalid graphs/media/statuses and reserved/conflicting names; bound literal annotations before cloning and JSON strings before parsing. Native clients pass the 94-case actual validator fixture against both language servers, recursive shapes and live SSE cancellation/resource cleanup with HTTP/MCP startup/invocation/shutdown. All 978 CLI tests, eight codegen/four Rust SDK/three TypeScript SDK tests, full all-feature framework tests, strict workspace clippy/formatting and 311 embedded assets pass. Ordinary HTTP/multipart and original native media builders pass regressions. An archive-installed CLI also compiles both artifact formats/all worker/module families and runs the native recursive generator from portable bundled sources. Native TypeScript typed responses/models, advanced schema/resource constraints and realtime/mixed SDK work remain open.

Share the TypeScript CLI fetch/SSE/form runtime and template structures with the checked framework-native HTTP generator. Emit typed request fields/bodies, status/business-error unions, guarded recursive models and bounded owned SSE; preserve omission/null states and escape control characters in schema literals/property names. Both native generators pass 95 actual validator cases, recursive models and live SSE cancellation/resource cleanup against HTTP/MCP services. Media tests caught and fixed request MIME parameter loss and empty Blob/ArrayBuffer uploads in the shared runtime. Native/CLI request media validation now preserves full headers while rejecting control-character injection. All 979 CLI tests, full all-feature framework tests, strict workspace clippy/formatting and 319 embedded assets pass. Multipart responses, broader schema constraints/resource resolution and realtime/mixed SDK work remain open.

Revalidate generated IAM, CAC, base/Twilio messaging, base/Stripe billing and ecommerce HTTP/MCP startup, discovery and invocation. Their complete runtime suites pass SDK, authenticated tenant/provider/persistence and lifecycle checks; ecommerce includes actual TS webhook comparisons and a separate durable inventory worker. Thirteen actual universal SDK form and 32 actual MCP generator/body comparisons pass. Fix stale messaging fixture signatures to follow the already-shared single-JSON-value record encoding while retaining explicit repeated URLSearchParams coverage.

A fresh archive-installed CLI passes relocated all-worker/module compilation, both SDK artifact formats, HTTP/MCP shared/separate/default/explicit listener settings and all five capability callbacks. Both native recursive generators run entirely from the portable embedded source tree; generated Rust SDK strict clippy and TypeScript compilation pass. Quoted request MIME profiles containing apostrophes/ampersands also pass both CLI languages/artifacts against Rust and TypeScript wire servers. Correct HTML-escaped metadata in shared source templates. An additional actual cross-fetch probe confirms an open SSE adapter gap (`response.body.getReader` is absent on its Node stream); this is the next runtime task.

Fix Node fetch-adapter SSE compatibility in the shared TypeScript runtime. Read native Web Streams or Node async-iterable bytes; own per-stream abort controllers to close cross-fetch upstream requests, forward caller aborts and remove listeners at completion. Actual cross-fetch 4.1.0 and native fetch pass both CLI artifacts and native clients, including imported-fetch generation, pending-read cancellation, unstarted return/throw, abort, MIME/size/transport errors, event ordering and exact server resource cleanup with HTTP/MCP graceful shutdown. All 979 CLI tests, strict framework clippy/formatting, native SDK tests, media and ordinary HTTP regressions pass. CI installs pinned cross-fetch and runs both transports. A separate probe finds cross-fetch Node upload gaps: native Blob bodies fail `.pipe`, and standard FormData is coerced to `[object FormData]`; those are next.

The SSE transport checkpoint also passes a fresh archive-installed CLI: both SDK artifact formats, portable native Rust/TypeScript generators, all four worker/module source families, strict generated Rust SDK clippy, TypeScript compilation, HTTP/MCP listener matrix and all five managed callbacks. Checkout CLI rebuilt after installer validation.

Fix actual cross-fetch Node upload failures by sharing a Web-body compatibility encoder across CLI checked TypeScript clients and dependency-free native TypeScript clients. Native fetch retains standard upload bodies; custom adapters receive Blob bytes or standard-encoded multipart bytes/MIME. Native, injected and imported cross-fetch pass raw/typed SDK byte comparisons, sliced arrays, Unicode/text/quoted vendor profiles, repeated and empty multipart files, URL-encoded composites and typed status/business errors. Thirteen actual universal SDK form, forty URL and 35 SSE comparisons pass. All 979 CLI tests, nine codegen/four Rust SDK/four TypeScript SDK tests, strict workspace clippy/formatting, fresh ordinary HTTP/SSE runtime regressions and 321 packaged assets pass. A fresh archive install passes both native generators, SDK formats, all worker/module families and HTTP/MCP listener settings/managed callbacks.

Decode schema URI fragments once in shared native/CLI SDK models and OpenAPI reference objects. Reuse canonical recursive edges, preserve literal percent names, and emit URI-safe relocated references after pointer-token escaping. Generated recursive Rust/TypeScript fixtures and raw codec comparisons cover Unicode/slash/tilde/percent definition names. Full parity remains open for schema resources/dynamic scope, external AsyncAPI imports, raw binary metadata and the broader feature gates above.


### Checked external WebSocket AsyncAPI import checkpoint

Import explicit AsyncAPI 3.0.0 JSON WebSocket server documents through `forklaunch_asyncapi` and CLI generation/check/watch. Validate offline against the licensed official specification schema, extract bounded transitive/recursive local payload graphs, preserve Draft 7 constraints, map server receive/send to client outgoing/incoming, retain disjoint multi-message `oneOf` rules and reject undeclared directions. Imported documents remain unchanged; per-target source ownership supports module pruning and service rename/delete. Mixed native/imported targets reject output/input collisions before writes. Relocated strict Rust/TypeScript consumers, live peers, official parser checks, configured watch recovery, invalid later-service staging and default/custom input lifecycle pass. Generated and renamed services pass the HTTP/MCP shared/separate listener, endpoint, port, atomic rejection and shutdown matrix. All 998 CLI tests, 22 AsyncAPI tests, full framework tests, strict workspace clippy/formatting and 342 packaged source assets pass. A fresh archive-installed CLI also passes external import, both SDK compilers, all worker/module families, HTTP/MCP startup and all five managed callbacks. Broader event projects, other payload dialects/protocols, JSON string wire encoding, upgrade parameters/authentication, resource-scoped schemas and richer message matching remain tracked separately.

### BullMQ abandoned-job and retention checkpoint

Actual TypeScript/native conformance passes age-based lazy retention in both producer directions, abandoned-job recovery in both consumer directions, maximum stalled/start attempts and retained/removed preprocessor failure callbacks. The pinned, licensed protocol source adds an optional outcome-data notification while preserving existing notifications and Redis scripts. All 66 protocol library tests pass on isolated Redis; full blueprint tests, strict workspace clippy/formatting and native Redis integration tests pass. Generated worker lifecycle and HTTP controls pass alongside service HTTP/MCP startup. All 998 CLI tests and 410 embedded source assets pass; a fresh archive install compiles relocated services/workers, both SDK families and portable native generators, and passes HTTP/MCP startup and all five managed callbacks. The vendored protocol also checks with its optional feature disabled. Broader worker options remain under audit.

### BullMQ job-default compatibility checkpoint

Preserve supported queue defaults before overriding wrapper retry/backoff settings and intentionally clearing default job IDs, matching actual TypeScript `add`/`addBulk`. An optional vendored protocol patch persists log/stack-trace limits, timestamps and size limits and reads the compressed Node log-limit key. Actual TypeScript/native fixtures pass persisted metadata, auto-generated IDs, retry overrides, oversized-job rejection before writes, bounded processing logs, delay/priority/LIFO order and concurrency limits. Strict worker all-target clippy, native tests and all 66 isolated protocol tests pass; generated worker runtime/lifecycle and the complete HTTP/MCP startup matrix pass again. Broader options and stack-trace processing semantics remain separate audit work.

### Checked schema pointer-profile checkpoint

Reject percent-encoded JSON-pointer separators at active schema locations before checked SDK, raw AsyncAPI export or AsyncAPI import generation. Preserve literal assertion/annotation data, encoded names, escaped tokens and general OpenAPI reference-object URI decoding. The codec conformance harness now records invalid-schema compilation rather than assuming every schema compiles; 4,320 actual TypeScript/native/generated codec comparisons pass, including rejected leading separators. Full framework tests, strict workspace clippy/formatting, 999 CLI tests and 410 packaged source assets pass. A relocated imported service passes rejected-profile generation/check with unchanged inputs/SDK files, both strict/live SDKs, watch/lifecycle and original/renamed HTTP/MCP startup matrices. Raw native/AJV interior-separator resolution beyond this checked profile remains an explicit separate task.

The checked pointer-profile checkpoint also passes a fresh archive-installed CLI: portable framework/blueprint and all worker/module families compile after relocation, both SDK artifact formats and native generators pass, and HTTP/MCP listener startup plus all five managed callbacks pass. This archive also contains the BullMQ job-default/log-retention compatibility patch.

BullMQ rate limits with concurrent workers and per-completion age-cleanup limits now pass actual TS/native conformance. Tests measure processing start times and retained job identities/counts, with strict worker all-target clippy and formatting passing. No runtime patch was required for these options.

Raw native socket codecs now match AJV's per-token URI-fragment decoding,
including interior encoded slashes, once-decoded percent names, Unicode,
annotation targets, recursive references and embedded resource scopes. The
isolated, renamed jsonschema/referencing dependencies preserve upstream RFC
behavior by default and opt socket codecs into AJV pointer and older-draft
`$ref` sibling validation without rewriting any schema data. All five dialects
pass 12,960 actual TypeScript-helper/AJV and generated TypeScript codec comparisons. Resolver library tests
pass in both modes (150 each); 1,992 upstream validator library tests pass,
with one test requiring an absent upstream Git-submodule fixture explicitly
filtered and documented in provenance. Checked SDK/export/import profiles
continue rejecting encoded separators. Broader parity remains open.

Fresh generated IAM, CAC, base/Twilio messaging, base/Stripe billing and
ecommerce runtime suites all pass after the schema compatibility update:
HTTP/MCP startup and authenticated calls, both SDKs, persistence, tenant/RBAC
checks and init/rename/sync/delete. IAM also exercises all 33 configured routes
and the retained credential/session routes.

The fresh archive-installed CLI passes portable relocation with both isolated
validator source trees, licenses, provenance, locks and bundled meta-schemas.
All module/worker families compile, native and TypeScript SDK generation
passes, and generated HTTP/MCP startup plus all five managed callbacks pass.
Strict framework workspace linting/formatting, framework tests, 999 CLI tests
and all 591 embedded Rust source/template assets also pass.

The configured Better Auth 1.7.6 reference audit now captures 70 HTTP routes
and four server-only helpers, with 48 actual protocol checks. It executes the
repository config plus real plugins, discovery and surfacing services against
an owned UUID/SQL adapter boundary. Signed/revoked cookies, password rotation,
Origin/callback validation, EdDSA/JWKS claims, default organization teams and
configured role denials are covered. Native Better Auth protocol, real ORM,
Google callback/provider behavior and account PCI compliance remain open.
Password reset/change-email/user-deletion/reference UI are disabled by actual
configuration; Google discovery lists the provider even without credentials.
Configured static access-control permissions do not create surfaced dynamic
role records. The implementation plan now tracks these scopes separately.

The actual Better Auth custom callback helpers now enforce configured trusted
callback origins, use the configured token-exchange authority, escape script
JSON and clear the configured session-cookie name/secure flag under HTTPS.
Forty checks execute the real server handlers and generated browser scripts
with actual session/JWT handlers under owned app/fetch/storage boundaries.
Untrusted destinations fail before token exchange; forged Host headers cannot
redirect exchange requests; malicious query values remain JSON data. Native
callback/helper implementation remains in the Better Auth module task.

Better Auth invitation expiry is corrected from milliseconds to seconds,
matching the intended 48-hour lifetime. A trusted operator-seeded dynamic
invitation grant exercises the actual handler and checks its persisted expiry;
the actual protocol fixture now passes 49 checks. Stored dynamic role grants
augment configured static roles, so native organization behavior must support
those grants as well as the default permission denials.

Native application-owned HTTP namespaces now support the raw Better Auth API's
routing model. Mounted Axum handlers retain global limits/request IDs while
remaining outside exported contracts and MCP discovery. Fixed-prefix validation
also covers parent nesting; namespace collisions with registered routes, docs
and MCP are rejected. Live HTTP cookie checks, exact MCP tool discovery,
authenticated invocation and shutdown pass, along with the full Axum suite and
strict framework clippy. Better Auth handler/storage implementation remains open.

The isolated native `forklaunch-iam-better-auth` crate now matches configured
password hashes and signed session-cookie primitives. A bidirectional fixture
runs 322 comparisons against actual Better Auth/Better Call, including Unicode
normalization, cross-language password verification, secure cookie attributes,
quoted/duplicate cookies, tampering and malformed URI data. CLI embeds the new
crate; protocol endpoints, persistent sessions and preset selection remain open.

Native Better Auth JWT/JWK primitives now match the actual configured Ed25519
signing profile and 15-minute claims, with 26 cross-library comparisons against
Better Auth/JOSE. Restoring private key bytes preserves IDs and signatures;
public JWKs exclude secrets. Durable encrypted key storage, key lifecycle and
the HTTP token endpoint remain open.

Native Better Auth storage now covers all eleven configured models with atomic
memory and fully encrypted PostgreSQL backends. Live database tests cover forced
tenant RLS, rejected bypass roles, atomic rollback/audit behavior, read-only
transactions, private key restoration, blind-index uniqueness across key
rotation and record/model ciphertext binding. Its schema migration ledger is
independent of other modules. Auth endpoint semantics, graph relationships and
indexed/paginated queries beyond the 4096-row scan bound are still open.

The subsequent keyring checkpoint completes encrypted signing-key persistence,
atomic initialization, live-key selection and operator rotation. Memory and
live PostgreSQL verify service reconstruction and concurrent callers; 27 actual
Better Auth/JOSE comparisons verify selection ties, default 30-day public-key
grace boundaries and expired-key replacement. Corrupted public/private key
metadata fails closed. Session-authenticated HTTP integration remains open.

The configured TypeScript Google provider now maps given/family name claims into
required application user fields. The actual handler previously rejected valid
verified Google sign-in with `400 MISSING_FIELD: firstName is required`. The new
fixture executes real RS256/JWKS verification and auth handlers, owning only the
Google certificates HTTP boundary, with account/session reuse and invalid token
rejection. Native Google OAuth and real ORM conformance remain open.

Actual session lifecycle now has 60 additional checks: exact hourly refresh,
creation-based freshness, explicit refresh suppression, scoped revocation,
transient sessions, replay rejection and millisecond expiry boundaries. The
configured get-session POST descriptor returns 405 without deferred refresh.
Fresh archive `kSwMp4` passed relocated auth foundation tests, live HTTP/MCP,
the complete generated startup/listener matrix, all five managed callbacks,
portable services/workers and both SDK generators; checkout CLI was restored.
Better Auth raw endpoints/preset and indexed auth lookups remain open.

The native persistent session backend now matches actual hourly refresh,
creation-based freshness, revocation scoping and transient expiry behavior.
The fixture passes 63 checks and 48 native body/storage comparisons, with shared
credential records seeded at the storage boundary. Memory and real encrypted
PostgreSQL lifecycle tests also pass. Native signup/sign-in, production HTTP
cookies/Origin/CSRF integration and indexed lookup remain open.

Native email signup/signin now commits encrypted user/account/session records
atomically. Thirty-nine comparisons execute actual configured email handlers and
match response/storage behavior, callbackURL naming, case normalization and
UTF-16 password limits. Memory/live PostgreSQL exercise concurrent signup, and a
controlled password-change race proves stale verification cannot mint a session.
Bounded blocking password work stays outside tenant write locks. HTTP validation,
cookie/CSRF/Origin integration, credential/account updates and indexed queries
remain open.

Native credential comparisons now total 81, including actual authenticated
password verification/change handlers. Wrong and oversized passwords preserve
credentials/sessions; revocation replaces all own sessions with one new persistent
session and preserves unrelated users. Memory and encrypted PostgreSQL tests cover
the same lifecycle. Production raw HTTP validation/cookies/Origin/CSRF and the
remaining user/account/provider/organization routes stay open.

Native trusted-origin and mutation/login CSRF policies now pass 2,822 comparisons
against the actual Better Auth matcher and configured HTTP handlers. Wildcard and
custom schemes, relative redirects, encoded traversal/separators, Cookie header
presence, null origins, Referer fallback and Fetch Metadata are covered. These
policies still require wiring into the production raw HTTP adapter.

Nine native raw session/JWT HTTP endpoints now integrate signed-cookie sessions,
Origin/callback guards, credentialed CORS and public JWKS/token issuance. The
get-session response emits a verifiable JWT header. A 161-comparison fixture checks
actual HTTP bodies, errors, persisted state and JWT claims. Live memory and
restricted-role encrypted PostgreSQL exercise HTTP/MCP startup, discovery/calls,
authentication, CORS/CSRF and shutdown; secure-cookie extraction/clearing is tested.
Raw auth endpoints remain outside the registered MCP contract surface. Credential
HTTP, providers/orgs, custom callbacks and the generated Better Auth preset are open.

Fresh packaged install `pUGSo8` validates the nine raw session/JWT routes from
bundled sources, including live HTTP/MCP and secure-cookie tests. Portable
services/workers, CLI/native SDKs, listener startup configurations and all five
managed callbacks pass. Checkout CLI restored; credential HTTP additions and the
remaining Better Auth preset are tracked separately.

Thirteen raw native HTTP endpoints now include credential signup/signin and
password verification/rotation. The credential HTTP fixture passes 122 actual
body/storage/cookie/callback comparisons; 63 parser/schema/guard/method cases match
full errors. Transient replacement cookies inherit the browser flag, raw additional
profile values preserve presence, private input fields stay ignored, and forwarded
IP/agent metadata matches. Live memory/restricted-role PostgreSQL exercise these
credential operations with MCP startup and shutdown. The 161 session/JWT HTTP
comparisons remain green. Production rate limiting now matches 153 actual HTTP
responses, including rolling idle windows, fallback/IPv6 buckets, normalized paths,
special endpoint rules, retry headers and pre-parser/CSRF ordering. Bounded
process-global memory indexes preserve insertion eviction; live TCP concurrency
admits exactly three of 32 requests and shuts down cleanly. User/account/provider/
organization routes, real TypeScript ORM conformance, callbacks and CLI generation
remain open.

Sixteen raw native auth routes now include profile updates and linked-account
listing/unlinking. Fifty actual HTTP response/cookie/storage comparisons pass,
including opaque profile fields, private token/hash filtering, transient cookies,
ECMAScript scope trimming and scoped errors. Atomic concurrency preserves one
remaining account, and revoked sessions cannot update profiles. Live memory and
restricted-role PostgreSQL verify profile HTTP operations beside MCP. Remaining
user/account/provider/org routes, real ORM conformance and CLI preset stay open.

Twenty raw native auth routes include the four configured-disabled reset-email,
verification-email, change-email and delete-user endpoints. Their 128 actual HTTP
response/cookie/storage comparisons preserve schema/Origin/session ordering and
empty deletion 404 bodies. Live memory/restricted-role PostgreSQL validate these
responses beside MCP. Token-based reset/verification, remaining user/account/
provider/org operations, real ORM and CLI preset remain open.

Fresh archive `ac1c0T` passes the twenty-route auth checkpoint at `54901dd98`: relocated
auth HTTP/MCP tests, portable modules/workers, both CLI/native SDK languages,
listener startup configurations and all five managed callbacks. Checkout CLI
restored. Subsequent token-reset work is newer than this snapshot.

Twenty-one native raw auth routes include single-use token password reset. Its
44 actual HTTP/storage comparisons cover body/query token precedence, validation,
UTF-16 password limits, duplicate selection/invalidation, expiry, missing users,
missing credential creation and session retention. Atomic consumption allows one
concurrent winner and hashing runs outside tenant locks. Live memory and
restricted-role PostgreSQL exercise reset and subsequent verification via HTTP
beside MCP. Email verification/callback routes, remaining user/account/provider/
org operations, real ORM conformance and CLI preset remain open.

Twenty-three raw native auth routes include GET reset-token and disabled deletion
callbacks. Fifty-two actual HTTP/storage comparisons cover trusted destinations,
raw encoded token segments, combined query errors, redirect query/content type,
configured authority despite forged Host headers and expired-verification cleanup.
GET redirects retain valid tokens for single-use POST reset. Live memory and
restricted-role PostgreSQL follow that lifecycle over HTTP beside MCP. Email
verification, remaining user/account/provider/org operations, real ORM conformance
and CLI preset remain open.

Configured HS256 email-token verification passes 96 actual JOSE/HTTP comparisons,
including signature/algorithm/critical headers, malformed claims, optional numeric
claim ordering, expiry boundaries, padding/noncanonical bits, BOMs, duplicate keys
and numeric overflow. Raw JSON number handling is local to token verification.
Email-verification HTTP/user/session/redirect integration remains open; native
raw route count stays twenty-three.

Twenty-four native raw auth routes now include signed email verification. Its 52
actual HTTP response/storage/cookie/header comparisons cover idempotent normal
verification, confirmation/verified/legacy email-change tokens, authoritative
session checks, session reuse/creation and transient cookies. Error callbacks
append existing query text; success callbacks preserve relative locations, Latin-1
byte strings and HTTP whitespace normalization. Malformed claims retain empty 500
responses. Live memory and restricted-role PostgreSQL verify HTTP mutations,
forgery rejection, issued cookies and subsequent session/JWT access beside MCP.
Configured email delivery/change requests remain disabled. Provider/org/account
operations, real ORM conformance, custom callbacks and CLI preset stay open.

Reset credential selection/update also requires accountId=userId, preserving
mismatched legacy rows and creating a proper credential when none matches. The
44-case actual HTTP fixture preserves hash equality groups to detect accidental
updates to legacy rows; memory and restricted PostgreSQL exercise both paths.

The native server-only `CredentialService::set_password` helper matches 36 actual
configured Better Auth API/storage comparisons. It creates only strictly scoped
credentials, fills empty/null/missing passwords and rejects already-set passwords;
authoritative session refresh/expiry and password validation retain actual order.
Memory and restricted PostgreSQL exercise concurrent initial assignment, legacy
row preservation and empty credentials. A deterministic memory-store race revokes
the session after derivation and proves no password is written. No public route is
added for this server-only API; the native raw HTTP count remains twenty-four.

Fresh archive `oLl8Px` at `953969c17` passes relocated auth tests (13 passed,
one explicit-PostgreSQL test ignored), live auth HTTP/MCP, all generated listener
startup configurations and five managed callbacks. Portable services/workers and
both SDK generators pass; checkout CLI restored. This snapshot includes 24 raw
auth routes and the server-only password helper, before organization additions.

Twenty-nine native raw auth routes include organization creation, slug checks,
listing, full organization graphs and active selection. All 134 actual configured
HTTP/storage/cookie comparisons pass. Creation atomically adds the owner, default
team and keyed team membership; it updates active organization/team unless told
to preserve the current selection. Metadata remains serialized in storage/list
responses and parsed in creation responses, matching the configured adapter.
Selection/full reads enforce membership and persist rejection-driven clearing
without clearing the active team. Password/session HTTP regression remains green.
Live memory and restricted PostgreSQL verify organization HTTP, session/JWT
organization claims, cookies and MCP discovery/invocation/shutdown. Organization
updates/deletion, remaining member/team/invitation/dynamic-role APIs, provider
flows, real ORM conformance, custom callbacks and CLI preset remain open.

Thirty-three native raw auth routes include metadata-only organization reads,
updates, deletion and permission checks. All 212 actual HTTP/storage/cookie
comparisons pass. Dynamic stored grants merge into each configured static role
and remain scoped to one organization; composite roles cannot pool permissions
across separate roles to satisfy a single request. Platform-only default roles
retain mutation denials. Schema/alias ordering, malformed permission JSON/schema
(including numeric overflow), field whitelists and endpoint-specific error bodies
and statuses match actual handlers. Deletion removes organization/member/invite
records and clears only the actor's matching active organization; configured
team/role rows and other sessions' selections remain, matching the actual adapter.
Memory/restricted PostgreSQL cover granted mutations, atomic concurrent deletion
and HTTP storage persistence beside JWT/MCP startup/lifecycle. Remaining member,
team, invitation, dynamic-role management, provider, real ORM, callback and CLI
preset gates remain open.

Thirty-eight native raw auth routes include active member/role reads, role updates,
member removal and organization departure. All 146 actual HTTP/storage/cookie
comparisons pass, including creator bypass boundaries, dynamic grants, duplicate
roles, Unicode trimming, error ordering and email-versus-ID user projections.
Departure/removal delete team memberships and release stored counters atomically;
only the actor's matching active organization clears, retaining active team.
Memory and restricted PostgreSQL exercise competing owners leaving, ensuring one
owner survives, and live HTTP member operations alongside JWT/MCP lifecycle.
Member listing, server-only addition, team/invitation/dynamic-role management,
provider flows, real ORM conformance, callbacks and CLI preset remain open.

The trusted server-only `OrganizationService::add_member` helper matches 40 actual
API/storage outcomes. Typed inputs preserve role arrays by joining them verbatim;
the explicit organization requires no session, while an optional session provides
active-scope fallback and refresh/expiry effects. User/team/organization checks,
duplicate ordering, the configured 100-member limit, keyed team membership reuse,
undercount reconciliation (including its timestamp) and retained overcounts match
the configured adapter. Memory/restricted PostgreSQL verify concurrent duplicate
and last-slot admission; live HTTP rejects public add-member access. Raw auth
route count remains 38. Member listing and the remaining team/invitation/dynamic
role, provider, real ORM, callback, JWT-helper and CLI preset gates remain open.

Thirty-nine native raw auth routes include scoped member listing. All 340 actual
HTTP/storage/cookie comparisons pass across schema ordering, active/ID/slug scope,
filtered totals, public-user projection, missing-user failures, stable date and
Unicode sorting, all filter operators and ECMAScript Number/slice pagination.
Unicode sorting uses ICU's en-US collation, matching the fixture's Node locale;
real ORM collation and query behavior remain a separate open conformance gate.
Memory/restricted PostgreSQL cover filtering, Unicode pages and orphan failures;
live HTTP listing runs beside session/JWT and MCP lifecycle. Team/invitation/
dynamic-role management, provider flows, remaining server JWT helpers, real ORM,
callbacks and generated preset remain open.

Forty-three native raw auth routes include organization/user team lists, team
member reads and active-team selection. All 318 actual HTTP/storage/cookie
comparisons pass, including self scope across organizations, permission-gated
other-user reads, malformed grants, distinct organization/team membership guards,
internal counter/key projections, transient cookies and selection timestamps.
Selection preserves the active organization; clearing an existing selection emits
a cookie once, while successful selection emits one even when unchanged. Memory
and restricted PostgreSQL verify team/session persistence and live HTTP alongside
JWT/MCP lifecycle. Team mutations, invitations, dynamic-role management, provider
flows, remaining JWT helpers, real ORM/callback and generated preset gates remain
open.

Forty-six native raw auth routes include team creation, name updates and removal.
All 110 actual HTTP/storage/cookie comparisons pass, including mutation-specific
authorization and unauthenticated error ordering, schema/field stripping, stored
grant failures, active-team protection and last-team safeguards. Creation stores
zero members without enrolling the actor; updates retain organization/counters.
Removal atomically deletes memberships and removes the team's comma-separated
references from the first 100 pending invitations that have not expired. Expired/
accepted invitations and other sessions' active-team selections remain unchanged.
Memory/restricted PostgreSQL verify concurrent removals preserve the last team,
invitation cleanup and live HTTP mutations beside JWT/MCP lifecycle. Adding and
removing team members, invitations, dynamic roles, providers, server JWT helpers,
real ORM/callback and generated preset gates remain open.

Forty-eight native raw auth routes include team-member addition/removal. All 164
actual HTTP/storage/cookie comparisons pass, covering scoped member permissions,
required/coerced user IDs (including ECMAScript number formatting and failed
object coercion), keyed duplicate reuse, legacy duplicate removal, counter
reconciliation/release and retained active-team references. Organization addition
shares the same membership helper; its 40 actual API/storage comparisons remain
green. Memory/restricted PostgreSQL verify concurrent additions reuse one row and
seat, concurrent removal has one winner and sessions retain active-team values.
Live HTTP runs the new routes beside JWT/MCP lifecycle. Invitation/dynamic-role
management, providers, server JWT helpers, real ORM/callback, raw JSON numeric
edge cases and generated preset gates remain open.

Raw auth JSON/metadata now passes 114 actual HTTP/storage/cookie comparisons,
including numeric overflow, IEEE double rounding/formatting, index/insertion key
ordering, literal serde marker keys, reserved-key failures after persistence and
update-only ISO date revival. All 233 framework and 101 blueprint workspace tests
pass with unified JSON features; explicit PostgreSQL auth passes. Shared generic
deserializer marker keys, unpaired surrogates/deep nesting and real ORM metadata
behavior remain separate gates. Fresh archive `5DF6N8` validates the preceding
48-route snapshot's generated service/worker HTTP/MCP startup matrix, both SDK
generators and five managed callbacks; checkout CLI rebuilt afterward.

Fifty-one raw routes include invitation lookup, scoped organization lists and
received-user lists. All 258 actual HTTP/storage/cookie comparisons pass, with
verified-email ordering under configured database-generated IDs, exact expiry
boundaries, joined inviter projection, cross-organization visibility and the
adapter's 100-row limit before user-list status filtering. HTTP rejects explicit
email queries; the trusted native helper accepts an email without a session.
Native memory/restricted PostgreSQL and live HTTP/JWT/MCP cover invitation reads.
Invitation creation/acceptance/rejection/cancellation, dynamic roles, providers,
JWT helpers, real ORM/callback, shared parser edges and preset gates remain open.

Fifty-three raw routes include rejection and cancellation. All 318 actual HTTP/
storage/cookie comparisons pass: rejection requires a pending invitation and
verified recipient but accepts expired invitations; scoped invitation:cancel
can replace any invitation status and repeated cancellation succeeds. Neither
operation changes membership or active selections. Malformed stored grants,
missing organization/member ordering and unsupported methods match. Memory/
restricted PostgreSQL verify repeated/concurrent cancellation and retained
membership; live HTTP executes both operations beside JWT/MCP. Creation and
acceptance, dynamic roles and remaining full-preset gates stay open.

Fifty-four raw routes include invitation creation/resend. All 196 actual HTTP/
storage/cookie comparisons pass across schema/error ordering, scoped grants,
verbatim roles and dynamic role existence, creator-role restrictions, duplicate
members/invitations, team validation and pending-invitation limits. Resend updates
expiry while retaining stored role/teams and runs before team/limit checks. The
adapter's 100-row limit precedes expiry filtering. Memory/restricted PostgreSQL
verify concurrent last-slot admission and retained team/member state; live HTTP
executes creation/resend beside JWT/MCP. Acceptance and remaining dynamic-role/
provider/helper/ORM/callback/preset gates stay open. Fresh archive `v8nGhW` at
`51973fa19` validates the preceding 53-route snapshot's portable services/workers,
HTTP/MCP startup matrix, both SDK generators and five signed callbacks. Checkout
CLI is restored from the subsequent creation sources.

Fifty-five raw routes include invitation acceptance. All 186 actual HTTP/storage/
cookie comparisons pass across recipient/verification/error ordering, exact
expiry, 100-member admission, duplicate organization memberships, verbatim roles,
team/key/counter enrollment, active scope and transient cookies. Single-team
acceptance selects that team and emits cookies; absent/multiple teams preserve
the existing active team and emit none. Memory/restricted PostgreSQL validate
competing acceptance and last-slot admission, plus rollback of invitation/team/
member/session state after missing later teams. Live HTTP executes no-team and
single-team acceptance beside JWT/MCP. This completes configured invitation
lifecycle; dynamic roles, providers, JWT helpers, real ORM/callback, shared parser
edges and generated full-preset gates remain open.

Fifty-seven raw routes include dynamic role listing/lookup. All 272 actual HTTP/
storage/cookie comparisons pass: explicit ac:read grants, raw membership, nullish
scope fallback, exact case-sensitive name/ID selection, union validation and
first-100 listing. Direct lookup can read later roles; malformed later JSON
fails lookup without breaking capped lists, and literal serde keys are retained.
Permission strings stay unchanged in storage while responses contain parsed JSON.
Memory/restricted PostgreSQL and live HTTP/JWT/MCP validate both routes. Dynamic
role mutations and remaining provider/helper/ORM/callback/full-preset gates stay
open.

Fifty-eight raw routes include dynamic role creation. All 212 actual HTTP/storage/
cookie comparisons pass, including nullish scope, lowercase names without
trimming, the configured predefined-name guard, permission/resource ordering,
duplicate names beyond 100 rows and ignored additional fields. Public grants
permit only the configured platform resource, and each requested action requires
an actor grant. Missing actions retain order and duplicates in missingPermissions.
Memory/restricted PostgreSQL verify atomic competing duplicate creation and stored
permission JSON; live HTTP runs creation beside JWT/MCP. Role update/deletion and
remaining provider/helper/ORM/callback/full-preset gates stay open.

Sixty raw routes include all five configured dynamic-role operations. Update and
deletion pass 332 and 324 actual HTTP/storage/cookie comparisons. Updates enforce
per-action grants before renaming, reject same-name renames, update duplicate
matches and retain member role strings. Stored updatedAt advances while the
response retains the prior projection. Deletion checks the first 100 matching
members after substring filtering, then exact trimmed roles, and removes matching
stored rows. Name-based predefined protection differs from ID selection. Memory/
restricted PostgreSQL verify atomic deletion, blocked assigned roles and duplicate
updates/deletes; live HTTP runs both routes beside JWT/MCP. Providers, server JWT
helpers, real ORM/callback, shared parser and full-preset generation remain open.

Fresh archive `bnXBOp` at `10349eea0` passes portable Better Auth tests and live
HTTP/JWT/MCP, the complete generated HTTP/MCP listener/startup matrix, portable
services/all four worker backends, CLI/native SDK targets and all five capability
callbacks. This archive includes sixty raw auth routes and the two completed
server-only helpers, including the configured invitation/dynamic-role lifecycle.
Remaining provider/helper/ORM/callback/shared-parser/full-preset gates stay open.

The configured server-only verifyJWT helper now supports persisted Ed25519 keys
with EdDSA/Ed25519 token aliases and issuer overrides. All 159 actual API and 144
key-store comparisons pass: optional claims, numeric/expiry boundaries, audience/
subject truthiness, critical headers, compact encoding, JWK metadata/import rules,
retired keys and first-100 key selection. Invalid tokens/storage/key metadata
return null without minting keys or reading private material. Memory/restricted
PostgreSQL verify restart/retirement and corrupt-private-key independence; live
HTTP confirms signing/verifying helper paths remain absent beside JWT/MCP. Three
of four configured server helpers now have native implementations. Signing,
additional-algorithm provisioning/interop, providers, real ORM/callback, shared
parser and full-preset generation remain open.

All four configured server-only helper entrypoints now have native implementations.
The configured EdDSA signJWT profile passes 129 actual API and 134 key-state
comparisons, with independently verified signatures. Cover schema/default-duration
ordering, key persistence after claim failure, exact claim setters, issuer/audience
precedence, fractional/negative/overflow durations and numeric key rotation. Memory/
restricted PostgreSQL exercise signing and verification beside key lifecycle; live
HTTP/JWT/MCP retain absent public helper paths. Additional signing-algorithm/key
provisioning, typed date/function options, custom callbacks and real ORM/storage
interop remain open; unsupported fresh key profiles fail explicitly. Sixty raw
HTTP routes remain implemented, with provider/OpenAPI/full-preset work outstanding.

Main through `b5852a5a1` is merged. Its dependency vulnerability audit now covers
Cargo's crates.io releases through the shared OSV/RustSec cache and reporting path.
Legacy/v3/v4 locks, local-package exclusions, malformed/future locks, managed-source
exclusions and per-ecosystem missing-lock detection pass; Git/private-registry
sources explicitly report incomplete coverage without exposing source URLs. All
1,016 CLI tests pass, including 15 dependency-audit tests. Commit-based Git advisory
queries remain unsupported.

JWT key profiles now include Ed25519 alias, ES256/P-256 and ES384/P-384. Native
PKCS8 persistence retains encrypted private material while public JWKS exposes
only coordinates. Actual TypeScript comparisons pass 151 signing calls/165 key
states and 213 verification calls/182 key-store operations. Memory/restricted
PostgreSQL validate concurrent provisioning, restored signing/session reuse,
rotation and retired public-key verification; all 17 native tests and live HTTP/
JWT/MCP pass. Strict all-feature clippy, rebuilt CLI and 709 embedded sources pass.
RSA/ES512, typed options, callbacks, real ORM and full preset remain open.

ES512/P-521 now joins the persistent signing/verification profiles. Actual
comparisons pass 159 signing API calls/176 key states and 240 verification calls/
201 key-store operations, including independent JOSE signature verification. Memory
and restricted PostgreSQL cover atomic provisioning, restart, session signing,
rotation and retired-key verification. Strict default/all-feature clippy, rebuilt
CLI and 709 embedded sources pass. RSA and broader option/provider/ORM/callback/
full-preset gates remain open.

RSA RS256/384/512 and PSS PS256/384/512 now support encrypted PKCS8 persistence,
restored signing/session reuse and public verification. Actual comparisons pass
189 signing calls/216 key states and 393 verification calls/309 key-store
operations, with independent JOSE verification and invalid hash/algorithm/PSS
salt rejection. Memory/restricted PostgreSQL validate concurrency, restart,
rotation and retired keys. Prime generation uses the blocking executor; default
session issuance prefers EdDSA before another live profile. Final keyring tests,
strict all-feature clippy, rebuilt CLI and 711 embedded sources pass; live HTTP/
JWT/MCP and 27 prior keyring comparisons pass. RSA provisioning supports 2048–4096
bits; larger keys and typed options/custom adapters/provider/ORM/full-preset gates
remain open.

Fresh archive `n6okrp` at `74213970a` passes 18 portable auth tests (one explicit
PostgreSQL test ignored), including live HTTP/JWT/MCP and all asymmetric signing
profiles. The complete generated HTTP/MCP startup/listener matrix, portable
services/all four worker backends, CLI/native SDK targets and all five signed
capability callbacks pass. This snapshot includes sixty raw auth routes, four
configured server helper entrypoints and Ed25519/ECDSA/RSA/PSS provisioning.
Larger RSA/typed options, Google OAuth, ORM/callback/shared-parser and full-preset
gates remain open.

Database OAuth state primitives now pass 66 actual state/cookie and 66 storage
comparisons, using real Better Auth endpoint context, Better Call and the memory
adapter. Native random state/PKCE, signed cookies, expiry/cleanup, malformed and
legacy data, link coercion and atomic consumption pass memory/restricted
PostgreSQL checks. Strict all-feature clippy, rebuilt CLI and 715 embedded sources
pass. Google HTTP authorization/callback and token/account lifecycle remain open;
this foundation adds no raw routes.

Configured Google authorization URL construction now passes 37 actual provider
comparisons: S256 PKCE, scopes, callback URI, login hints, reserved parameters,
overwrite behavior, Unicode/query encoding and JavaScript numeric-key ordering.
Strict all-feature clippy, rebuilt CLI and 717 embedded sources pass; CI runs the
fixture. HTTP social-login/ID-token user-account-session integration, callback
code exchange and token lifecycle remain open. No raw route is added yet.

Google ID-token verification primitives now pass 147 comparisons against the
actual provider, including RS256 signature/JWK import behavior, issuer/audience,
nonce, numeric claim boundaries and duplicate key selection. The verifier takes
trusted supplied certificate keys; HTTP certificate fetching and persistent
user/account/session integration remain open. Strict all-feature/all-target
clippy, rebuilt CLI and all 719 embedded Rust sources pass; CI runs the fixture.

Configured `/sign-in/social` now supports Google authorization URL/state issuance
and direct verified ID-token sign-in with persistent user/account/session
resolution. All 165 actual HTTP/storage comparisons pass, including nested body
validation, missing provider/claims, forged/expired tokens, duplicate-key order,
profile preservation, token replacement and safe verified-email linking. Google
certificate transport has fixed trusted endpoints, no redirects, timeouts and a
one-MiB response bound. The centralized direct sign-in verifier imports all
selected keys and uses the first, matching the configured TypeScript path.
Concurrent signup, orphan rejection, encrypted tokens and tenant isolation pass
memory/restricted PostgreSQL checks. Authorization-code callbacks and the
remaining account/token HTTP lifecycle remain open.

Google authorization-code callbacks now pass 226 actual HTTP/storage/wire
comparisons, including S256 verifier binding, trusted fixed token endpoints,
refused redirects, one-time state/cookie correlation, exact ten-minute expiry,
provider errors, POST normalization, success/new-user/error redirects and stored
access/refresh/ID tokens. Token-response normalization covers numeric coercion,
invalid dates and scope forms. Explicit callback linking preserves account
ownership, verified-email policy and merged scopes; malformed provider tokens
match the upstream empty-500/cookie-header boundary. Native memory tests cover
identity races, link conflict rejection and token/scope preservation. The same
checks pass encrypted restricted PostgreSQL with tenant isolation. Strict default and
all-feature/all-target clippy, all 21 native auth tests, rebuilt CLI and 725
embedded sources pass. The `/link-social` initiation route, access/refresh-token
and account-info routes, OpenAPI/error/update-session routes, full ORM/application
hooks and CLI preset remain open. The implemented raw-route count is 62 of 70.

A fresh relocated archive installation (`VwxVQI`, snapshot `f1a966960`) passes
all portable services and four worker backends, CLI/native SDK generation and
all 21 portable auth tests. The complete generated HTTP/MCP startup matrix
passes shared/separate listeners, enabled/disabled/custom endpoints, explicit
and ephemeral ports, TypeScript +2000 fallback, atomic startup rejection and
graceful shutdown. MCP initialization/discovery/health invocation/Origin
rejection and all five actual-TypeScript-signed capability callbacks pass. This
snapshot contains 62 raw auth routes and Google code callbacks; later linking
initiation work is validated separately. The checkout CLI was restored after
the archive install.

Google `/link-social` now supports direct verified ID-token linking and browser
authorization with server-owned user/email state. Existing own accounts update
tokens; new links enforce verified provider email and matching local email;
another owner's provider identity returns a conflict. The combined social/link
HTTP fixture passes 289 actual response/storage comparisons, including nested
schemas and ignored direct-link fields. The browser callback fixture passes 236
actual response/storage/wire comparisons through the real linking entrypoint.
Direct-link ownership/policy/token checks pass memory and encrypted restricted
PostgreSQL. Configured `/update-session` matches the TypeScript plugin field
policy: protected organization/team fields cannot be set, and there are no
configured editable session fields. The implemented route count is 64/70.
Access-token refresh, account-info, OpenAPI/error routes, full ORM/application
hooks and CLI preset remain open.

Google `/get-access-token`, `/refresh-token` and `/account-info` now pass 157
actual response/storage/wire comparisons. Account selection uses local owned
account IDs; automatic refresh observes the five-second boundary and preserves
stored scopes and omitted provider fields. Live provider tests verify token
rotation and reject an account unlinked during an in-flight refresh. All 22
native tests, encrypted restricted PostgreSQL, strict all-feature clippy, the
161 HTTP and 289 social regressions, rebuilt CLI and 728 packaged assets pass.
The implemented raw-route count is 67/70. Error/OpenAPI/reference routes,
full ORM/application hooks and the native CLI auth preset remain open.

Configured `/error`, `/open-api/generate-schema` and disabled `/reference` now
complete all 70 raw routes. The default error HTML is adapted under the upstream
MIT license and matches full rendered responses, URLSearchParams first-value
selection, preserved entities and production redirects. The OpenAPI response
uses the fixed preset's declarative contract with the trusted server URL; it is
compared in full with the actual configured TypeScript generator. Custom plugin
schemas/options are outside this fixed preset. All 27 development and 27
production public-response comparisons, 22 native tests and strict all-feature
clippy pass. Full ORM/application hooks, application surfacing/compliance,
retention and native CLI preset integration remain open.

Fresh archive installation `IqcM1T` (snapshot `4f0540ba6`, all 70 raw routes)
passes 22 portable auth tests, all portable services/four worker backends and
CLI/native SDK generation. The complete generated HTTP/MCP startup matrix,
initialization/discovery/health invocation/Origin rejection, shared/separate
listeners, enabled/disabled/custom endpoints, explicit/ephemeral ports,
TypeScript +2000 fallback, atomic startup rejection and graceful shutdown pass.
All five actual-TypeScript-signed managed capability callbacks pass. The
checkout CLI was restored after the archive installation. Later application
surfacing changes are validated separately from this snapshot.

Native application `SurfacingService` now matches 37 actual TypeScript result
comparisons. It selects the newest unexpired organization-bearing session,
retains exact member role strings, and returns stored permission strings
without parsing, splitting or unioning them. Expiry boundaries, duplicate
ordering, other users/organizations and missing/falsy scopes are covered. The
shared memory/restricted PostgreSQL check passes with ciphertext inspection
and tenant isolation; strict clippy and 734 packaged assets pass. HTTP/MCP
application-route wiring, compliance/retention and CLI preset remain open.

Real MikroORM 7.2.3 / PostgreSQL conformance exposed a TypeScript adapter
integration defect: compliant password/private-JWK objects reached Better Auth
without explicit plaintext access, causing sign-in and session-JWT 500s. The
configured adapter now reveals classified results only inside Better Auth's
trusted request context, including transaction adapters, preserving audit reads
and leaving ORM entity fields wrapped. All 24 real-ORM checks pass: encrypted
columns, audited access, missing/wrong tenant rejection, rollback, restart and
native consumption of actual ORM passwords/sessions/accounts with JWT checks.
The adapter utility passes a strict isolated type check against the installed
adapter signature. Existing 161 HTTP, 157 token, 27 public and 37 surfacing
comparisons pass. CI now installs the pinned real ORM/adapter and supplies shared
fixture environment settings. This is initial real-ORM conformance; complete
storage/hook coverage, compliance/retention and CLI preset remain open.

Native Better Auth compliance now preserves classified export fields as opaque
objects with explicit audited plaintext access. Fourteen comparisons against
the real encrypted ORM cover Account/Jwks selection, other subjects, erasure,
remaining user/session records and partial per-entity read failures. Native
memory/restricted PostgreSQL also verify atomic rollback on write failure,
report labels, tenant isolation and ciphertext protection. Erasure rechecks
ownership before deleting a previously selected account.

Seven-calendar-year Account retention passes 41 actual TypeScript comparisons:
UTC, Los Angeles, Lord Howe and Apia cutoff behavior, leap days, repeated hours,
half-hour gaps, skipped days, exact age boundaries, dry runs, entity filters,
batches, repeat runs, preserved metadata and persisted anonymization timestamps.
The implementation validates timezone round-trips and selects the earliest
valid UTC instant. All 26 native auth tests, restricted encrypted PostgreSQL,
strict all-feature clippy, rebuilt CLI and 739 embedded assets pass. Application
HTTP/MCP wiring, custom callback helpers, full ORM/hook coverage and CLI preset
integration remain open.

The optional `application-http` feature now combines the five configured
application contracts with the raw `/api/auth` namespace. Discovery is public;
role/permission surfacing verifies request HMAC; compliance export/erasure
verifies JWKS JWTs and resolves the `system` role from current trusted storage.
Role claims in tokens do not bypass revocation. All five actual TS controller
contracts and nine response schemas match, including string IDs and opaque
permission strings. A live TCP test verifies HTTP and MCP initialization,
discovery, signed tools, missing/invalid credentials, role revocation, classified
export protection, erasure/replay, raw-route separation and graceful shutdown.
All 27 native tests, strict all-feature clippy and 742 packaged assets pass.
The native CLI preset, custom callback helpers and remaining full ORM/hooks
coverage are still open; this does not declare full parity.

Native CLI generation now accepts `iam-better-auth` in Rust/mixed applications
and module initialization. It retains the default service name `iam`, rejects
conflicting IAM presets before writing files, embeds the encrypted auth store,
and emits operator configuration, migrations, application contracts and health.
The relocated generated fixture passes sync/reinitialization preservation,
TypeScript/Rust SDK compilation, real restricted PostgreSQL startup, tenant
session isolation, independent JWT verification, HMAC MCP tools, shared/separate
and disabled MCP listeners, restart persistence, graceful shutdown, rename,
contract/SDK refresh and deletion. All 1,016 CLI tests pass. E2E CI includes the
fixture and archive installation includes the new preset. Custom callback
helpers, additional docs sources and complete ORM/hooks remain open.

Fresh archive installation `XXfFfM` (snapshot `f288c81c4`) passes with the
new native Better Auth preset: portable service and SDK compilation, all 27
packaged auth tests, every other portable service/four worker backends,
CLI/native SDK generation and the complete generated HTTP/MCP startup matrix.
All five actual-TypeScript-signed managed capability callbacks pass. The checkout
CLI was restored after archive installation. Callback-helper changes after this
snapshot have separate validation.

Both configured internal popup helpers are native. `/api/auth/test-callback`
validates trusted destinations, exchanges through the existing auth handlers,
clears only the configured session cookie and replaces the destination fragment
with JWT/session/email values. `/api/auth/oauth-redirect` emits matching
script-safe OAuth/SSO HTML, including duplicate-query coercion. Neither becomes
an MCP tool. Ninety-two actual Express 5.2.1 / TypeScript comparisons cover HTTP
and HTTPS, invalid origins/userinfo/schemes, anonymous/signed/forged/expired
cookies, corrupted signing keys, response media/Accept negotiation, HEAD, cookie
headers and HTML escaping. Independently generated JWTs verify against their
own public keys and match claims; signatures are not compared byte-for-byte.

The relocated generated service also passes these helpers against a fresh
PostgreSQL container, plus shared/separate/disabled HTTP/MCP listeners, tenant
isolation, SDKs, restart, graceful shutdown and rename/delete. All 27 native
tests, strict default/HTTP/all-feature clippy, 40 original callback checks,
161 raw HTTP regressions, five application contracts/nine response schemas and
749 packaged sources pass. `application-http` no longer enables unused SQL or
field-encryption features unless PostgreSQL is selected. Additional docs
sources, nondefault base paths, complete ORM/hooks/client integration and
broader custom options remain open.

### Additional Scalar documentation sources

`RouterConfig.docs_sources` accepts typed URL or inline JSON sources with optional
slug/default selection. Scalar receives the generated application API first,
then the additional documents. BetterAuth applications now include the complete
raw auth schema without adding its endpoints to OpenAPI application contracts,
SDK operations or MCP tools. The TS preset preserves the schema's existing
server URL and relative paths, fixing its duplicated `/api/auth` request paths.
Documentation titles and both Scalar/Swagger inline configuration are escaped.

Twelve comparisons execute the actual pinned Scalar 0.10.25 renderer, the actual
TS registration factory and Better Auth schema generator, then evaluate served
native HTML in a JS VM. They cover remote/inline/string documents, multiple
sources, slugs/defaults and script terminator/Unicode escaping. Live native TCP
and generated fresh PostgreSQL services verify both docs sources and unchanged
MCP discovery. All 27 auth tests, 38 framework tests, strict all-feature/all-target
clippy for both crates, generated HTTP/MCP/SDK/restart/rename/delete checks and
751 packaged sources pass. Nondefault base paths, complete ORM/hooks/client
integration and broader custom options remain open.

### Configurable non-root authentication namespaces

`AuthHttp::with_base_path` and generated `BETTER_AUTH_BASE_PATH` configure a fixed
non-root namespace; a path already present in `BETTER_AUTH_URL` takes precedence.
Raw routes, both popup helpers, Google/reset callbacks, docs and the generated
default JWKS URL use that resolution. JWT issuer/audience use the canonical
origin, matching Better Auth's normalized options. Unsafe namespaces and MCP
collisions fail before service startup. The actual TS server helpers now use
resolved auth URLs, and its configured Google callback no longer concatenates
an ignored base path or omits a slash for relative base-path settings.

All 343 actual Express/TS/schema/provider comparisons pass across seven
HTTP/HTTPS/default/custom/trailing-slash/URL-override configurations. The full
application HTTP/MCP authentication/compliance test runs under three namespaces.
A fresh generated PostgreSQL service passes custom-path separate MCP, URL-path
override on persisted-session restart, disabled MCP, default self-JWKS without
an override, SDKs and rename/delete. All 28 native tests, 1,016 CLI tests, strict
clippy, 161 raw HTTP regressions, 40 existing callbacks, 12 Scalar comparisons
and 751 packaged sources pass. Root/dynamic/encoded namespaces, full ORM/hooks/
client integration and broader custom options remain open.

### Root-mounted authentication and reserved namespaces

`BETTER_AUTH_BASE_PATH=/` (or empty) mounts raw authentication at root while
keeping application contracts, docs, SDKs and MCP reachable. The framework's
`merge_internal` API reserves declared fixed namespaces; collision validation
and parent nesting apply to each reservation. The actual TS preset now limits
its root catch-all to known auth namespaces, avoiding interception of application
routes. Root reset errors no longer introduce an extra slash.

All 450 actual Express/TS comparisons pass across nine configurations, including
630 raw-route dispatch probes and root signed sessions/Google/reset/popup flows.
The full native application HTTP/MCP test runs under five configurations. A fresh
generated PostgreSQL service passes root HTTP/MCP and default self-JWKS without
an override, as well as custom/separate and URL-override/disabled listeners,
persisted sessions, SDKs and rename/delete. All 28 native/40 framework tests,
strict all-feature/all-target clippy for both crates and 751 packaged sources
pass. Dynamic/encoded namespaces and broader ORM/hooks/client/options work
remain separate gates.

### Archive-installed root/docs snapshot

Fresh Cargo archive installation `EklWnG` validates committed snapshot `f20252656`.
All portable services and four worker backends compile, both SDK targets pass,
and all 28 packaged auth tests pass. The complete generated HTTP/MCP matrix
covers enabled/disabled/custom endpoints, shared/separate listeners,
explicit/ephemeral ports, TypeScript's +2000 fallback, atomic startup rejection
and graceful shutdown. All five actual-TypeScript-signed native capability
callbacks pass forgery/staleness/replay/redelivery checks. The checkout CLI was
rebuilt after the archive run. This confirms source portability and service
startup; broader parity and published registry installation remain open.

### Actual shared and generated frontend client interoperability

The shared `clientIamBetterAuthSdkClient` and generated `iamSdkClient` factories
now accept `authBasePath` (default `/api/auth`, `/` for root); a path already in
`host` takes precedence. Both initial templates and subsequent AST injection
carry the option. Eight live scenarios execute both actual TS factories and
Better Auth 1.7.6 over native PostgreSQL HTTP: default/custom/root/URL-override
namespaces, 80 requests total. Signup additional fields, sessions, profile
updates, independently verified JWT/JWKS, signout and signin pass. The fixture
owns Node cookie persistence and public-origin forwarding; universal SDK core
construction remains a separate boundary and does not establish unified mixed
SDK parity.

The final run also exercises automatic pinned-client dependency setup, actual
CLI TypeScript client generation, restricted PostgreSQL, shared/separate/disabled
MCP, self-JWKS, SDKs, persistence, shutdown and rename/delete. All 1,016 CLI tests
and 751 packaged source checks pass. E2E CI installs the pinned client and reacts
to changes in both the auth module and frontend factory. Broader custom client
plugins/hooks, unified mixed SDK behavior and indexed storage remain open.


### BullMQ deduplication and retry diagnostics (2026-10-12)

The actual `bullmq@6.3.10` wrapper comparison passes six simple/throttle/debounce
scenarios with both producer directions, including bulk submission, shared
cross-producer keys, delayed replacement, completion release and expiry. Sixteen
worker scenarios compare both implementations over immediate/delayed retries,
unlimited/zero/one/two-entry histories, recovered jobs and terminal failures.
Native failure reasons and histories now travel in the existing lock-checked
transition scripts, preserving removal and stale-lock semantics without Lua
changes. Rust diagnostics supply native history strings. The full existing
retry/retention/stall/start-limit/drain/ACL regression remains green.

All 66 pinned protocol tests pass against an isolated Redis container; all five
adapter unit tests pass (two real-server tests are ignored in that unit command).
Strict all-feature/all-target adapter Clippy and the 751-file source packaging
check pass. Logs: `/private/tmp/fl-rust-bullmq-history{,-unit,-adapter,-clippy,-package}.log`.
Native TypeScript typed SDK support was already complete; the current package
inventory now reflects that existing implementation rather than the older gap.


Active-job keep-last coverage also passes the full existing BullMQ wire
regression (`/private/tmp/fl-rust-bullmq-active-dedup.log`). Fresh generated
workers from CLI commit `182be8817` pass default-handler and explicit-handler
failure histories, health/readiness/authenticated controls, retry/drain/restart
and relocated sync/change/rename/delete. The companion service passes the full
shared/separate/disabled/custom HTTP/MCP listener, startup-rejection and shutdown
matrix (`/private/tmp/fl-rust-bullmq-history-generated.log`, owned Redis,
`run.0121Ws`). The CLI builds, and the protocol compiles with compatibility
features disabled.


### BullMQ scheduler timing and zero-limit parity (2026-10-12)

The separate scheduler fixture passes twelve actual `bullmq@6.3.10` scenarios:
interval and timezone-aware cron schedules in both producer/consumer directions,
three-iteration limits, successful/failed jobs, native inspection/removal, queued
cancellation during active work and sequence continuity after preprocessor
rejection. Eight lifecycle scenarios also assert zero disabling start limits.
The fixture first reproduced missing queued work during active native processing
and rejected native jobs under a zero start limit; both mismatches are fixed in
the optional compatibility feature. Existing Lua scripts are unchanged.

All 66 isolated protocol tests, strict all-feature/all-target adapter Clippy and
all 752 embedded source/template assets pass. CI now runs the scheduler fixture.
Logs: `/private/tmp/fl-rust-bullmq-scheduler-{final,clippy,unit,package}.log`;
negative reproductions: `/private/tmp/fl-rust-bullmq-scheduler-timing-before.log`
and `/private/tmp/fl-rust-bullmq-zero-start-before.log`.


### BullMQ scheduler option retention (2026-10-12)

The scheduler fixture reproduced missing native `keepLogs` metadata after
iteration. Ordinary job maps, persisted scheduler templates, first iterations
and subsequent iterations now share the diagnostic/size option encoder.
All twelve scheduler scenarios pass; every job in the eight bidirectional
interval/cron lifecycle scenarios retains `keepLogs: 2`, `stackTraceLimit: 1`
and `sizeLimit: 2048`. Actual workers retain the newest two of three log rows and
one failure diagnostic on failed jobs. This proves metadata and retention;
broader scheduler timestamp/date/options behavior remains separate.

All 66 isolated protocol tests, strict all-feature/all-target adapter Clippy,
all 752 source/template package assets and the CLI build pass. Logs:
`/private/tmp/fl-rust-bullmq-scheduler-metadata{,-clippy,-unit,-package,-cli}.log`;
negative reproduction: `/private/tmp/fl-rust-bullmq-scheduler-metadata-before.log`.


### BullMQ scheduler time metadata (2026-10-12)

The scheduler comparison reproduced missing iteration creation timestamps.
All twelve scheduler scenarios now pass with timestamp assertions on every
job in the eight bidirectional interval/cron lifecycle cases, alongside bounded
logs/histories and option retention. Cron `prevMillis` matches the scheduled
instant in each qualified job ID; interval jobs omit it as the actual Node
implementation does. All 66 isolated protocol tests, strict all-feature/all-target
adapter Clippy and the CLI rebuild pass. Logs:
`/private/tmp/fl-rust-bullmq-scheduler-time-{metadata,clippy,unit,cli}.log`;
negative reproduction: `/private/tmp/fl-rust-bullmq-scheduler-time-metadata-before.log`.

The preceding fresh generated CLI run also passes worker histories, controls,
retry/drain/restart and relocated lifecycle, plus the companion service's full
HTTP/MCP matrix (`/private/tmp/fl-rust-bullmq-scheduler-generated.log`, owned
Redis, `run.Y0u1m9`). The subsequent time-field-only scheduler change is covered
by actual bidirectional broker clients and the rebuilt checkout CLI.


Unify mixed-application HTTP SDK generation. Native contracts and TypeScript
OpenAPI now produce one TypeScript package and per-service Rust crates from either
HTTP artifact mode. Persist imported TypeScript names so missing exports cannot
silently erase clients during native lifecycle refresh. Watch the manifest and
actual configured artifacts for both languages, preserving output on errors and
recovering when inputs return. All 1,023 CLI tests and 752 packaged assets pass.
The new E2E fixture exports a generated native service and a fixture-owned real tsx
HTTP server, relocates the application, verifies custom-path watch loss/recovery,
and compiles/calls both generated SDK languages with escaped parameters, typed
success/error responses, JSON requests and request hooks. Generated Rust SDK Clippy
and native HTTP/MCP listener/startup/shutdown checks pass. This establishes the
shared HTTP client surface; mixed TypeScript scaffolding and dev/lifecycle
orchestration remain separate work.


Correct mixed project lifecycle dispatch and manifest preservation. Real CLI
regression reproduced TypeScript service registration loss during native sync;
service/worker/library sync/delete now dispatch by project metadata, and native
sync refuses non-native cleanup. Router deletion follows the owning service's
language and supports the generated async startup function. All root language,
SDK, workspace and artifact fields survive TypeScript project-view roundtrips,
as does the configured application name. A generated TypeScript-first mixed
fixture passes both languages' sync/delete, TypeScript rename roundtrips and
router removal with byte-preserving rejection of unsupported server edits. The
cleanup hook is fixture-owned to avoid modifying shared package-manager caches.
All 1,025 CLI tests and native sync/delete regressions pass. Automatic mixed
scaffolding, application-wide orchestration and deployment remain separate gates.


Support native backend attachment to existing TypeScript applications. Rust
service/worker/library/module init creates a root Cargo workspace and mixed
metadata for Node/Bun apps, preserves existing TypeScript files and Cargo
configuration, and supplies a separate native Dockerfile/ignore file. Dry-run,
configuration rejection, metadata aliases, output containment and customized
native-file preservation are tested. Actual Node/Bun scaffolds accept and compile
relocated native service/Redis-worker/library/CAC projects without TypeScript
source changes, and both native services pass the HTTP/MCP listener/startup/
shutdown matrix. The separate mixed native image builds all binaries and passes
HTTP health/OpenAPI, MCP initialization/discovery/tool invocation and SIGTERM.
All 1,030 CLI tests, 752 packaged assets and the mixed lifecycle regression pass.
CI and the migration guide include this workflow. Rust-first TypeScript setup,
application-wide mixed sync/dev orchestration and deployment remain open.


Application-wide mixed sync checkpoint (2026-10-12): `sync all --confirm` now
runs native discovery and TypeScript discovery together for Node/Bun applications,
retains native registrations during TypeScript orphan cleanup, restores registered
custom Cargo paths even without a crates directory, and refreshes both language
exports before generating the configured SDK targets. Native-only applications
with `sdkTargets = ["rust"]` do not acquire a TypeScript SDK workspace. Required
prompt values use checked argument lookup instead of panicking on synthetic
matches; confirmation accepts detected descriptions/types. Per-project sync
errors propagate rather than reporting successful completion. Removing a native
library at a custom path removes only dependencies matching its registration,
preserving unrelated dependencies and user path overrides. The generated mixed
fixture passes TypeScript discovery, custom-path restoration/removal, both SDK
checks, workspace compilation and the full HTTP/MCP listener/startup/shutdown
matrix; the Rust-only target fixture passes. All 1,032 CLI tests pass. CI includes
`rust_mixed_sync.sh`. Rust-first TypeScript scaffolding, development orchestration
and the wider deployment/runtime parity gaps remain open.


Configured OpenAPI export checkpoint (2026-10-12): native and TypeScript exporters
now refresh each registered `openapiPath` alongside the requested export-directory
layout, keeping SDK watch/import and release consumers consistent. All selected
services are checked together before export for cross-service destination
collisions; configured external/traversing/dangling-symlink paths reject.
Registered copies publish atomically, and aliases of the same exported file
preserve its contents. Three unit tests cover refresh, collision/path rejection
and symlink behavior. The generated mixed fixture now uses custom paths for both
languages, checks both SDK formats, replaces stale registered files through an
explicit `--output`, then passes workspace compilation, HTTP/MCP startup and
lifecycle cleanup. All 1,035 CLI tests and the release orchestration regression
pass. This closes artifact-path export consistency; broader parity gaps remain.


Rust-first TypeScript attachment checkpoint (2026-10-12): the previous generated
Rust app rejected TypeScript service initialization with `Invalid TestFramework:
cargo-test`. Initialization now stages the standard Node/Express/Zod workspace
and requested TypeScript project before publishing changed files. The app switches
to mixed metadata and Node tooling while retaining native Cargo paths, sources,
Dockerfiles, Compose entries, SDK targets, package scripts and custom dependency
versions. Root PNPM membership includes modules and existing SDKs; default build
runs Cargo plus all JavaScript packages, and other JS tool commands run where
their configuration lives. Atomic file replacement, internal symlink rebasing,
external-link rejection and changed-file conflict detection protect application
files. Dry-run and rejected project generation leave the app byte-for-byte intact.
The modules directory and foundational project names must be available.

The actual CLI fixture checks service/worker/library/CAC-module first attachment,
Rust-only SDK selection, subsequent native initialization, and reserved-name
rejection. It uses an empty named account: local TypeScript application generation
no longer performs the unused token lookup/refresh that previously gated rendering.
Real dependency installation and the combined Cargo/TypeScript workspace build
pass. The unmodified generated TypeScript service answers HTTP and MCP
initialization/discovery/tool invocation; the native service passes the full
HTTP/MCP configuration/startup/shutdown matrix. All 1,038 CLI tests and the existing
mixed rename/sync/delete/router regression pass. CI includes
`rust_typescript_attach.sh`. This closes default Rust-first workspace scaffolding;
unified development orchestration, mixed compliance aggregation and platform
deployment validation remain separate work.


Mixed compliance aggregation checkpoint (2026-10-12): generated compliance clients
now accept typed native adapters alongside the existing TypeScript SDK clients,
retain each success/business-error response type, and include every registered
database service/worker. Services initialized before IAM require explicit
compliance implementations; their ordinary client does not satisfy the generated
signature. Missing adapter methods fail at construction. TypeScript Better Auth
keeps its core wrapper; native adapters do not. IAM variants are recognized even
under custom native service names. Better Auth initialization now reuses existing
SDK imports and declares its client-sdk runtime dependency, preserving overrides.
Package dependency parsing accepts JSON fields in any order and reports missing
names instead of panicking.

The actual CLI fixture creates fresh mixed apps for iam-base and iam-better-auth,
installs dependencies, builds every TypeScript package, checks strict consumer
types and runs the real legacy/generated SDKs against owned HTTP responders.
Bearer propagation, export/erase results, native business errors, explicit legacy
adapters and missing-method failures pass for both variants. These checks cover
client transport and typing; persistence remains covered separately by module
suites. All 1,044 CLI tests pass. CI includes rust_mixed_compliance.sh. Unified
development orchestration, root workspace integration for TypeScript-first apps,
and deployment validation remain open.


TypeScript-first root workspace checkpoint (2026-10-12): reproducing HTTP SDK
generation on an actual Node-first mixed application showed a root PNPM workspace
containing only the SDK, omitting all four TypeScript module packages. SDK
workspace generation now merges module membership and tooling at the application
root, including configured modules paths, nested exclusions and missing workspace
policies. It parses YAML structurally, supports inline package lists, preserves
custom root scripts/dependency versions/policies, and adds Bun workspaces and
Bun SDK commands. A missing root build runs Cargo when present and then the
JavaScript packages. Redundant direct-child entries are omitted because Bun
otherwise executes their builds twice. Module source and nested configuration
remain unchanged; invalid workspace configuration rejects before either root
configuration file is rewritten.

All 1,047 CLI tests pass. Fresh Node and Bun mixed apps install dependencies and
build Cargo plus every module and the HTTP SDK from their application root. The
Bun fixture asserts exactly one successful build per module. Both applications'
TypeScript services answer HTTP and MCP initialization/discovery/tool invocation;
the native services pass the complete HTTP/MCP startup/configuration/shutdown
matrix. The Rust/mixed release orchestration regression also passes. CI includes
rust_sdk_workspace.sh. Unified development orchestration, socket-only workspace
integration and platform deployment remain separate work.


Socket-only workspace checkpoint (2026-10-12): socket SDK generation now uses
shared pure workspace planning, so TypeScript-first Node/Bun apps include their
modules and socket packages in root installation/builds. Separate sdk:socket:*
commands coexist with HTTP SDK commands and preserve overrides. Socket check mode
includes root package/workspace drift without writing them. Existing PNPM YAML
comments/layout are retained when membership suffices; inserted entries retain
sequence order/indentation and handle missing trailing newlines. Both protocol
workspace plans coexist, and invalid planning remains non-mutating.

All 1,049 CLI tests pass. The actual socket-only Node/Bun fixture invokes the
bundled compiler, generates/checks SDKs and AsyncAPI, rejects stale workspace
membership without rewriting configuration, installs real dependencies, and
builds every module/socket SDK plus Cargo from the root. Bun builds each module
exactly once. Both TypeScript services pass live HTTP/MCP calls, and native
services pass the complete HTTP/MCP startup/configuration/shutdown matrix. CI
includes rust_socket_workspace.sh. Carrying generated nested Bun install policy
to the new root remains a follow-up; unified development orchestration and
platform deployment remain open.


Bun root install-policy checkpoint (2026-10-12): HTTP and socket workspace planning
now carries generated minimumReleaseAge and exact minimumReleaseAgeExcludes from
the nested Bun configuration into root bunfig.toml. Root overrides, comments and
unrelated settings remain intact. Only these portable generated settings move;
module-relative Bun options remain nested rather than changing their path base.
Planning stays read-only and rejects malformed/outside-root destinations before
publishing root configuration. Socket check mode includes Bun policy drift.

All 1,051 CLI tests pass. Both previously validated Bun applications regenerate
with the real CLI, inherit the generated settings, preserve a custom root age and
comment, install real dependencies, and run their generated HTTP/socket SDK
build commands. Missing socket policy rejects in check mode without rewriting the
file and regenerates successfully. The shared CI workspace fixture now asserts
policy inheritance and override preservation for both protocols. Local Bun 1.2.19
confirms configuration/install/build compatibility; these checks do not establish
release-age enforcement behavior across Bun versions. Mixed development and
platform deployment remain open.


Mixed development checkpoint (2026-10-12): `forklaunch dev` resolves Compose and
starts native services/workers with debug compilation, source mounts, project-scoped
Cargo caches and restart watches for registered crates/libraries/frameworks/SDKs.
TypeScript services retain their existing development commands. Selection and
dependency closure preserve infrastructure containers and inactive native projects.
Private temporary configuration and signal supervision preserve application files
and stop Compose before CLI exit. Generated root workspaces expose the command
through their dev script while preserving custom scripts. Native watch requires
Compose 2.32; custom native entrypoints require explicit development configuration.

Actual Rust-first Node and TypeScript-first Bun applications both pass HTTP/MCP
startup, initialization/discovery/tool calls, Rust and TypeScript source reloads,
HTTP/MCP after edits and SIGTERM shutdown. The fixture caught and fixed root
Compose bind paths incorrectly interpreted as named volumes. All 1,057 CLI tests
pass; CI runs both runtime variants through rust_mixed_dev.sh. Production Compose
and Dockerfiles remain unchanged. Automatic source-to-contract/SDK refresh,
Windows runtime validation and platform deployment remain separate work.


Mixed release Dockerfile checkpoint (2026-10-12): release metadata now selects
the native root Dockerfile.rust for Node/Bun applications instead of assigning the
TypeScript Dockerfile to every service. Native worker service/consumer entries use
the same rule. Project-local Dockerfiles retain project-local contexts; configured
TypeScript Dockerfile names remain intact. All 1,058 CLI tests pass. The actual
release command also passes ten owned-platform/tool scenarios, now including Node
and Bun mixed build/export order and emitted buildContext/dockerfile pairs. The
fixture uses a unique synthetic named account and removes only that account on
exit; it makes no publication calls. Live platform/ECS launch-command and runtime
compatibility remain unverified; corrected metadata alone does not close that gate.


Captured storefront lifecycle checkpoint (2026-10-12): static captures now retain
a contained metadata.staticPath and use dedicated library lifecycle handling.
Rename/description changes preserve asset bytes and manifest comments, update
topology and publish the manifest atomically with filesystem rollback. Deletion
stages assets before publishing metadata and retains recoverable data if rollback
fails. Path checks reject external locations, symlinked capture directories,
application metadata and directories containing other registered projects.

Nested storefront initialization no longer attempts TypeScript attachment. A later
Rust-to-TypeScript workspace attachment preserves the capture's original path,
including legacy captures without path metadata. Sync validates captures without
creating package/Cargo artifacts; a captured Cargo.toml is never classified as a
native crate. Prebuilt release validation honors static paths and containment.
All 1,061 CLI tests and the actual Rust/Node/Bun lifecycle fixture pass, including
read-only previews, binary asset preservation, old-capture mixed migration, sync,
rename and deletion. CI includes rust_storefront_lifecycle.sh. Static captures
remain outside API/SDK generation; frontend hosting and native relay are separate
tasks. Existing live mixed HTTP/MCP checks remain the service-runtime evidence.


Archive installation checkpoint (2026-10-12): a fresh 2,004-file CLI archive
installs with --locked outside the checkout, generates and relocates the full
service/worker/module fixture, compiles both SDK families, and passes actual
HTTP/MCP startup, listener settings, capability callbacks and graceful shutdown.
The archive explicitly contains the mixed dev command and HTTP/socket/Bun
workspace planners. Strict ESM/CommonJS socket consumers, recursive native/CLI
HTTP SDK consumers, all four worker backends and bundled auth/transport tests
pass. This run validates the mixed-dev/release checkpoint through 76103f22f; the
subsequent storefront lifecycle has its separate CLI/runtime evidence above.

The fixture now installs its own TypeScript 5.9.3 consumer compiler. Its previous
checkout compiler had advanced to 7.0.2 and rejected an inferred fixture rootDir;
the source layout is now explicit and the archive test uses a reproducible
compiler. This does not establish general TypeScript 7 support.


Indexed authentication reads checkpoint (2026-10-12): AuthTransaction now exposes
find_unique for declared unique string fields and get_many for bounded primary-key
sets. PostgreSQL reuses current/previous-key blind indexes, deduplicates key-rotation
hits and verifies each decrypted field before returning a match. Primary-key reads
preserve record insertion order, deduplicate IDs, skip missing rows and bound inputs
and results to 4,096. Memory and fixture adapters implement the same semantics.

Credential signup/signin, Google email lookup, verification, member/invitation
email resolution and organization slugs now avoid complete user/organization scans.
Full organization user joins use primary keys while retaining their prior 100-user
selection and ordering. Memory and restricted-role PostgreSQL tenants seeded with
4,100 users pass credential operations and live HTTP/MCP initialization, discovery,
tool invocation and shutdown. PostgreSQL also passes cross-tenant misses, encryption
rotation, update/delete index cleanup, tampered-index rejection and the existing
transaction/encryption/audit suite. All 29 native tests pass; the normally ignored
PostgreSQL suite passes separately. Seven actual TypeScript Better Auth suites pass
979 response/storage/protocol comparisons. Strict default and all-feature/all-target
clippy pass and the CLI rebuild embeds the changes. Source/template inventory remains
752 files. Custom AuthTransaction implementations must add both read methods.

This closes exact unique reads and bounded user joins, not full storage scalability:
other account/session/member scans and general indexed pagination remain open.

### SDK regeneration stability (2026-10-12)

- [x] HTTP, socket-contract and AsyncAPI watchers fingerprint artifact contents with
  streaming SHA-256 instead of filesystem timestamps. Identical re-exports do not
  trigger regeneration; equal-size edits with preserved timestamps are detected.
- [x] HTTP/WebSocket SDK outputs, TypeScript workspace files, HTTP source tracking
  and Cargo membership updates preserve unchanged files, preventing avoidable
  development rebuilds when SDK generation repeats.
- Validation: 1,063 CLI tests pass; the final streaming fingerprint implementation
  additionally passes all four watch tests. Regression tests cover repeated native
  and TypeScript generation, workspace timestamps, and timestamp collisions.
- Still open: automatic contract export and SDK refresh integrated with `forklaunch
  dev`, including custom artifact paths and mixed-runtime restart handling. Existing
  live HTTP/MCP development validation remains recorded above; this checkpoint
  changes generation/watch behavior and does not claim a new startup validation.

### Native export package selection (2026-10-12)

- [x] Native contract export now invokes Cargo using the registered crate's manifest,
  honoring custom crate paths, package names and Cargo `default-run` selection rather
  than assuming that the service registration name is also the package name.
- Validation: 1,064 CLI tests pass. The new regression compiles and executes a real
  dependency-free Cargo package in a path containing spaces, with a different package
  name, two binaries and a default binary; it verifies custom contract and OpenAPI
  outputs and the requested export copy. The CLI rebuild passes with both this
  change and the SDK regeneration stability changes.

### Native development contract/SDK loop (2026-10-12)

- [x] `forklaunch dev --export-contracts` runs the freshly built native binary in
  contract mode before startup and each Compose reload, honoring registered contract
  and OpenAPI paths. Artifacts are staged before publication; failed exports retain
  prior outputs, and identical outputs retain timestamps. Shell variables are escaped
  for Compose interpolation. Path containment/collisions and read-only planning are
  checked, including native auxiliary workers.
- [x] `--sdk-target typescript|rust|all` implies native exports and owns an HTTP SDK
  watcher for the lifetime of development. `--sdk-service` scopes generation. Shutdown
  reaps the owned watcher; newly generated Rust SDK directories are watched even when
  absent at planning time. Dry-run does not create those directories.
- [x] Standalone SDK watch waits for initial artifacts instead of exiting; `--once`
  retains strict failure behavior. Watch captures input content before regeneration
  so concurrent exports are observed on the next pass. Unknown service selections fail.
- [x] Real Node and Bun mixed Compose fixtures pass native/TypeScript reloads, native
  schema-to-SDK refresh, HTTP/MCP before and after source edits, SIGTERM shutdown and
  production configuration preservation. Node starts standalone SDK watch before the
  first contract exists; Bun runs the owned watcher and checks it exits with dev.
- Validation: 1,068 CLI tests pass; CLI rebuild passes; Cargo source inventory passes
  with 754 required embedded source/template files. The initial live run exposed and
  led to correction of Compose variable interpolation; both corrected runs pass.
  Retained runs: `rust-mixed-dev/run.4t05u9pg` (Node), `run.v8f0ns32` (Bun).
- Still open: automatic TypeScript exports for combined mixed-app SDKs and automatic
  socket artifact/WebSocket SDK refresh in development. These remain separate from
  the now-validated native HTTP contract/SDK loop. Full parity is not claimed.

### Mixed TypeScript development export (2026-10-12, validation in progress)

`dev --export-contracts` now plans TypeScript workspace rebuild and OpenAPI export
before startup/reload, alongside native exports. Shared libraries are mounted and
watched; per-container dependency/build volumes isolate runtime outputs. Default
TypeScript OpenAPI version bundles and dotted operation namespaces import into
both SDK languages. Export failure preserves previous artifacts; dry-run remains
read-only. The Node/Bun fixture now covers service schemas, shared-library schemas,
combined SDK refresh, HTTP/MCP after reload, and a TypeScript-only selection replay.

Evidence so far: the 1,070-test CLI suite passes with the isolated-volume change;
the subsequent focused export suite passes both planning and failed/empty-export
artifact preservation, startup gating and unchanged timestamp checks. Cargo package
inventory includes all 755 required embedded source/template files. SDKs from an
actual retained TypeScript export compile in Rust and TypeScript.
The earlier live fixture passed initial HTTP/MCP startup but its schema edit
incorrectly added a request field that MikroORM rejected. That fixture input is
corrected to change only the response. Fresh full reload checks remain pending: the
shared Docker builder reported a missing snapshot. Retries with an owned separate
builder also exceeded the 20-minute image-build deadline before service startup
under heavy shared-machine load. Retained attempts: `run.xw81wnhl` (Node) and
`run.tn_lh14_` (Bun). These runs are not passes; the full reload gate remains open.
Automatic socket export/WebSocket SDK watch and named API-version selection remain
open. The full source-archive installation suite has not been rerun for this change.

Checklist reconciliation: four older billing entries now point to existing
phase-16 implementations and validation: embedded PostgreSQL migrations, direct
and managed Stripe transport, webhook subscription synchronization, and controlled
HTTP provider tests. Docker/Axum fixtures satisfy the original container/mock test
intent without adding testcontainers/wiremock dependencies. Customer persistence,
provider/local-write reconciliation, and configured mapper review remain open.

### Socket development follow-up (implementation in progress)

Native dev contract exports now feed a bounded host-side socket exporter. Planning
checks socket output/ownership collisions against registered service artifacts.
Valid changes refresh portable artifacts, empty registries retire owned output,
and malformed contracts preserve prior output. Atomic socket publication also
keeps unchanged timestamps. `dev --sdk-source http|websocket|all` selects the SDK
surfaces, while `sdk generate/watch --from all` processes HTTP and sockets
sequentially to preserve shared workspace metadata. One owned watcher handles both.

Compiler checking, all 1,074 CLI tests, CLI rebuild, 756-source packaging and
combined Node/Bun SDK workspace builds now pass, including real HTTP/MCP startup.
The socket export/remove/restore Compose integration remains under validation.
No development parity checkbox is closed until that final gate passes.

The earlier Docker cleanup failure has cleared: the owned builder and container
`forklaunch-parity-ts-dev-20261012` are absent after daemon recovery. No shared
daemon restart or global cache cleanup was performed.

### Smaller-task completion and regression checks (2026-10-12)

The compatibility guide records all 44 source package versions, native 0.1.0
counterparts, supported artifact profiles and upgrade boundaries. It explicitly
avoids registry-release and universal runtime-compatibility claims. Three bounded
architecture decisions are now recorded: the native registry OpenAPI emitter is
canonical; explicit transport coercion retains the existing stricter JSON policy;
built-in PostgreSQL persistence uses optional SQLx behind domain store traits.
The service/worker/mixed examples checklist now references the existing runnable
guide and previously validated generated runtime fixtures.

The native SDK crate now commits reviewed catalog OpenAPI, Rust HTTP SDK and
TypeScript HTTP SDK snapshots. They include path/query/header fields, optional
nullable fields, typed success/error responses, response headers and empty 204s.
Eight SDK crate tests and strict all-target clippy pass; CI's existing framework
workspace job runs the snapshot test. Snapshot updates require an explicit local
flag and are refused in CI. Full cross-language snapshot equivalence stays open.

The socket development code passes all 1,074 CLI tests, CLI rebuild and the
756-source packaging inventory (before the new snapshot sources). Combined
`--from all` generation/check, preserved application package policy and real
Node/Bun workspace builds pass in `rust-socket-workspace/run.i4az_z7b`, followed by
both languages' actual HTTP/MCP startup, discovery and invocation checks. A first
fixture attempt omitted its required HTTP artifact; the corrected fixture supplies
both surfaces explicitly. Automatic dev socket export/removal/restoration is now
covered by the pending Bun Compose run; it is not yet claimed as a live pass.

Cleanup removed seven obsolete failed mixed-dev applications while retaining their
logs, three old generated Cargo caches (about 23 GiB), and old test-only target and
node_modules trees across eight fixture families. Source worktrees and retained
fixture sources/logs remain. The previously stuck owned Docker builder is now
absent. Fresh Node and Bun dev checks own distinct projects and clean their Docker
resources on exit.

### Actual TypeScript conformance snapshots (2026-10-12)

`rust_ws_metadata_parity.mjs` now checks a committed 34-document AsyncAPI baseline
after exact actual-TypeScript/native equality. Normalization sorts object keys
only, preserving arrays, special/prototype keys, literal values and all schema
constraints. All cases pass, including official JSON Schema validation, 63 parser
documents, 11 documented parser limitations and independent recursive/literal
reference and payload checks. External non-WebSocket protocol scope stays open.

`rust_billing_contracts_parity.mjs` now records 66 configured TypeScript base/Stripe
routes and 28 complete validator/OpenAPI DTO schemas after the existing native
route and selected schema-property comparisons. Full DTO snapshots add drift
visibility without claiming that those selected comparisons prove every schema
constraint. Both suites pass again without the update flag. The shared JSON
snapshot helper rejects updates in CI; fixture/helper changes trigger the parity
workflow. This closes representative-TypeScript snapshot coverage and normalized
AsyncAPI snapshots, while canonical whole-service OpenAPI equivalence remains open.

The package inventory now includes all 760 required native embedded sources,
including the new native snapshots. The first fresh mixed-dev run reached real
HTTP/MCP and native reload but failed because its shared-schema fixture imported
an unexported `object` helper. The fixture now uses supported `optional({...})`
syntax and waits for service readiness between schema edits. Fresh corrected
Node/Bun runs remain pending; that failure is not a development parity pass.

Two older IAM architectural recommendations are also reconciled with existing
implementation evidence: ForkLaunch owns the base IAM credentials/directory/session
logic, and the separate native Better Auth crate follows API/behavior comparisons
rather than depending on a Rust clone of Better Auth. This does not close the
remaining provider, custom client, ORM/hooks, scalable storage or compliance work.

### Interactive native naming (2026-10-12)

Native initialization now prompts for an omitted name only when stdin and stderr
are terminals. The default is `my-<kind>`; service/worker/library names select their
Cargo package and project directory together. Explicit positional names retain
existing validation. Noninteractive omission fails immediately with guidance and
no application writes. The real-PTY fixture covers application, service, library
and worker prompts, custom/default selection, invalid-name rejection and closed
stdin behavior, then removes its temporary workspace. It is included in the init
command integration suite. All 1,074 CLI unit tests and the rebuilt CLI pass.

The combined SDK workspace fixture now removes its own node_modules/build caches
on success by default, retaining generated source; `FORKLAUNCH_KEEP_TEST_CACHES=1`
opts out for debugging. The cleanup block was exercised against the completed
Node/Bun run and both SDK source trees remain. The expanded Bun dev fixture also
adds the serde_json dependency required by its custom socket registration. That
missing fixture dependency was corrected in the in-flight run before service
startup checks; socket lifecycle/reload validation remains pending.

### Development validation follow-up (2026-10-12, ongoing)

The corrected Node full mixed loop passes in `rust-mixed-dev/run.xb4uz99y`:
HTTP/MCP before/after source edits, native/TypeScript/shared-library contract and
SDK refresh, supervised shutdown and preserved production configuration. Its
first TypeScript-only replay hit the fixture's four-minute fresh image-build limit;
that is not a pass. A retry uses the same retained application and a build-aware
startup deadline. Future full runs keep the owned service image until replay and
use `--no-build`, then remove it, avoiding a redundant dependency install.

Bun's expanded fixture now supplies serde_json for its explicit socket registration,
uses the actual `socket_development.ts` output name, and refreshes ephemeral host
ports while waiting for startup (Docker may change them after a restart). A fresh
run at `rust-mixed-dev/run.b25r_8kn` is validating socket export/change/removal/restore
plus the shared-library loop. The superseded attempts are retained as logs only.
No additional development checklist item is closed while these checks remain open.

The Node TypeScript-only replay now passes on the retained `run.xb4uz99y`
application: a shared-library-only edit updates the SDK, actual HTTP/MCP calls pass,
and the owned watcher exits on shutdown. Bun `run.b25r_8kn` completed socket
creation/change/removal/restoration and TypeScript/shared-library SDK refresh,
but its final probe retained a native host port from before a later restart.
That probe failed and the run is not counted as a full pass. The fixture now
refreshes native readiness/port again immediately before the final probe; fresh
Bun run `run.3r582t9d` includes this correction and the image-reuse replay path.

Billing snapshot review also replaced unresolved dependency-injection placeholders
and JSON-erased role Sets with explicit auth methods and role arrays. Both baseline
update and normal comparison pass all 66 route/28 schema cases again. The
compatibility guide now carries the documentation site's standard frontmatter.

### Explicit compliance metadata derives (2026-10-12)

The new core derive exposes explicit none/PII/PHI/PCI field classification, storage
names and nullability to application-owned persistence adapters. Only classified,
explicitly nullable fields qualify for anonymization. It does not infer compliance
from encryption or Option/serde metadata. Generic structs, raw identifiers and
renamed core dependencies compile; malformed/duplicate declarations fail closed.
Core/macro tests, compile-fail documentation examples and strict all-target clippy
pass. Generic retention enforcement and database compliance integration stay open.

A rebuilt CLI also generates an application containing the new macro crate, core
metadata API and integration tests; all core/macro tests and documentation examples
pass from that generated workspace. Its temporary source is removed after validation.
The CLI package inventory includes all 764 required embedded sources/templates.

### Combined development loop and image reuse (2026-10-12)

Bun `rust-mixed-dev/run.3r582t9d` passes the complete mixed loop: HTTP/MCP
startup and post-edit calls, native and TypeScript reloads, native/TypeScript/
shared-library contract and SDK refresh, socket creation/change/removal/restore,
SIGTERM watcher shutdown and unchanged production configuration. Its subsequent
TypeScript-only replay failed because Compose rejects `up --watch --no-build`.
This is a separate failed replay, not a failure of the completed mixed loop.

The CLI now implements watched `--no-build` through a private image-only plan:
remove build definitions, preserve explicit/default local image names and restart
rules, and forbid pulling replacement build-backed images. Custom rebuild rules
fail explicitly. All 1,075 CLI tests and the rebuild pass. Real TypeScript-only
Bun replay with image reuse is still pending; the earlier Node replay passed.
No additional development checklist gate is closed at this checkpoint.

### Development validation gate completed (2026-10-12)

The corrected Bun TypeScript-only replay exits successfully on retained
`rust-mixed-dev/run.3r582t9d/app`, using watched `--no-build` image reuse. It checks
a shared-library-only change, regenerated SDK content, the actual Compose restart,
HTTP/MCP calls and owned watcher shutdown. Its containers, volumes, network and
image are removed. The image rebuild was only needed because the preceding failed
replay had already performed cleanup. An independent live Compose probe also proves
source-triggered restart with an image-only plan; its owned resources are removed.

Together with the complete Bun mixed loop and Node `run.xb4uz99y` mixed loop plus
separate successful TypeScript-only replay, this closes unified mixed development
and source-triggered contract/SDK refresh. The aggregate old Bun command remains
recorded as failed at its first replay attempt; its main loop and corrected standalone
replay are the successful evidence, not a claimed fresh aggregate exit.

Evidence logs: `/private/tmp/fl-dev-typescript-runtime-bun-sockets-ports.log` (main
loop), `/private/tmp/fl-dev-bun-typescript-replay-final.log` (corrected replay),
`/private/tmp/fl-dev-node-typescript-replay-final.log`, and
`/private/tmp/fl-compose-restart-image-only-final.log`. All 1,075 CLI tests pass.
The earlier CLI package inventory passes all 764 embedded sources/templates.

Current checklist: **617/682 (90.5%), 65 unchecked**. This is an unweighted task
count, not a claim of universal feature coverage. Fourteen entries were resolved
from the 79-open checkpoint, including explicitly documented stale checklist
reconciliation. Production deployment/publication, broader authentication and
persistence, schema/protocol coverage, documentation and release gates remain.

### Generic retention enforcement hooks (2026-10-12)

Core persistence now exposes a typed async store boundary and bounded retention
runner. Policies use explicit compliance metadata and an application-resolved
exclusive cutoff. Calls require tenant identities; duplicate scopes, future
cutoffs and invalid batch bounds fail before store access. Dry runs count without
mutation. Delete and nullable classified-field anonymization preserve prior
committed counts on a failed batch and continue other tenant/entity passes. A
batch cap prevents a broken adapter from looping forever; remaining work is
reported as incomplete enforcement. Counter overflow is an error.

Six native regression tests cover tenant fences, cutoff boundaries, already
anonymized rows, field selection, dry runs, oldest-first deletion, committed-count
reporting, entity filters, empty anonymization field lists and faulty adapters.
The actual TypeScript RetentionService comparison passes 24 combinations of
action, dry-run, batch size and tenant selection. Strict core all-target clippy
passes; CI includes the comparison. Generic database adapters, full calendar/TZ
cutoff handling and directory/ecommerce compliance integration remain open.

Separately, user-authorized cleanup removed 19 clean inactive committed worktrees
and pruned stale registrations. Branch references remain intact; ignored lockfiles
and restoration metadata are backed up under
`~/.codex/workspace-archives/forklaunch-2026-10-12`. Dirty and active workspaces were
preserved. Free disk rose from approximately 74 GiB to 99 GiB.

Validation also passes core all-feature tests, the CLI rebuild and all 767 required
embedded package sources. Checklist: **618/682 (90.6%), 64 open**.


### Application-configured IAM directory compliance (2026-10-12)

Applications can register generic `ComplianceMetadata` types with
`DirectoryCompliance` and use `iam_router_with_compliance`. The default remains
unclassified. Explicit scalar subject mappings select records within the trusted
store tenant; export projects IDs and classified fields, while erasure deletes
whole records to match the actual TypeScript service. Each entity commits
separately. Failed reads/deletes/commits report sanitized failures, never claim
uncommitted deletes, and produce an HTTP/MCP error for incomplete operations.

Validation passed:

- Five actual TypeScript/native configured compliance comparisons, alongside
  the existing 44 directory operations, 33 route contracts and 22 schema layouts.
- Eleven native IAM-base tests, including commit failure/rollback and metadata
  validation; the shared compliance fixture also passed against encrypted
  PostgreSQL with restricted-role tenant RLS.
- Strict all-feature/all-target IAM-base and IAM-axum Clippy.
- Generated HTTP/MCP startup, system-role rejection/acceptance, classified export,
  erasure and repeated-operation 404; existing CRUD, both live SDKs, restart,
  source preservation, relocation, rename and deletion passed.
- Rebuilt CLI and all 770 embedded Rust source/template package entries.

Non-ID subject lookups retain the store's 4,096-record bound and fail if exceeded;
ORM relation inference and scalable indexed traversal are separate tasks. Logs:
`/private/tmp/fl-directory-compliance-{parity,postgres,all-tests,clippy,runtime,package}.log`.
Checklist: **619/682 (90.8%), 63 open**. Percentages count checklist items equally.


### Shared template rendering (2026-10-12)

`generate_with_template`, native scaffolding and SDK templates now share the
same Ramhorn rendering primitive. Native and SDK callers also share embedded
file lookup and contextual diagnostics. The legacy directory walk, TypeScript
namespace/database replacements, native preservation rules and SDK ownership
logic remain at their existing boundaries. All 1,075 CLI tests pass
(`/private/tmp/fl-shared-template-tests.log`).

Checklist: **620/682 (90.9%), 62 open**.


### Checked binary WebSocket metadata (2026-10-12)

`SocketSchemaEntry::binary()` persists an explicit binary marker with a canonical
schema. Checked plans emit byte codecs in both SDK languages, preserve invalid
UTF-8/JSON-looking payloads, and publish `application/octet-stream` per message
in AsyncAPI. JSON Schema format annotations alone do not opt into raw bytes.
Ambiguous message unions, extra binary schema assertions and binary close
reasons fail generation. Ping/pong may use binary codecs within protocol bounds.

Validation: both generated SDKs compile and exchange ten actual binary messages
with an isolated `ws` server, plus control frames and oversize rejection. Empty
payloads, all byte values, invalid UTF-8 and JSON-looking bytes round-trip exactly.
The existing TypeScript SDK suite passes 12,960 native and actual TypeScript/AJV
codec comparisons and its authorization, cancellation, queue, control/close and
malformed-peer checks. All 77 selected native tests, strict all-target/all-feature
Clippy, rebuilt CLI and all 770 package entries pass. CI includes
`framework-rs/parity/rust_ws_binary_runtime.mjs`. Logs are
`/private/tmp/fl-binary-socket-{runtime,json-regression,tests-final,clippy-final,package}.log`.

Checklist: **621/682 (91.1%), 61 open**. External binary AsyncAPI imports remain
separately scoped; this closes explicit native artifact metadata and generation.


### Context-aware schema dictionary bundling (2026-10-12)

Checked unscoped schema bundling now traverses each referenced dictionary in a
separate interpretation context. It copies targets into collision-checked
schema definitions and rewrites only structural schema locations, preserving
literal `const`/annotation data even when the same source value is also a schema.
Recursive targets, nested contexts and document extraction use bounded shared
copies; missing references, active resources and invalid targets fail explicitly.
Socket union relocation now selects the checked unscoped helper.

Validation passed: 34 existing actual TypeScript/native metadata snapshots,
six original/exported AJV alias comparisons, native recursive/union constraint
checks, 74 selected framework tests, strict all-feature/all-target Clippy,
generated Rust consumers with warnings denied, and strict TypeScript ESM/CJS
consumers with runtime alias-union validation. CLI rebuild and all 770 package
entries pass. Logs: `/private/tmp/fl-schema-context-{metadata,tests-final,rust-sdk,ts-sdk-final,clippy,package}.log`.

Checklist: **622/682 (91.2%), 60 open**. General resource IDs/anchors/dynamic
resolution remain open; resource-preserving relocation still rejects aliases
whose scope cannot be safely resolved.


### Telemetry presence checkpoint (2026-10-12)

The generated telemetry fixture now starts both HTTP and MCP, invokes health,
and rejects untrusted origins before verifying real OTLP exports. Assertions
cover W3C parent IDs, trace correlation, response status/size/duration metrics,
zero active-request balances after completion, exclusion of correlation IDs
from metric dimensions, structured error logs and SIGTERM flushing. Offline
exports still succeed with intentionally invalid live telemetry settings.

The relocated generated app/worker build passes. Both native telemetry middleware
tests pass, covering auth/unmatched requests and streaming completion, error,
cancellation and HEAD. The actual receiver fixture passes HTTP/JSON and protobuf
signals, exporter authentication/resource configuration, disabled export and
sanitized invalid configuration. Existing CI runs both the direct OTLP fixture
and the generated shell fixture. Logs:
`/private/tmp/fl-telemetry-presence-{generated,middleware,otlp}.log`.

Checklist: **623/682 (91.3%), 59 open**. This verifies telemetry presence and
lifecycle behavior; it does not claim identical TypeScript metric names or
vendor-specific telemetry configuration.


### SSE emission and validation checkpoint (2026-10-12)

The optional `forklaunch-axum/sse` helper emits bounded `SseFrame<T>` streams.
`respond` validates typed data with garde; `respond_with` accepts whole-envelope
validation. Every-event validation is the default; explicit first-event mode
matches the actual TypeScript emitter's validation frequency. The first event
is prepared before headers, later errors terminate the body, and cancellation
drops producers without background tasks or prefetch queues. IDs and wire byte
limits are checked in both modes. Standard multiline framing and sanitized errors
are intentional differences from TypeScript's raw interpolation/error detail.
Applications supply validators matching their contracts; arbitrary contract JSON
Schema is not automatically applied by this helper.

Validation passed: 46 all-feature Axum tests, including six SSE tests for typed
validation, first/later errors, exact Unicode/escaped-string byte limits and
cancellation before the first event or while awaiting subsequent events; strict
all-target/all-feature Clippy; 16 frames compared with the actual TypeScript
emitter plus real Zod response validation and empty-stream behavior. The relocated
generated service passes both SDKs, contract/OpenAPI artifacts, native/imported
fetch, stream failures and resource release, with real HTTP/MCP startup and
shutdown. The final CLI rebuild and all 772 package entries pass. The generated
runtime run preceded the final escaped-string accounting fix; final unit tests
cover that boundary. CI runs the emitter conformance and generated runtime tests.
Logs: `/private/tmp/fl-sse-emitter-{tests-final,clippy-final,parity-final,generated,cli-build-final,package}.log`.

Checklist: **624/682 (91.5%), 58 open**. The broader schema-constraint,
multipart, resource-scope and external AsyncAPI tasks remain open.


### External AsyncAPI binary messages (2026-10-12)

External AsyncAPI 3.0 WebSocket imports now preserve explicit
`application/octet-stream` messages as native binary metadata. Canonical
string/binary payloads may be inline or referenced locally. Send/receive may
use different wire encodings; each binary direction selects one message.
Mixed wire alternatives, ambiguous byte messages, additional schema assertions,
reference siblings/cycles/external targets and encoded pointer separators fail
checked import. JSON `format: binary` alone does not enable byte transport.

Validation passed: 25 AsyncAPI tests, strict Clippy, both compiled SDKs with ten
live imported-binary roundtrips (including invalid UTF-8, JSON-looking bytes and
all byte values), and the existing native binary/control-frame regression.
The rebuilt CLI passes relocated import/check/watch, strict SDK compilation,
live JSON peers, rename/delete ownership, and generated HTTP/MCP startup and
shutdown before and after rename. All 772 embedded package entries pass. CI
runs both native-artifact and imported-binary modes. Logs:
`/private/tmp/fl-asyncapi-binary-{tests,clippy,runtime,native-regression,lifecycle,cli,package}.log`.

Checklist remains **624/682 (91.5%), 58 open**: this completes binary payload
support within the broader external AsyncAPI item; its other profiles remain open.


### External AsyncAPI JSON Schema dialects (2026-10-12)

The importer now accepts Multi Format payloads for Draft 4, Draft 6, Draft 7,
2019-09 and 2020-12 with explicit matching `schemaFormat`/`$schema` declarations.
Document-local references and dialect-specific constraints survive extraction;
message unions retain their dialect. Unknown/mismatched formats, nested mixed
dialects and alternatives with different dialects fail checked import. Draft 4
message discrimination no longer treats ignored `const` annotations as assertions;
required enum tags provide actual discrimination.

Validation passed: 45 AsyncAPI/codegen tests, strict AsyncAPI Clippy, generated
TypeScript compilation and Rust compilation for all five dialects, ten live
roundtrips across both SDKs and twenty invalid numeric-bound rejections. Native
checks also cover references inside Multi Format schemas, Draft 4 unions and
mixed-dialect rejection. The source document remains unchanged. The CLI rebuild
and all 772 embedded source/template entries pass. CI includes the new
`rust_asyncapi_dialects_runtime.mjs` fixture. Logs:
`/private/tmp/fl-asyncapi-dialects-{tests-final,clippy-final,runtime,cli,package}.log`.

Checklist remains **624/682 (91.5%), 58 open**. This completes supported JSON
Schema dialect imports within the broader AsyncAPI task; other formats/protocols,
JSON string wire encoding, upgrade auth/parameters and traits/replies remain.


### Explicit JSON WebSocket wire encoding (2026-10-12)

`SocketSchemaEntry::json` / portable `jsonEncoding: true` now preserve JSON
strings and null through shared SDK plans and both generated clients. Native
`PayloadCodec::with_json_encoding` retains validators and byte limits while
requiring strict UTF-8 JSON, including quotes/escapes in wire byte counts.
External AsyncAPI JSON messages automatically select this mode. Native artifacts
retain existing raw-string behavior by default; mixed raw/JSON alternatives and
simultaneous binary/JSON flags are rejected. Explicit JSON exports carry JSON
media even when another document default is configured.

Validation passed: 84 selected framework tests and strict all-feature/all-target
Clippy across WebSocket, AsyncAPI and SDK crates; all 12,960 existing actual
TypeScript/native codec comparisons and the complete generated TypeScript
WebSocket runtime fixture; both generated clients with 16 exact-wire string/null
roundtrips, malformed-peer rejection and encoding/UTF-8/byte-limit checks. The
five-dialect and imported-binary live fixtures also pass with the new codecs.
The rebuilt CLI passes relocated import/check/watch, strict SDK consumers,
live peers, rename/delete ownership, and generated HTTP/MCP startup/shutdown
before and after rename. All 772 embedded package entries pass. CI includes
`rust_ws_json_encoding_runtime.mjs`. User-facing generation and compatibility
docs now describe the supported importer instead of labeling all imports pending.
Logs: `/private/tmp/fl-ws-json-encoding-{tests,clippy,regression,runtime-final,dialects,binary,cli,package,lifecycle}.log`.

Checklist remains **624/682 (91.5%), 58 open**. Binary messages, the five JSON
Schema dialects and JSON string/null wire encoding are complete subparts of the
external AsyncAPI task. Its event protocols, upgrade auth/parameters and
traits/replies remain open; no full-parity claim is made.

### BullMQ options audit and generated runtime validation (2026-10-12)

Completed the [pinned BullMQ options audit](RUST-BULLMQ-OPTIONS-AUDIT.md), including
producer/consumer mapping, retry and retention policies, waiting/active inspection,
tenant prefixes and restricted Redis ACL behavior. Fixed missing event-stream
metadata limits and `skipMetasUpdate`, enforced the Redis 5 minimum with a working
`skipVersionCheck` for both queue and worker, and accepted zero attempts as one
execution without retry, matching TypeScript.

Validation: the full actual TS/native BullMQ interoperability fixture passed;
focused options coverage passed for metadata defaults/custom/preserved settings,
150-job event-stream trimming from both producers, zero attempts in both language
directions, and ACL-denied INFO/HSET opt-outs. All 67 pinned protocol unit tests
and all 12 actual TS/native scheduler scenarios passed; strict all-feature/all-target
adapter Clippy passed. The rebuilt CLI
included all 772 embedded package entries. A relocated generated BullMQ worker
passed processing/retries, lifecycle transitions and graceful shutdown; its API
passed real HTTP/MCP startup, initialization, discovery, health invocation,
listener configuration and shutdown checks.

Checklist: **625/682 (91.6%), 57 open**. Audit completion does not close the broader
production-worker gate: custom scheduler strategies, BullMQ telemetry hooks,
Cluster/Sentinel connections and full flow/parent interoperability remain explicit
implementation requirements in the audit.

### Optional TanStack Query SDK helpers (2026-10-12)

Both the native TypeScript SDK generator and CLI share optional, dependency-free
`forkLaunchQueryOptions` and `forkLaunchMutationOptions`. They preserve generated
request/variable types and successful status-specific response envelopes, throw
HTTP failure envelopes through `ForkLaunchSdkHttpError`, and propagate transport
errors. Query cancellation composes TanStack and caller signals with listener
cleanup; cache keys, retry policy, selection and invalidation stay explicit.

Validation: actual Query Core and React Query 5.104.2 checks passed for strict
request/response inference, scoped keys, cache reuse/invalidation, HTTP errors,
retries, mutations, network errors and caller/query/pre-aborted cancellation.
Eight SDK crate tests, reviewed output snapshots, strict SDK Clippy and all 31
CLI generation tests passed. The rebuilt CLI generated and typechecked both SDKs
through contract and OpenAPI paths; both clients passed live Rust and TypeScript
wire fixtures. The generated Rust service also passed real HTTP/MCP startup,
initialization/discovery/tool invocation and graceful shutdown checks.

Checklist: **626/682 (91.8%), 56 open**. No TanStack runtime dependency is added to
ordinary generated SDK use. See the framework README for cache scoping and usage.

### Literal JSON object keys across request, MCP and persistence boundaries (2026-10-12)

Completed the [JSON boundary audit](RUST-JSON-BOUNDARY-AUDIT.md) and fixed literal
`$serde_json::private::Number` / `$serde_json::private::RawValue` object keys being
interpreted as internal Serde markers under unified dependency features. The
vendored 1.0.151 parser distinguishes real map keys from internal number/raw
markers, preserving exact numbers, RawValue, nested DTO values and owned/borrowed
Value conversions. Framework, blueprint and generated application roots share
one patched parser; dependency pins prevent silent bypass by newer registry
versions. Generation/sync adds missing patches and preserves user overrides.

Validation: 237 default-feature and 262 all-feature upstream/parser regression
tests passed, plus alloc-only compilation. Framework and blueprint library
suites passed 242 and 108 tests respectively (4 and 5 environment-gated tests
ignored). Strict framework workspace Clippy, focused manual/routed MCP checks,
and five CLI workspace migration tests passed. Actual TS/native Redis and S3
fixtures passed both encrypted wire formats and both producer/reader directions
with literal marker objects under raw-value/arbitrary-precision features. The
expanded actual TS/AJV/native WebSocket matrix passed all **13,908** cases.

All **859** bundled source/template entries are packaged. A fresh source archive
installed the CLI, generated and relocated all native module families, compiled
both languages' SDKs through native/contract/OpenAPI/AsyncAPI paths, and passed
live HTTP/MCP startup and capability checks. A separate relocated HTTP fixture
passed literal-object HTTP and MCP roundtrips, both SDK languages against Rust
and TS wire servers, contract/OpenAPI regeneration and graceful shutdown.

Checklist: **627/682 (91.9%), 55 open**. External workspace consumers must apply
the documented root Cargo patch. Registry publication/upstream adoption,
unpaired surrogates, deeper nesting and broader ORM/JavaScript normalization
remain separate gates.


### Canonical actual TypeScript service fixture (2026-10-12)

The paired IAM runner now bundles the repository's actual Express, core, common
and Zod implementation, starts it beside the existing native sample IAM router,
and checks eighteen live HTTP outcomes plus both generated OpenAPI endpoints.
Successful user reads/lists, empty missing-user responses and missing credentials
match. Exact malformed/expired/forged JWT expectations retain three differences:
TypeScript returns 403 and native returns 401. TypeScript uses `authenticated`
for JWT-only access; the native IAM contracts still label it `protected` without
RBAC. These are explicit remaining auth/error conformance work, not normalized
away. Pagination is intentionally unchanged in the sample handlers.

External TypeScript dependencies are pinned in the fixture manifest; CI installs
them in a dedicated prefix. The runner bundles source in an isolated temporary
directory, disables telemetry export, uses ephemeral loopback listeners and
cleans up its own processes/files. The native example passes strict Clippy.
Whole-document OpenAPI comparison, remaining validation/auth modes and MCP
framework conformance remain open. Checklist: **628/682 (92.1%), 54 open**.


### Canonical fixture follow-up: access labels and valid OpenAPI (2026-10-12)

The actual service comparison exposed TypeScript OpenAPI security requirements
that did not resolve to their declared schemes and path parameters missing the
mandatory required flag. The emitter now references its declared default,
explicit Authorization or custom-header scheme names; HTTP Basic/JWT/API-key
requirements carry empty scope arrays rather than RBAC permission names. Four
actual-source emitter cases and both live documents enforce these invariants.
JWT-only native IAM contracts now use `authenticated`, matching TypeScript's
access semantics without changing credential checks or adding implicit RBAC.
All eight IAM-interface/sample-service library tests and the eighteen live
HTTP outcomes pass. The three rejected-JWT status differences remain explicit;
whole-document OpenAPI and broad auth/error conformance remain open.
Checklist remains **628/682 (92.1%), 54 open**.


### Canonical whole-document snapshots and normalized comparison (2026-10-12)

Both real service OpenAPI documents now have committed, key-sorted snapshots.
The live comparison resolves local schema references and normalizes schema
annotations, set ordering and disjoint primitive unions. Operation sets,
parameters, security requirements and successful response schemas match.
Ten remaining error-response entries are pinned separately in
`framework-rs/parity/fixtures/canonical-iam/openapi-differences.json`; the comparison cannot
silently discard them or accept new differences. The fixture's optional nullable
DTO fields and integer query limit now match the native sample's published types.
Snapshot creation and a separate ordinary verification run pass alongside all
eighteen live HTTP checks. This closes the snapshot/comparison harness task,
not full document/error parity or the three rejected-JWT status differences.
Checklist: **629/682 (92.2%), 53 open**.


### Canonical rejected-JWT status alignment (2026-10-12)

Declared JWT routes now return 403 for presented but rejected credentials,
matching actual TypeScript behavior for malformed, expired and forged JWTs.
Missing credentials remain 401. The mapping is applied at the native HTTP
contract boundary; opaque bearer sessions and application-owned authenticators
without JWT metadata retain their 401 behavior. Error envelopes remain native
JSON and are still a separate parity task. A nine-case middleware regression
checks JWT versus opaque/untyped auth, missing/invalid/forbidden failures and
correlation IDs. All 58 auth/Axum library tests, strict all-feature/all-target
Clippy, the eighteen actual-framework HTTP outcomes and OpenAPI snapshots pass.
The canonical fixture has no remaining rejected-JWT status exceptions.
Checklist remains **629/682 (92.2%), 53 open**.


### Typed JWT issuance and actual JOSE interoperability (2026-10-12)

`forklaunch_auth::issue_jwt_hs256` now issues explicit server-owned `JwtClaims`
through jsonwebtoken, complementing existing HS256 and configured JWK/JWKS
verification. Empty subjects/keys fail; applications retain expiry, secret,
rotation and grant policy. The canonical native example issues a test token,
actual JOSE verifies every claim and both TypeScript/Rust services accept it.
JOSE-issued tokens continue to authenticate against native verification.
All twenty live HTTP outcomes, full OpenAPI snapshots, fifteen auth tests and
strict auth/example Clippy checks pass. Documentation distinguishes the simple
JWT helper from Better Auth's provider-specific signing and cookie/OIDC flows.
Checklist: **630/682 (92.4%), 52 open**.


### Six-surface runtime performance harness (2026-10-12)

The optional `forklaunch-testing/benchmarks` example and `rust_benchmarks.mjs`
measure typed JSON validation, contract-driven HTTP, generated TypeScript SDK
calls, Redis SET/GET, S3 PUT/GET and typed WebSocket round trips. Every result is
checked outside its timer. Warmups, sample distributions, build/compiler/runtime
versions, source revision/dirty state and host metadata are recorded in JSON.
Storage uses unique owned keys; missing infrastructure is explicitly skipped or
rejected with `--require-infra`. The optimized profile is the default; CI runs a
short debug smoke against its isolated Redis/MinIO and uploads the report without
unstable latency thresholds. Documentation states the single-client, 1033-byte,
unencrypted measurement boundaries and excludes production performance claims.

All six paths passed 50 debug samples after ten warmups and 100 optimized samples
after twenty warmups against live dedicated Redis/MinIO. Strict benchmark Clippy,
workspace formatting and 861 packaged-source checks pass. Redis reported zero
remaining keys; both owned infrastructure containers were removed after use.
The generated benchmark example is included in the CLI source bundle.
Checklist: **631/682 (92.5%), 51 open**.


### Strict HS256 time claims found during security review (2026-10-12)

The expanded actual TypeScript fixture first reproduced a native 200 response
for a token that expired thirty seconds earlier, where TypeScript returned 403.
The HS256 principal helper inherited jsonwebtoken's default sixty-second leeway
and disabled not-before checking. It now uses zero leeway, enforces `exp <= now`
as expired, validates optional `nbf`/`iat` numeric types, preserves fractional
not-before comparisons and rejects empty signing keys. Future `iat` alone remains
valid, matching JOSE without maxTokenAge. Explicit caller-owned JWK/JWKS Validation
policies remain unchanged. All 34 actual-framework HTTP outcomes, sixteen auth
tests, OpenAPI snapshots and strict auth Clippy pass. Earlier relocated generated
HTTP/MCP checks and 108 blueprint library tests also passed (five environment-gated
tests were ignored); their owned generated workspace was removed.
Checklist remains **631/682 (92.5%), 51 open**. Dependency and broader security
review continue; this is a concrete finding/fix, not a completed security audit.


### Engineering security review and advisory gate (2026-10-12)

[RUST-SECURITY-REVIEW.md](RUST-SECURITY-REVIEW.md) records auth/JWT/HMAC, encryption,
secrets, HTTP/MCP and generated SDK boundaries with concrete evidence and explicit
application-owned controls. Removing the redundant AWS legacy Rustls connector
removed four framework vulnerability findings (old h2 and webpki). Framework and
blueprint lockfiles upgrade anyhow to 1.0.103. The CLI now aliases its YAML parser
to serde_norway 0.9.42, removing the archived serde_yml/libyml implementations;
all 1,076 CLI tests pass. S3 passes nineteen local and two live MinIO tests.

The cargo-audit 0.22.2 gate retains three exact reviewed findings: AWS S3's LRU
uses a non-panicking String key; the RSA crate performs generation/encoding while
ring performs private signing; paste is a compile-time maintenance notice.
All findings stay visible, with no scanner ignores or severity/platform filters.
Changed versions/new advisories fail; offline tests exercise those failure paths.
CI uploads current reports. The broad framework library run passes 245 tests
(four ignored), and the rebuilt CLI again passes relocated HTTP/MCP startup,
invocation/Origin rejection/shutdown and both SDKs against Rust/TypeScript via
contract/OpenAPI artifacts. The owned generated workspace and MinIO container
were removed. This completes the engineering review, not the separate remaining
provider, deployment, scalability and implementation gates.
Checklist: **632/682 (92.7%), 50 open**.


### Generated Rust SDK diagnostic redaction (2026-10-12)

Both Rust emitters redact client URLs, default-header values and typed/transport
request values in Debug output, retaining field names and body-presence flags.
Reqwest transport errors strip their URLs. Generated consumers assert that URL
credentials, auth headers, query/path tokens and text/binary bodies do not leak.
Native SDK media/strict compilation, five Rust SDK tests and strict all-target
Clippy pass. Rebuilt CLI-generated clients pass the new assertions through both
contract/OpenAPI artifacts, calling Rust and compiled TypeScript services.
HTTP/MCP startup, initialization, discovery, invocation, Origin rejection and
graceful shutdown pass. The owned generated workspace was removed.
Checklist remains **632/682 (92.7%), 50 open**.


### Typed multipart request extraction (2026-10-12)

The opt-in Axum multipart feature supplies `Multipart<T>` with automatic Serde
decoding and garde validation, plus independent field/count/total byte limits.
Files preserve bytes and filenames never become paths. Requests reject malformed
MIME, missing/unknown fields, invalid UTF-8, model violations and size overflow
before handler entry, without echoing uploaded values. Seven comparisons execute
the actual TypeScript discriminator and Express content parser with Busboy; they
cover Unicode, binary/empty/repeated files, repeated text and filename-less
octet-stream classification. Legacy charset decoding and mixed text/file name
collisions are explicitly outside this comparison's portable contract.
All-feature Axum tests pass (48 library and three live integration tests), strict
all-feature/all-target Clippy passes, and all 863 embedded source/template entries
are packaged. CI runs the actual-parser comparison; owned fixture output cleans
itself up. The multipart checkbox stays open for SDK response decoding.
Checklist remains **632/682 (92.7%), 50 open**.


### Multipart SDK response decoding (2026-10-12)

CLI contract/OpenAPI generation and both framework-native emitters now decode
`multipart/form-data` responses into TypeScript FormData or Rust ordered part
records. Names, repeated parts, filenames and binary/empty file bytes survive.
Decoding enforces a configurable positive 8 MiB default wire-byte bound and a
1,024-part bound. Wrong media, absent/invalid boundaries, malformed/truncated
bodies and excessive bytes/parts fail. Multipart/mixed remains explicitly
unsupported; parts are transport values, not automatically validated JSON DTOs.
Rust uses multer; TypeScript uses the platform FormData parser with bounded
Web/Node stream collection and cancellation. Generated Cargo dependencies are
conditional. Standalone native source documents its required dependencies.

The full relocated HTTP fixture passes both artifact formats, both native SDK
generators, native/injected cross-fetch, positive/zero byte limits, excessive
part count, malformed cases and abort signals against Rust and compiled
TypeScript services. Native generated Rust Clippy is strict. HTTP/MCP startup,
initialization/discovery/invocation, Origin rejection and graceful shutdown pass.
All 84 CLI SDK tests and 32 framework codegen/SDK tests pass, as do strict
framework SDK Clippy and 864 packaged source/template checks. Reviewed snapshots
include the preceding diagnostic redaction and the new response runtime. Owned
generated workspaces were removed. Together with typed extraction, this closes
the multipart runtime/SDK checkbox. Broad MIME/MCP/schema conformance stays open.
Checklist: **633/682 (92.8%), 49 open**.


### Authentication framework candidate evaluation (2026-10-12)

[RUST-AUTH-FRAMEWORK-EVALUATION.md](RUST-AUTH-FRAMEWORK-EVALUATION.md) completes
the ten selection criteria with official release metadata and hashes of inspected
release sources. It covers maintenance/dependencies, password upgrades,
session/CSRF storage, OAuth/OIDC, tenant and entitlement boundaries, passkeys,
framework independence and contract/SDK/MCP integration. It selects narrow
tower-sessions, openidconnect/oauth2 and later webauthn-rs adapters while keeping
ForkLaunch IAM and policy ownership. Higher-level framework adoption is deferred.
The review records the tower-sessions/axum-login maintenance request, rs-auth
account-linking/verified-email and state-store concerns, and explicit integration
acceptance conditions. This is a source spot-review, not an audit certification
or completed provider/cookie integration. No production dependency changed.
All 24 non-registry source links returned HTTP 200; twelve registry metadata
links point to the successfully fetched official API (the HTML /versions route
returned 404). The evidence JSON parses and local document links resolve. Owned
downloaded review sources were removed after recording hashes.
Checklist: **643/682 (94.3%), 39 open**. The ten new completions are evaluation
tasks; the percentage measures checklist completion, not runtime feature weight.

### Optional server-side cookie sessions

`forklaunch-axum/cookie-session` integrates tower-sessions 0.15 behind an opt-in
feature. Applications supply the session store and live subject resolver; routes
explicitly declare their cookie name. Secure/HttpOnly/SameSite=Strict cookies,
random synchronizer tokens, exact HTTPS Origin checks, ID/token rotation and
logout are covered. Unsafe login/logout requires CSRF; bearer routes never use
cookie authentication as a fallback. OpenAPI uses distinct named cookie schemes.
MCP preserves transport browser-security headers over tool-argument overrides.

Validation: 97 affected all-feature tests, the additional feature-disabled
fail-closed test, strict all-target Clippy, 865 packaged source/template files,
and the three-lockfile RustSec gate pass with no new advisory exceptions.
Live loopback HTTP and MCP exercise valid/invalid cookies; tests also cover
fixation, old tokens, cross-origin requests, logout and current disabled-user
resolution. Logs: `/private/tmp/fl-cookie-final-tests.log`,
`fl-cookie-default.log`, `fl-cookie-clippy.log`, `fl-cookie-package.log`,
`fl-cookie-audit.log`.

This closes the tower-sessions adapter recommendation. Production store choice,
atomic application revocation, OAuth/OIDC providers and Better Auth client/ORM
parity remain separate work. Read-only access does not extend the thirty-minute
session expiry; session writes do. Checklist: **644/682 (94.4%), 38 open**.

### Document-local SDK schema anchors

The shared CLI/native HTTP SDK compiler now resolves static `$anchor` fragments
at schema positions, including OpenAPI components and guarded recursive models.
Anchor and pointer aliases reuse one graph node. A bounded index excludes
literal examples/defaults and nested resource scopes; malformed/duplicate or
missing anchors fail closed. Draft 4/6/7 do not gain newer anchor semantics.

The mutual-recursion fixture now uses named anchors. A freshly rebuilt CLI
created and relocated the application, generated both SDKs from contract and
OpenAPI artifacts, compiled both languages and ran live recursive round trips.
Both native generators passed the same consumers. HTTP/MCP startup, discovery,
invocation, Origin rejection, port/listener variants and shutdown passed.
Validation: 86 CLI SDK tests, 34 native SDK/codegen tests, strict codegen and
generated-client Clippy. Logs: `/private/tmp/fl-anchor-cli-tests.log`,
`fl-anchor-final-tests.log`, `fl-anchor-clippy.log`, `fl-anchor-runtime.log`.

The resource-ID/dynamic-reference checklist item remains open; this is a
verified part of it. Checklist remains **644/682 (94.4%), 38 open**.

### Bounded auth-store pagination

Better Auth transactions now expose insertion-order keyset pages with opaque
model-scoped cursors and 1..=4096 limits. PostgreSQL uses a new
(tenant_id, model, sequence) index and decrypts at most limit+1 records per page;
the memory backend maintains ordering and unique-field indexes. Replacement,
deletion and rollback keep those indexes consistent. Page Debug output omits
auth record contents. Stable traversal uses one existing serialized transaction.

Memory and live restricted-role encrypted PostgreSQL pass 4,100-record paging,
exact boundaries, invalid limits/cursor models, deleted cursor rows, replacement
ordering and rollback. The PostgreSQL test also retains encryption, tenant RLS,
rotation, ciphertext/index transplant rejection and atomic audit checks. The
all-feature crate suite passes 29 tests (one database test run separately), and
final memory-store regressions pass after replacing unique-field scans. Strict
all-target Clippy and packaging of 866 embedded sources/templates pass.
Logs: `/private/tmp/fl-auth-pagination-postgres.log`,
`fl-auth-pagination-all-tests.log`, `fl-auth-pagination-final-store.log`,
`fl-auth-pagination-clippy.log`, `fl-auth-pagination-package.log`.
The owned PostgreSQL container was removed after validation.

Account/session/member query migration and encrypted lookup backfill remain
open. The existing full-model list still rejects oversized results, preventing
silent authorization truncation. Checklist remains **644/682 (94.4%), 38 open**.

### Actual MCP transport/body conformance

`rust_mcp_transport_parity.mjs` bundles the repository generator and Zod validator
against pinned FastMCP 4.22.4/Zod 4.6.7/MCP SDK 1.32.1. The official client
initializes and discovers the actual FastMCP server over InMemoryTransport;
32 tool calls proxy to a live HTTP receiver. No validator or FastMCP substitute
is used. Multipart text/Unicode/newline/name handling and rejected non-string
shapes, file/binary bytes, form composites, JSON/text and custom media match the
native encoder. The existing generator-only fixture independently checks exact
multipart framing. Native MCP tests invoke the real multipart/form/file HTTP
extractors behind authentication guards.

All 32 actual transport cases and all 18 native MCP tests pass. CI installs the
shared pinned fixture dependencies and runs the new transport comparison.
Sessions, HTTP connections and temporary sources close/clean up on completion.
Logs: `/private/tmp/fl-mcp-transport-final.log`, `fl-mcp-transport-native.log`.
An initial fixture run exposed a retained FastMCP ping timer; retaining and
closing the session returned by `connect` fixes fixture shutdown. The clean
final run exits normally without force termination.

This closes the multipart/expanded-generator MCP item, not the separate
versioned-API or broad route/schema conformance gates. Checklist:
**645/682 (94.6%), 37 open**.

### Indexed opaque-session authentication

Better Auth exact token lookup now uses the memory unique-field index and
rotating PostgreSQL blind indexes. Fetch/refresh, single-token revocation and
sign-out no longer load the session model. Opening a PostgreSQL store rebuilds
legacy token claims under the tenant write lock in 256-record pages. Duplicate
or corrupt legacy records fail startup with transactional rollback. Coordinated
index-aware writer rollout is required; old binaries do not maintain claims.
Token uniqueness is a deliberate stricter constraint than the configured TS
entity's unindexed token field, preventing ambiguous credential ownership.

Memory and restricted-role encrypted PostgreSQL authenticate and revoke with
4,100 sessions while the legacy list still fails closed. The PostgreSQL fixture
removes token claims from the legacy dataset, reopens the store and validates
backfill beyond 4,096 rows. All 30 native tests and strict all-target Clippy pass;
the separate database test passes. Actual Better Auth comparison retains 63
checks/48 body/storage matches. A rebuilt CLI generates the latest migrations
and sources, and the relocated app passes both SDKs, actual frontend clients,
HTTP/MCP listener/startup/shutdown, restart and rename/delete.
Logs: `/private/tmp/fl-session-index-tests.log`, `fl-session-index-postgres.log`,
`fl-session-index-parity.log`, `fl-session-index-clippy.log`,
`fl-session-index-runtime.log`. The owned database and generated workspace were
removed after completion. Account/member indexes and session collection queries
remain open. Checklist remains **645/682 (94.6%), 37 open**.

### Callback/client checklist reconciliation

The callback exchange/client integration checkbox remained open after the
implemented popup-helper and actual shared/generated frontend-client work
recorded above. The fresh `fl-session-index-runtime.log` run revalidates both:
trusted signed callback exchange, JWT/session/email fragment fields and cookie
clearing, independently verified token/JWKS, and eight actual client namespace
scenarios (80 requests). HTTP/MCP startup, persistence, shutdown and module
rename/delete also pass. Mark the specific callback/client task complete;
optional plugins/hooks and unified mixed SDK behavior retain their own gates.
This is reconciliation of verified implementation, not a newly added runtime
feature. Checklist: **646/682 (94.7%), 36 open**.

### Named-version MCP reference regression

The real FastMCP fixture exposed an array-index lookup against the named version
map, plus missing response metadata on bodyless versioned routes. Select the
version from the same ordered values used to build input schemas and always
retain its response contract. Actual MCP calls now verify both named versions
of POST and GET, forwarded bodies/query strings, decoded responses and rejected
mismatched input. All 32 existing native/TypeScript transport comparisons also
pass (`/private/tmp/fl-mcp-version-after.log`). Rust versioned APIs remain open;
this repairs the TypeScript reference behavior. Checklist unchanged: 646/682.

### Session collections beyond the model scan bound

Single-session revocation now resolves the target token through its unique index.
Listing/revoke-others/revoke-all traverse 256-record keyset pages in one locked
transaction, retaining foreign sessions, insertion order and the configured
expired-session distinction. Memory and restricted encrypted PostgreSQL verify
4,099 owned sessions, deletion across page boundaries and foreign-user isolation.
The PostgreSQL fixture also retains unrelated credential-fixture sessions.
All 30 native tests, strict all-target Clippy and 63 actual Better Auth checks
(48 native response/storage comparisons) pass. Logs:
`/private/tmp/fl-session-collections-tests.log`, `fl-session-collections-memory.log`,
`fl-session-collections-postgres.log`, `fl-session-collections-clippy.log`,
`fl-session-collections-parity.log`. Full indexed relationship traversal remains
open; checklist unchanged at 646/682.

### Encrypted relationship indexes and scoped collection pages

Add transaction-scoped `find_page` with ordered memory indexes and forced-RLS
PostgreSQL relationship blind indexes. Retained-key queries deduplicate matches
and verify decrypted values/sequence before returning rows. Startup rebuilds
legacy claims in bounded pages under the tenant write lock; migration 0003
requires runtime table privileges and a coordinated index-aware writer rollout.
Startup backfill cost remains proportional to stored records.

Credential lookup, linked-account listing/removal, session collections and basic
organization membership lookup/listing use these indexes. More than 4,096 users,
accounts and sessions pass native and restricted-role PostgreSQL HTTP/MCP tests.
Fixtures cover cursor deletion, replacement/removal, rollback, exact Unicode/case
matching, invalid limits/fields, tenant isolation, legacy backfill, key rotation
and transplanted index rejection. All 30 native tests and strict all-target
Clippy pass. Actual TypeScript comparisons pass: 81 credential, 50 profile/account,
134 organization and 48 session response/storage cases (63 session checks).

The rebuilt CLI packages 867 Rust source/template files. Its relocated generated
app passes both SDKs, eight frontend-client scenarios, HTTP/MCP startup and tool
invocation, persistence/restart, source preservation, rename and delete. Logs:
`/private/tmp/fl-auth-relations-tests-final.log`, `fl-auth-relations-clippy-final.log`,
`fl-auth-relations-postgres-final.log`, `fl-auth-relations-accounts.log`,
`fl-auth-relations-profiles.log`, `fl-auth-relations-organizations.log`,
`fl-auth-relations-sessions.log`, `fl-auth-relations-package.log`,
`fl-auth-relations-runtime.log`. Broader account/organization/provider/compliance
callers still need migration; checklist remains **646/682 (94.7%), 36 open**.

### Indexed membership and permission queries

Organization members/full graphs/deletion, dynamic roles, membership guards and
application role/permission surfacing now query scoped relationship indexes.
Existing insertion ordering, filter evaluation and configured limits stay intact.
Legacy non-string organization values preserve JSON equality via bounded pages.
The large-tenant fixture adds 4,100 foreign members and malformed foreign dynamic
role records: own member listing, grants, full graphs, role reads, surfacing and
delete remain correct without reading or modifying the foreign scope.

All 31 native tests, strict Clippy and restricted encrypted PostgreSQL pass.
Twelve actual TypeScript suites pass 2,615 comparisons across membership, dynamic
roles, permissions, invitations and surfacing. The rebuilt generated app passes
both SDKs, eight frontend-client scenarios, HTTP/MCP startup/invocation, restart,
source preservation and rename/delete. Logs: `/private/tmp/fl-auth-member-index-`
`tests-final.log`, `large-final.log`, `postgres.log`, `clippy-final.log`,
`runtime.log` and the twelve named comparison logs. Team/provider/verification/
compliance query migration remains open. Checklist remains **646/682 (94.7%)**.

Cleanup removed seven previously validated Better Auth generated workspaces,
151 further generated runs with successful final log receipts and committed
fixture sources, and the two newly completed generated runs. Nested Git repos,
tracked files and active workspaces were excluded. Detailed old-run receipts:
`/private/tmp/fl-validated-workspace-cleanup.json` and
`/private/tmp/fl-completed-workspaces-removed.json`.

### Indexed teams/invitations and claim-owner writes

Team reads, member selection/mutations and invitation listing/creation now use
scoped indexes. Membership-key lookup retains precedence over legacy team/user
pairs. Migration 0004 indexes unique claims by record owner for replacement,
backfill and FK deletion. PostgreSQL EXPLAIN verifies the selective owner index
on the large dataset. The shared fixture now includes 4,100 records in each of
users, accounts, organization members/roles, teams/team memberships and invitations,
plus the existing 4,100-session scenario. Native/live HTTP/MCP behavior remains
correct beyond the old model scan bound.

All 31 native tests, strict Clippy, 1,550 actual TypeScript team/invitation
comparisons and restricted encrypted PostgreSQL pass. The expanded backfill
exceeded the owned fixture's 256 MiB temporary volume; a fresh 1 GiB fixture
passes the full test. CI PostgreSQL has no corresponding 256 MiB volume limit.
The CLI packages 868 source/template files; the generated relocated app passes
both SDKs, frontend clients, HTTP/MCP startup/invocation, restart and lifecycle
checks. The generated workspace was removed after completion. Logs:
`/private/tmp/fl-auth-team-index-tests-final.log`, `fl-auth-team-index-clippy-final.log`,
`fl-auth-team-index-postgres-final.log`, `fl-auth-team-index-runtime.log`,
`fl-auth-team-index-package.log` and seven named comparison logs. Another 296 MiB
of build outputs were removed only where dependency files traced them to already
deleted, validated generated workspaces (`fl-orphan-generated-artifacts.json`).
Provider/password, verification, keyring and compliance/retention traversal
remain open. Checklist stays **646/682 (94.7%), 36 open**.


### Provider, verification and compliance collection migration

Google account resolution, password updates/revocation, reset tokens and account
compliance now use relationship indexes. OAuth state consumption and reset
callback cleanup preserve global expiration behavior using 256-row keyset pages.
Memory and restricted PostgreSQL fixtures include 4,100 foreign verification
records, account ownership, one-time state/cookie correlation, token reset,
export redaction and scoped erasure. The PostgreSQL run passed in 258.90 seconds;
all 31 portable auth tests and strict all-feature/all-target Clippy passed.
Actual account, OAuth state, Google account/callback/HTTP/token and callback
comparisons passed. The password-reset fixture exposed an existing response
mismatch: invalid reset tokens returned 401 instead of 400. The route-specific
mapping now matches all 44 actual HTTP/storage comparisons; the live HTTP/MCP
regression also verifies replay rejection. All 14 encrypted-ORM/native compliance
comparisons passed. Evidence: `/private/tmp/fl-auth-provider-index-*.log`.
Keyring and retention scans remain open; checklist stays 646/682 (94.7%).

Generated provider-query validation also passed: both SDKs, eight actual frontend
client scenarios (80 HTTP actions), HTTP/MCP shared/separate/disabled listeners,
root/custom/URL namespaces, self-JWKS, persistence, shutdown and rename/delete.
All 868 embedded sources passed. The completed owned workspace `run.VztQJh` was
removed; runtime and package evidence use the same provider-index log prefix.


### Final auth traversal migration and configured JWKS window

All production Better Auth service callers now use indexed matches or keyset
pages. Keyring signing, verification and JWKS publication read the configured
adapter's first 100 inserted keys. An actual 101-record endpoint fixture caught
and now verifies the prior JWKS publication mismatch. Operator rotation pages
through all keys; it deliberately retains upstream's public first-100 window,
so operators must prune historical rows before new keys fall outside that window.
Retention uses 256-record pages and a bounded oldest-batch selection with stable
insertion ties. Dry runs count without collecting decrypted account rows.

All 31 portable auth tests passed; the restricted encrypted PostgreSQL fixture
passed in 290.36 seconds. Both stores exercise more than 4,096 account and key
records, retained-key behavior, final-page rotation, retention ordering/zero-size
batches/dry runs/replay, tenant isolation, audit and rollback. Strict all-feature,
all-target Clippy passed. Actual TS comparisons: key lifecycle 30, retention 41,
signing 189 API + 216 storage, verification 393 API + 309 storage, JWT/JWKS 26.
The rebuilt CLI packages all 868 sources. Evidence:
`/private/tmp/fl-auth-final-scans-{tests,clippy,postgres,keyring,retention,sign_jwt,verify_jwt,jwt,package}.log`.
Global expiration/retention scans remain linear and response arrays retain memory
proportional to their matches; startup index backfill requires coordinated writers.

The final rebuilt generated preset also passed both SDKs, all eight frontend
client scenarios, HTTP/MCP startup (shared/separate/disabled), custom/root/URL
namespaces, self-JWKS, persistence, shutdown and rename/delete
(`fl-auth-final-scans-runtime.log`). Indexed auth storage gate is complete:
**647/682 (94.9%), 35 open**. Broader Better Auth custom hooks/options and dynamic
namespace semantics remain separate, open work.


### Ecommerce classified export and erasure hooks

Added tenant-bound memory/PostgreSQL hooks for the actual Order shipping-address
classification. Default order-ID subject resolution matches the TypeScript
walker's fallback; explicit customer-ID mapping matches its constructor override.
The real encrypted ORM fixture revealed that classified values serialize as
opaque objects. Rust now preserves that behavior with a non-Debug protected
address and explicit audited `deanon` access; JSON export cannot leak addresses.
Whole-order erasure leaves unclassified records untouched and reports incomplete
operations. PostgreSQL decrypt/audit/delete work commits atomically, with foreign
tenant isolation and audit-failure rollback verified. All five native ecommerce
tests, restricted PostgreSQL, strict default/all-feature Clippy and 12 actual
MikroORM/native comparisons pass. The same-realm fixture mode preserves MikroORM's
`instanceof Function` handling of real lazy enum factories; default fixture
execution is unchanged. CI includes the new comparison. Evidence:
`/private/tmp/fl-ecommerce-compliance-{tests,postgres,clippy,clippy-default,actual}.log`.
Retention remains open; checklist remains 647/682 (94.9%).


### Ecommerce explicit retention policies

The ecommerce stores now implement the framework `RetentionStore` boundary for
`OrderCompliance`. No default retention duration is imposed. Trusted applications
resolve a calendar cutoff and opt into deletion or anonymization, with tenant
binding, bounded oldest-first batches, dry-run counts and incomplete-enforcement
reporting. The configured shipping address is non-nullable: the actual TS service
preserves it during anonymization and records completion; native behavior matches.
Deletion removes the whole Order. Migration `5_order_retention.sql` stores forced-
RLS completion markers with a cascading record FK, so ordinary DTO updates cannot
lose markers and reused deleted IDs cannot inherit them. Runtime grants include
the marker table. Memory stores bind retention via `for_tenant`.

Six portable native ecommerce tests and restricted PostgreSQL pass, including
competing passes, exact cutoff exclusion, replay, marker preservation across
updates/deletion/recreation, foreign-tenant rejection and audit-failure rollback.
Strict default/all-feature/all-target Clippy passes. Eight real encrypted ORM
retention comparisons and twelve classified export/erasure comparisons pass;
fixtures load the actual ecommerce entity metadata without unrelated auth registry
entries. The rebuilt CLI packages 879 sources, and the Rust security audit reports
no unexpected findings. Evidence: `/private/tmp/fl-ecommerce-retention-*.log`.
The earlier generated ecommerce run passed HTTP/MCP and checkout but lacked the
local Stripe fixture path; a fresh complete run supplies `STRIPE_SDK_PATH`.

The complete rebuilt generated ecommerce run passes HTTP/MCP startup/discovery,
both SDKs, actual TS checkout/webhook comparisons, cross-process checkout races,
provider verification/capture, encrypted persistence, durable inventory worker,
Compose ownership, source preservation, relocation and rename/sync/delete
(`fl-ecommerce-retention-runtime.log`). Both owned generated workspaces were
removed, including the superseded run missing its local Stripe dependency path.
Ecommerce compliance gate is complete: **648/682 (95.0%), 34 open**. Interrupted
provider capture/reconciliation remains separate work.


### Relay handoff concurrency prerequisite

The Rust/mixed relay audit reproduced a real TypeScript template race: two
independent request entity managers, synchronized after reading one ticket, both
redeemed it. The template now conditionally updates the unused, unexpired ticket
in one database statement before minting a session. The real MikroORM/PostgreSQL
fixture now permits exactly one redemption and checks nonce uniqueness, replay,
unknown and expired tickets. CI watches the relay template and runs this fixture.
Evidence: `/private/tmp/fl-relay-orm-before.log` (2 redemptions instead of 1) and
`fl-relay-orm-after.log` (pass). This fixes the reference behavior; native relay
service, HTTP/MCP and generated lifecycle work remain open. Checklist stays
648/682 (95.0%), 34 open.

Relay redirect audit also reproduced browser URL normalization turning
`/\t/foreign.test`, `/\n/foreign.test` and `/\r/foreign.test` into another-origin
URLs. The TS template now rejects ASCII controls before accepting root-relative
paths, with real template/URL-parser regression coverage. Evidence:
`fl-relay-redirect-before.log` fails and `fl-relay-redirect-after.log` passes.
The native relay primitive applies the same guard.


### Rust and mixed relay completion (2026-10-13)

The Better Auth adapter now implements relay nonce claiming, five-minute one-time
handoffs, application-owned token/owner and claim hooks, one-day session cookies,
and root-relative redirect validation. Nonce claims commit before hooks and
handoff claims commit before session minting. The new encrypted auth model shares
existing RLS, audit and unique-index enforcement; migration 0005 permits it.
HTTP/MCP ingest and claim contracts use route-scoped HMAC policies: instance keys
cannot authorize unrelated routes. Browser handoffs remain outside SDK/MCP.

`init module --language rust -m relay` extends the existing Better Auth service,
including mixed applications, and creates preserved application hook source.
Generated contracts include both relay operations. Dry-run, duplicate rejection,
relocation, sync/reinit preservation, both SDK builds, live TypeScript SDK calls,
HTTP/MCP startup and invocation, rename and deletion pass. Safe scaffold hooks
redirect without login and decline claims until applications supply their policy.

Validation receipts:
- `fl-relay-full-tests.log`: 35 native tests pass, PostgreSQL test separately run.
- `fl-relay-native-postgres.log`: full restricted PostgreSQL fixture passes,
  including concurrent relay claims/redemptions and cookie authentication.
- `fl-relay-route-keys-tests.log`: 41 framework unit + 2 integration tests pass.
- `fl-relay-http-tests.log`: live HMAC, cookie login, claim-purpose ordering,
  redirects, MCP invocation and graceful shutdown pass.
- `fl-relay-comparison.log`: actual TS ORM/native default-hook, nonce uniqueness,
  synchronized concurrent redemption, replay/unknown/expired and redirect cases.
- `fl-relay-generated.log` and `fl-relay-generated-mixed.log`: complete generated
  Rust/mixed lifecycle and HTTP/MCP checks; mixed fixture includes SDK calls.
- `fl-relay-final-clippy.log` and `fl-relay-route-keys-clippy.log`: strict checks pass.
- `fl-relay-package.log`: 886 embedded source/template files packaged.
- `fl-relay-cli-tests.log`: all 1,078 CLI tests pass.

Checklist: 649/682 (95.2%), 33 open. These are verified checklist counts, not an
estimate that all remaining tasks have equal effort. Broader provider options,
versioned routing, contract/schema conformance and deployment/publication gates
remain open.


### Workspace documentation completion (2026-10-13)

Both workspaces now have indexed guides covering every member's purpose and
feature/default selection, linked public API source, setup/composition, runtime
operations, persistence/auth boundaries, provider/worker semantics, SDKs and
remaining compatibility limits. Existing runnable generation and protocol guides
are linked rather than duplicated. The blueprint README no longer describes
completed indexed-query migration as outstanding, and documents ecommerce
protected compliance export and explicit retention policy.

Strict all-feature `cargo doc --workspace --no-deps` builds pass for all 19
framework and 30 distinct blueprint crates. The first strict framework build
caught broken HMAC and OTLP intra-doc links; fully qualified links now resolve.
CI runs both strict documentation builds. Every crate is in the guide catalog,
and all 81 local guide links/anchors resolve. Both workspace format checks pass,
including module ordering left by recent additions. Evidence:
`/private/tmp/fl-framework-docs.log`, `fl-blueprint-docs.log`,
`fl-framework-format.log`, `fl-blueprint-format.log`.

Checklist: 651/682 (95.5%), 31 open. Documentation describes current verified
capabilities and the remaining gaps; it does not close their implementation or
publication/deployment gates. Six completed relay scratch workspaces were removed
following commit `01686c344`; approximately 60 GiB remained free.


### Bundled static schema resources in HTTP SDK imports (2026-10-13)

The shared CLI/native HTTP SDK compiler now indexes bundled `$id` resources,
resolves relative references against lexical bases, scopes anchors per resource,
and preserves recursive identity across URI/pointer aliases. This follows the
[JSON Schema resource/base-URI model](https://json-schema.org/draft/2020-12/draft-bhutton-json-schema-01).
Generation never fetches external schemas. Indexing shares the existing depth,
node and byte bounds and excludes literal annotations. Duplicate/malformed or
missing resource identities fail explicitly. Source packaging includes the new
shared resolver module and both workspace lockfiles record the URI dependency.

The resource-enabled recursive fixture passes both CLI artifact sources
(contract/OpenAPI), both live client languages, both native framework generators,
HTTP/MCP discovery/invocation and shutdown. It includes self-recursive resource
roots and relative references between mutually recursive resources. The original
resource-free recursive fixture also passes. Unit checks cover scoped duplicate
anchor names, URN identities, literal preservation, missing/duplicate/dynamic
resources, OpenAPI component-local pointers, and expansion bounds.

Evidence: `/private/tmp/fl-resource-generated.log`,
`fl-resource-recursive-regression.log`, `fl-resource-native-tests.log` (27 shared
compiler + 5 Rust generator + 8 TS generator tests),
`fl-resource-native-clippy.log`, `fl-resource-cli-tests.log` (1,084 pass),
`fl-resource-package.log` (889 packaged source/template checks), and
`fl-resource-format.log`. CI includes the resource-enabled generated runtime.

The broad resource-reference gate stays open: draft-4 IDs, mixed resource
dialects, resource references into literal/schema-dictionary contexts,
nonempty ID fragments, dynamic/recursive references and WebSocket resource-aware
union relocation remain explicitly unsupported. Checklist remains 651/682
(95.5%), 31 open.


Schema resource follow-up adds lexical dialect tracking, draft-4 `id`, and
embedded resources using different drafts. The model walk now selects keyword
meaning from the current schema resource, including standalone OpenAPI components,
instead of applying the document root's draft everywhere. Tests confirm legacy
`const` and conditional annotations do not become modern validation/type rules.
Dialect strings and index copies consume the shared byte budget before copying.

The full generated matrix passes static, legacy and mixed-resource variants:
both client languages, both artifact imports, both native generators, strict
Rust client lint, HTTP/MCP startup, discovery, invocation and graceful shutdown.
Evidence: `fl-resource-dialects-generated.log` (three complete passes),
`fl-resource-dialects-native-tests.log` (29 shared + 5 Rust + 8 TS tests), and
`fl-resource-dialects-clippy.log`. All 1,086 CLI tests and workspace formatting
also pass (`fl-resource-dialects-cli-tests.log`, `fl-resource-dialects-format.log`).
Dynamic/recursive references, ID fragments,
resource/literal contexts and WebSocket relocation remain open; count unchanged.


### 2026-10-13 — HTTP SDK dynamic schema scope and native anchor correctness

The shared CLI/native graph compiler now supports explicit 2020-12
`$dynamicAnchor`/`$dynamicRef` resources. Each reference model is identified by
its target and inherited dynamic bindings, so the same base resource can safely
specialize under independent string/integer recursive roots. Static references
remain static; dynamic references whose initial target is a plain anchor or
JSON pointer retain static resolution. Binding copies use the existing byte
budget. Missing/older dialect declarations and WebSocket dynamic imports fail
explicitly instead of producing models that disagree with runtime validation.

The native validator's opt-in `forklaunch-static-anchor-resolution` correction
prevents ordinary `$ref` from dynamically rebinding and isolates named-reference
cache entries by actual resolved target. Pointer aliases retain their upstream
behavior. Provenance and CI cover the feature; all 1,993 feature-enabled and
1,992 default vendor validator tests pass, along with 150 referencing tests.
The one existing archive-missing Git-submodule test remains explicitly skipped.

`rust_schema_dynamic_parity.mjs` drives the actual private graph compiler and
native validator: 21 native/original and bundled-schema checks pass, including
recursive context specialization, static-vs-dynamic targets, plain anchors and
URI/pointer fallback. AJV 8.20 agrees on 14 checks. Three recorded results expose
AJV sibling-scope leakage / plain-anchor rebinding, and four URI-reference checks
hit AJV's explicit fragment-only limitation. These are documented differences,
not reported as AJV parity. Expected behavior follows
[JSON Schema 2020-12 §8.2.3.2](https://json-schema.org/draft/2020-12/draft-bhutton-json-schema-01#section-8.2.3.2).

Broader regression: 13,908 actual TypeScript/AJV/native codec checks pass;
30 shared compiler, 5 Rust generator, 8 TypeScript generator, 18 codec and
7 transport unit/integration tests pass; all 1,087 CLI tests, strict native
clippy and framework formatting pass. CLI packaging includes all 890 expected
source/template files. Generated dynamic schemas pass both SDK languages,
both contract/OpenAPI imports, both native generators, strict Rust client lint,
and real HTTP/MCP startup, discovery, calls and graceful shutdown.
Evidence: `fl-resource-dynamic-{ajv,ws-parity,native,final-unit,cli-tests,clippy,format,package,generated,vendor-tests,upstream,referencing}.log`.

Checklist remains **651/682 (95.5%), 31 open**. The broad resource gate still
includes 2019-09 recursive references, legacy ID fragments, resource/literal
contexts and WebSocket resource-aware relocation; this batch does not close it.

The combined generated resource suite also passes all four variants (static,
legacy, mixed and dynamic), recorded in `fl-resource-dynamic-regression.log`.
Strict all-feature framework API docs pass and the security audit reports no
unexpected advisories (`fl-resource-dynamic-docs.log`, `fl-resource-dynamic-audit.log`).


### 2026-10-13 — 2019-09 recursive SDK resource roots

The shared HTTP graph compiler now supports explicit 2019-09 resource-root
`$recursiveAnchor` booleans and `$recursiveRef: "#"`. Binding state is isolated
from named 2020-12 anchors, participates in model identity and consumes the same
expansion budget. Non-anchored resources preserve static root fallback; malformed
anchors, non-root anchors, other recursive URI values and unsupported dialects
fail before generation. This follows the
[2019-09 recursive-reference profile](https://json-schema.org/draft/2019-09/draft-handrews-json-schema-02#rfc.section.8.2.4.2).

The combined conformance fixture passes 36 native-original/compiled cases:
27 independent AJV agreements, five explicitly recorded AJV scope/anchor
result differences, and four unsupported AJV URI-reference cases. New tests
cover nested resource roots, false/absent recursive anchors, strict profile
boundaries and both static/dynamic forms. All 31 shared compiler, 5 native Rust,
8 native TypeScript and 1,088 CLI tests pass, with strict native lint, formatting
and 890 packaged source/template checks. The generated recursive-resource mode
passes both languages, both artifact sources, both native generators, strict
Rust consumer lint, and live HTTP/MCP startup, invocation and shutdown.
Evidence: `fl-resource-recursive-{ajv,unit,clippy,cli-tests,format,package,generated}.log`.

Five committed dynamic fixtures and the completed recursive fixture were cleaned
up (182 generated workspaces removed cumulatively). Checklist remains
**651/682 (95.5%), 31 open**; remaining resource work includes legacy fragment
IDs and resource-aware socket unions.


### 2026-10-13 — Resource-aware socket unions and raw AsyncAPI export

Socket plans now resolve each standalone resource graph before combining message
schemas. Two messages can reuse the same `$id` and anchor names without sharing
recursive bindings. The public `bundle_schema_resources` operation is shared
with raw AsyncAPI export, which resolves resource scopes before document-pointer
relocation. Resource-free schemas retain their original annotation/pointer
layout. Both workspace locks record the AsyncAPI → SDK-codegen dependency.

`rust_ws_resources_runtime.mjs` generates strict TypeScript and Rust consumers
for static, Draft-4, 2020-12 dynamic and 2019-09 recursive unions. Both clients
complete 16 live roundtrips; malformed recursive cross-variant values and
constraint-invalid outgoing values fail. The emitted AsyncAPI document passes
the official schema with format validation. CI runs the fixture. Runtime
bundling explicitly requires one dialect per standalone message: model-only
HTTP imports can preserve mixed embedded dialects, but runtime translation of
mixed-dialect constraints remains open.

Validation: 27 AsyncAPI, 32 shared compiler, 5 Rust generator, 8 TypeScript
generator and 1,088 CLI tests pass; strict native clippy, framework formatting,
all-feature API docs, 890 packaged sources and security audit pass with no
unexpected advisories. Existing metadata checks retain 34 actual TypeScript/native
comparisons, 63 parser documents with 11 known limitations, and six dictionary
alias comparisons. Generated TypeScript regression retains both module formats,
13,908 native/TypeScript codec comparisons and full live transport controls.

A rebuilt CLI passes relocated generation/check/watch, invalid edit recovery,
staging, ownership/containment, retirement, strict consumers, actual TypeScript
socket peer roundtrips, service rename/delete, and generated HTTP/MCP startup,
discovery, invocation and shutdown before/after rename. Two stale readiness
assertions were updated to the watch banner already merged from main. The first
live-client invocation omitted the parser environment variable; validation
resumed at that step with the fixture dependency set and all remaining lifecycle
checks passed. Evidence: `fl-socket-resource-{runtime,tests,clippy,cli-tests,metadata,ts-regression,cli-runtime,cli-resume,docs,format,package,audit}.log`.

Checklist remains **651/682 (95.5%), 31 open**. The resource gate still covers
legacy ID fragments and mixed runtime/import constraints. Completed socket
fixtures are cleaned after committing the source change.


### 2026-10-13 — Legacy fragment anchors and AJV-compatible ID siblings

The shared resource index resolves Draft-4 `id` and Draft-6/7 `$id` fragment-only
anchors without replacing their enclosing resource root. Recursive aliases,
colon-bearing legacy names and identical anchor names in different resources
remain distinct. Static anchor grammar now follows the declared 2019-09 or
2020-12 dialect. Malformed names and modern fragment IDs fail explicitly.
Legacy IDs combining a URI/path and nonempty fragment remain outside this
profile; this batch implements the defined fragment-only form from
[Draft 7 §8.2.3](https://json-schema.org/draft-07/draft-handrews-json-schema-01#rfc.section.8.2.3).

An additional relative-base fixture found that the native resolver ignored IDs
beside legacy `$ref` while AJV applied them. The existing opt-in
`forklaunch-ajv-ref-siblings` feature now forwards to referencing and retains
those ID scopes. Default upstream behavior is unchanged. All six new sibling
scope cases agree with actual AJV. The combined fixture passes 57 native/original
and compiled cases: 48 independent AJV agreements, the five previously recorded
AJV scope/anchor differences and four unsupported AJV URI cases.

Validation: all eight generated resource variants (static, legacy, mixed,
dynamic, recursive, fragment4/6/7) pass both SDK languages, both artifact
formats, both native generators, strict Rust consumer lint and live HTTP/MCP
startup, discovery, calls and shutdown. Seven socket variants pass both typed
clients and 28 live roundtrips; raw AsyncAPI passes its official schema. The
broader actual TypeScript/native codec fixture still passes 13,908 cases.
All 34 shared compiler, 27 AsyncAPI, 5 Rust SDK, 8 TypeScript SDK, 18 codec,
7 transport and 1,090 CLI tests pass. Strict native lint, formatting, API docs,
890 packaged sources and security audit pass with no unexpected advisories.

Vendor validation: 151 referencing compatibility tests and all 1,992 available
default validator tests pass. The combined compatibility suite passes 1,986
checks; seven default-profile assertions explicitly require ignored legacy
siblings and are skipped only for that opt-in profile. They still run in the
default suite. Both profiles omit the previously documented test whose Git
submodule fixture is absent from the published archive. CI/provenance record
the exact exclusions; the unfiltered compatibility differences are preserved in
`fl-legacy-fragment-vendor-profile-differences.log`.
Evidence: `fl-legacy-fragment-{ajv,socket,codec,unit,final-unit,clippy,cli-tests,all-generated,referencing,vendor,vendor-default,docs,format,package,audit}.log`.

Checklist remains **651/682 (95.5%), 31 open**, with the remaining resource
boundaries now stated directly on the checklist. Eleven owned generated
fixtures from this batch are removed after commit (195 cumulative).


### 2026-10-13 — External AsyncAPI component/payload resource imports

`bundle_document_schema_resources` now extracts resource-aware schemas from
OpenAPI/AsyncAPI documents using the shared bounded graph. The resource index
recognizes standalone payloads, Multi Format schema wrappers and component
schemas. Bundled URI aliases resolve without retrieval. Resource-free documents
retain the existing contextual/literal bundler, preserving its broader legacy
annotation handling. The external AsyncAPI importer uses the new entry point.

Runtime dialect checking follows each message's reachable graph, so separate
messages can use different drafts without conflating constraints. A message
whose reference chain crosses incompatible dialects fails explicitly, including
alias roots whose `$schema` would otherwise overwrite the target dialect.
Duplicate IDs and missing remote resources also fail before output.

The expanded socket fixture compiles fourteen clients per language across
seven raw/fragment resource variants and their external-import counterparts.
Both languages pass 42 live roundtrips, strict models, constraint rejection,
official AsyncAPI schema checks, unchanged external source documents and three
negative resource-boundary cases. All five existing imported dialect clients
still pass live peers and 20 invalid-bound checks. Native tests pass: 27 AsyncAPI,
35 shared compiler, 5 Rust SDK and 8 TypeScript SDK; all 1,091 CLI tests pass.
Strict clippy, formatting, all-feature docs and 890 packaged sources pass.

New `rust_asyncapi_resources_runtime.sh` runs the relocated CLI import lifecycle
with component IDs: generation/check/watch, invalid-input recovery, strict
consumers, live peers, input ownership, rename/delete and HTTP/MCP startup,
discovery, calls and shutdown before/after rename. It passes and is included in
CI beside the resource-free variant. Evidence:
`fl-asyncapi-resources-{runtime,dialects,unit,clippy,cli-tests,generated,docs,format,package}.log`.
The committed generated fixture is cleaned (196 cumulative). Four build
artifacts proven to reference previously deleted owned fixtures were also removed.
Checklist remains **651/682 (95.5%), 31 open**; mixed runtime dialect translation
and combined URI/path-plus-fragment legacy IDs remain on the resource gate.

### 2026-10-13 — Complete schema applicator graph traversal

The shared graph now resolves references beneath schema dependencies, tuple
items, additional items, unevaluated properties/items and content schemas.
Every graph phase (indexing, alias checks, dependency analysis, expansion and
model lowering) follows those positions. Required-property dependency arrays
remain literal strings. Same-instance cycles through conditionals, negation
and schema dependencies fail explicitly; structurally guarded recursion remains
supported. This prevents dangling references after bundled definitions move.

Dialect-inactive keywords are excluded from graph/index processing and retain
literal reference locations during contract relocation. Generated TypeScript
codecs sanitize only their private compilation copy, preserving caller data and
explicit references into annotation storage. Standalone socket model compilation
now explicitly applies its Draft 7 default without changing implicit HTTP model
schemas or resource-free runtime payload annotations.

Validation: 75 original/bundled native cases, 66 independent AJV agreements,
five previously recorded AJV differences and four unsupported URI cases;
13,908 generated TypeScript/native and actual TypeScript validator comparisons;
strict ESM/CommonJS annotation regressions; 42 live roundtrips across both
resource-aware socket SDKs and external imports; 34 actual TypeScript metadata
comparisons and 63 parser documents (11 known limitations). Native affected
suites pass (20 contract, 37 shared compiler, 27 AsyncAPI, eight TypeScript SDK),
as do 1,093 CLI tests, strict clippy/docs and all 890 packaged sources.
The relocated dynamic-resource service passes HTTP/MCP startup, initialization,
discovery, invocation, Origin rejection and graceful shutdown, plus both SDKs
from contract/OpenAPI artifacts and native generators.
Evidence: `fl-schema-applicators-{unit,contract,ajv,typescript,annotations,sockets,metadata,cli,clippy,docs,package,generated}.log`.
Checklist remains **651/682 (95.5%), 31 open**: this advances the broader schema
constraint/resource gates without prematurely closing their remaining work.

### 2026-10-13 — TypeScript literal, tuple and intersection models

The shared native/CLI emitter now represents boolean false schemas as `never`,
object/array constants and enum members as literal types, legacy/2020 tuple
positions with required/optional slots and constrained tails, and `allOf` as
intersections including constraints on the containing schema. Empty object
constants reject primitive/array values. Runtime validators remain responsible
for constraints that TypeScript cannot express, including numeric bounds and
arbitrary array lengths; Rust heterogeneous arrays retain lossless JSON values.

New CI fixture `rust_schema_types_parity.mjs` compiles actual emitter output:
12 schemas, 20 accepted values and 32 statically rejected values. Each expected
result also matches native validation of the original and bundled schemas.
The existing 75-case resource fixture, 13,908 actual TypeScript/native codec
comparisons, live generated client suite, native SDK unit suites and all 1,093
CLI tests pass. Strict clippy/docs, formatting and 890 packaged sources pass.
Evidence: `fl-schema-types-{static,resources,runtime,unit,cli,clippy,docs,package}.log`.
The prior completed generated service workspace was removed (197 cumulative).
Checklist remains **651/682 (95.5%), 31 open**; broader constraint/emitter parity
still includes reference siblings and remaining model/runtime boundaries.

### 2026-10-13 — Validation siblings of schema references

The shared graph now imports validation siblings of a single static, dynamic or
recursive reference. It expresses the target as an `allOf` member while keeping
sibling constraints on the containing schema, preserving evaluated-property
annotations for `unevaluatedProperties` and existing intersection members.
Legacy dialect behavior follows the explicit ForkLaunch AJV compatibility
profile. OpenAPI Reference Objects outside schema positions retain their
separate annotation-only rules. Multiple reference applicators on the same
schema remain explicitly unsupported.

TypeScript intersections preserve the target and sibling model types. Rust
keeps a concrete referenced/typed intersection constituent when the surrounding
schema only adds validation constraints, with every constraint still enforced
by the runtime codec. Recursive typed HTTP responses remain usable.

The expanded resource fixture passes 108 original/bundled native cases and
99 independent AJV agreements, with the same five recorded AJV differences and
four unsupported URI cases. Strict TypeScript models pass 21 accepted and 35
rejected values across 13 fixtures. Sixteen generated clients per language,
including external AsyncAPI imports, pass 48 live socket roundtrips. Native SDK
and AsyncAPI suites, 1,093 CLI tests, strict clippy/docs, formatting and 890
packaged sources pass. A new `siblings` mode in the generated resource matrix
passes relocated HTTP/MCP startup, discovery, invocation, Origin rejection and
shutdown, plus typed Rust/TypeScript recursive clients from both artifact
formats and both native generators. Evidence:
`fl-schema-siblings-{parity,types,sockets,unit,models,cli,clippy,docs,package,generated}.log`.
Checklist remains **651/682 (95.5%), 31 open**; the schema gates still require
mixed runtime dialects, remaining reference profiles and emitter conformance.

### 2026-10-13 — Composed static/dynamic/recursive references

The graph evaluates every applicable reference keyword on a schema and retains
all targets in the containing intersection. Static plus 2020-12 dynamic or
2019-09 recursive references now preserve both constraints and recursive scope.
This follows the independent-applicator semantics in JSON Schema Core 2020-12
sections 7.2 and 8.2.3:
https://json-schema.org/draft/2020-12/json-schema-core#section-8.2.3.

The expanded fixture passes 120 original/bundled native cases, 107 independent
AJV default-mode agreements and 12 additional AJV allErrors-mode comparisons.
Four new recorded default-mode AJV differences arise from its generated dynamic
reference validity variable being shadowed: it skips the adjacent static
constraint, while allErrors mode and native evaluation correctly enforce both.
Together with prior cases, nine AJV behavior differences and four unsupported
URI cases remain explicit. Bundled generated schemas enforce both constraints
with either AJV mode.

Twenty generated clients per language, including external imports, pass 60 live
socket roundtrips and reject payloads that violate only the static sibling.
The generated `composed` HTTP service mode passes typed recursive clients through
both artifact formats/native generators plus HTTP/MCP startup, discovery,
invocation, Origin rejection and graceful shutdown. Native suites, 1,093 CLI
tests, strict clippy/docs, formatting and 890 packaged sources pass. Evidence:
`fl-schema-composed-{parity,sockets,unit,cli,clippy,docs,package,generated}.log`.
The first generated attempt exposed a fixture assertion that assumed one
reference keyword; the assertion now checks both and the complete rerun passes.
Checklist remains **651/682 (95.5%), 31 open**. Multiple-reference composition is
no longer a remaining schema-profile boundary.

### 2026-10-13 — Legacy URI/path plus named-fragment IDs

Draft-4/6/7 IDs such as `nodes#Word` now resolve as exact offline aliases in the
shared graph and native codec. Distinct aliases on one path retain independent
cached targets. Recursive aliases and nested fragment-only IDs work through
borrowed and owned registries. An alias does not invent the bare `nodes`
resource or a JSON-pointer base; those missing targets still fail explicitly.
Modern nonempty ID fragments and duplicate aliases remain rejected.

Native support uses the opt-in `forklaunch-legacy-fragment-ids` vendor feature,
forwarded through jsonschema and enabled by the codec. Registry retrieval skips
already indexed exact aliases, and validator cache identity preserves their full
URIs. Upstream defaults remain unchanged. CI and both vendor provenance files
record the feature and dedicated regressions.

Validation: 144 original/bundled native cases, 131 independent default-mode AJV
agreements, the same nine recorded AJV behavior differences/four unsupported URI
cases, and 12 additional allErrors-mode comparisons; 152 resolver compatibility
tests, 1,987 validator compatibility tests, 1,992 default validator tests and the
isolated legacy-feature regression. Async retrieval feature compilation passes.
The generated matrix covers 26 clients per language and 78 live socket
roundtrips, including external AsyncAPI imports. All three new generated HTTP
modes (`uri4`, `uri6`, `uri7`) pass typed recursive SDKs from both artifact
formats/native generators plus HTTP/MCP startup, discovery, invocation, Origin
rejection and shutdown. The 13,908 actual TypeScript/native codec comparisons,
native affected suites (27 AsyncAPI, 38 shared compiler, five Rust SDK, eight
TypeScript SDK, 18 codecs and seven transports), 1,094 CLI tests, strict
clippy/docs, formatting and 890 packaged sources pass.
Evidence: `fl-schema-uri-fragment-{parity,referencing,vendor,default,isolated,async,sockets,typescript,unit,cli,clippy,docs,package,generated-uri4,generated-uri6,generated-uri7}.log`.

Checklist remains **651/682 (95.5%), 31 open**. The resource gate now identifies
mixed runtime dialects and AJV-permissive legacy fragment spellings as remaining
work: an additional direct AJV probe accepts pointer-shaped, digit-leading and
underscore-leading legacy IDs that the shared compiler still rejects. This
receipt covers named fragments within the existing profile; it does not claim
those broader spellings are implemented.

### 2026-10-13 — Legacy fragment spellings and URI normalization

Legacy IDs now support pointer-shaped fragments, digit/underscore names and
percent-encoded names through the shared compiler/native codec. Exact registered
IDs take precedence over document pointers. URI normalization decodes unreserved
characters and normalizes escape case across paths, queries and fragments while
preserving encoded delimiters: `#%2Falias` and `#/alias` remain separate targets.
Normalized duplicate aliases fail explicitly. Encoded-slash references without
an explicit matching alias still obey the checked JSON-pointer boundary.

Registry object analysis now applies the existing AJV sibling-ID policy during
indexing as well as ID extraction/compilation, so a legacy ID beside `$ref` can
be referenced elsewhere. Both borrowed and owned registry paths are covered.
The opt-in vendor features retain default upstream behavior; malformed URI
syntax is outside this compatibility profile.

Validation: 240 original/bundled native cases, 227 default-mode AJV agreements,
the same nine recorded AJV differences/four unsupported URI cases, and 12
additional allErrors-mode comparisons. The expanded matrix compiles 32 clients
per language and passes 96 live socket roundtrips, including external imports.
Three generated `pointer4`/`pointer6`/`pointer7` service variants pass HTTP/MCP
startup, discovery, invocation, Origin rejection and shutdown, typed recursive
SDKs through both artifact formats and both native generators. All 13,908 actual
TypeScript/native codec comparisons and the generated live client suite pass.
Resolver suites pass 153 compatibility/150 default tests; validator suites pass
1,987 compatibility/1,992 default tests. Affected native suites, all 1,095 CLI
tests, strict clippy/docs, formatting and 890 packaged sources pass. Evidence:
`fl-schema-permissive-{parity,sockets,typescript,referencing,referencing-default,vendor,default,unit,cli,clippy,docs,package,generated-pointer4,generated-pointer6,generated-pointer7}.log`.
Checklist remains **651/682 (95.5%), 31 open**. Mixed-dialect runtime translation
is the remaining identified resource-import boundary.

### 2026-10-13 — Mixed runtime resource drafts and socket unions

The runtime graph now collects reachable dialects and translates mixed Draft
4/6/7/2019-09/2020-12 resources to 2020-12. References bind against the original
resource index before conversion, preserving tuple-pointer identities, lexical
IDs and dynamic/recursive bindings. Draft 4 boolean exclusive bounds become
numeric bounds; legacy tuple items and additionalItems become prefixItems and
items. Existing inactive-keyword filtering preserves older draft behavior.
Only reachable resources trigger conversion, unsupported reachable dialects
fail explicitly, and caller-owned schemas remain unchanged. Message unions
also convert differing standalone drafts before relocating their references.

The new CI fixture checks 201 original/translated native cases, including all
20 ordered draft pairs, recursion, bounds, tuple tails, property dependencies,
inactive annotations and evaluated locations. Independent per-resource AJV
engines verify original constraints; AJV 2020 agrees with 198 translated cases.
Three cases explicitly record AJV's pre-existing behavior of marking every
array item evaluated after contains, versus native matching-index tracking.
This difference is reproducible without translation and remains part of the
separate schema-constraint/emitter gate; resource support does not imply all
validator implementations agree.

The expanded socket fixture compiles 36 clients per language and passes 108
live roundtrips, including mixed resources, mixed message unions and external
AsyncAPI imports. The generated mixed-schema service passes both artifact
formats, both native SDK generators, typed Rust/TypeScript consumers and live
HTTP/MCP startup, discovery, invocation, Origin rejection and shutdown.
The existing 240 resource cases and 12 composed-reference allErrors checks,
13 TypeScript model fixtures (21 accepted/35 rejected values), 13,908 actual
TypeScript/native codec comparisons, 40 shared compiler/5 Rust SDK/8 TypeScript
SDK unit tests, 1,096 CLI tests, strict clippy and 890 packaged sources pass.
Evidence: `fl-schema-mixed-{sockets,http,types,codecs,units,cli-final,clippy-final,package}.log`.

The resource-import gate is complete for the documented offline profiles.
Checklist: **652/682 (95.6%), 30 open**. Broader validation/emitter, provider,
module, deployment and publishing gates remain open.

### 2026-10-13 — Ordered HTTP version selection and MCP projections

Rust version declarations now retain insertion order and JavaScript's canonical
integer-key enumeration order. `RouteContract::for_version` projects request
and response metadata while preserving common params, path and auth settings.
Local and live Axum routers compile reusable version plans at construction,
choose the last matching request and validate the selected response status.
Handlers receive the selected version and every matching name. Embedded JSON
schemas use the bounded/offline validator moved from the socket codec into the
optional contract schema-validation module; socket diagnostics remain compatible.
The metadata-only Axum adapter also selects versions and remains compatible with
callers mutating its public application registry.

MCP exports one tool per version with independent input/output schemas and
version-qualified names for multiple declarations. Transport bindings project
versions alongside tool definitions, preserving route authentication and regular
HTTP matching. A call through an earlier tool can select a later overlapping
HTTP version, as in TypeScript.

Validation: 35 actual TypeScript-parser/native selection cases cover overlaps,
rejection, declaration order and canonical/noncanonical integer keys. A live
loopback service passes HTTP requests, MCP initialization, discovery and both
versioned calls, then graceful shutdown. All 134 affected native library tests
(55 Axum, 20 contract, 22 HTTP, 19 MCP, 18 socket), 1,096 CLI tests and strict
clippy pass; the blueprint sample compiles. Evidence:
`fl-version-{actual-parity,live-tests,units,cli,clippy,blueprint}.log`.
All 13,908 generated/actual TypeScript/native codec comparisons and the generated
live TypeScript socket suite pass after sharing the validator. Strict docs and
894 embedded package sources pass (`fl-version-{codecs,docs,package}.log`).

Checklist stays **652/682 (95.6%), 30 open**. This is the runtime/MCP portion of
versioning; OpenAPI export, SDK projections and non-JSON media/coercion coverage
still prevent closing the versioned-API gate.

### 2026-10-13 — Versioned OpenAPI exports and typed SDK methods

Standard OpenAPI operations now project the last declared version and retain all
request/response contracts in `x-forklaunch-version-contracts`, alongside the
existing version-name list. `generate_versioned_openapi_json` exports independent
named documents with unversioned routes under the empty key. Components and
security metadata are generated per document, so repeated schema names across
versions retain their own meanings.

CLI imports preserve full version metadata from standard OpenAPI and import all
named documents with reference resolution isolated to each document. Contract
and OpenAPI imports share bounded version expansion with native SDK method
naming: `write` plus `v1`/`v2` becomes `writeVersionV1`/`writeVersionV2` and the
corresponding Rust snake-case methods. Every method retains its request schema,
response schema/status and common params. Colliding normalized aliases fail
explicitly, following the existing generated-identifier profile. Shared metadata
clones obey the graph byte/node budget; a repeated-large-params regression proves
version expansion cannot bypass the cap. Native projections also avoid cloning
all sibling versions for every projected route.

A fresh relocated generated service passes typed Rust and strict TypeScript
consumers through contract, standard OpenAPI and named OpenAPI imports, followed
by both native generators. Each of the four passes starts HTTP/MCP, initializes
and discovers versioned tools, invokes both versions, rejects a mismatched body,
checks distinct tool schemas and shuts down gracefully. The TypeScript consumers
also reject wrong-version body types at compile time. The E2E workflow includes
this fixture. Earlier fixture-only method-name mismatches were corrected before
the complete fresh run.

Validation: 1,099 CLI tests; 20 contract/22 HTTP/7 OpenAPI/43 shared compiler tests;
strict clippy/docs, formatting, and 898 embedded package sources. Evidence:
`fl-version-generated-complete.log`,
`fl-version-export-{cli-final,units,clippy-final,docs,package-final}.log`.
Checklist remains **652/682 (95.6%), 30 open**. Non-JSON media and
request-transformation conformance remain on the versioned-API gate.

### 2026-10-13 — Versioned text bodies and shared parser registration

Versioned routes now derive their wire parser from the first declared version,
including an absent first body, instead of the unversioned base contract. Local
and metadata adapters normalize empty text and preserve binary bytes before
selection. Registration rejects incompatible parser kinds; file/binary share a
parser, and JSON/unknown share a parser. Encoded schemas across a route's versions
have a combined 32 MiB construction budget before JSON decoding.

Validation: 40 actual TypeScript request-parser/native selection comparisons and
six actual TypeScript registration/native parser-compatibility checks; 56 Axum
and 23 HTTP unit tests; strict all-feature/all-target Clippy for both crates; and
898 embedded package sources. A fresh relocated generated service passed empty,
short and Unicode text HTTP/MCP calls, JSON version calls, startup/discovery,
rejection and graceful shutdown. Both typed SDKs passed all three artifact
sources (contract, standard OpenAPI, named OpenAPI) and both native generators.
The initial generated probe assumed a tool name rather than using discovery;
the corrected complete run is recorded in
`/private/tmp/fl-version-media-generated-final.log` (`run.5wqn6V`). Other receipts:
`fl-version-media-actual.log`, `fl-version-media-units.log`,
`fl-version-media-clippy.log`, and `fl-version-media-package.log`.

Checklist stays **652/682 (95.6%), 30 open**. Binary/form/multipart schema matching,
full middleware coercion/transform behavior and the broader versioning gate
remain open. These parser tests compare actual TypeScript request validation;
they do not claim all Express wire-parser behavior is identical.

### 2026-10-13 — Versioned opaque files and native byte constraints

Version matching no longer decodes file/binary bytes as UTF-8. The standard
opaque OpenAPI binary-string shape (with descriptive annotations) accepts and
preserves every byte. Native array and boolean schemas validate the byte-array
representation. Unsupported assertions on opaque string schemas fail at route
construction instead of silently losing their constraints. JSON/unknown parser
aliases also consistently use JSON during matching.

Validation: 45 actual TypeScript/native request-selection comparisons (including
five actual file-validator Buffer inputs) and six parser registration checks;
57 Axum and 24 HTTP tests; strict Clippy; and 898 embedded package sources.
A fresh relocated generated service (`run.ZkUoe6`) passed raw HTTP file calls
with empty/non-UTF-8/Unicode bytes and MCP file calls with representable strings,
plus JSON/text calls, both typed SDKs through all three artifact sources and both
native generators, startup/discovery and graceful shutdown. Receipts:
`/private/tmp/fl-version-file-{actual,units,clippy,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. This closes the opaque file
matching defect, not all media semantics: arbitrary transformations, the separate
TypeScript base64-string binary validator, multipart/form matching and full
wire-parser conformance remain tracked. MCP string-file calls do not establish
arbitrary binary MCP transport parity.

### 2026-10-13 — Versioned flat URL-encoded forms and shared body decoding

Version selection parses URL-encoded flat fields and repeated string arrays into
schema values, preserving original handler bytes. It follows qs repeated-key
grouping, canonical integer-key ordering and malformed percent/UTF-8 escape
fallback. Bracket-array names are supported; nested/indexed paths are rejected
explicitly. Parsing has a 1,000-parameter bound. JSON/text/byte-array body decoding
now happens once per request, shared by all candidate versions.

Validation: 29 comparisons against the repository's actual Express content
middleware, including raw-body capture, repeated keys, bracket arrays, Unicode,
malformed escapes and prototype names; the existing 45 request-selector and six
registration comparisons; 57 Axum/25 HTTP tests; strict Clippy; 899 embedded
package files. Generated `run.l5FVJF` passed HTTP and MCP form version calls,
invalid-form rejection, all previous media cases, both typed SDKs through three
artifact sources and both native generators, startup and graceful shutdown.
The wire-parser fixture is included in Rust parity CI. Receipts:
`/private/tmp/fl-version-form-{actual-final,regression,units,clippy,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. Nested/indexed forms, multipart
version selection, configurable parser options and broader transformations are
still incomplete. MCP form arrays retain TypeScript's existing JSON-composite
serialization; this batch does not change that transport contract.

### 2026-10-13 — Nested URL-encoded version selection

The form matcher now handles nested bracket paths, sparse indexed arrays,
array/object/scalar collisions, array overflow objects, balanced and unterminated
bracket names, prototype keys and equals signs inside bracket keys. Arrays compact
only after merging, matching qs. Input depth is bounded at 32 bracket levels and
merged output at 64 levels; the 1,000-parameter limit remains. Numeric indices use
the exactly representable JavaScript integer range.

Validation: **1,060 actual TypeScript content-middleware/native comparisons**,
including every triple from ten collision/overflow/prototype fragments; 57 Axum
and 25 HTTP tests; strict Clippy; and 900 embedded package sources. Generated
`run.cjSiEE` adds a third form version requiring a nested array and verifies both
bracket-array and sparse-index HTTP inputs select it while preserving wire bytes.
The full generated HTTP/MCP matrix, both typed SDKs through three artifact forms,
both native generators, startup/discovery and graceful shutdown pass. Receipts:
`/private/tmp/fl-version-nested-{actual-final,units,clippy,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. Nested forms no longer block the
standard UTF-8 selection profile. Multipart matching, configurable parser options,
non-UTF-8 charsets, parser-limit error-status equivalence and arbitrary request
transformations remain incomplete; this is not a claim of every Express option.

### 2026-10-13 — Buffered multipart version matching and generated SDK lint fix

Version matching now parses buffered multipart text fields and native byte-array
file fields once per request. The matcher shares `MultipartLimits` with the
existing typed Axum extractor and honors extension overrides; overwritten parts
count toward all limits. Parsing polls immediately ready memory without entering
an executor or waiting for I/O. Handler wire bytes are preserved. HTTP now depends
on the same multer parser already used by Axum; both workspace locks are updated.

Validation: seven actual TypeScript multipart parser/native version-selection
comparisons and the existing seven typed-extractor comparisons; 58 Axum and 26
HTTP tests (including malformed bodies and custom limits); five Rust SDK tests;
strict Clippy and blueprint dependency checks; 901 embedded package files. The
new generated multipart routes exposed nested-condition lint errors in the
shared Rust SDK template's empty-form delimiter handling. The template now uses
an Option filter, retaining compatibility without an edition-specific let chain.
Fresh generated `run.0xntgf` passes multipart HTTP/MCP calls, raw attachment byte
preservation, the full prior media matrix, both SDKs through three artifact forms
and both native generators, startup/discovery and graceful shutdown. Receipts:
`/private/tmp/fl-version-multipart-{actual-final,units,clippy,blueprint,sdk-tests,generated-final,package-final}.log`.

Checklist remains **652/682 (95.6%), 30 open**. Native multipart byte-array schema
matching is complete for this profile; nested OpenAPI opaque-file annotations,
parser-limit error-status equivalence, charset/configurable parser behavior and
arbitrary transformations remain open. Mixed text/file duplicate-name ordering
retains the already documented nonportable TypeScript behavior.

### 2026-10-13 — Parser failure statuses and disabled-validation behavior

Body parsing now distinguishes malformed input (400), form/multipart limits
(413), and invalid multipart configuration (500). Failures remain fatal when
schema validation is None/Warning and when a body has only a named schema.
Local, metadata and live Axum adapters preserve these distinctions. Streamed
Axum body-limit errors are identified through their typed error chain and return
413; unrelated read errors remain 400 and do not expose I/O details. Public error
classification adds body parser/size/configuration locations and PayloadTooLarge.
Both workspace locks include Axum's direct http-body-util dependency.

Validation: **1,062 actual TypeScript form-parser/native comparisons**, including
parameter-limit 413 and nesting-limit 400; 59 Axum and 27 HTTP tests; strict
all-feature/all-target Clippy; blueprint checks; and 901 embedded package files.
Fresh generated `run.ctLYC7` verifies error statuses with the full HTTP/MCP media
matrix and both SDKs through three artifact forms/native generators, startup,
discovery and graceful shutdown. Receipts:
`/private/tmp/fl-version-parser-status-{actual,units,clippy,blueprint,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. The standard parser-limit status
gap is resolved. The versioning gate still includes nested opaque-file schema
annotations, configurable parser/charset behavior and request transformations;
this receipt does not claim every TypeScript error payload or middleware hook.

### 2026-10-13 — Multipart opaque-file annotations and legacy File artifacts

Unscoped multipart file schemas now validate the parser's byte-array or text
values, matching the actual TypeScript file validator. Lowering reuses the
bounded contract bundler and schema-location walker, including references stored
inside literal metadata; exported schemas and const/default/example data remain
unchanged. It recognizes the actual legacy File artifact's exact instance-guard
oneOf marker. Other string assertions and differing oneOf shapes are rejected
instead of discarded. Resource-scoped schemas retain their previous validator
path; opaque-file lowering in that profile remains open.

Validation: **127 actual TypeScript multipart parser/file-validator/native
comparisons**, covering five drafts, direct properties, definition references,
literal-metadata references and text-file inputs; seven existing typed-extractor
comparisons; 59 Axum and 29 HTTP tests; strict Clippy; and 902 embedded package
files. Generated `run.Paq2su` now declares its attachment as an opaque binary file
and passes the full live HTTP/MCP media matrix, raw binary attachment preservation,
both SDKs through three artifact forms and native generators, startup/discovery
and graceful shutdown. Receipts:
`/private/tmp/fl-version-multipart-schema-{actual,units,clippy,generated,package}.log`.

Checklist is still **652/682 (95.6%), 30 open**, verified from the checklist file.
Remaining versioning work is narrowed to scoped opaque-file schemas, configurable
parser/charset behavior and request transformations.


### 2026-10-13 — Resource-scoped multipart file assertions

Multipart opaque-file assertions now compile at their original resource scope,
with IDs, anchors, literal-metadata references and mixed resource drafts intact.
The opt-in validator hook preserves the source registry and exported schemas;
evaluated-item/property tracking sees the adapted assertions. Default JSON and
socket validation remain unchanged. Rewritten nodes stay owned through compile
cache teardown, and source-only optimizations are disabled for this profile.

Validation: **271 actual TypeScript parser/file-validator/native comparisons**
across five drafts and scoped/unscoped reference layouts, plus seven typed
multipart extractor comparisons; 24 contract, 27 HTTP and 59 Axum unit tests;
strict Clippy; 1,995 default and 1,990 compatibility vendor tests; 13,908 actual
TypeScript/AJV/native codec comparisons; 201 mixed-draft cases (198 AJV agreements,
three recorded existing contains/unevaluatedItems differences); 902 package files.
Generated `run.8VOTWn` uses a resource-scoped anchored attachment and passes HTTP
and MCP startup, discovery, calls, raw byte preservation, both typed SDKs through
all three artifacts and native generators, and graceful shutdown. Receipts:
`/private/tmp/fl-scoped-{actual,native,clippy,vendor-default,vendor-compat,codec,mixed,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. The scoped-file gap is closed;
configurable parsers/charsets and request transformations still keep the broader
versioning gate open.


### 2026-10-13 — Default JSON/text wire parsing in version selection

Version matching now uses the TypeScript defaults for strict JSON roots,
empty JSON bodies, one leading UTF-8 BOM and malformed UTF-8 replacement. Parser
failures remain fatal for unconstrained schemas. Wire bytes are preserved;
standard Axum String/Json extractors still use their own decoding rules, so this
receipt does not claim parsed-value handler delivery or arbitrary transformations.

Validation: 23 actual TypeScript content-middleware/native wire comparisons,
45 existing version cases and six registration checks; 59 Axum and 28 HTTP tests;
strict Clippy; 902 package files. Fresh generated `run.Ux0dcU` verifies BOM-aware
selection with intact handler bytes, the full HTTP/MCP startup/call/shutdown
matrix, and both SDKs through three artifact forms and native generators. The
new comparison runs in CI. Receipts:
`/private/tmp/fl-wire-{actual,versions,native,clippy,generated,package}.log`.
Checklist remains **652/682 (95.6%), 30 open**; parser configuration/charsets and
request transformation delivery remain open.


### 2026-10-13 — Parsed version bodies reach typed handlers

`VersionedBody<T>` deserializes the value already parsed and validated during
version selection, without consuming or rewriting request bytes. Local and
metadata handlers receive the shared value on `SelectedVersion.parsed_body`.
The live adapter keeps selection metadata on both framework requests and Axum
extensions; empty JSON parsed as `{}` no longer fails a later missing-body check.
The extractor supports JSON, text, file byte arrays, nested forms and multipart.
Missing selection/body metadata and incompatible handler types reject explicitly
without exposing payloads in errors. Custom schema transforms and unversioned
parsed-body delivery remain outside this change.

Validation: 61 Axum and 29 HTTP tests, including every parser with simultaneous
parsed/raw access and extractor rejection cases; strict Clippy; 23 actual wire
comparisons; 1,062 actual URL-encoded comparisons; 903 package files. Fresh
`run.qzu84p` verifies BOM-prefixed JSON and malformed UTF-8 text through generated
typed handlers, HTTP/MCP startup/discovery/calls/shutdown, and both SDKs through
three artifacts and native generators. Receipts:
`/private/tmp/fl-parsed-{native,clippy,actual,form,generated,package}.log`.
Checklist remains **652/682 (95.6%), 30 open**; configurable parser/charset behavior
and arbitrary request transformations remain on the versioning gate.


### 2026-10-13 — Request charset decoding and unsupported-encoding status

Versioned JSON/text/form matching and parsed handler values honor Content-Type
charsets. JSON requires `utf-`; forms accept UTF-8 and ISO-8859-1 with Latin-1
percent decoding; text supports Unicode endian/autodetection variants and the
encoding_rs legacy tables. Unknown/disallowed encodings return 415 as fatal parser
errors. Original wire bytes remain intact. Multibyte decoding retries the byte
after a failed lead; UTF-32 trailing bytes produce the TypeScript replacement
character. UTF-8 forms now follow body-parser replacement/BOM behavior before qs
percent decoding. Both workspace locks include the direct decoder/MIME edges.

Validation: **128 actual TypeScript content-middleware/native parsed-value and
status comparisons**, including UTF-8/16/32, endian detection, quoted charset
labels, Latin-1, ASCII, Windows-1252, Shift-JIS, GB18030, Big5 and KOI8-R; 1,062
existing form comparisons; 61 Axum and 29 HTTP tests; strict Clippy; blueprint
check; 904 package files. Fresh `run.SUIJG0` passes live decoded UTF-16 JSON/text,
Latin-1 text/forms, 415 rejection and the full HTTP/MCP/SDK matrix across three
artifacts and native generators. Receipts:
`/private/tmp/fl-charset-{actual,form,native,clippy,blueprint,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. Every iconv alias/table, unpaired
surrogate behavior, custom charset defaults, form charset-sentinel options and
arbitrary transformations are not claimed complete.


### 2026-10-13 — Configurable versioned JSON/text/form parsers

`RouteOptions.body_parser` now configures JSON strict mode, JSON/text/form default
charsets, form extended mode, depth and parameter limits across every version.
Explicit Content-Type charsets override defaults. Flat parsing retains bracketed
keys; configured depth overflow is 400 and parameter overflow is 413. Invalid
default charsets and unsupported limits fail registration (depth <=32, parameters
1..=100,000). Non-default settings are exported in contract artifacts; defaults
preserve the prior shape.

Validation: **170 actual TypeScript content-middleware/native value/status
comparisons**, 1,062 existing form comparisons, 24 contract/30 HTTP/61 Axum unit
tests, strict Clippy and 905 embedded package files. Fresh generated `run.dbD25l`
includes a non-strict scalar JSON route, verifies custom/default artifact metadata,
HTTP and MCP scalar calls, every previous media/charset check, both SDKs through
three artifact formats and native generators, startup/discovery and shutdown.
Receipts: `/private/tmp/fl-parser-options-{actual,form,native,clippy,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. Compression, per-parser byte limits,
verify/reviver hooks, form sentinel/entity options, complete legacy codec coverage
and arbitrary schema transformations remain open; this configuration currently
applies to the versioned pipeline.


### 2026-10-13 — Bounded compression and per-parser byte limits

Versioned JSON/text/form/raw parsers now configure byte limits and inflation
independently. Defaults match body-parser's 100 KiB decoded limit with inflation
enabled. Identity, gzip (including concatenated members), zlib deflate and Brotli
are decoded with at most limit+1 output bytes. Unknown/disabled encodings return
415, malformed streams return 400 and decoded-limit failures return 413. Runtime
configuration caps limits at 32 MiB; multipart keeps its own limits and transport
limits still bound wire bytes. Raw request bytes remain original, while selected
parsed values contain decompressed data (TS verify buffers are decompressed).

Validation: **267 actual TypeScript parser/native value/status comparisons**,
24 contract/30 HTTP/61 Axum tests, strict Clippy, blueprint check, no new unreviewed
advisories across all three locks, and 906 packaged files. Fresh `run.ttVFp4`
passes compressed JSON/text/file HTTP calls for every encoding, expansion-limit
rejection, configured text limits and disabled scalar inflation, all HTTP/MCP
startup/discovery/call/shutdown checks, and both SDKs through three artifact forms
and native generators. Receipts:
`/private/tmp/fl-wire-limits-{actual,native,config,clippy,blueprint,audit,generated,package}.log`.

Checklist remains **652/682 (95.6%), 30 open**. Custom verify/reviver hooks, form
sentinel/entity options, complete legacy codec coverage and schema transformations
remain on the broader parser/versioning work.


### 2026-10-13 — Form charset sentinels and numeric entities

Versioned form options now support charsetSentinel and interpretNumericEntities.
The first literal `utf8=` part is removed and, for a recognized sentinel, selects
UTF-8 or Latin-1 percent decoding. It still counts against parameter limits.
Numeric entities affect Latin-1 values only, preserving keys, Number rounding,
UTF-16 code-unit wrapping and paired surrogates. Both options default to false;
unpaired-surrogate representation remains the previously documented limitation.

Validation: **404 actual TypeScript parser/native comparisons**, including both
sentinel orders, unknown/lowercase sentinels, all option combinations, repeats,
large/overflowing entities, pairs, and sentinel-inclusive limits; 24 contract,
30 HTTP and 61 Axum tests; strict Clippy; 906 package files. Fresh `run.TaomOs`
proves sentinel/entity decoding changes the selected HTTP version while preserving
wire data, and passes all HTTP/MCP/media/compression checks, both SDKs across three
artifacts and native generators, startup/discovery and graceful shutdown.
Receipts: `/private/tmp/fl-form-options-{actual,native,clippy,generated,package}.log`.
Checklist remains **652/682 (95.6%), 30 open**. Verify/reviver hooks, schema/request
transformations and complete legacy codec/invalid-string coverage remain open.

### Native per-version body hooks and adapter ownership (2026-10-13)

Native callbacks now preprocess each version's original parsed input before schema
checks and transform successful candidates afterward. Rejections are isolated to
the candidate; a later failed transform preserves the earlier successful version.
Direct/local/metadata adapters expose the selected transformed value and preserve
raw bytes. Nested live routers retain hooks. Local handlers served through Axum
own selection/response validation, so callbacks run once; transport authentication
and byte limits still run before local dispatch. Live/metadata registries reject
missing or ambiguous method/operation registrations.

Validation: 45 actual TypeScript version comparisons, six parser registration
checks and **18 actual Zod preprocessing/transformation comparisons**; 404 parser
wire comparisons; 24 contract, 31 HTTP and 63 Axum tests plus three integration
tests; strict Clippy; 907 package files. Fresh `run.iFWY4d` passes generated HTTP
and MCP transformation/fallback/rejection, both typed SDK transformation calls
across contract/OpenAPI/named OpenAPI and native generators, startup/discovery,
Origin rejection and graceful shutdown. The multi-route native exporter fixture
now preserves distinct schema names rather than accidentally sharing components.
Receipts: `/private/tmp/fl-body-hooks-{actual,wire,native,clippy,package}.log` and
`/private/tmp/fl-body-hooks-generated-final.log`.

Runtime callbacks are explicit native code, not serialized JavaScript. Parser
verify/reviver, missing-body defaults, non-body transformations and the documented
codec/string representation boundaries remain open. Checklist **652/682 (95.6%),
30 open**; this completes a portion of the broad versioning gate.

### Shared body-parser verification callbacks (2026-10-13)

`BodyParserHooks::with_verify` and `insert_parser` provide native verification of
bounded, decompressed bytes before character decoding/parsing. Callbacks receive
request metadata and the charset (absent for raw/file). They run once across all
versions, including empty bodies; size/encoding errors take precedence. Failure
returns a fixed 403 under Error/Warning/None validation modes without leaking
callback details. Multipart/SSE registrations are rejected. Runtime hooks remain
native/read-only and are not serialized as JavaScript functions.

Validation: **484 actual TypeScript/native parser comparisons** (80 new verify
cases), including all four parsers, gzip/deflate/Brotli, UTF-16, empty bodies,
strict/invalid JSON and bounded-byte precedence. 24 contract, 31 HTTP, 64 Axum
and three integration tests pass, including once-only direct/local adapter calls
under every validation mode. Strict Clippy and 908 package files pass. Fresh
`run.9eT6Uy` passes generated HTTP/MCP verification, compressed success/rejection,
private-error omission, both typed SDKs across three artifact forms/native
generators, startup/discovery/Origin rejection and graceful shutdown.
Receipts: `/private/tmp/fl-parser-hooks-{actual,native,clippy,package,generated}.log`.
Checklist remains **652/682 (95.6%), 30 open**. JSON reviver and non-body/missing-body
transformation semantics remain in the broad versioning gate.

### Native JSON reviver traversal (2026-10-13)

`BodyParserHooks::with_json_reviver` applies child-first transformations once,
before version-specific callbacks. It preserves JavaScript numeric-key ordering,
other source-key order and duplicate-key last-value/original-position behavior
through an ordered parser, without changing global serde_json map features.
Object properties and the root can be removed; empty decoded bodies bypass the
reviver. Callback failures return a fixed 400. Traversal is depth-bounded.

Validation: **513 actual TypeScript/native parser comparisons**, including 29 new
reviver fixtures with ordered callback effects, duplicate keys, nested arrays,
property removal, compression, primitive roots, empty/BOM input and errors.
24 contract, 34 HTTP, 64 Axum and three integration tests pass, plus strict
Clippy, blueprint-core compilation and 909 source-package files. Fresh `run.NtsKUh`
passes HTTP/MCP reviver transformation/rejection, private-error omission, all prior
media/parser/verification checks and both SDKs across three artifact forms and
native generators, including service startup/discovery and graceful shutdown.
Receipts: `/private/tmp/fl-reviver-{actual,native,clippy,blueprint,package,generated}.log`.

Sparse array deletion explicitly returns 400: native JSON values cannot represent
JavaScript holes. Mutable reviver holders/source context, broader undefined/default
validation and non-body transformations remain open. Root deletion is an absent
parsed value and requires a compatible contract/handler; the typed required-body
extractor requires a present value. Checklist **652/682 (95.6%), 30 open**.


### Mixed-app live TypeScript SDK imports (2026-10-13)

Live SDK mode now accepts a mixed workspace containing registered TypeScript
projects, removes only their declaration entry points, and leaves native package
metadata and generated SDKs intact. Generated mode restores declaration resolution
without parsing Tokio as a JavaScript runtime. Shared root tsconfig/editor tasks
remain application-owned in hybrid workspaces. Pure Rust live mode fails before
mutation. Legacy TypeScript-only Node/Bun behavior is covered by a regression test.

Fresh relocated `rust-mixed-sdk-runtime/run.77wpkisb` proves strict TypeScript
package imports observe source edits immediately, generated/live round trips,
dry-run preservation, unchanged native artifacts and shared configuration. The
same fixture passes SDK watch recovery, service lifecycle preservation, native
HTTP/MCP startup/endpoint modes/Origin rejection/graceful shutdown, and both SDKs
calling real native and tsx HTTP services. All **1,100 CLI tests pass**. Receipts:
`/private/tmp/fl-live-sdk-{generated,tests,cli-all}.log`.

This closes the direct live TypeScript import gate: **653/682 (95.7%), 29 open**.
It does not make Rust source directly importable from TypeScript; native services
continue using their generated contract clients.


Better Auth rate-limit customization now supports positive default windows/budgets
and exact auth-relative path overrides, including endpoint disablement. The
standard `rust_better_auth_rate_limits.mjs` entry point runs 153 default and 174
configured HTTP comparisons against actual Better Auth 1.7.6. Sensitive built-in
rules override defaults; explicit endpoint settings override both. Retry headers,
rolling boundaries, path normalization, parser/guard ordering and atomic request
counting are preserved. Configuration rejects zero budgets/windows and unsupported
wildcard/query/encoded paths. Dynamic rule callbacks, wildcard rules and external
storage remain open; this narrows the existing customization gate without changing
the broad checklist completion count.

Validation receipt: 36 portable auth tests pass (the dedicated PostgreSQL unit
test remains explicitly ignored in that run), strict all-feature/all-target auth
clippy passes, and fresh generated run `cK6KDS` passes against restricted-role
PostgreSQL. Eight actual frontend client scenarios, both generated SDKs,
HTTP/MCP shared/separate/disabled listeners, Origin rejection, restart/shutdown,
relocation, preservation, rename and deletion all pass.


Better Auth rate-limit wildcard rules now match upstream first-match insertion
order, including wildcard-before-exact and exact-before-wildcard precedence,
endpoint disablement, `*` segment boundaries, whole-segment `**`, and `?` within
wildcard patterns. Replacing a configured pattern preserves its original order.
The shared trusted-origin wildcard compiler is reused and compiled once per
configured rule. The standard rate-limit fixture now passes 969 actual HTTP
comparisons across default, exact and three wildcard profiles; all 2,822 actual
trusted-origin/callback/CSRF comparisons remain green. Dynamic callbacks and
external storage remain open under the existing customization gate.

Wildcard validation receipt: 37 portable native auth tests and strict all-feature,
all-target clippy pass, including live HTTP/MCP startup/shutdown and concurrent
rate limiting. The PostgreSQL-specific unit test remains explicitly ignored.


Native Better Auth custom rate-limit callbacks now await asynchronous decisions
from method, original URI, normalized auth-relative path and headers. Each callback
receives the inherited sensitive/default rule and can retain it, disable the limit,
or select a positive window/budget. Only the first matching rule executes; counter
locks are acquired after the callback completes, and default/static rules avoid
cloning request metadata. Callback failures fail closed without exposing private
messages. The standard fixture passes 1,485 actual Better Auth HTTP comparisons
across default, exact, wildcard and two asynchronous-rule profiles. Bodies remain
untouched for downstream parsing. Body-reading callbacks, plugin rate rules and
external counter stores remain open under the broader customization gate.

Callback validation receipt: the existing 39 auth tests pass, and all seven focused
rate-limit regressions pass after correcting the new privacy test's router mount.
Strict all-feature/all-target clippy and the 1,485-case HTTP fixture pass. The
PostgreSQL-specific unit test remains explicitly ignored in the portable suite.


Native plugin rate rules now accept ordered path predicates and positive limits.
The first matching plugin rule overrides built-in sensitive/default rules; the
first matching custom rule can override it, and asynchronous custom callbacks
receive the effective plugin window/budget. The expanded fixture passes 1,947
actual Better Auth HTTP comparisons, including reversed plugin ordering, custom
disablement and callback inheritance. Body-reading callbacks and external counter
stores remain open; this does not claim complete Better Auth plugin support.

Plugin-rule validation receipt: all 41 portable native auth tests pass, with the
PostgreSQL-specific unit test explicitly ignored. The final eight focused rate
regressions, strict all-feature/all-target clippy, and 1,947 HTTP comparisons pass;
plugin predicate call traces also match actual Better Auth, including static
custom overrides. Fresh generated run `IxtPZO` passes restricted-role PostgreSQL,
eight blueprint/generated frontend-client scenarios, both SDKs, HTTP/MCP
shared/separate/disabled listeners, Origin rejection, restart/shutdown,
relocation/preservation, rename and deletion. The final predicate-order adjustment
was checked with native regressions and actual-library traces after that generated
run; the checkout CLI was rebuilt with the final sources.


Customer persistence now has typed native create/read/update/delete/list operations
over the existing durable billing store. PostgreSQL migration 3 adds the customer
kind; email/name use tenant-, record- and field-bound encrypted envelopes and
atomic actor audits. Shared memory/restricted-role PostgreSQL tests cover durable
CRUD, concurrent disjoint updates, duplicate-ID races, nullable-name clearing,
empty-ID filtering and cache rejection. PostgreSQL also verifies tenant isolation,
audit-failure rollback, restart/reopening, retained-key rotation, ciphertext
substitution rejection and absence of customer plaintext in storage. This fills
the planned native customer persistence gap; the five configured TypeScript
billing services remain unchanged and all 168 actual service comparisons pass.

Stripe customer create/retrieve/update/list/delete now use the existing direct
and managed client helpers alongside checkout lifecycle operations. The official
Stripe 22.6.2 fixture passes 26 direct cases; actual TypeScript managed transport
passes 28 cases, including customer nested address/metadata encoding, expansions,
pagination parameters, null updates and deletion. Provider/local-write atomicity,
reconciliation and configured mapper review remain separately open.


Billing completion receipt: 11 portable billing tests, the dedicated restricted-role
PostgreSQL integration, strict all-target/all-feature billing clippy, 168 base-service
comparisons and 26 direct/28 managed Stripe cases pass. Generated run `kpWfCb`
passes mixed billing/Stripe modules, both SDKs from contract and OpenAPI imports,
HTTP/MCP startup, encrypted persistence and lifecycle. The first run `4fppO5`
exposed repeated whole-document SDK indexing; commit `299933772` reuses one
borrow-bound document index while preserving cumulative expansion limits. Its
80-operation/300-component regression, all 109 CLI SDK tests, 43 shared codegen
tests and strict codegen clippy pass. Customer persistence and customer/checkout
helper gates are closed; provider/local-write reconciliation remains open.


Better Auth custom rate-limit storage receipt (2026-10-13):
`AuthRateLimitOptions::with_custom_storage` accepts an async application-owned
atomic check/increment backend. Effective plugin/custom rules and normalized
`IP|auth-path` keys reach the backend; disabled limits bypass it. Retry omission
uses the effective window and explicit zero survives. Errors return a generic
failure without private details or process-local fallback. Applications own
backend atomicity and expiry; built-in Redis/PostgreSQL counter backends and
body-reading rule callbacks remain open.

All 2,409 actual Better Auth HTTP comparisons pass, including exact backend and
plugin call traces. Native validation passes 43 tests (one PostgreSQL-specific
unit test ignored), including concurrent custom consumption and fail-closed
errors, plus strict all-feature/all-target clippy. Fresh generated run `93F5fo`
passes restricted-role PostgreSQL, eight actual frontend-client scenarios, both
SDKs, HTTP/MCP shared/separate/disabled listeners, Origin rejection,
restart/shutdown, relocation/preservation, rename and deletion. Logs are in
`/private/tmp/fl-auth-storage-{parity,tests,clippy,generated}.log`. This advances
the broader customization gate without marking it complete.


AsyncAPI trait import receipt (2026-10-13): message and operation traits now
resolve bounded local references and merge in declaration order, with target
fields applied last. This follows the [AsyncAPI 3 merge mechanism](https://www.asyncapi.com/docs/reference/specification/v3.0.0#traits-merge-mechanism).
Arrays replace earlier arrays and nested null members remove inherited values.
Payload scopes remain anchored to the original message; traits cannot introduce
payloads. Inherited content types and bindings receive the existing wire checks;
unsupported application headers, security and bindings still fail explicitly.
Cycles, external references, invalid trait targets and more than 64 traits per
object fail before generation; expansion shares the import byte budget.

All 29 native AsyncAPI tests and strict all-feature/all-target clippy pass.
The official AsyncAPI parser agrees on local references, merge ordering, target
precedence, arrays and nested null removal. Fresh relocated run `xjjTgI` uses
trait-inherited message content types and operation metadata, compiles both SDKs,
runs live WebSocket peers, checks watch recovery and input ownership, and passes
service rename/deletion and the full generated HTTP/MCP startup/shutdown matrix.
The source-package fixture includes all 913 current embedded assets. Logs:
`/private/tmp/fl-asyncapi-traits-{tests,parity,clippy,generated,package}.log`.
The broader event/protocol, upgrade authentication/parameters, replies and richer
message matching import gate remains open.


Optional version-body hooks receipt (2026-10-13): native preprocessing can now
receive absence separately from explicit JSON null and supply defaults before
per-version schema validation. Optional transforms can produce an absent result
after successful validation. Existing required-value hooks still skip absent
inputs; registering either callback form replaces its counterpart.
`OptionalVersionedBody<T>` extracts a selected optional result without consuming
raw bytes and rejects missing version selection or incompatible present values.

The actual repository TypeScript parser/Zod comparison adds 12 cases for absent
input/defaults, explicit null, absent transformed output, failed transformations,
independent version candidates and fallback. All 45 prior version-selection,
six parser-registration and 18 transformation comparisons also pass. Native
validation passes 34 HTTP tests, 65 Axum tests and three integration tests, plus
strict all-feature/all-target clippy. Fresh generated run `uzzJd8` passes both
SDK languages through contracts, full/named OpenAPI and native generators;
HTTP/MCP root deletion/defaults and absent transformed results; all prior
media/parser/verify/reviver cases; listener startup and graceful shutdown.
Logs: `/private/tmp/fl-optional-body-{parity,tests,clippy,generated}.log`.
Sparse arrays, mutable reviver holders/source context and broader non-body
transformations remain open. Checklist remains 655/682 complete, 27 open.


TypeScript verified-session authorization receipt (2026-10-13):
`forklaunch_auth::typescript::TypeScriptRbacPolicy` provides explicit compatibility
with the actual middleware's omitted/empty rule sets, permission-before-role
precedence, access-dependent checks, exact scope membership, and subscription
before feature checks. Native RBAC retains its existing semantics. The new
`RouterConfig::with_authorizer` callback replaces native RBAC only when explicitly
configured, receives an already verified/enriched session, and cannot run after
failed credential verification. Async grant resolution remains in the verifier.
The policy does not replace credential parsing, session-schema validation,
missing surfacing callbacks, public route dispatch or HMAC's sessionless path.

All 1,495 actual repository middleware session-policy comparisons pass. The
fixture substitutes credential verification and telemetry only; the actual
middleware and its authorization guards evaluate every policy. Native validation
passes 16 auth tests, 66 Axum tests and three integration tests, plus strict
all-feature/all-target clippy. Fresh generated `WQrswi` verifies signed JWTs and
runs permissions, exact/unknown scopes, subscriptions/features, missing policy
and rejected credentials over HTTP/MCP, compiles both SDKs and passes the full
listener/startup/shutdown matrix. All 915 current embedded source assets are
packaged. CI runs both the actual-source comparison and generated runtime.
Logs: `/private/tmp/fl-ts-rbac-{parity,tests,clippy,generated,package}.log`.
Broader auth transport/session and error-envelope parity remain open.


OAuth/OIDC library adapter receipt (2026-10-13): optional `forklaunch-auth/oidc`
now uses [oauth2 5](https://docs.rs/oauth2/5.0.0/oauth2/) and
[openidconnect 4](https://docs.rs/openidconnect/latest/openidconnect/) for reusable
authorization-code provider integration. Discovery is pinned to operator-owned
issuer/client/callback configuration; transport permits HTTPS or loopback HTTP,
refuses redirects/URL credentials/fragments and bounds responses/time. Pending
state is non-cloneable, expires in ten minutes and binds issuer/client/callback;
applications retain it in the initiating browser's server-side session. Public
and HTTP Basic clients use S256 PKCE. RS256 verification checks issuer, audience,
expiry, nonce, present azp, nonempty subject, future iat and present at_hash.
Provider errors and Debug omit secrets.

All 22 auth tests and strict all-feature/all-target clippy pass. Local provider
coverage includes real discovery/JWKS/token HTTP, PKCE wire bytes, public and
confidential clients, state/config/expiry failures before exchange, invalid
metadata/scopes, nonce/signature/issuer/audience/azp/expiry failures, substituted
access tokens, redirects and declared/chunked response bounds. The all-lockfile
security gate and its tests pass with no unreviewed findings. The new framework
RSA advisory is retained with a public-verification-only source review; it is
not claimed fixed. No existing locked package version was removed/upgraded.
Fresh generated `Z2FzoB` enables the optional adapter, compiles both SDKs and
passes signed-JWT authorization and the HTTP/MCP listener/startup/shutdown matrix.
All 918 embedded assets are packaged. Logs:
`/private/tmp/fl-oidc-{tests,clippy,security,security-tests,generated,package}.log`.

This closes the explicit OAuth/OIDC library-integration requirement. The generic
adapter does not provision accounts/sessions or implement refresh grants or
automatic key refresh; the configured Better Auth Google flow remains separate.
Checklist: 656/682 complete (96.2%), 26 open.


OIDC refresh receipt (2026-10-13): the optional generic adapter now supports
refresh grants using trusted original login claims and encrypted server-owned
tokens. Newly returned ID tokens retain issuer, subject and audience, cannot
backdate issuance before the login, and preserve authentication time and nonce
when supplied, following [OIDC refresh rules](https://openid.net/specs/openid-connect-core-1_0.html#RefreshTokenResponse).
An omitted ID token produces absent claims; an omitted replacement refresh token
retains the supplied token. Rotated tokens are returned for application-owned
atomic persistence. Empty/non-Bearer token responses and mismatched original
provider/client claims fail closed. This supersedes the earlier refresh limitation.

All 25 auth tests and strict all-feature/all-target clippy pass, including real
local-provider refresh wire requests, identity/nonce/time/hash failures, token
rotation and optional tokens. Generated run `YAAZD4` compiles both SDKs and passes
signed-JWT authorization plus the HTTP/MCP listener/startup/shutdown matrix.
All 919 embedded assets are packaged. Logs:
`/private/tmp/fl-oidc-refresh-{tests,clippy,generated,package}.log`.
Automatic key refresh and account/session provisioning remain open; the broader
provider gate stays open. Checklist remains 656/682 (96.2%), 26 open.


Reference billing pagination regression (2026-10-13): clamp numeric customer
cursors to the stored collection length before slicing, matching the existing
reference IAM pagination behavior. Empty collections, terminal/beyond-end cursors,
maximum integer cursors/limits and normal multi-page traversal are covered.
The three billing unit tests and portable persistence test pass; the separately
configured PostgreSQL test remains ignored in this run. Strict all-feature/
all-target billing clippy passes. Logs:
`/private/tmp/fl-billing-cursor-{tests,clippy}.log`. This does not close the broader
billing reconciliation gate.


All-module lifecycle receipt (2026-10-13): `rust_module_lifecycle.sh` now checks
all eight built-in variants together in a relocated mixed app with a TypeScript
library: iam-base, iam-better-auth, cac-base, messaging-base, messaging-twilio,
ecommerce-stripe, billing-base and billing-stripe. Run `smv1a8ho` passes repeated
initialization, sync, preserved user source/package metadata, rename, variant
ownership, OpenAPI component naming, Rust/TypeScript compilation from both
OpenAPI and native contracts, SDK drift checks and deletion of contracts/SDKs/
registrations. The TypeScript library survives byte-for-byte and the final empty
Cargo workspace remains valid. The fixture is included in E2E CI and documented
in the blueprint workspace guide. Earlier family runtime receipts cover live
HTTP/MCP, encrypted storage and providers; relay/storefront have their own
checked lifecycle fixtures. This closes the combined module lifecycle gate,
not the remaining deployment or provider behavior gates.

The fresh source-archive installer first exposed invalid Better Auth response
component names (`Surface User Roles`/`Surface User Permissions`). Native module
contracts now use `SurfaceUserRoles`/`SurfaceUserPermissions`, preserving actual
TS operation names and response shapes. Five actual TS contracts/nine response
schemas and strict all-feature Better Auth clippy pass. The component-name audit
also replaces explicit `Vec<String>` query schema names in IAM/ecommerce/billing
with `StringArray`, preserving wire arrays. Six interface tests and strict
all-feature/all-target clippy pass. Component keys follow the
[OpenAPI 3.1 naming rule](https://spec.openapis.org/oas/v3.1.0.html#components-object).

Archive run `A9tzbZ` includes the Better Auth correction and passes installation,
portable module/worker compilation, both SDK import paths/native generators,
HTTP/MCP startup/listener/shutdown checks and all five managed capability
callbacks. Later query-name changes pass the final all-module matrix and the
919-asset source-package check. Logs:
`/private/tmp/fl-parity-packaged-install-final.log`,
`/private/tmp/fl-module-matrix-final.log`,
`/private/tmp/fl-module-interface-{tests,clippy}.log`,
`/private/tmp/fl-auth-component-{parity,clippy}.log`,
`/private/tmp/fl-module-matrix-package.log`.
Checklist: 657/682 complete (96.3%), 25 open. Cleanup removed 17.5 GiB of inactive
port-owned test executables older than 24 hours; active builds and other worktrees
were preserved.


General OpenAPI component-name receipt (2026-10-13): the exporter now maps
arbitrary contract schema names to valid, collision-safe OpenAPI component keys.
Ordinary names remain unchanged; empty names, characters outside the OpenAPI
component alphabet and the reserved `_fl_encoded_` prefix use UTF-8 hexadecimal
encoding. The reserved-prefix case prevents a user name from colliding with an
encoded key. Both route references and relocated recursive schema references
use the mapped key; schema titles and literal annotations remain unchanged.

All eight OpenAPI tests and strict all-feature/all-target clippy pass, covering
spaces, generic syntax, Unicode, slash/tilde/percent, empty names, literal-prefix
collisions, recursive roots and definitions. Generated run `XjdxFB` exercises
unusual names through both CLI artifact imports, both native SDK generators,
strict Rust/TypeScript consumers, live recursive requests/responses and HTTP/MCP
startup/discovery/invocation/shutdown. The fixture is in E2E CI. All 920 embedded
assets are packaged. Logs:
`/private/tmp/fl-component-names-{tests,clippy,generated,package}.log`.
The broader schema/emitter conformance gate remains open; checklist stays
657/682 (96.3%), 25 open.


Versioned field transformation receipt (2026-10-13): native runtime hooks now
preprocess/default and transform declared parameters, query fields and headers
around each version's schema checks. Missing and null remain distinct; absent
post-transform values are accepted. Last-successful selection, unknown-field
stripping and TypeScript's truthy replacement of existing header keys are covered
by 320 comparisons against the actual repository parser/Zod source. Typed Axum
extractors expose selected results while preserving raw query/header values.
Authentication remains before callbacks; credential-transform compatibility is
not claimed. Registration rejects unknown fields/versions and ambiguous routes.

Generated run `j0VNl1` passes both SDK compilers from contract and OpenAPI imports,
36 live HTTP/MCP transformation cases, invalid-input rejection and the full
HTTP/MCP startup/listener/shutdown matrix. The first run `HwpggS` exposed raw
percent-encoded path captures. Shared matching now decodes captures once after
matching the complete route; encoded slashes do not split routes, double encoding
is preserved after one decode, and malformed escapes/non-UTF-8 captures return
400. Unit and generated live tests cover these cases, including validation-off
local dispatch and unchanged raw paths. The new fixtures are registered in CI.

39 HTTP tests, 67 Axum tests and three integration tests pass, as do strict
all-feature/all-target clippy and the no-default-feature Axum check. Logs:
`/private/tmp/fl-version-fields-{tests,clippy,parity,generated,disabled,package}.log`.
The percent-encoding dependency was already locked; only the HTTP dependency edge
was added. Sparse values, mutable holder/context behavior, cross-field side-effect
ordering and remaining codec boundaries stay open. Checklist remains 657/682
(96.3%), 25 open.


Billing webhook recovery receipt (2026-10-13): migration 4 adds tenant-isolated,
metadata-only attempts committed before callback effects and an operator resolution
audit. Failed, cancelled or interrupted callbacks remain blocked across replicas
and restarts. This intentionally tightens native retry behavior: ambiguous effects
are not blindly re-executed. Successful receipt insertion and attempt removal are
atomic. Bounded pending inspection and exact-attempt resolution support Applied
(suppress replay) or RetryAllowed (operator-verified safe retry); this API does not
query or repair provider state automatically. Active callbacks retain their lock,
and stale attempts cannot resolve newer retries. Actor/correlation context and
atomic audit insertion are required. Runtime/operator grants are documented.

Three base unit tests, memory/restricted-role PostgreSQL integration and the native
HTTP/MCP integration pass. Tests include cancellation after a committed local write,
store reopening without replay, tenant separation, active callback contention,
stale resolution, anonymous resolution rejection, audit-write rollback, successful
deduplication and a two-connection pool. Strict base/Axum all-feature/all-target
clippy passes. Generated run `f21N1N` passes signed direct/platform blocked replay,
restart, encrypted storage, both live SDKs/import paths, managed gateway callbacks,
HTTP/MCP startup/shutdown and rename/delete lifecycle. The earlier `Wj5b8x` run
found a malformed platform test envelope, corrected before the passing run.
All 925 embedded assets are packaged. Logs:
`/private/tmp/fl-billing-reconciliation-tests.log`,
`/private/tmp/fl-billing-reconciliation-clippy.log`,
`/private/tmp/fl-billing-reconciliation-generated-final.log`,
`/private/tmp/fl-billing-reconciliation-package.log`.
Outbound provider/local-write recovery and remaining mapper decisions stay open;
checklist remains 657/682 (96.3%), 25 open.





Verified PayPal capture recovery receipt (2026-10-13): `PaypalOrderReader` and
`PaypalClient::retrieve_order_details` add authenticated bounded order retrieval.
`PaypalPaymentService::reconcile_captured_payment` verifies completed/final capture
state, exact amount/currency and order reference before a local repair. One purchase
unit/one capture matches this client's creation format. Pending/refunded, multiple,
substituted and mismatched evidence is rejected; no capture/refund/delivery request
is issued. Ordinary TypeScript provisional-claim and failure semantics remain,
with 22 actual client cases and 19 wrapper operations passing unchanged.

`PaymentReconciliationStore` adds audited snapshot-checked repair for memory and
PostgreSQL. Migration 6 adds tenant-RLS audit metadata. Runtime roles need SELECT
and operators need INSERT; generated documentation and fixture grants are updated.
Audit failure rolls back a changed payment, stale snapshots and anonymous actors
are rejected, and a delayed legacy claim release cannot reset verified completion.
Seven base business-rule tests, two recovery tests (including restricted PostgreSQL
and reopening), the PayPal live provider test and both native Axum tests pass.
Strict all-feature/all-target clippy passes for base/PayPal/Axum. Provider cases
reject amount/currency/reference/status mismatches, overflow/invalid money,
multiple captures/units, refund/authorization metadata and substituted order IDs.

Generated run `oDyeRU` compiles both SDKs from both artifacts and passes live
HTTP/MCP, checkout/webhook comparisons, worker delivery, encrypted persistence,
restart and CLI/Compose lifecycle. Its runtime phase was resumed in the same
compiled workspace after supplying the required Stripe fixture dependency path.
All 930 embedded assets are packaged. Logs:
`/private/tmp/fl-paypal-reconciliation-{tests,clippy,parity,package}.log`,
`/private/tmp/fl-paypal-reconciliation-generated.log`,
`/private/tmp/fl-paypal-reconciliation-generated-final.log`.
A disk-full build and stopped retained PostgreSQL fixture were resolved by removing
1.46 GiB of inactive port-owned executables and starting a fresh owned database.
No source/unrelated workspaces were removed. Uncaptured/ambiguous claims and
cancelled-order provider references remain open; checklist remains 657/682
(96.3%), 25 open.


Verified Stripe capture recovery receipt (2026-10-13): Stripe client/service
helpers retrieve expanded charge evidence and use the audited payment-repair
boundary introduced for PayPal. Full intent/charge amounts, currencies, provider
reference and order metadata must match; only succeeded, paid, fully captured,
unrefunded and undisputed evidence is accepted. Connect account headers are retained.
Provider reads do not create/capture/refund charges or deliver orders; raw client
secrets/provider responses never enter the audit. Twenty invalid provider variants
are rejected by live HTTP tests without another mutation or audit entry.

Both Stripe native tests and strict all-feature/all-target clippy pass; fifteen
actual TypeScript comparisons confirm ordinary provider/Connect/resume/lifecycle
semantics remain unchanged. Generated run `wg2LRs` passes both SDKs/import formats,
HTTP/MCP startup, checkout/webhook comparisons, cross-process checkout admission,
worker/inventory persistence, restart and CLI/Compose lifecycle. All 931 embedded
assets are packaged. Logs:
`/private/tmp/fl-stripe-reconciliation-{tests,clippy,parity,generated,package}.log`.
Incomplete captures and cancelled-order references still require separate recovery;
checklist remains 657/682 (96.3%), 25 open.


Trusted custom JWT signer receipt (2026-10-13): `JwtSigner` and
`KeyRing::sign_jwt_with_signer` accept a typed asynchronous server callback,
preserve Better Auth custom-sign claim/default semantics, and bypass local key
selection, JOSE claim setters and storage. Validation precedes callback invocation;
callback failures are sanitized. Panic-on-access storage and pending-callback tests
prove no database access and caller-controlled cancellation. Non-finite JavaScript
claims are rejected because the public boundary uses finite JSON values.

All 45 native auth tests pass (one database opt-in test ignored); strict all-feature,
all-target Clippy passes. Actual Better Auth comparisons cover 62 custom-sign cases,
plus 189 existing signing API and 216 persisted key-state regression cases.
Generated run `8smf2l` validates both SDKs, eight actual client/namespace scenarios,
HTTP/MCP shared/separate/disabled startup, encrypted tenant sessions, independent
JWT verification, restart, relocation/preservation, rename and delete. All 932
embedded assets are packaged. Logs:
`/private/tmp/fl-custom-jwt-{tests,clippy,parity,local-regression,generated,package}.log`.
This helper does not configure session-token HTTP routes or remote JWKS; those
remain open within broader auth coverage. Checklist remains 657/682 (96.3%), 25 open.


External HTTP JWT signer receipt (2026-10-13): trusted `ExternalJwtSigner`
configuration wires the typed callback into session-authenticated `/token`,
`get-session` JWT headers and popup callback exchange. Explicit asymmetric
algorithm/public-key URL metadata is required. Local `/jwks` returns 404 and
signing creates no local key rows; key publication, rotation and verifier
configuration remain application-owned, as does callback algorithm enforcement.

Nine actual Better Auth HTTP cases compare configured claims, headers, anonymous
and forged sessions, and remote-JWKS suppression. Live TCP tests independently
verify signatures, deny revoked sessions without invoking the signer, sanitize
provider failures, check popup exchange and confirm empty local key storage.
The complete native suite passes 47 tests (one database opt-in test ignored),
updated focused tests and strict all-target Clippy pass, and 161 existing actual
HTTP/session comparisons remain unchanged. Generated run `gRhDXs` passes both
SDKs, all eight client/namespace scenarios, encrypted persistence, HTTP/MCP
startup/discovery/invocation, restart and rename/delete. All 934 embedded assets
are packaged. Logs:
`/private/tmp/fl-external-jwt-{tests,all-tests,clippy,parity,http-regression,generated,package}.log`.
Checklist remains 657/682 (96.3%), 25 open; broader auth adapters/options remain.


Common naming/hash symbol receipt (2026-10-13): the following public TypeScript
exports map directly to `forklaunch_common` string-input APIs. One actual-source
fixture executes 591 inputs across all twelve exports (7,092 comparisons), covering
separator versus stripped punctuation, numeric prefixes, original-input fallback,
Unicode case conversion, UTF-16 leading-surrogate behavior and signed hash overflow.

| TypeScript export | Rust export |
| --- | --- |
| `removeLeadingSeparators` | `remove_leading_separators` |
| `splitBySeparators` | `split_by_separators` |
| `removeNonAlphanumeric` | `remove_non_alphanumeric` |
| `removeLeadingNumbers` | `remove_leading_numbers` |
| `capitalize` | `capitalize` |
| `uncapitalize` | `uncapitalize` |
| `toCamelCaseIdentifier` | `to_camel_case_identifier` |
| `toPrettyCamelCase` | `to_pretty_camel_case` |
| `toValidIdentifier` | `to_valid_identifier` |
| `isValidIdentifier` | `is_valid_identifier` |
| `getTransformationInfo` | `get_transformation_info` / `TransformationInfo` |
| `hashString` | `hash_string` |

Operation names now use the actual TS transformation: `foo!bar` becomes `foobar`
instead of `fooBar`, leading numbers are removed, and empty conversions retain the
original string. Clients using affected names need regeneration. Rust string inputs
do not represent isolated JavaScript surrogate code units or non-string coercions.
The hash is non-cryptographic and adds no security primitive.

All 98 affected common/contract/macro/OpenAPI/SDK tests and strict all-target Clippy
pass. Generated run `ftABml` validates both SDKs from both artifact formats against
Rust and TypeScript HTTP services, HTTP/MCP startup/discovery/invocation/shutdown
and both native multipart SDKs. Packaging caught and fixed a missing common-example
template link; all 936 embedded assets are included. Logs:
`/private/tmp/fl-common-naming-{parity,tests,clippy,generated,package}.log`.
Remaining utility/symbol and transport boundaries stay open; checklist is still
657/682 (96.3%), 25 open.


Common slash helper receipt (2026-10-13): `removeTrailingSlash`,
`removeDoubleLeadingSlash` and `sanitizePathSlashes` now map to their exact
single-step Rust counterparts, including empty/root inputs, relative paths,
repeated slashes and trailing line separators. Native contract/request/router
normalization moved to explicit `normalize_route_path`, preserving its existing
leading-slash insertion, boundary collapse and root behavior. Callers needing the
old normalization should use that function rather than the TypeScript helpers.

The common actual-source matrix now checks 697 inputs across fifteen exports
(10,455 comparisons). All 159 affected common/contract/HTTP/Axum/MCP tests and
strict all-feature/all-target Clippy pass. Generated run `ecWquM` validates
versioned field transforms/defaults, parameter decoding/raw wire preservation,
both SDKs from both artifacts and HTTP/MCP listener/startup/shutdown profiles.
All 936 embedded assets remain packaged. Logs:
`/private/tmp/fl-common-path-{parity,tests,clippy,generated,package}.log`.
OpenAPI parameter-string conversion and the rest of the public utility/symbol
audit remain open; checklist stays 657/682 (96.3%), 25 open.


Native platform deployment receipt (2026-10-13): CLI release components now carry
`runtime: "tokio"` and their actual Cargo executable name, including custom crate
names and both worker components. JavaScript components retain their existing
manifest shape. The companion checkout `../forklaunch-platform-rust-parity`,
branch `rohin/rust-runtime-parity`, commits `fe0974937` and `050b1ef99`, preserves
these fields through platform schemas and starts native containers with direct
argv and a cleared image entrypoint. Runtime-aware build groups, CodeBuild and
image gates skip JavaScript installation for native-only images, use independent
mixed-image contexts and reject conflicting Node/Bun workspace installers.
Multiple image builds now emit independently scoped helpers and compile together.

Native tasks default to port 8000, network-accessible service hosts and enabled
worker health listeners. Native DATABASE_URL values survive worker processing,
fall back to the final DB_URL unless explicitly unset, use secret task references
and secret stack outputs. Explicit connections remain authoritative. This does
not grant database privileges, synthesize tenants or run owner migrations.

All 1,101 CLI tests pass. Real optimized native and mixed Cargo/PNPM/Node release
builds retain component metadata, generate both SDK targets and reuse prebuilt
artifacts without publishing (`/private/tmp/fl-native-generated-release.log`).
Platform validation covers 117 focused deployment/build
checks plus five manifest-schema checks, generated mixed-program compilation,
legacy JavaScript snapshot equality and full deployment-worker type checking
against rebuilt local core/platform-management declarations. Generated relocated
Linux container run `tBwUZM` passes HTTP/OpenAPI/MCP startup, discovery/invocation,
health checks and graceful SIGTERM shutdown. Logs:
`/private/tmp/fl-platform-native-{build,install,listeners,database,schema}-tests.log`,
`/private/tmp/fl-platform-native-{worker,database}-types.log`,
`/private/tmp/fl-platform-native-container.log`,
`/private/tmp/fl-native-release-all-cli-tests.log`.

No AWS deployment was performed. Native migration jobs, database role/tenant
provisioning, actual ECS/load-balancer behavior and worker role/scaling semantics
remain open. Both generated native worker components currently execute a combined
consumer/control process; distinct HTTP-only/consumer-only roles are not implied.
The companion deployment guide records these limits. The checklist remains
657/682 (96.3%), 25 open.


Basic credential compatibility receipt (2026-10-13): opt-in
`forklaunch_auth::typescript::basic::verify_basic` matches 815 actual TypeScript
middleware cases for missing/empty headers, exact/custom prefixes, first-space
selection, Node's two base64 alphabets/ignored bytes/padding/trailing bits,
UTF-16 low-byte input, replacement UTF-8, empty/extra colon fields and accepted or
rejected synchronous login callbacks. No principal is returned before successful
verification. Strict native `parse_basic` remains unchanged.

`RouterConfig::with_response_authenticator` carries safe application-selected
credential errors through native HTTP/MCP, followed by ordinary authorization.
The existing verifier API and cookie verifier behavior remain available. Native
errors retain structured JSON; this does not close the TS text-error envelope gate.
All 96 affected auth/Axum tests plus the new live HTTP/MCP Basic test pass,
as do strict all-feature/all-target Clippy and the 1,495 session-policy comparisons.
Generated relocated run `Fud7KV` validates Basic/JWT/policy cases over HTTP and MCP,
both SDK compilers and the complete shared/separate listener startup/shutdown
matrix. All 938 embedded assets are packaged. Logs:
`/private/tmp/fl-typescript-basic-{parity,adapter-tests,sockets,clippy,generated,package,rbac-regression}.log`.
Async/custom Basic resource decoders, session-schema validation and remaining
credential/error transports remain open; checklist stays 657/682 (96.3%), 25 open.


OpenAPI path utility receipt (2026-10-13): `open_api_compliant_path` now implements
the exact public TypeScript global ASCII-word replacement, including literal
prefixes, file suffixes, adjacent parameters and non-ASCII boundaries. Native
contract construction and route decoration use explicit `route_open_api_path`,
preserving their existing whole-segment conversion/matching. This does not claim
native routing support for partial-segment parameters.

The actual-source common matrix covers 711 inputs across sixteen exports (11,376
comparisons). All 130 affected common/contract/OpenAPI/Axum/MCP tests and strict
all-feature/all-target Clippy pass. Generated relocated run `0tl7w8` passes versioned
field transforms/defaults, raw wire preservation, both SDK compilers and the full
HTTP/MCP startup/shutdown matrix; all 938 embedded assets are packaged. Logs:
`/private/tmp/fl-common-openapi-{parity,tests,clippy,generated,package}.log`.
Native embedded-parameter route matching and the remaining public-symbol audit
stay open; checklist remains 657/682 (96.3%), 25 open.


Same-channel AsyncAPI reply receipt (2026-10-13): server-view WebSocket imports
now accept fixed same-channel operation replies, inline or via bounded local
reply references. Replies populate the opposite message direction and retain
existing schema/disjointness/media validation. Explicit nonempty message subsets
must belong to the same root channel. Dynamic addresses, cross-channel routing,
external/cyclic/sibling references, missing/foreign messages, overlapping schemas
and oversized subsets fail before either SDK is changed. Stream clients expose
send/receive; applications own correlation, ordering and reply deadlines.

All 18 AsyncAPI tests pass, including eight JSON/binary/action/reference variants
and twelve rejection mutations, as do strict all-target Clippy and all 1,101 CLI
tests (`/private/tmp/fl-asyncapi-reply-cli-tests.log`). Generated
relocated run `hsJ5Me` validates the document with the official parser, compiles
strict Rust/TypeScript SDKs and exchanges real request/reply messages with a live
peer. It checks invalid-reply output preservation, watch recovery, relocation,
rename/delete ownership and both services' complete HTTP/MCP startup/shutdown
matrices. All 938 embedded assets remain packaged. Logs:
`/private/tmp/fl-asyncapi-reply-{tests,clippy,generated,package}.log`.
The [AsyncAPI 3 reply model](https://www.asyncapi.com/docs/reference/specification/v3.0.0#operationReplyObject)
remains broader than this fixed WebSocket profile: dynamic/cross-channel replies,
automatic correlation, upgrade security/parameters, other protocols and event
projects stay open. Checklist remains 657/682 (96.3%), 25 open.


OIDC signing-key rotation receipt (2026-10-13): login and refresh responses with
ID tokens now fetch fresh JWKS from the discovery-pinned URL before verification.
This supersedes the automatic-key-refresh limitation in the earlier receipts.
Keys are fetched once per verification without credential headers or token-grant
retries, using existing no-redirect, 1 MiB and 15-second transport bounds. Failed,
revoked, malformed and oversized key sets never fall back to discovery-time keys.
Refresh responses without ID tokens skip the key request. The deliberate absence
of a cache adds provider latency/load and makes key availability a login dependency;
endpoint metadata changes still require explicit rediscovery.

All 28 auth tests and strict all-feature/all-target Clippy pass. A local provider
rotates actual RSA key material between discovery, login and refresh, asserting
identity binding, exact grant/key request counts and credential-free key fetches.
Generated relocated run `z9iUdx` executes the vendored OIDC wire tests, compiles
both SDKs and passes Basic/JWT authorization and the complete HTTP/MCP listener,
discovery, invocation and shutdown matrix. All 939 embedded assets are packaged.
Logs: `/private/tmp/fl-oidc-key-refresh-{tests,clippy,generated,package}.log`.
Account/session provisioning and broader provider/options coverage remain open.
Checklist remains 657/682 (96.3%), 25 open.


Basic async resource decoder receipt (2026-10-13): the opt-in
`typescript::basic::verify_basic_resource` verifies through a trusted async
callback instead of synchronous username/password login. Exact prefix and first
space-token behavior precede the callback; its token is passed unchanged. Null
claims return the TypeScript 403, while other JSON claims are preserved for
application-owned session validation and principal binding. Callback errors are
explicit public responses. Cancellation drops the callback without retries.

All 30 auth tests and strict all-feature/all-target Clippy pass. The actual-source
Basic comparison fixture now checks 1,067 cases, including 252 async resource
cases covering prefix errors, empty/opaque tokens, decoder precedence and JSON
claim shapes. Generated relocated run `6W7xXG` compiles both SDKs, passes vendored
OIDC wire tests and verifies Basic resource login, invalid sessions, role denials
and trusted claim propagation over both HTTP and MCP. Its complete listener and
startup/shutdown matrix passes. The wire fixture accounts for Fetch trimming a
trailing Authorization space; direct middleware comparisons retain empty-token
coverage. All 939 embedded assets remain packaged. Logs:
`/private/tmp/fl-basic-resource-{tests,parity,clippy,generated,runtime,startup,package}.log`.
Session schemas, broader callbacks and error envelopes remain open; checklist is
657/682 (96.3%), 25 open.


JWT custom resource verifier receipt (2026-10-13): the opt-in
`typescript::jwt::verify_jwt_resource` awaits a trusted verifier/decoder after
exact Bearer/custom prefix and first-token checks. Falsy JSON results reject
verification; callback errors become the TypeScript generic 403 without exposing
provider details. The application must verify credentials in the callback and
validate/bind the returned session before authorization. This adds no implicit
payload-only decoder and leaves strict built-in JWT verification unchanged.

All 32 auth tests and strict all-feature/all-target Clippy pass. The shared
actual-source credential fixture now checks 1,931 cases (864 new JWT callback
cases), covering error redaction, falsy/structured claims and header precedence.
Generated relocated run `TNYJ4G` compiles both SDKs, runs vendored OIDC tests, and
passes real signed/expired/invalid JWT callbacks, role checks and trusted claims
over HTTP/MCP alongside the Basic/RBAC cases and complete listener/startup/shutdown
matrix. All 940 embedded assets are packaged. Logs:
`/private/tmp/fl-jwt-resource-{tests,parity,clippy,generated,package}.log`.
Broader session-schema/callback and error-envelope work remains open; checklist
remains 657/682 (96.3%), 25 open.


Native worker role receipt (2026-10-13): generated PostgreSQL, Redis, BullMQ and
Kafka executables now accept `--control-service` before opening job storage.
This serves management HTTP without consumers, leases or retention. Tokenless
mode exposes health/readiness only; authenticated controls require a scoped
Redis publisher. The process does not subscribe, stays running after remote drain,
and reports local readiness rather than remote queue health. Subscriber counts
remain delivery fanout counts, not acknowledgements. Management routes expose no
MCP/application SDK methods.

Scaffolding/sync advertise `workerControlService` only when installed sources
support the entry point. New releases carry explicit control/consumer roles;
legacy/custom sources retain their old command. A source-preservation regression
withdraws unsupported metadata and restores it after re-adoption. Companion
platform commit `5774b2bbc` preserves roles through schemas, validates compatible
runtime/component combinations and selects direct argv with `--control-service`
only for the control component.

All 1,101 CLI tests and six worker HTTP tests pass, with strict worker Clippy.
Generated Redis run `sQqS08` verifies real two-consumer fanout, storage-independent
control startup, no extra subscriber, post-drain availability, normal consumer
lease/retry/isolation behavior and relocation/rename/backend-change/delete.
Generated matrix `rw_v_b0r` builds all four backends, starts each control role with
unavailable job storage, checks configuration rejection and graceful SIGTERM,
compiles both SDK targets, and passes the companion service's complete HTTP/MCP
startup/discovery/invocation/shutdown matrix. All 940 embedded assets are packaged.
Platform validation passes 22 native-command/generated-program tests, six schema
tests and the full deployment-worker type check. Logs:
`/private/tmp/fl-worker-control-{tests,clippy,redis-generated,matrix-build,matrix-runtime,matrix-mcp,matrix-sdk,source-capability,package}.log`,
`/private/tmp/fl-worker-role-cli-tests.log`, and
`/private/tmp/fl-platform-worker-role-{generated-tests,schema,worker-types}.log`.
Actual AWS tasks/load balancers/scaling, migration jobs and tenant/runtime-role
provisioning remain unverified. Checklist remains 657/682 (96.3%), 25 open.


HMAC header/verifier-dispatch receipt (2026-10-13):
`typescript::hmac::verify_hmac` adds opt-in TypeScript error ordering, exact prefix,
first-four-component parsing, embedded-equals preservation, ignored suffixes,
configured key lookup and trusted async verification. Missing keys reach the
verifier as absent and must reject there. Successful verification creates no
session or principal. Timestamp text is raw; JavaScript Date conversion, signing,
freshness and replay policy are explicitly outside this dispatch adapter.

The actual middleware dispatch fixture checks 1,320 header/config/verifier cases;
16 existing actual createHmacToken/header-generator comparisons also pass. All 34
auth tests and strict all-feature/all-target Clippy pass. Generated relocated auth
run `inFisa` compiles both SDKs and verifies real HMAC signatures and 60-second skew,
missing keys, malformed credentials and invalid/expired signatures on HTTP/MCP,
alongside Basic/JWT/RBAC and the complete startup/discovery/invocation/shutdown
matrix. All 942 embedded assets are packaged. The dispatch fixture runs in CI.
Logs: `/private/tmp/fl-typescript-hmac-{parity,signatures,tests,clippy,generated,package}.log`.
Remaining session/callback/date and error-envelope boundaries remain open;
checklist remains 657/682 (96.3%), 25 open.


Crate release pipeline receipt (2026-10-13): all 49 public framework, blueprint
and protocol-fork crates now package with versioned internal dependencies. Three
sample crates explicitly disable publication. `scripts/rust_release.py` validates
the complete publication graph and packages the crates plus the pinned literal-key
Serde JSON patch as a separate checksummed artifact. A loopback sparse registry
stages copies with owned dependency registry metadata rewritten; original source
files and release archives remain unchanged. External dependencies use crates.io.

A separate consumer installs all 49 crates from that registry, applies only the
extracted parser artifact as a root path patch, and compiles defaults plus Axum MCP
(protocol-fork defaults disabled). Literal JSON marker-key roundtrips and the full
HTTP/MCP listener/startup/discovery/invocation/Origin/shutdown matrix pass. Nine
regression tests cover graph order, private/unpublishable dependencies, source and
archive integrity, live sparse index metadata and clean commit/tag/receipt/upload
boundaries. The manual publishing workflow defaults to validation only; uploads
require an explicit input, matching versioned tag, clean checkout, successful
receipt and configured credentials. Required parser assets are preserved on an
existing public GitHub release before sequential Cargo uploads.

Logs: `/private/tmp/fl-rust-{registry-prepare,registry-verify,registry-startup,release-tests,release-package}.log`.
All 1,101 CLI tests pass, and all 942 embedded CLI assets remain packaged.
CLI log: `/private/tmp/fl-rust-release-cli-tests.log`. Local validation ran on macOS; the
new Linux workflow has not been executed here. No public crate or GitHub release
was uploaded. Optional adapter runtime checks, public publication/installation and
AWS deployment remain separate gates. Checklist: **658/682 (96.5%), 24 open**.


Ordered session callback receipt (2026-10-13):
`TypeScriptRbacPolicy::authorize_session` accepts trusted request-scoped
`TypeScriptSessionResolver` implementations, validates original verified claims,
then awaits scopes, permissions or roles, subscription and features in middleware
order. Rejections stop subsequent lookups; authenticated/internal access skips
permission/role callbacks. Missing required callbacks return the exact TypeScript
500 messages. Missing scope callbacks retain the upstream skip behavior. Parsed
defaults/transforms never replace the original session. Applications still own
credential verification, identity binding, schema validation and public callback
error formatting.

All 2,335 actual middleware callback-order comparisons pass, including controlled
validator outcomes, original claims, missing callbacks, empty grants, access,
precedence and async failures. All 36 auth tests and strict all-feature/all-target
Clippy pass; native regressions include cancellation without later callbacks.
Generated relocated run `r2lhuP` compiles both SDKs, executes vendored OIDC tests,
and validates real signed-JWT session validation/ordered grants over HTTP/MCP,
alongside existing Basic/JWT/HMAC/RBAC cases and the complete listener/startup/
shutdown matrix. The comparison fixture runs in CI. Logs:
`/private/tmp/fl-typescript-session-{parity,tests,clippy,generated,package}.log`.
Schema-validator equivalence, remaining callback/date boundaries and error
envelopes stay open. Checklist remains **658/682 (96.5%), 24 open**.


Auth rejection wire-format receipt (2026-10-13):
`RouterConfig::auth_error_format` explicitly selects native JSON (the default) or
TypeScript plain-text authentication/authorization errors. `TypeScriptText` sends
the public message with `text/plain; charset=utf-8`; credential/session policies
still determine statuses/messages. Successful responses and request-validation/
handler errors retain their existing formats. JSON retains correlation IDs and
HEAD suppresses bodies in both modes. MCP preserves the rejected HTTP body.

The actual TypeScript/native canonical services pass 46 HTTP outcomes, including
exact missing/invalid/expired/forged JWT rejection bodies, content types and header
parsing. Both OpenAPI snapshots pass with ten explicit document differences still
retained. All 70 Axum unit tests, two MCP integrations, one WebSocket integration
and strict all-feature/all-target Clippy pass. Generated relocated run `szxAHb`
builds both SDKs, executes 11 vendored OIDC tests, verifies Basic/JWT/HMAC/session
policies in both JSON/text error modes and passes the complete HTTP/MCP startup
and shutdown matrix. An initial MCP assertion omitted the standard HTTP reason
phrase; the corrected exact-body assertion passes in the runtime receipt. All
944 embedded sources/templates remain packaged. Logs:
`/private/tmp/fl-auth-text-{canonical-expanded,tests,clippy,generated,runtime,startup,package}.log`.
Broader schema/errors/OpenAPI conformance remains open. Checklist remains
**658/682 (96.5%), 24 open**.


Terminal Stripe cancellation receipt (2026-10-13):
`StripePaymentService::reconcile_canceled_payment` reads authenticated provider
evidence in the configured Connect account. Only canceled intents with positive
cancellation time, zero received/capturable funds, explicit null latest charge
and matching intent/order/amount/currency qualify. It marks the local payment
failed, preserves its reference and atomically audits the operator. It never
mutates provider state or transitions/delivers an order.

`PaymentCancellationStore` implements memory/PostgreSQL snapshot checks and
atomic audit/update. Migration 7 uses forced tenant RLS; runtime roles need SELECT
and operators additionally need INSERT on the new audit table. Confirmation/claim
mutations reject recorded terminal cancellation. Capture and cancellation repairs
reject contradictory prior verified evidence. Existing ordinary failed-payment
confirmation behavior remains unchanged when no verified cancellation exists.

Provider tests reject 16 insufficient/mismatched cancellation responses and verify
read-only requests, Connect binding and secret exclusion. Restricted PostgreSQL
tests pass snapshot rejection, audit-write rollback, tenant isolation, persisted
audit fields, restart and blocked delayed confirmations. Base/Stripe suites and
strict all-feature/all-target Clippy pass. Generated relocated run `Nacuku` executes
the vendored provider/reconciliation tests, builds both SDK targets from both
artifact formats, and passes real HTTP/MCP, tenant/encryption, checkout/provider
webhook, inventory worker and restart checks. Eight actual checkout cases, eleven
actual webhook cases and four native replay/concurrency scenarios pass. All 947
embedded source/template files are packaged. Logs:
`/private/tmp/fl-terminal-payment-{tests,postgres,clippy,generated,package}.log`.

Pending/ambiguous claims, canceled intents with prior charges, PayPal cancellation
and canceled-order references remain open; this narrow repair does not close the
umbrella recovery gate. Checklist remains **658/682 (96.5%), 24 open**.

### 2026-10-13 — Public API source inventory and parse-error formatting

The checked source graph now covers all 44 tracked package manifests, 608 resolved
public type entry points and 1,662 declaration/re-export records. It retains
public method signatures, DTO type aliases, local/external re-export edges,
private module source lists and executable entry points. CI checks drift using
TypeScript 5.9.3 without loading application code. Regression checks cover
wildcard/root-directory resolution, explicitly exported validator test helpers,
packages without root exports, service members, DTOs and private module separation.
This is syntactic coverage; semantic Rust/fixture mappings remain open.

`forklaunch-validation` now exports the string-path `ParseError` DTO and
`pretty_print_parse_errors`. Formatting matches **918 actual TypeScript cases**
for absent/null/empty errors and prefixes, root/nested/empty path segments,
Unicode, control characters, multiline messages, ordering and three-digit error
numbers. It does not claim Zod/TypeBox/Garde validator equivalence.

Six native validation tests and strict all-target/all-feature Clippy pass. Fresh
relocated generated service `WQudHl` builds both SDKs and passes eleven vendored
OIDC tests. Structured session errors pass HTTP and MCP with exact decoded message
checks in JSON and text modes, plus the complete listener/startup/shutdown matrix.
The original fixture compared escaped JSON with literal newlines; that assertion
was corrected and both runtime modes rerun successfully. All **949** embedded Rust
source/template files are packaged, including the new formatter example. Logs:
`/private/tmp/fl-parse-errors-{parity,tests,clippy,generated,runtime,startup,package}.log`.
Checklist remains **658/682 (96.5%), 24 open**.


### 2026-10-13 — Testing environment and token compatibility

`forklaunch-testing` now maps `DatabaseType`, `TestEnvConfig`,
`setupTestEnvironment` and `TEST_TOKENS` to native typed configuration,
`setup_test_environment`, `TestService::with_test_environment` and the shared
mock token constants. Nine database spellings normalize to five families; SQLite
needs no container. Container endpoints expose explicit host/port mappings for
databases, Redis, Kafka and S3. Test-only defaults, custom overrides (including
empty strings and literal prototype-like keys), absent infrastructure and retained
baseline variables match **970 actual TypeScript source comparisons**. Missing
required mappings return an error. Native commands receive an owned environment
map; a real child-process check proves propagation without parent mutation.

Two existing native testing tests, strict all-feature/all-target Clippy, nine
release graph/packaging boundary tests and all **951** embedded source/template
files pass. No dependency was added to the default runtime; example serialization
uses dev dependencies. This does not implement Docker ownership/cleanup, ORM
setup, database clearing or the full blueprint harness. Logs:
`/private/tmp/fl-testing-env-{parity,tests,clippy,package,release-tests}.log`.
Checklist remains **658/682 (96.5%), 24 open**.

### 2026-10-13 — Typed Redis inspection and actual safe-value interop

`JsonCache` now exposes single/batch existence checks and batch deletion without
reading, decoding or decrypting values. Redis existence batches use MULTI/EXEC,
retain order/duplicates/misses, and support lists and corrupt ciphertext. Keys are
validated before sending batch commands. Literal-prefix scans remain separate
from explicit Redis glob scans, with sorted unique SCAN output. Prefix existence
results retain key/result pairs across concurrent disappearance. Native backend,
configuration and raw connection access map the TS configuration/client helpers.

The previous JSON interop fixture substituted JSON.parse/JSON.stringify for
TypeScript's safeParse/safeStringify, hiding top-level string differences. It now
loads both actual common helpers. `JsonRecordCodec::with_typescript_safe_values`
adds an explicit compatibility profile: strings are written verbatim, valid JSON
text parses to its JSON value, and non-JSON text falls back to a string. Strict
JSON remains default. Invalid UTF-8, missing/wrong tenant, bad ciphertext and Rust
type mismatches remain errors. Both envelope formats pass actual Redis tests for
ordinary/JSON-looking/malformed strings, scalar values, arrays/objects, rotation,
expiry, persisted keys, empty/duplicate batches, corrupt/list existence checks,
atomic invalid-key rejection, raw client access and glob/literal-prefix scans.
JavaScript special values/number rounding and falsy queue batch filtering remain.

Three codec unit tests, seven Redis tests including the real backend, strict
all-feature/all-target core/Redis Clippy, nine release checks and **951** packaged
Rust source/template files pass. Added an explicit serde derive dev dependency
for existing core tests that previously relied on workspace feature unification.
Fresh relocated generated infrastructure run `Dq9Tv6` passes ownership/lifecycle,
Redis/S3 binary and encrypted JSON round-trips, typed inspection and safe-string
checks, both SDKs and the full HTTP/MCP startup/shutdown matrix. Its production
codec/Redis sources match the final source. A separate fresh application from the
final rebuilt CLI also verifies the updated core dev-dependency manifest and
exact runtime sources. Logs:
`/private/tmp/fl-json-peek-{parity,clippy,codec-tests,tests,generated,package,release-tests}.log`.
Checklist remains **658/682 (96.5%), 24 open**.

### 2026-10-13 — Explicit TypeScript queue batch semantics

`JsonQueue::peek_records_typescript` and `dequeue_batch_records_typescript` now
apply TS truthiness filtering after authenticated decoding. Null, false, zero and
empty strings are omitted, while empty arrays/objects and literal private-protocol
marker keys survive. Counts apply before filtering. Zero-count dequeues remove
nothing; zero-count peeks use one LRANGE over the full current list. Ordinary
native methods preserve falsy values and retain empty zero-count behavior.
Authentication/type/parse failures propagate and failed peeks leave items intact.
Dequeues do not requeue removed records after a decoding failure. Supported counts
are nonnegative integers; negative/fractional JavaScript inputs are not emulated.

Actual TS Redis source, including its real safeParse/safeStringify functions,
passes both envelope formats, five count boundaries, mixed truthy/falsy values,
FIFO order, remaining lengths, wrong-tenant checks and corrupted-peek retention.
The fixture now builds the native example once and executes it for each case.
Seven native Redis tests including the backend and strict core/Redis Clippy pass.
Fresh relocated run `L0EhpR` validates the compatibility queue methods, encrypted
Redis/S3 factories, both SDKs, CLI ownership/lifecycle and the full HTTP/MCP
startup/shutdown matrix. All **951** embedded source/template files pass packaging.
Logs: `/private/tmp/fl-json-batch-{parity,clippy,tests,generated,package}.log`.
Checklist remains **658/682 (96.5%), 24 open**.


### 2026-10-13 — Optional signed base IAM sessions

Base IAM now supports explicitly configured RS256 bearer issuance via the
`jwt-sessions` feature. Generated modules accept a complete private-key/key-ID/
issuer/audience configuration and reject incomplete or invalid configuration
before listening. Tokens bind tenant/subject, server-resolved roles/permissions,
unique jti, issuer/audience and exact database-clock one-hour expiry. Full-token
SHA-256 storage preserves the local revocation, disabled-user, password-change
and tenant isolation checks. Opaque issuance remains the default. Public-key
hosting, directory identity integration, cookie/CSRF and OAuth/OIDC flows remain
open; downstream stateless JWT consumers require their own current-grant,
revocation and tenant policy.

Two native signer tests, restricted-role PostgreSQL signed and opaque session
suites and strict all-feature Clippy pass. Fresh relocated generated run `Jd8inE`
passes independent Node RSA signature/claim verification, unique issuance,
rejection of valid-but-unstored JWTs, opaque fallback, invalid-config startup
rejection, all directory HTTP/MCP operations, both SDKs, compliance, tenant
isolation, restart/logout, graceful shutdown and rename/delete lifecycle. The
expired directory-JWT fixture now expects the established TypeScript/Rust 403
policy; stored-session failures remain 401. CLI tests pass **1,101**, embedded
source packaging checks **953** assets and release-tool tests pass **9** cases.
No registry upload or external deployment occurred.
Logs: `/private/tmp/fl-iam-jwt-{unit,postgres,opaque,clippy,cli-tests,package,release-tests,generated-final}.log`.
Checklist remains **658/682 (96.5%), 24 open**.


### 2026-10-13 — Base IAM browser cookie/CSRF adapter

Optional `forklaunch-iam-axum/browser-sessions` adds BrowserIam using the existing
shared CookieSessions implementation. Explicit application-owned storage holds
the server-side credential; browser responses contain only the CSRF proof or
current subject/grants/expiry. Login rotates cookie and CSRF IDs and revokes the
prior credential. Logout revokes the IAM session before clearing the cookie.
Current persistent sessions, user state and grants govern each authentication.
The internal `/auth/browser` mount preserves shared HTTP middleware and keeps
browser endpoints out of OpenAPI/SDK/MCP operations. Existing bearer endpoints
retain their separate header requirement. All browser responses are no-store.

The restricted-role PostgreSQL suite passes browser fixation/CSRF/Origin checks,
current-grant changes, disabled users, tenant isolation, cookie replacement,
logout and password-change revocation, alongside the existing bearer suite.
Shared cookie tests pass, including authenticated HTTP/MCP requests. Fixture
schema-permission setup is serialized to avoid PostgreSQL catalog ACL races,
and successful fixtures remove their tenant rows before dropping owned roles.
Fresh relocated run `5EzzDH` passes mounted browser HTTP checks, unchanged 40-tool
MCP startup/discovery, signed/opaque sessions, both SDKs, directory/compliance,
restart/logout and CLI rename/delete. Packaging includes **955** source/template
assets; release-tool tests pass **9** cases and the source API inventory is current.

Tests explicitly use MemoryStore. Reusing it across router instances is not a
process-restart durability claim. Production durable-store generation and OAuth/
OIDC flows remain open; the broader checklist stays **658/682 (96.5%), 24 open**.
Logs: `/private/tmp/fl-iam-browser-{postgres-final,clippy-final,shared,generated,package,release-final,inventory}.log`.


### 2026-10-13 — Generated durable encrypted browser sessions

`PostgresCookieStore` now implements the shared session-store interface using
hashed cookie IDs, encrypted server-side records, tenant RLS and a server-owned
namespace. Sealed payloads bind record ID, namespace and indexed expiry. Collision
retries cannot overwrite an existing session, and updates cannot recreate deleted
or expired cookies. Retained encryption keys support rotation; expiry cleanup is
explicit and tenant/namespace scoped. Debug/errors omit keys and record contents,
and records are bounded to 64 KiB. Migration 3 adds the browser session table.

Generated IAM enables this store only when SESSION_COOKIE_ORIGIN is a valid exact
HTTPS origin. It shares the existing encryption key ring and uses the service name
as namespace; service rename requires browser re-login. Cookie persistence and IAM
writes are separate transactions, so failed cleanup can leave an unreachable
credential until its normal expiry. Operators grant runtime CRUD on the new table
and schedule expiry cleanup. No production MemoryStore fallback is introduced.

Three restricted-role PostgreSQL suites pass bearer/browser lifecycle plus durable
storage isolation, RLS, ciphertext/namespace/expiry swapping, retained-key rotation,
ID collisions, expiry cleanup, size limits and stale-write rejection. Strict IAM/
sample Clippy and workspace formatting pass. Fresh relocated run `2qSPQB` uses the
actual generated PostgreSQL wiring and proves a browser cookie survives process
shutdown/restart, rejects the other tenant and is invalid after logout. It also
passes invalid-origin startup rejection, signed/opaque sessions, both SDKs,
directory/compliance HTTP/MCP operations and CLI rename/delete. Packaging checks
**958** embedded source/template assets; CLI tests pass **1,101**, release-tool
tests pass **9** cases and
the source inventory remains current. Public-key hosting, identity/provider
integration and OAuth/OIDC still have their documented boundaries. No registry
upload or deployment occurred. Checklist remains **658/682 (96.5%), 24 open**.
Logs: `/private/tmp/fl-iam-cookie-store-{postgres,clippy-final,format,generated,package,release,inventory,cli-tests}.log`.


### 2026-10-13 — Stripe canceled intents with verified failed charges

Cancellation reconciliation now accepts an expanded failed charge only when its
intent reference, amount and currency match, paid/captured/refunded/disputed are
explicitly false, and captured/refunded amounts are zero. The parent intent must
still be terminal canceled with a positive cancellation timestamp, zero received/
capturable funds and matching merchant-scoped order identity. Null latest charges
remain supported; unexpanded references, missing evidence, pending/successful,
authorized or refunded charges remain rejected. Repair remains read-only at the
provider and atomically audited locally.

Two provider fixtures pass, including 28 new insufficient-evidence cases, successful
repair, delayed claim release and blocked later confirmation. The restricted-role
PostgreSQL audit/rollback/tenant/restart fixture and strict all-feature Clippy pass.
Evidence: `/private/tmp/fl-stripe-failed-charge-{tests,postgres,clippy}.log`.
Pending/ambiguous claims, paid/authorized prior charges, PayPal cancellation and
canceled-order reference reconciliation remain open. Checklist stays
**658/682 (96.5%), 24 open**.


### 2026-10-13 — ARM64 container and offline ECS smoke artifacts

A fresh generated dependency-free accounts service built using the generated
Dockerfile for Linux ARM64, matching the companion platform's Fargate architecture.
The native process passed the platform's curl health settings, HTTP health/OpenAPI,
MCP initialization/discovery/health invocation and foreign-Origin rejection. SIGTERM
exited cleanly with code zero; its owned container was removed. Image
`forklaunch-rust-ecs-parity:local-validation` and generated workspace
`/private/tmp/fl-rust-ecs-parity-yoqpv_u7` are retained for the pending cloud target.
Logs: `/private/tmp/fl-rust-ecs-image-arm64-build.log` and
`/private/tmp/fl-rust-ecs-local-runtime.log`. This is local container evidence.

`scripts/rust_ecs_smoke_plan.py` now creates reviewable task-definition and service
request JSON offline, with explicit target configuration, digest-pinned ECR image,
ARM64/Fargate/awsvpc, direct native argv, cleared entrypoint and platform health
settings. It requires an exact registered task revision before emitting a usable
service request. Eight tests cover account/region/revision constraints, malformed
network identifiers, unsafe inputs, collisions and actual CLI output. CI runs them.
Resource existence, VPC membership, permissions and connectivity require target
inspection; the tool neither contacts AWS nor provisions resources.

The available AWS account was inspected read-only; its existing unrelated app
cluster was left untouched. A dedicated test target is still awaiting the user's
selection. No image push or AWS mutation occurred. Actual ECS/load balancer,
provisioning, migration and scaling gates remain open; checklist stays
**658/682 (96.5%), 24 open**.


### 2026-10-13 — Explicit OIDC identities and local session issuance

Base IAM migration 4 adds tenant-scoped, case-sensitive issuer/subject bindings
to local users, forced RLS and cascading user deletion. Administrative bind/unbind
APIs require caller authorization, preserve existing ownership and never infer
identity from email. Unbinding prevents new issuance; existing sessions require
separate revocation. OIDC-enabled runtime roles need CRUD on the new identity table.

Optional `oidc-sessions` performs the actual framework provider exchange before
resolving the binding and issuing a persistent opaque or RS256 local credential.
Provider roles/emails grant no local privileges and provider tokens are discarded.
The identity/user lock serializes issuance with unlink, disable and deletion.
Returned sessions reuse the existing local expiry, tenant, current-grant and
revocation policy. The callback caller must consume pending state bound to the
initiating browser; HTTP callback and durable pending-state integration remain open.

Three provider tests pass, including a loopback provider's real discovery/JWKS/
nonce-bound RS256 ID-token exchange, a locally issued RS256 session, wrong-nonce
rejection, absent/same-email bindings, issuer/case/tenant isolation, restart,
revocation, disabled/deleted users, ownership conflicts and RLS. Strict IAM
all-feature/all-target Clippy and the minimal postgres-feature build pass. CI
explicitly executes the ignored PostgreSQL provider fixtures. Evidence:
`/private/tmp/fl-iam-provider-{tests,clippy}.log`. Broader provider/browser and
directory integration remain open; checklist stays **658/682 (96.5%), 24 open**.


### 2026-10-13 — Auth error schemas and safe handler-error wire compatibility

Opt-in TypeScript auth-error formatting now projects missing 401/403 plain-text
OpenAPI schemas on authenticated routes. Inline primitive schemas avoid application
component-name collisions; explicit contracts retain precedence and the runtime
registry is unchanged. Native default documents remain unchanged. Actual-source
canonical OpenAPI differences shrink from ten to six. The six remaining 400/404/
500/null-schema differences remain checked rather than being normalized away.

`HandlerErrorFormat::TypeScriptText` separately formats errors returned by
LocalRouter handlers as the actual TypeScript generic exception text with the
request correlation ID, without returning internal error details. It preserves
the default native JSON format and does not catch panics or rewrite arbitrary
Axum handler responses. The actual Express/native fixture now compares **52 live
HTTP outcomes**, including six thrown-handler outcomes with exact status, media,
text, correlation and secret-redaction checks.

Nine OpenAPI tests, four focused Axum tests, strict all-target OpenAPI/Axum Clippy,
and the live canonical comparison pass. Snapshot verification was repeated
without update mode. Evidence: `/private/tmp/fl-auth-openapi-{tests,clippy,canonical-check}.log`
and `/private/tmp/fl-handler-errors-{tests,clippy,canonical}.log`. Broad validator/
error/document parity remains open: **658/682 (96.5%), 24 open**.


### 2026-10-13 — Narrow PayPal terminal-void repair

`PaypalPaymentService::reconcile_voided_payment` reads authenticated order details
and requires VOIDED/CAPTURE, exactly one matching purchase unit, exact order
reference/amount/currency and explicitly empty captures/authorizations/refunds.
Missing histories, nonterminal states and previous monetary effects still require
review. The method records local failure through existing atomic actor audit,
stale-snapshot and tenant safeguards; it neither mutates provider funds nor
transitions the local order.

`VerifiedCancellation.canceled_at` is now optional because PayPal supplies no
verified cancellation timestamp. Stripe keeps its real timestamp; PayPal records
None. Migration 8 removes only NOT NULL, preserving existing timestamps and the
positive-value check. The separate reconciled_at column records observation time.
Callers using this public DTO must handle optional cancellation time.

Ten nonignored PayPal/base/Stripe tests, two restricted PostgreSQL repair/migration
tests and strict all-feature/all-target Clippy pass. Provider denial fixtures cover
missing/mismatched/ambiguous evidence, and the migration fixture exercises the
actual migration 7→8 while retaining existing Stripe values. Fixtures follow the
[PayPal Orders schema](https://developer.paypal.com/api/orders/v2/schema.json);
these are local provider fixtures, not PayPal sandbox observations. Evidence:
`/private/tmp/fl-paypal-void-{tests,postgres,clippy}.log`. Broad recovery remains
open: **658/682 (96.5%), 24 open**.


### 2026-10-13 — Request error envelopes and explicit empty-response documentation

`typescript_request_validation_error_response` formats caller-provided public
ParseErrors with TypeScript's Request prefix, paths/messages and correlation ID.
The final default Express 400 response uses plain text: although its parse
middleware initially selects JSON media, the enriched send applies the default
string response contract. Live testing caught this distinction before completion.
Explicit response declarations can override media. Native validator diagnostics
and default structured errors remain separate; Zod message equivalence is not
claimed by this formatter.

The TypeScript OpenAPI emitter now omits content for explicit undefined/void
responses. Null/nullish retain JSON body documentation and no-body responses
retain declared headers. Thirty actual Zod/TypeBox emitter cases pass, and the
existing live empty-404 case confirms the corrected document matches the wire.
The canonical document comparison now retains **five** real differences.

Validation passes **918** formatting cases, **25** actual request middleware
body/status comparisons, six native tests, strict validation Clippy, and **60**
live HTTP outcomes including eight request-validation outcomes. Snapshot checking
passes with update mode disabled. Evidence:
`/private/tmp/fl-request-errors-{parity-final,clippy-final,tests,canonical-check}.log`
and `/private/tmp/fl-openapi-no-body.log`. Checklist remains
**658/682 (96.5%), 24 open**.


### 2026-10-13 — Cancelled-order payment-reference review

`review_cancelled_order_payments` provides a tenant-bound read-only report with
1–100-entry keyset pages. It requires a cancelled order and distinguishes missing
references, unverified pending/claim-or-capture/failed states, recorded capture
or cancellation evidence, and contradictory evidence. A local Succeeded value
alone is not treated as proof of external capture. Every record/reference is
preserved; the report authorizes no refund, reference reuse or order reopening.

Memory holds record/audit locks together. PostgreSQL uses one MVCC statement per
page, selecting order status and payment fields without decrypting order PII.
Historical evidence must match payment ID/reference/amount/currency, and conflicting
provider names or capture/cancellation/status evidence is flagged. Existing audit
schemas cannot bind a historical merchant account or distinguish delete/recreate
with an identical tuple, so results remain historical evidence, not fresh provider
truth. Each page is independent; concurrent insertions before a cursor require
restarting the scan.

The shared memory/restricted PostgreSQL fixtures cover all classifications, empty/
missing/noncancelled orders, tenant isolation, pagination, stale replacement
references, amount mismatch and no PII output. One memory test, two PostgreSQL
tests (including timestamp migration regression) and strict all-feature/all-target
Clippy pass. Evidence: `/private/tmp/fl-cancelled-order-review-{tests,postgres,clippy}.log`.
Gate875 remains open for unresolved provider effects and reference reconciliation;
checklist stays **658/682 (96.5%), 24 open**.


### 2026-10-13 — Completed configured base IAM browser OIDC flow

The generated base IAM service now supports an explicitly configured standard
OIDC authorization-code/S256 PKCE flow. SESSION_OIDC_ISSUER/CLIENT_ID and optional
CLIENT_SECRET require the existing exact HTTPS cookie origin. Discovery and
configuration validation precede listening. Explicit tenant/issuer/subject bindings
resolve existing local identities; provider emails and roles cannot grant access.

Migration 5 adds encrypted tenant/namespace/browser-bound pending state. It has
absolute ten-minute expiry, one current flow per browser and atomic consumption
before provider exchange. Cookie deletion cascades pending rows. Callbacks survive
process restarts, rotate cookie/CSRF IDs and use existing local session revocation.
A static CSP-hashed callback page removes code/state from the URL, makes same-origin
CSRF-protected requests and navigates only to `/`. Callback GET never creates or
replaces a cookie. Public raw endpoints remain outside SDK/MCP operation metadata.

Validation passes four native PostgreSQL HTTP suites, four provider/pending tests
(including a claim race on independent restricted connections), twelve framework
OIDC tests, strict IAM/framework Clippy, browser-only feature compilation and VM
relay checks. Real Chromium separately proves cross-site Strict-cookie exclusion,
same-origin cookie binding, CSP execution, HttpOnly behavior, no Referer leakage
and rejection without the initiating cookie.

Fresh generated run `5zxkME` passes real discovery/JWKS/PKCE/code exchange through
the generated executable, pending-state survival across a process restart,
server-resolved grants, cookie/CSRF rotation, replay denial, logout and HTTP/MCP
startup. Existing password/opaque/RS256, tenant/encryption, directory/compliance,
both SDKs and source-preserving rename/delete lifecycle checks pass as well.
Packaging checks 974 embedded sources/templates; nine release-tool tests pass.
The owned generated workspace is removed after this receipt. Evidence:
`/private/tmp/fl-iam-browser-oidc-*.log`,
`/private/tmp/fl-iam-oidc-{generated,browser-relay,package,release}.log`.

This closes the configured base IAM provider-flow gate: **659/682 (96.6%), 23 open**.
It does not close directory credential integration, broader Better Auth social/
linking/adapter work, deployment or public registry gates. Base login discards
provider access/refresh tokens; downstream provider APIs need their own token
lifecycle. Generic provider refresh remains available in the framework.


### 2026-10-13 — Final Express parser error profile after main merge

`ParserErrorFormat::TypeScriptText` is opt-in for typed parser failures and
transport body limits. It matches main's final Express response: safe 4xx/5xx
text, UTF-8 media type and correlation ID, without raw parser input or private
verifier details. Native JSON defaults and ordinary schema diagnostics remain
unchanged. Both LocalRouter and Axum versioned routes are covered; Content-Length
and streamed body limits remain bounded. Arbitrary adapter errors and untyped
handler parser policies are outside this profile.

Three Axum matrix tests, seven HTTP tests and strict all-target Clippy pass.
The actual Express/Rust canonical fixture passes **88 live HTTP outcomes**,
including 28 new parser comparisons: malformed/strict-null JSON, parser and
transport limits, unsupported charset, verifier failure, correlation and no
handler execution after rejection. Checked OpenAPI snapshots remain enabled
without update mode; the same **five** real document differences remain.
Evidence: `/private/tmp/fl-parser-errors-{tests-final,clippy-final,canonical}.log`.
Gate806 stays open; checklist remains **659/682 (96.6%), 23 open**.


### 2026-10-13 — Verified interrupted checkout completion

The Stripe billing crate's opt-in `postgres-reconciliation` feature adds
`reconcile_checkout_webhook` for interrupted checkout completed/expired events.
Two authenticated GETs bind the pending event ID/type to the current subscription
checkout session, customer, live/test mode, terminal state and already-applied
SUCCESS/FAILED metadata. Connected-account/context events are rejected. The API
never repeats a provider mutation or treats checkout completion as payment
settlement.

Local repair requires an actor and existing encrypted local ownership matching
the verified provider session/customer. An opaque snapshot binds tenant, exact
pending attempt, encrypted cache/backup contents and row versions. Under the
event lock, compare-and-swap rejects stale state; backup status changes, inspected
cache removal, reconciliation audit, receipt and attempt removal commit together.
Absent ownership, unavailable provider events, missing required backup, inconsistent
statuses and other subscription/plan/payment-link/portal effects stay unresolved.

Validation passes two provider evidence/wire tests, 32 provider-evidence denials,
the restricted PostgreSQL suite, all 12 billing regression tests and strict
all-feature/all-target Clippy. PostgreSQL checks cover failures after local writes
and receipt insertion, rollback, same-content row replacement, stale attempts,
tenant/customer mismatch, concurrent one-winner completion, reopened receipt
visibility, backup-disabled expiry and partially completed backup updates.
Evidence: `/private/tmp/fl-billing-checkout-repair-{provider,postgres,clippy,regression}.log`.
Gate858 remains open; checklist stays **659/682 (96.6%), 23 open**.


### 2026-10-13 — Main integration and exact-session Better Auth authorization

Main is merged through `c5a4248d1` in merge commit `ac1f18e2a`, preserving the
existing adapter integration and main's auth encryption policy. All **1,120 CLI
tests** pass. The regenerated API inventory and regression check pass at
44 packages, 608 entry points and 1,662 declaration/re-export records.

Native application authorization now uses validated user/organization/session
scope from verified claims, checks that exact unexpired session and current
membership, and never selects a different session. Authenticated issuance adds
the persisted `sessionId`, including external signing. Old low-level helpers
without session scope confer no scoped privileges. Two new HMAC routes sign
all three scope components; legacy user-only routes return empty grants. Builtin
roles supply configured platform permissions; custom role values remain opaque.
Seven application contracts are exposed alongside the 70 raw auth routes.

Session primary/foreign-key comparisons follow PostgreSQL UUID case semantics;
organization/member text links retain exact matching. Actual PostgreSQL tests
confirm uppercase session IDs work, uppercase organization/user text membership
links do not broaden access, and wrong sessions fail. Updated CI pins the actual
MikroORM adapter at 0.5.11. Service-bound account/key encryption is verified with
context restoration, legacy marker rejection, encrypted columns, transaction
rollback and restart. Decoded real ORM records are consumed by native auth;
this does not assert interchangeable encrypted storage schemas.

Validation passes **52** actual TypeScript authorization comparisons, **7**
application contracts / **13** response schemas, **26** JWT/JWKS comparisons,
**49** configured protocol checks, **41** real PostgreSQL/ORM/native checks, two
native scope tests, 20 native HTTP/MCP tests and the final five-mount application
fixture. Strict all-feature/all-target Clippy passes. Rebuilt CLI and packaging
checks cover 977 embedded sources/templates. The generated scoped-route/runtime
rerun is in progress and will have its own receipt. Evidence:
`/private/tmp/fl-better-auth-scoped-{unit,http,application-final,conformance,contracts,jwt,reference,clippy}.log`,
`/private/tmp/fl-rust-scoped-orm.log`, `/private/tmp/fl-rust-main-cli-tests.log`,
`/private/tmp/fl-rust-main-package.log`. Broader Better Auth options/hooks and
other parity gates remain open: **659/682 (96.6%), 23 open**.


### 2026-10-13 — Generated scoped auth validation and adapter gate reconciliation

Generated Better Auth run `AO2PeS` passes scoped HMAC roles/permissions through
HTTP, the typed SDK and MCP; changed scope invalidates the signature, wrong
sessions return no grants and legacy routes remain empty with active ownership.
Persisted session IDs appear in signed JWTs. All eight actual frontend-client
profiles, shared/separate/disabled MCP, custom/root/URL-overridden auth namespaces,
self-JWKS, encrypted tenant isolation, restart and shutdown pass. Both SDKs build;
source-preserving rename, artifact checking, workspace compilation and deletion
pass. A stale six-tool assertion was corrected to eight; runtime/lifecycle checks
were rerun in the same isolated generated workspace with a fresh database role.
Evidence: `/private/tmp/fl-rust-scoped-generated{,-resume}.log`.

Review of the auth evaluation's selected adapter scope also closes its stale
integration recommendation. Both reviewed adapters now have optional features,
locked dependency review and the generated session/OIDC evidence above. Broader
framework adoption was deliberately deferred by the evaluation; passkeys/custom
provider options remain in broader IAM parity work. No extra framework is adopted
merely to close a checkbox. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13 — Malformed URI errors and conservative OpenAPI projection

The opt-in TypeScript parser error profile now handles malformed percent escapes
and invalid UTF-8 path parameters. Actual Express routing rejects these before
request-context creation, so the native compatibility response uses the same
safe 400 text and `No correlation ID` suffix. Default native JSON diagnostics
and valid escaped/Unicode paths are preserved.

OpenAPI infers a text 400 only for unversioned, unconstrained string path
parameters with no body/query/header validators. Every declared parameter must
match a complete placeholder in both runtime and OpenAPI paths. Explicit 400
responses are preserved; constrained, mixed and versioned validation cannot
be mislabeled as text-only errors. Registry/runtime contracts remain unchanged.

The native URI matrix, ten OpenAPI tests, strict Clippy and checked canonical
snapshots pass. **96 live HTTP outcomes** now include eight malformed URI
comparisons. Real OpenAPI differences fall from five to **four**: `/users`
400/404/500 and `/users/{id}` 500. Remaining schema-validator diagnostics and
adapter errors still keep gate806 open. Evidence:
`/private/tmp/fl-uri-errors-{openapi-tests-final,runtime-tests-final,clippy-final,canonical-final}.log`.
Checklist remains **660/682 (96.8%), 22 open**.

The completed generated auth workspace `AO2PeS` has been removed after its
committed validation receipt; logs and reproducible fixtures are retained.


### 2026-10-13 — Billing status and portal expiry corrections

Configured TypeScript status enums and native HTTP schemas now accept SUCCESS,
CREATED and UPDATED emitted by billing callbacks. The contract snapshot changes
only by adding these three values to ten schemas; unknown statuses still fail.
Both webhook implementations now interpret Stripe portal creation timestamps as
Unix seconds and add the application five-minute expiry with checked bounds.
Whole-valued decimal/exponent JSON numbers match integer spellings; malformed,
negative, fractional and out-of-range values fail before writes or receipts.
Provider URL validity and configured cache TTL remain separate.

Validation passes 141 actual configured mapper cases, 43 actual TypeScript/native
webhook state/provider-effect comparisons, 66 route contracts, 28 DTO schemas,
11 native all-feature tests and strict all-target/all-feature Clippy. Public API
inventory offsets were refreshed and its regression checks pass. Evidence:
`/private/tmp/fl-billing-status-expiry-{mappers,webhook,contracts,tests,clippy}.log`
and `/private/tmp/fl-rust-billing-fixes-inventory.log`.

Gate855 remains open for provider/local UUID identity handling, cache-only
configured mappers and remaining validator differences. No additional checklist
item closes: **660/682 (96.8%), 22 open**.


### 2026-10-13 — Better Auth server-error diagnostics

Raw authentication HTTP failures now emit default tracing diagnostics and support
a trusted `AuthHttp::with_api_error_observer` callback. The outer router observes
each 5xx once, including empty failures and rate-policy failures, and strips
internal response metadata. Events use matched route templates instead of token
paths; database fields are limited to bounded structural code/constraint/table/
column identifiers. Request data, messages, stacks and SQL row details are absent.
The PostgreSQL adapter preserves structural driver fields for these diagnostics.
Observer panics do not alter HTTP responses; application panic-hook policy remains.

The actual configured TypeScript fixture confirms ordinary success/4xx outcomes
and handled APIError500 do not invoke its unexpected-error hook. Native diagnostics
cover all raw 5xx responses, an explicit difference while error provenance is not
modeled. This does not close gate918 or claim exact hook dispatch equivalence.

Validation: three native observer tests, one real PostgreSQL metadata/redaction
check, 23 native HTTP tests, strict all-feature/all-target Clippy, minimal HTTP
feature build and the actual TypeScript hook/helper fixture pass. CI now includes
the new TypeScript and PostgreSQL fixtures. Release regression checks and embedded
source packaging pass. Evidence: `/private/tmp/fl-better-auth-observer-{native,pg,http,clippy,minimal}.log`,
`/private/tmp/fl-better-auth-error-observer-ts.log` and
`/private/tmp/fl-rust-observer-{release,package}.log`.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13 — Cache-only billing reads without entity mutation

Configured TypeScript checkout and portal reads now consume the cached response
DTO without invoking an update mapper or loading a database entity. Cache-only
sessions work with database backup either enabled or disabled; reads preserve
provider payloads and do not mutate ORM state or cache entries. The shared internal
helper restores expiresAt and optional createdAt/updatedAt Dates after the actual
Redis safeStringify/safeParse JSON boundary. Missing/malformed required dates and
malformed optional dates reject; opaque nested provider strings are unchanged.

Validation passes 229 configured mapper/cache cases, 168 actual TypeScript/native
base lifecycle operations, 144 Stripe SDK service operations and 43 webhook
comparisons. Strict standalone helper typechecking and refreshed public API
inventory checks pass. Evidence: `/private/tmp/fl-billing-cache-read-{mappers,base,stripe,webhook}.log`
and `/private/tmp/fl-rust-cache-read-inventory.log`. Provider/local identity mapping
and cache-only update mappers remain open in gate855.

Completed generated mixed-development applications in runs 4t05u9pg, v8f0ns32,
xb4uz99y and 3r582t9d were removed after checking their committed pass receipts
and absence of owned running processes/Compose resources. Their runtime logs
remain in the original run directories. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13 — Explicit directory and credential identity association

Base IAM now supports authorized one-to-one associations between existing
tenant-local directory users and credential/provider users. Migration0006 adds
composite foreign keys and forced RLS. The new administrative bind/unbind/exact-
pair deletion methods require a nonblank actor and commit directory audits with
their mutations. Matching email, profile fields, directory passwords, organization
and grants never create or populate a link. Ordinary directory deletion is
restricted while linked; coordinated deletion removes the credential user, all
persisted sessions/provider bindings and the directory row in one transaction.
Unlinking alone preserves the independent credential account and sessions. Legacy
auth-user deletion cascades associations with its existing unaudited semantics.

The resolver hashes the complete issued opaque or RS256 token and joins a current
persisted session, enabled user and existing tenant-local association. Arbitrary
user/session IDs, foreign tenants, altered JWTs and expired/revoked sessions fail.
The result is an identity association; callers still authorize directory operations
and resolve current grants. Native/generated READMEs describe explicit integration
and the additional bridge-table runtime grant. Default HTTP/MCP routes are unchanged.

Restricted PostgreSQL checks cover audit denial rollback, absent/blank actor,
competing bindings, bind/delete and login/coordinated-delete races, restart,
revocation/expiry, SQL tenant isolation and old runtime grants. Existing directory
and auth PostgreSQL regressions, strict all-feature/all-target Clippy, 44 actual
TypeScript/native operations, 33 route contracts, 22 schemas and five generic
compliance operations pass. CI includes the new ignored database fixture. Evidence:
`/private/tmp/fl-iam-directory-identity-{pg,existing-pg,auth-pg,clippy,ts}.log`.
Higher-level directory authorization and generic mapper/filter extensions keep
gate603 open. Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13 — Declared unversioned request validation boundary

An opt-in RequestValidators registry binds application callbacks to declared
unversioned request contracts and method/operation identities. Registration
rejects duplicates, missing/ambiguous routes and stale request schemas; validators
survive nesting and run after authentication, before local or Axum handlers, in
Error validation mode. Default/native checks are preserved. Raw query access keeps
repeated values available rather than silently using the flattened query map.
Callbacks own safe diagnostics; this is not a generic Zod translator or automatic
parsed-value transformation layer.

The canonical fixture supplies its actual integer/string query rules and compares
invalid, fractional, infinite, repeated, empty and unknown inputs against the real
TypeScript service. Handler counters prove rejected requests invoke no handler.
Explicit ECMAScript whitespace handling rejects leading/trailing U0085 while
accepting FEFF. The checked comparison now passes **132 live HTTP outcomes**,
including 36 query outcomes; four OpenAPI differences remain unchanged.

All 53 Axum library tests and strict all-target Clippy pass. Registration checks
and eight native local/Axum runtime scenarios cover nesting, Error/Warning/None
mode and auth-before-callback ordering. Evidence:
`/private/tmp/fl-query-validator-{axum-suite,clippy-final,canonical-final}.log`
and `/private/tmp/fl-query-validator-whitespace-final.log`. Generic validator
diagnostics/coercion/transformation and response validation remain open.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13 — Durable billing provider identity foundation

Migration5 introduces explicit administrative Stripe aliases with separate public
UUID, unchanged storage key and provider reference. Every lookup requires the
tenant, deployment label, account, live/test mode and resource kind. Provisioning
requires durable ownership, a nonblank actor and atomic audit; cache-only records
are rejected. Composite ownership foreign keys prevent deleting a bound record.
Opaque generation-bound revocation rejects stale handles after reprovisioning.
Audit RLS permits only tenant-local SELECT/INSERT, so broader non-bypass runtime
grants cannot rewrite or erase audit rows. Existing ciphertext and keys remain
unchanged; native/generated READMEs document the new grants.

The supplied account scope is explicitly administrative configuration, not proof
of account control. These APIs make no provider calls and do not yet alter service
wrappers, webhooks, checkout repair or public HTTP routes. No automatic migration
or providerFields-based inference occurs. Authenticated provider scope, runtime
wiring, cache ownership and portal replacement remain in gate855.

The database fixture also exposed asynchronous transaction-drop rollback after
an ordinary webhook callback failure. That path now awaits rollback before
returning, preserving the original callback error and separately committed intent
while releasing its advisory lock for immediate reconciliation. Cancellation and
interrupted-event behavior retain their existing checks.

Fourteen all-feature billing tests, including restricted PostgreSQL integration
and existing interruption/reconciliation cases, plus strict all-target Clippy pass.
Tests cover scope/tenant isolation, duplicate races, audit rollback, missing/blank
actors, immutable audits, stale revoke, cache-only rejection, restart and migration
compatibility from versions1–4. Both failed-run fixture tenants/roles were cleaned.
Evidence: `/private/tmp/fl-billing-provider-binding-{postgres,regression,clippy}.log`.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13 — Bounded response validation and TypeScript recursion fix

Opt-in ResponseValidators bind application callbacks to unversioned finite JSON/
text response contracts. Only registered Error-mode routes buffer response bodies,
with explicit collection byte/time bounds. Native status validation runs first.
Successful wire responses retain repeated headers and extensions; diagnostic
replacements retain unrelated metadata while clearing stale representation and
integrity headers. Collection size, timeout, read and UTF-8 failures return safe
native JSON 500. Trusted synchronous callbacks must return promptly; collection
timeouts do not preempt callback execution.

The separate response formatter renders trusted Header/Response ParseErrors into
the actual TypeScript Invalid response 500 text envelope. Live tests exposed
TypeScript recursively validating its own missing-header diagnostic, eventually
returning an HTML stack trace. A per-response WeakSet guard with try/finally fixes
that path without mutating shared route contracts. Two independent invalid
responses followed by a valid response prove validation remains active.

HEAD and bodyless 204/205/304 skip both header/body callbacks, preserving normal
HTTP behavior when Axum has removed the payload. This does not claim exact
TypeScript HEAD validation equivalence. Callback header inspection uses a
single-value map, though repeated headers survive the wire. Versioned and declared
streaming/binary routes are rejected by registration; generic schema translation
and Warning-mode diagnostics remain outside this slice.

Validation passes **152 live HTTP outcomes**, including 20 local/Axum response
cases; four OpenAPI differences remain. Sixty Axum library tests, six validation
tests, strict both-crate all-target Clippy and actual-source comparisons of
918 formatter, 25 request and 45 response cases pass. Public source inventory
remains current. Evidence: `/private/tmp/fl-response-validator-{canonical-final,tests-final,clippy-final,formatter}.log`,
`/private/tmp/fl-response-validator-head-{tests,clippy}.log` and
`/private/tmp/fl-rust-response-validation-inventory.log`.

### 2026-10-13 — Committed-checkpoint billing generation and cleanup

An isolated checkout at `893962e3d` passes all **1,120 CLI tests**, nine release
regression checks, CLI rebuild and **990 packaged source/template checks**.
Packaging initially caught missing template links for the new base IAM/billing
integration guides; the checkpoint includes their committed fix.

Fresh generated billing `run.13kXCU` passes base/Stripe HTTP/MCP startup, discovery,
health invocation and Origin rejection; all five service families, both SDKs,
JWT roles, configured party lookups, signed webhooks/platform events, keyless
managed calls, configuration failures, encrypted persistence, tenant isolation,
restart, MCP modes and graceful shutdown pass. Source-preserving rename, sync,
artifact checks, workspace compilation and deletion also pass. The generated
database role is removed. The temporary checkpoint checkout and generated apps
were removed after validation; logs remain at
`/private/tmp/fl-rust-checkpoint-{cli-tests,cli-build,package-final,release,billing-runtime}.log`.
This checkpoint validates the committed billing foundation; the newer opt-in
directory authorization and response-validator integrations have separate checks.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13 — Verified direct Stripe account context

The direct Stripe transport now verifies the configured account and live/test mode
using authenticated account and balance GETs. A sealed observation expires five
seconds after verification starts and is tied to the exact immutable client
generation; key prefixes and public request fields do not establish identity.
Managed verification fails before network access until the gateway supplies an
authenticated account/mode/generation contract. Existing wrappers are not yet
automatically gated, and this API neither provisions ownership nor performs
provider mutations. Eleven Stripe tests, 27 official-SDK/native transport cases
and strict all-target Clippy pass. Logs:
`/private/tmp/fl-billing-direct-context-{tests,sdk,clippy,regression}.log`.
The new guide is included through an explicit generated-workspace template link.
Checklist remains **660/682 (96.8%), 22 open**; scoped wrapper integration and
managed protocol support remain part of the billing identity gate.


### 2026-10-13 — Stored-session directory authorization and MCP audit correlation

Generated IAM supports opt-in `IAM_DIRECTORY_AUTHORIZATION=stored-session`.
JWT signature/issuer/audience checks precede exact persisted-token resolution.
An explicit tenant-local binding selects the directory identity; one locked
snapshot supplies current role names, permission slugs and organization. These
grants replace credential claims before framework authorization. The default
JWT-claims mode, internal HMAC routes and legacy bearer sessions remain available.
Native mode deliberately reads fresh grants; the actual TypeScript surfacing
oracle confirms its existing user-keyed cache semantics separately.

Full generated run `gujjFz` passes HTTP/MCP startup, discovery, Origin rejection,
both SDKs, current-grant removal/restoration with the same token, organization
moves, signed-but-unpersisted/tampered token denial, disable/unbind/revoke/delete
and restart. Existing OIDC/browser flows, classified compliance, tenant isolation,
source preservation, sync, rename and delete also pass. Audit assertions caught
missing MCP transport correlation forwarding; the fix gives the envelope header
precedence while preserving argument-only correlation. Four focused forwarding
cases and generated HTTP/MCP audit actor/correlation checks now pass.

Restricted PostgreSQL authorization/identity tests, strict base/IAM Axum/MCP
Clippy and the actual TypeScript grant oracle pass. Logs:
`/private/tmp/fl-iam-directory-auth-{pg,axum,clippy,generated}.log`.
The generated harness removes its database role; failed owned fixture workspaces
were removed. Generic entity/DTO mapper hooks, richer filters and caller-owned
transaction composition remain gate 603. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13 — Reusable TypeScript scalar coercion

Opt-in `forklaunch_validation::typescript_coerce` now exposes number, integer
number and boolean conversion for the JSON value domain (`None` represents
undefined). It matches the actual ForkLaunch Zod preprocessors for ECMAScript
whitespace, signed zero, decimal/radix literals, nested arrays, booleans, null and
plain JSON objects. Long radix literals use one ties-to-even rounding step;
integer results retain IEEE-754 semantics rather than imposing an i64 range.
Number schemas accept infinities as the actual unconstrained Zod v3 schema does;
integer schemas reject them. Arbitrary JavaScript objects/prototypes, Dates,
Symbols and functions are outside this helper's scope. Native strict coercion
remains unchanged. Canonical query validation now uses the public helpers.

**6,498 actual-source comparisons**, eight native validation tests and strict
all-target Clippy pass. Canonical HTTP remains **152 matching live outcomes**,
with four explicitly tracked OpenAPI differences. The differential fixture is
wired into CI. Logs: `/private/tmp/fl-scalar-coercion-{parity,tests,clippy,canonical}.log`.
Checklist remains **660/682 (96.8%), 22 open**; broader validator/schema and
diagnostic parity remains open.


### 2026-10-13 — Single-pass billing portal update mapping

The TypeScript base portal service now maps an update once and returns the DTO
written to cache. Previously it invoked the update mapper again after the cache
write without forwarding provider arguments, allowing a second database read or
mutation and loss of Stripe fields in the returned response. Cached declared
dates are restored before mapping; invalid cached dates reject before mutation.
The cache read uses the DTO type rather than the entity type.

All **237 configured-mapper cases**, **168 TypeScript/Rust base operations** and
**144 official-SDK TypeScript/Rust Stripe service operations** pass. New cases
check both configured variants and backup modes, exactly one request/response
mapping pass, provider payload preservation, returned/cache consistency and
invalid-cache rejection before writes. Logs:
`/private/tmp/fl-billing-single-update-{mappers,base,stripe}.log`.
These tests supply the entity expected by configured update mappers; reconstructing
entities for cache-only updates remains open. Public API inventory is refreshed
and checked (44 packages, 608 entry points, 1,662 declarations).

The current CLI's Rust/mixed release regression also passes after fixing its
owned command fixture to read identity from the explicit Cargo manifest instead
of treating the manifest pathname as a package name. No product release behavior
changed. Receipt: `/private/tmp/fl-rust-current-release.log`.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13 — Scoped durable Stripe checkout reads

The opt-in `postgres-identity` feature adds `get_bound_checkout_session` for
explicit public UUID aliases. Each read verifies the direct Stripe account/mode,
resolves tenant/account/mode/deployment/kind ownership, then fetches only the
bound provider object. Object ID/type, subscription mode and customer ownership
are checked. A final joined database statement reads the exact binding generation
and encrypted durable record, appends a sensitive-read audit, and is followed by
another context-expiry check. Public aliases leave storage IDs/ciphertext intact.
This is a point-in-time read; it does not lock out future revocation.

All **17 base/Stripe regression tests**, including restricted PostgreSQL integration,
and strict all-target Clippy pass. Coverage includes unbound/foreign scopes,
malformed/deleted customers, replacement during the provider GET, audit denial,
expired context and unchanged encrypted storage. CI runs the new ignored PG test.
Logs: `/private/tmp/fl-billing-bound-read-{postgres,clippy,regression}.log`.

This API requires durable ownership and an existing Stripe customer reference;
local customer UUIDs need their own trusted mapping and fail closed here. Existing
cache-based trait reads, managed gateway mode, HTTP routes and provider mutations
are not switched automatically. Broader identity/wrapper integration remains
open. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13 — Typed directory mapper and transaction extensions

`BaseIamDirectory::unit_of_work` provides an explicit caller-owned transaction
for the four existing directory families. Sealed entity markers bind each family's
create/update/read DTOs; custom mapper hooks accept typed application input, output
and context while operating within the same tenant/audit/organization scope.
Related operations see staged writes and commit together. Returned errors,
projection failures, canceled futures and caught mapper panics poison the unit,
preventing later commit of partial graph/audit changes. Core mapping, password
ignoring, relationship validation, encryption and identity deletion restrictions
remain in force. No raw connection or arbitrary SQL/table extension is exposed.

Five focused tests, 19 base regressions, restricted PostgreSQL graph/audit rollback,
strict all-target Clippy, minimal-feature compilation and an actual TypeScript
custom-mapper oracle pass. The oracle covers caller EntityManager/extra arguments,
custom projection and deferred service flush; it records that TS permission
update omits extra arguments. Native context propagation is uniform. Actual
atomic rollback is proven against PostgreSQL, not inferred from the owned TS ORM
fixture. CI runs the new oracle and ignored PG test. Logs:
`/private/tmp/fl-iam-unit-of-work-{native,pg,clippy,regression,minimal,ts}.log`.
The committed generated IAM workspace was cleaned after process/role checks.
Safe filter extensions and broader custom persistence semantics remain open;
checklist **660/682 (96.8%), 22 open**.


### 2026-10-13 — Explicit TypeScript date-coercion profile

`typescript_coerce::date_epoch_millis` supports numeric JavaScript epochs and
a deterministic ISO string profile: UTC date-only/partial dates, signed six-digit
years and explicit-offset datetimes. Integer milliseconds span the full JavaScript
Date range, including TimeClip limits, Gregorian rollover, fractional truncation
and valid zero-only 24:00. Non-date JSON types preserve actual Zod diagnostics.
Native date parsing remains unchanged. Legacy/local-time strings and ambiguous
V8 fallbacks return a distinct `UnsupportedStringFormat`, not a fabricated Zod
rejection; this is deliberately not full `Date.parse` compatibility.

**7,633 actual-source outcomes** across UTC, Los Angeles and Kathmandu, **602
explicit unsupported cases**, 11 validation unit tests and strict all-target
Clippy pass. CI runs the new oracle. Logs:
`/private/tmp/fl-date-coercion-{parity,tests-final,clippy-final}.log`.
Broader runtime/schema validation remains open; checklist **660/682 (96.8%),
22 open**.


### 2026-10-13 — Configured cache-only portal updates

Base/Stripe portal mapper contracts now allow an optional `toCachedDto` hook.
With database backup disabled, the base service passes the normalized cached DTO,
selected EntityManager and extra arguments directly to that hook and writes its
validated result once. The configured application mappers implement this path
without ORM allocation, lookup or mutation, using their actual request/response
schemas. Session/customer identity remains fixed; declared dates are restored and
Stripe provider payload aliases are updated together. The configured cache-only
Stripe wrapper rejects customer reassignment before creating a provider session.
Custom mappers without the optional hook and database-backup paths retain entity
mapping. Custom hooks own their projections and must preserve identity.

**249 configured-mapper cases** pass, including actual composed Stripe/base
services, absence of a persisted entity, no ORM reads/writes, extra context and
caller EntityManager propagation, provider replacement, immutable identities,
invalid dates and unchanged cache on rejected mapping. **168 base** and **144
Stripe service** TypeScript/Rust operation comparisons pass. Public API inventory
is refreshed and checked. Logs: `/private/tmp/fl-billing-cache-update-{mappers,base,stripe-final,inventory}.log`.
A full workspace TypeScript build was attempted but cannot resolve absent
workspace dependency links; the available isolated compiler also predates the
repository's TypeScript 7 config. These runtime/source fixtures do not claim a
full workspace typecheck (`/private/tmp/fl-billing-cache-update-tsc.log`).

Provider/local effects are still non-atomic, and generic/backup customer-update
semantics remain separate from the configured cache-only profile. Remaining
provider identity, reconciliation and validator work keeps the billing gates
open. Checklist **660/682 (96.8%), 22 open**.


### 2026-10-13: bounded typed directory filters

Caller-owned directory transactions now support typed `list` and `mapped_list`
filters over declared fields. Comparisons preserve SQL three-valued NULL logic,
UUID normalization only for identities, literal text and relationship membership.
Tenant/organization scope stays outside application predicates. Validation bounds
depth, nodes, values and bytes before reads; evaluation and candidate scans are
bounded, with explicit scan-limit errors. Failures poison the whole transaction.

Verified: 33 actual MikroORM/PostgreSQL comparisons, 23 native tests, restricted
encrypted PostgreSQL audit/rollback checks, strict Clippy and minimal-feature
compilation. CI runs the actual ORM comparison. Receipts:
`/private/tmp/fl-iam-filter-{orm,native,pg,clippy,minimal}.log`.

Empty logical groups, arbitrary fields/JSON paths, range/date/LIKE operators,
sorting, pagination and custom ORM entities remain unsupported. This does not
close generic directory extensions. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13: generation-scoped managed checkout reads

Companion commit `b91838722` adds dedicated signed context and checkout-read
endpoints. Contexts bind instance, organization, connected account, explicit mode,
API version and database-generated binding generation within five seconds.
Fresh ownership reads before/after provider work reject rebinding, including ABA
changes; bounded provider bodies, immutable configuration and authenticated scope
echoes prevent stale or ambiguous evidence from becoming durable checkout reads.
The Rust client and local gateway mock implement the same read-only protocol.
Existing financial mutation routes are unchanged.

Verified: 29 actual-source platform tests, including controller HMAC, replay,
rate limits, no-store refusals and real MikroORM/PostgreSQL fresh binding queries;
11 native tests plus both restricted PostgreSQL bound-checkout variants; strict
Clippy, focused actual-dependency TypeScript checks, signed cross-language HTTP
and real gateway-mock conformance. Fixtures cover replacement races, final
database-wait expiry, audit failure and ciphertext preservation. Receipts:
`/private/tmp/fl-managed-payments-context-tests.log` and
`/private/tmp/fl-billing-managed-context-{native,postgres,clippy,http}.log`.

Full platform bootstrap/typecheck, live Stripe and cloud deployment remain
unvalidated. Durable alias reads still require explicit trusted ownership and
provider-customer mappings; broader billing integration/reconciliation stays
open. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13: explicit request-validator rejection contracts

`RequestValidators::insert_with_rejection` attaches caller-owned response metadata
to the effective contract registry, OpenAPI, SDK and MCP exports. Existing route
declarations cannot be overwritten; replacing configuration removes only its
contributed metadata. Runtime status/media mismatches return a sanitized 500.
Body-schema correctness remains the callback author's responsibility, and empty
text bodies remain valid. Warning/disabled validators do not add rejection docs.

Verified: 64 Axum and 24 contract tests, strict Clippy including MCP, eight focused
request-validation tests after review, and updated then normally verified 152
live TypeScript/Rust HTTP outcomes. Canonical differences fall from four to three
by documenting the existing GET /users query 400 callback. List 404 and both 500
differences remain; no unreachable response was added to force equality. Logs:
`/private/tmp/fl-request-rejection-{tests,clippy,clippy-mcp,canonical-update,canonical}.log`
and `/private/tmp/fl-request-rejection-review-{tests,clippy}.log`.

Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13: configured checkout public/provider identity reads

Configured Stripe checkout reads now resolve the provider reference from the
server-written cached DTO through an optional mapper hook. The wrapper checks
requested/cache identity before retrieval; the configured hook validates actual
response schemas, object type, subscription mode, live/test mode and known
provider-customer evidence. Provider responses cannot replace the public ID or
local customer relation. Customer assignment from an initially null Stripe
customer is admitted without rewriting the application relation. Custom mapper
fallback preserves the existing provider-ID-first behavior. These cached
references do not establish connected-account ownership; the separately scoped
gateway protocol remains necessary for managed rebinding safety.

Verified: 295 actual-source configured mapper cases and 144 legacy
TypeScript/Rust Stripe service comparisons. A focused strict TypeScript5.9
check of the actual base/Stripe service files and transitive framework source
also passes with real dependency types and explicit source export mappings:
`/private/tmp/fl-billing-focused-types/tsconfig-services.json`. This caught and
fixed an earlier OpenAPI generic type error: absent-body markers are now optional
validator capabilities and guarded before schema-kind checks. Canonical runtime
and document checks pass. Full configured-module typechecking still reaches
unrelated Bun ambient/Express shim type mismatches; it is not claimed clean.
Logs: `/private/tmp/fl-checkout-configured-stripe-services.log`,
`/private/tmp/fl-billing-focused-services-final.log`, and
`/private/tmp/fl-billing-focused-types-configured.log`. Public inventory remains
44 packages, 608 entrypoints and 1662 declaration/reexport records.

Configured mutations still need explicit ownership/identity integration, and
remaining URI/schema/mapping boundaries stay open. Checklist: **660/682 (96.8%),
22 open**.


### 2026-10-13: undocumented-response diagnostics and error-handler reentrancy

The opt-in `ResponseStatusErrorFormat::TypeScriptText` covers framework-detected
undocumented response statuses in local and Axum handlers, including selected
versions. Existing JSON defaults and documented handler responses remain intact.
The profile currently preserves native rejection status 500; TypeScript's
selection of an existing 4xx/5xx status, default error-string union and empty
`.send()` bypass are separate remaining semantics.

Actual-source probes also exposed TypeScript final-error reentrancy: the final
diagnostic was validated against the same failing route, causing Express to emit
an HTML stack trace. The final framework diagnostic now uses the raw Express
sender, retaining status selection, redaction and correlation. Ordinary handler
response validation still runs. Repeated rejection requests remain independent.

Verified: 68 Axum and 40 HTTP tests, strict Clippy, 168 cross-runtime HTTP
outcomes and two repeated TypeScript 418 regression cases. Three canonical
OpenAPI differences remain explicit. Receipts:
`/private/tmp/fl-status-error-{oracle,tests,canonical,clippy}.log`.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13: request correlation through directory CRUD and compliance

Axum handlers now receive the same `RequestContext` used by authentication and
validation. Directory CRUD and compliance operations reuse that correlation with
the verified actor, replacing independently generated audit IDs. Generated
HTTP/MCP probes check actual create/read/update/delete audits (separate from
stored-session snapshot reads), explicit and generated correlations, HMAC service
actors, directory actors and classified compliance export/erase audits.

Verified: focused explicit/generated/public context regressions, 67 Axum tests
at the IAM checkpoint, strict IAM/framework Clippy, then full generated IAM
HTTP/MCP, both SDKs, OIDC/browser sessions, encrypted persistence, restart,
revocation and rename/sync/delete lifecycle. The new fixture was corrected to
the declared create contract (201 text); created IDs are recovered from exactly
one tenant/actor/correlation/create audit row before subsequent exact-ID checks.
Logs: `/private/tmp/fl-iam-crud-context-{native,regression,clippy,framework-clippy,generated}.log`.

The fresh generated billing checkpoint also passes HTTP/MCP startup, both SDKs,
encrypted storage, restart, managed configuration and complete CLI lifecycle:
`/private/tmp/fl-rust-current-billing-runtime.log`. Package verification includes
1011 embedded Rust source/template assets. Checklist remains **660/682 (96.8%),
22 open**.


### 2026-10-13: TypeScript response warning mode

Undocumented nondefault response statuses now produce an explicit contract
diagnostic before attempting schema compilation. Error mode retains final
redaction; Warning logs and sends the response; None bypasses validation.
Declared-schema failures also respect the resolved global Warning setting,
which the previous switch ignored. Arbitrary validator exceptions are not
swallowed.

Verified: 48 actual-source repeated HTTP outcomes and 20 warning emissions,
covering local and global settings and declared-schema failures. The independent
JavaScript oracle is wired into CI with the pinned canonical dependencies.
Receipt: `/private/tmp/fl-status-warning-oracle.log`. Broader default error
unions and empty-send semantics stay open. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13: portal replacement preflight

Configured cache-only portal updates now validate the cached public/customer
identity and actual update/response schemas before creating a replacement Stripe
session. Malformed or foreign cache data and invalid updates produce no provider
call or cache write. The optional hook preserves custom and database-backup paths.
Verified: 305 configured mapper cases, 144 legacy TypeScript/Rust service
comparisons and focused strict source typechecking. Receipts:
`/private/tmp/fl-portal-preflight-{mappers,types,legacy}.log`.
Checklist remains **660/682 (96.8%), 22 open**.


### 2026-10-13: packaged source checkpoint after HTTP/IAM context changes

The CLI archive containing 2359 files was installed from its extracted source
and used to generate relocated services, billing/IAM/messaging/ecommerce
modules and durable workers. Packaged framework tests, optional infrastructure
feature compilation, generated workspace checks, OpenAPI/AsyncAPI exports,
Rust/TypeScript SDK generation and HTTP/MCP surfaces pass. The native source
checkpoint includes commit `3bf5bfcbe`; subsequently prepared URI/filter/status
slices have their own validation receipts. This checks local source installation,
not public registry publication or AWS deployment.

Receipt: `/private/tmp/fl-rust-current-packaged-install.log`. The owned extracted
archive workspace was removed by the passing harness. Committed generated
billing `run.Jr7VIh` and IAM `run.dTQtrb`/`run.7W1Dep` workspaces were also removed
after process/database-role checks; logs were retained. Checklist remains
**660/682 (96.8%), 22 open**.


### 2026-10-13: empty directory logical-group normalization

Bounded filters now admit empty logical groups with actual MikroORM SQL
normalization. Empty AND is an omitted SQL fragment, empty OR is false, and
negating an omitted group is false. Omission remains distinct from SQL UNKNOWN
and Boolean true: `Any([All([]), predicate])` matches only the predicate.
Mandatory tenant/organization restrictions remain outside these expressions.
This supersedes the earlier unsupported-empty-group limitation.

Verified: 3282 actual MikroORM/PostgreSQL-to-native comparisons, including
exhaustive binary groups and unary negation through depth two over empty and
ordinary/nullable predicates; 20 native tests; restricted PostgreSQL scope,
audit and rollback tests; strict Clippy. Receipts:
`/private/tmp/fl-iam-empty-filter-{native,pg,orm,clippy}.log`. Broader arbitrary
filters, indexed traversal and custom persistence remain open. Checklist:
**660/682 (96.8%), 22 open**.


### 2026-10-13: opt-in response diagnostic status preservation

The TypeScript-text status rejection profile now preserves rejected 400–599
statuses and chooses the corresponding redacted client/server diagnostic.
Undocumented success/redirect statuses still become 500. Native JSON defaults
remain unchanged; documented responses retain their existing behavior. This
supersedes the preceding text-profile limitation that all rejections became 500.

Verified: 68 Axum and 40 HTTP tests, 216 combinations of policy/mode/local or Axum
handler/version/status, strict Clippy, and 184 cross-runtime HTTP outcomes.
Repeated 418/503 failures now match actual TypeScript status/text/media; declared
500 responses remain intact. The standalone warning fixture also passes 48
outcomes and 20 warnings. Receipts:
`/private/tmp/fl-status-preserved-{tests,clippy,canonical,warning}.log`.
Default error-string admission and empty-send semantics remain separate work;
three canonical document differences remain. Checklist: **660/682 (96.8%),
22 open**.


### 2026-10-13: optional Stripe checkout URI across runtime and contracts

Configured Stripe checkout create/response schemas and DTOs now admit an absent
URI, matching native optional values and provider sessions without a URL.
Nullable entity storage writes SQL NULL; response mappers normalize it to an
absent DTO field. Native contract generation no longer forces URI into Stripe's
required fields. Existing applications with database backup must apply the
reviewed nullability migration before deploying nullable writers; the generated
[checkout URI migration guide](../blueprint/billing-stripe/CHECKOUT_URI_MIGRATION.md)
explains deployment and rollback. Customer identity semantics are unchanged.

Verified: 32 actual Zod/TypeBox schema, mapper and JSON-cache lifecycle cases;
actual configured MikroORM entity under a restricted PostgreSQL role, including
old NOT NULL rejection and upgrade; 305 configured regressions; focused strict
service types; 66 actual TS/native routes and 28 validator/OpenAPI DTO schemas.
The reviewed contract snapshot changes exactly two required-field entries. CI
runs the new fixture with TypeBox and PostgreSQL and watches private billing
module sources. Receipts: `/private/tmp/fl-checkout-uri{,-configured,-types,-contracts}.log`.

The TypeBox versus Zod create stripeFields-presence discrepancy is resolved in
the follow-up below. Provider customer mapping and mutation reconciliation remain open.
Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13: optional checkout provider options

Create checkout `stripeFields` is now explicitly optional in both configured
validators and the public TypeScript DTO, matching native omitted options.
Update options remain optional; response provider evidence remains required.
Omitted options and an empty object produce identical provider requests, without
inferring a provider customer from the application customer ID. Explicit Stripe
customer and metadata options are preserved.

Verified: 44 actual Zod/TypeBox URI/options/schema/mapper/cache cases, 305
configured regressions, 66 actual TS/native route contracts and 28 DTO schemas
with unchanged contract snapshots, plus focused actual-service TypeScript
compilation and a compile-only DTO optionality fixture. The standalone
`rust_billing_dto_types.mjs` runs the latter against actual DTO/common source and
Stripe SDK types in CI, without declaration stubs. The public source
inventory remains 44 packages, 608 entry points and 1,662 records. Contract
receipt: `/private/tmp/fl-checkout-options-contracts.log`. This does not tighten
the existing arbitrary-value `type<T>()` runtime validator or complete provider
identity and mutation reconciliation. Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13: configured payment-link public/provider read identities

The configured TypeScript Stripe payment-link mapper now opts into a cache-first
read hook. The wrapper checks the requested public ID against the cached DTO,
then validates the retained `plink_` reference, object discriminator and mode
before issuing the provider GET. The returned provider ID and mode must match.
The response retains public/local fields with fresh provider evidence, without
rewriting cached state. Custom mappers without the hook keep their prior read
behavior. This is read-reference validation, not proof of provider-account
ownership or authorization for financial mutations.

The actual configured mapper fixture passes 325 cases, including 20 payment-link
cases for cache failures, malformed identities, provider failures/mismatches,
backup mode and custom mapper fallback. Focused strict compilation includes the
actual payment-link service and transitive source, but is not a full configured
module/workspace type check. Receipts: `/private/tmp/fl-payment-link-read-types.log`
and `/private/tmp/fl-payment-link-read-configured.log`. The 144 legacy TS/native
service comparisons and normal 66-route/28-schema contract oracle also pass
(`/private/tmp/fl-payment-link-read-{legacy,contracts}.log`).

Native default payment-link creation still uses provider IDs; configured TS
creation uses application UUIDs. Scoped durable native alias reads, configured
listing and mutation identity/reconciliation remain separate work. Checklist:
**660/682 (96.8%), 22 open**.


### 2026-10-13: optional billing-portal create options

Portal creation now explicitly permits omitted `stripeFields` in both validators
and the public DTO, matching the native optional provider options and existing
wrapper payload behavior. Updates remain optional and response evidence remains
required. Twelve actual Zod/TypeBox cases exercise configured mappers and both
service layers with omitted, empty and explicit options, in cache and backup
dispatch modes. They verify identical omitted/empty provider requests, retained
return URLs, customer identity, response evidence and cache/persistence dispatch.
ORM, cache and Stripe transport are fixture seams; this does not claim a live
provider or ORM integration check.

The public DTO compiler now checks portal as well as checkout optionality in CI.
Focused actual-service type checking passes (`/private/tmp/fl-portal-options-types.log`).
The 325-case configured regression and normal 66-route/28-schema oracle pass
with unchanged contract snapshots. Schema/mapper receipt:
`/private/tmp/fl-portal-options-schema.log`.
Other create-option schemas and provider identity/reconciliation remain open.
Checklist: **660/682 (96.8%), 22 open**.


### 2026-10-13: bounded default error strings and canonical OpenAPI agreement

`DefaultErrorResponses` registers selected missing 400/401/403/404/500 statuses
for unversioned routes, with positive byte/time limits and explicit-declaration
conflict checks. Error-mode runtime admission and effective registry projection
share the same selections. Finite text, serialized JSON strings and absent
bodies are admitted; invalid values and collection failures produce redacted
text. Selected responses normalize at Axum's matched middleware boundary,
covering early failures and response callbacks. Parser/auth profile checks reject
known incompatible combinations. Explicit declarations, callback header checks,
nesting and configuration reset are covered.

The canonical comparison passes 366 live TS/Rust outcomes, including 182 added
default-error comparisons and 42 on documented IAM paths. Only after those
checks passed were the final three normalized OpenAPI differences removed.
Successful responses, parameter and auth schemas are unchanged. Native suites
pass 76 Axum and 43 HTTP tests, with focused early-return regressions and strict
MCP-enabled Clippy. Receipts: `/private/tmp/fl-default-errors-{tests,clippy,canonical,canonical-update,review-tests,core-review}.log`.
The framework README documents usage and the source API
change: `ForkLaunchRouter::registry()` now validates profiles and is no longer
a const function.

This proves agreement for the canonical IAM fixture. Gate 209 still requires
broader HTTP/AsyncAPI artifact coverage; gate 806 still includes native schema
diagnostics, router/extractor origins and versioned default-error behavior.
Zero-byte transport bodies cannot preserve TypeScript's original distinction
between absent values and empty buffers; direct LocalRouter handler errors
remain its separate `Err` result channel. Checklist: **660/682 (96.8%), 22 open**.

## PostgreSQL member-query and generated auth receipt (2026-10-13)

Generated Better Auth services now opt into PostgreSQL member-query semantics;
direct library stores retain the memory-adapter default. The actual configured
MikroORM/native matrix passes 434 cases covering UUIDs, timestamps, SQL LIKE,
collation, filters, sort errors and fractional pagination. It passes in UTC and
in explicitly asserted America/Los_Angeles sessions, including date-only
predicates whose result counts change with the timezone. Query operands and
transient projections are bounded; projection overflow fails explicitly.
Ordinary indexed pages still work above the 4,096-candidate projection limit.
Restricted PostgreSQL tests verify mandatory organization scope, UUID spelling
preservation, size/count rejection and fixture rollback. Strict Clippy passes.
Receipts: `/private/tmp/fl-iam-member-pg-{matrix,timezone,unit,regression,clippy,build}.log`.

A freshly rebuilt CLI generated and relocated the updated preset. Actual
blueprint and generated frontend clients pass default/custom/root mount flows;
HTTP/MCP shared, separate and disabled listeners, both SDKs, encrypted sessions,
JWT/JWKS, HMAC/role enforcement, restart, sync and rename/delete all pass.
Receipt: `/private/tmp/fl-latest-member-profile-generated.log`.
The owned generated workspace was removed after confirming its processes and
database roles were gone. Broader auth hook/adapter/options coverage remains
open under gate 918; this receipt does not close that gate.

## Framework router fallback receipt (2026-10-13)

`RouterOriginFormat::TypeScriptHtml` is an opt-in profile for framework-owned
unknown-path and unsupported-method responses. One hundred actual TypeScript/
Rust request comparisons (200 HTTP observations) match status, HTML/text bytes,
media, HEAD representation length, CSP/nosniff and Allow handling. The encoder
matches the installed Express finalhandler's encodeurl and HTML escaping,
including malformed percent runs. With TypeScript parser formatting, malformed
matched parameters produce the reference 400 before method rejection.

All 80 Axum tests and strict MCP-enabled all-target Clippy pass. Tests preserve
native defaults, application responses, custom method and nested/root internal
fallbacks, exported methods and explicit OPTIONS. CORS and correlation are
separate policies; no arbitrary application response is rewritten. Framework
README usage and CI are updated. Receipts:
`/private/tmp/fl-router-origin-{tests,clippy,parity}.log`.
Gate 806 remains open for its other origins/profiles and schema diagnostics.

## Plan/subscription create-option receipt (2026-10-13)

Plan and subscription create DTOs now explicitly allow omitted Stripe options,
matching the existing service payload construction and native contracts. Zod
and TypeBox schemas agree; the services still derive required provider fields
from their application DTOs. Twelve actual dual-validator cases cover omitted,
empty and explicit metadata options through configured mappers and both service
layers, with exact provider payload and persisted-response checks. Actual public
DTO compilation verifies optional create/update fields and required response
evidence; payment-link create options remain required. Focused strict compilation
of the real billing services passes, as do 325 configured mapper cases and the
unchanged 66-route/28-schema contract snapshot. CI and the 44-package, 608-entry,
1,662-declaration source inventory are updated.
Receipts: `/private/tmp/fl-recurring-options{,-configured,-types,-contracts,-inventory}.log`.

## Managed payment-link read receipt (2026-10-13)

Companion commit `5e2e9bfcb` adds the dedicated signed payment-link read to the
existing immutable account/mode/generation context protocol. Fixed internal kinds
select provider paths, ID prefixes and object evidence; caller paths are never
accepted. The native durable public-UUID read now supports direct and managed
clients, with exact scope-envelope, generation, expiry and audited encrypted-row
checks. Local financial fields/ciphertext remain unchanged; alias provisioning,
mutation integration and HTTP route wiring remain explicit separate work.

Validation passes 32 actual companion protocol/controller/database tests plus
focused strict TypeScript, signed native HTTP and exact mock-source conformance,
2 direct/managed restricted PostgreSQL fixtures, 8 native unit tests and strict
all-feature/all-target Clippy. The new PostgreSQL fixtures are in CI. The
cross-repository fixture still requires an explicit companion source checkout;
this is not a full platform bootstrap, live Stripe or cloud deployment receipt.
Logs: `/private/tmp/fl-managed-payment-link-{platform,http,pg,lib,clippy}.log`.

## Remaining configured billing acceptance criteria (2026-10-13)

Gate 855 remains open for these actual-source differences. Gate 858 separately
tracks interrupted external effects and recovery.

| Remaining boundary | Concrete acceptance criterion |
| --- | --- |
| Payment-link lists — completed in the receipt below | Configured cache-key/public identities are validated before bounded sequential provider reads; requested order/duplicates, malformed/missing data, active filtering and a 150-record list pass without account-wide provider listing. |
| Payment-link updates | Avoid duplicate configured mapper execution and unconditional ORM access in cache-only operation; verify cache identity and intended persistence dispatch before external effects. |
| Checkout/payment-link reference mutations | Resolve trusted scoped provider ownership before expire/update/success/failure calls; public UUIDs and cached provider references alone must not authorize a provider mutation. |
| Native create identity | Explicitly define and test application public UUID allocation versus retained provider IDs for checkout, payment-link and portal creates; the existing opt-in durable alias reads do not supply this lifecycle. |
| Plan/subscription provider inputs | Define trusted provider references/resolvers for plan product, subscription customer and plan inputs. Never infer Stripe ownership or identity from a local UUID or display name. |
| Provider payload validation | Align deliberate required-field/object presence across public types, Zod, TypeBox and native contracts, including payment-link required create options/line items. Opaque SDK nested types are not a runtime validation guarantee. |

The 325 configured mapper comparisons and 144 custom-mapper wrapper operations
cover distinct scopes; neither receipt establishes all six lifecycle criteria.

## Better Auth API-error observer receipt (2026-10-13)

`AuthApiErrorObserverMode::UnexpectedOnly` uses private typed failure provenance
to match the configured Better Auth callback boundary. Handled API errors skip
callbacks even at status 500; unexpected failures invoke them. Existing callers
retain `AllServerErrors` by default, while generated presets explicitly select
the new mode for installed callbacks. Built-in sanitized tracing still emits for
all 5xx: this receipt establishes callback-dispatch parity, not exact default
TypeScript logger event counts.

Actual-source tests distinguish get-session failures, individual/all/other
session revocation, invalid permission schemas and malformed permission JSON.
Six native observer tests cover corresponding boundaries, identical wire bytes,
trace/callback counts, forged code/JSON/unrelated typed metadata, and panic
isolation. The opaque core context test, existing session lifecycle regression
and strict core/IAM all-feature/all-target Clippy pass. The existing CI oracle
asserts the template mode. Logs:
`/private/tmp/fl-iam-error-origin-{core,native,sessions,ts,clippy}.log`.

BlueprintError's new private diagnostic context is excluded from public value
semantics. Its README documents the source compatibility change for external
struct literals/exhaustive patterns and migration to constructors. Signup failure
status and signout failure suppression are separately identified remaining fault
paths; gate 918 stays open.

## Request correlation receipt (2026-10-13)

`CorrelationIdFormat::TypeScript` selects a supplied Node-compatible correlation
value or UUIDv4 and shares it across request context, framework errors and
telemetry. Original request-header absence remains observable to validators;
repeated/Latin-1 supplied values survive framework header conversion. Response
propagation respects application overrides and leaves native request IDs
independent. Raw HTTP probes establish Express's unusual non-HEAD finalhandler
UTF-8 serialization; the profile reproduces it only for marked HTML fallbacks.
HEAD and ordinary responses preserve original Latin-1 bytes.

Validation passes 108 actual-source TS/native request comparisons (216 HTTP
observations), all 84 Axum tests and strict all-target Clippy with MCP and cookie
sessions. The UUID dependency was already pinned; lockfiles add only the direct
Axum dependency edge. README and CI are updated. Receipts:
`/private/tmp/fl-correlation-{tests,clippy,parity}.log`.
The JSON parser comparison uses the existing versioned parser; generic
unversioned JSON parsing and CORS behavior remain separate parity work.

## Configured payment-link list receipt (2026-10-13)

Configured lists now validate every cached DTO and cache-key/public-ID pair
before provider reads, then resolve retained references sequentially. They
preserve requested order/duplicates, reject missing/corrupt data and require an
explicit boolean provider active state. No provider account-wide list or first-
page ID comparison remains in this configured path. The trusted constructor
limit defaults to 100 and accepts 1–1000; overflow fails explicitly before record
or provider reads. A configured 150-record list passes with peak concurrency 1.
The existing listKeys enumeration itself is not bounded; explicit ID batches
avoid it. Custom mapper fallbacks and native default provider IDs remain separate.

Validation passes 353 actual configured cases, 168 base and 144 Stripe wrapper
comparisons, focused strict real-service TypeScript and source inventory checks.
The new generated `PAYMENT_LINK_READS.md` explains bounds and ownership limits.
Cached references support reads, not authorization for financial mutations.
Receipts: `/private/tmp/fl-payment-link-list-{configured,types,base,legacy,inventory}.log`.

## Current generated auth and packaging follow-up (2026-10-13)

A fresh CLI snapshot containing typed auth-error provenance, generated observer
selection, router fallback/correlation support and PostgreSQL member-query
selection passes the full generated Better Auth fixture. HTTP/MCP shared,
separate and disabled listeners, actual blueprint/generated frontend clients,
both SDKs, encrypted tenant sessions, JWT/JWKS/HMAC/role behavior, restart,
relocation, preservation and rename/delete pass. The generated service retains
its configured native HTTP profiles; separate differential fixtures exercise
the opt-in framework wire profiles. Logs:
`/private/tmp/fl-observer-correlation-{cli-build,generated}.log`.
The owned run directory was removed after confirming no fixture roles/processes.

Cargo package listing also confirms the new payment-link read guide, core error
constructor migration guide, error provenance source and correlation module.
The core README required an explicit template symlink, which is now present.
Receipt: `/private/tmp/fl-current-parity-package-list.txt`.
This listing is not a new full source-archive installation test. Checklist remains
**660/682 (96.8%), 22 open**.

## Signup/signout fault boundary receipt (2026-10-13)

The actual configured auth source distinguishes user creation from later signup
failures. Native user insertion errors now return handled 422 FAILED_TO_CREATE_USER;
account/session creation and transaction-entry/completion errors retain empty
unexpected 500 responses without cookies and invoke the selected callback once.
Existing validation/duplicate responses are unchanged. HTTP signout suppresses
read/delete failures and clears exactly session_token, session_data and
dont_remember; oauth_state is not part of this reference signout operation.
The strict SessionService result remains available, and failed deletions retain
the rolled-back persisted session. Browser cookie clearing is not proof of
server revocation when storage fails.

Seven actual-source injected branches, eight native observer tests, a final
focused run covering seven signup cases and ten signout variants, and strict
IAM all-feature/all-target Clippy pass. Shared sanitized diagnostics live outside
the HTTP module so domain services do not depend on the adapter. The new actual
fixture is wired into CI. Receipts:
`/private/tmp/fl-iam-fault-boundaries-{ts,native,final,clippy}.log`.
Adapter transaction-completion injection and native memory rollback do not claim
a real PostgreSQL COMMIT-failure simulation. Gate 918 remains open.

## Static CORS, provider payload and optional JWT header receipt (2026-10-14)

The opt-in static TypeScript CORS profile covers fixed/reflected/allowlisted
origins, credentials, allowed/exposed headers, preflight continuation/status and
nonnegative JavaScript-safe integer max-age. HTML routing now derives automatic
OPTIONS methods while preserving explicit handlers and internal ownership.
89 native tests and strict MCP/cookie-session Clippy pass. Actual HTTP comparison
covers 450 pairs: 310 exact, plus 140 pairs explicitly asserting that Hyper omits
Content-Length on 204 while Express sends zero. This transport difference is
retained, not normalized away. Existing 100 fallback and 108 correlation pairs
also pass. Dynamic origin delegates and arbitrary JS coercions remain outside
the static profile. Handler incremental Vary changes require the prepared
TypeScriptCorsHeaders extension. Logs: /private/tmp/fl-cors-{tests,clippy,parity,origin-regression,correlation-regression}.log.

PaymentLink creation now requires a provider object containing a line_items
array of objects in both actual validators/services and native contracts/services,
before provider I/O. Nested options survive; empty arrays remain structurally
admissible, and this does not claim complete Stripe SDK payload validation.
60 actual dual-validator cases, 353 configured mapper cases, 170 TS/SDK/native
operations, 66 routes/28 schemas, focused real-source typechecking and strict
Stripe Clippy pass. The reviewed contract snapshot changes only the required
Stripe create shape. Generated runtime assertions await the new CLI snapshot.
Logs: /private/tmp/fl-payment-link-options*.log.

AuthHttp::with_jwt_header_disabled(true) matches the optional JWT plugin setting,
skipping signing/key minting for session headers while preserving explicit
token/JWKS endpoints. The configured preset keeps its existing enabled default.
The actual configured oracle, three native external-JWT tests and strict IAM
Clippy pass: /private/tmp/fl-iam-jwt-header-{native,clippy,ts}.log.
All three differential fixtures are included in CI. Broad gates remain open;
checklist remains 660/682 (96.8%), 22 open.

## Cache-only PaymentLink update and JWKS grace receipt (2026-10-14)

Configured billing-base PaymentLink updates now use an optional validated
toCachedDto hook when database backup is disabled. Public identities are checked
before/after mapping, known dates are restored from JSON caches, and invalid
patches/cache rows fail before writes. Backup and custom mapper fallback retain
the existing entity path. This does not enable Stripe mutations from cached
references or resolve the Stripe wrapper's duplicate update mapping.
12 focused actual-source cases, 353 configured mapper cases, 168 base and 170
Stripe operations and focused real-source typechecking pass. Logs:
/private/tmp/fl-payment-link-cache-update{,-configured,-types,-base,-legacy}.log.

KeyRing and AuthHttp accept a validated Duration JWKS grace period, preserving
the default 30 days, strict expiry boundary, no-expiry visibility, signing-key
eligibility and remote-JWKS behavior. Whole nonnegative milliseconds within
signed range form the supported typed subset; JavaScript negative/nonfinite
values and sub-millisecond configuration are explicitly outside it.
70 actual TypeScript visibility comparisons, 40 native boundary cases, invalid
configuration checks, four HTTP/external-JWT tests, the existing full default
key lifecycle and strict IAM Clippy pass. Logs:
/private/tmp/fl-iam-jwks-grace-{ts,boundary,http,clippy}.log. The initial native
log contains a corrected fixture-only key-id mismatch; final boundary rerun
is green. Both new fixtures are wired into CI. Checklist remains 660/682,22open.

## Configured subscription lookup and generated integration correction (2026-10-14)

Configured base/Stripe subscription mappers now advertise trusted lowercase
party discriminators consumed by the base lookup service. Generic/custom mappers
keep uppercase fallback. Metadata is validated and copied before use, retaining
active filtering and user/organization isolation. 28 actual configured cases,
353 configured regressions, focused service typechecking and 168 base/170 Stripe
operations pass: /private/tmp/fl-subscription-party-types*.log.

The first fresh generated billing run passed HTTP/MCP startup, initialization,
discovery, health invocation and Origin rejection for both billing and stripe.
It then caught a genuine integration error: new malformed PaymentLink options
were rejected before provider I/O but mapped to HTTP500. The adapter now maps
that exact validation code to sanitized400; unknown failures remain sanitized500.
A focused response/body-secrecy test and strict Axum Clippy pass:
/private/tmp/fl-payment-link-options-http400{,-clippy}.log.
The failing generated log is /private/tmp/fl-oct14-billing-generated.log; it is
not a completed runtime receipt. A rebuilt generated rerun remains required.

## Fixed JWKS route configuration receipt (2026-10-14)

AuthHttp::with_jwks_path relocates a fixed ASCII JWKS GET route and both raw
and application OpenAPI operations; jwks_path exposes the relative client path.
The default remains /jwks. Old paths, HEAD/POST, and remote-signer publication
retain404. Root-auth application namespace isolation remains enforced.
Three native tests, 45 actual TypeScript route/schema/config cases and strict
IAM Clippy pass: /private/tmp/fl-iam-jwks-path-{native,clippy,ts}.log.
The new fixture is included in CI.

This is a bounded option profile, not full jwksPath equivalence: upstream root
/ serves the trailing-slash auth mount, while Axum nesting maps inner root to
the bare mount. Native explicitly rejects it; the actual oracle retains this
remaining gap. Encoded/dynamic/query/fragment paths, conflicting owned namespaces
and arbitrary upstream route-shadowing behavior also remain unsupported. Unknown
application namespaces are subject to registry/mount collision checks.
Gate 918 remains open and checklist remains 660/682 (96.8%), 22 open.

## Cached PaymentLink DTO normalization receipt (2026-10-14)

Configured base/Stripe PaymentLink response mappers now expose an optional
normalizeCachedDto hook. Base get/list normalizes each selected row once, checks
cache-key/public identity and rejects hooks that change identity. Declared
createdAt/updatedAt fields become Dates from JSON caches; nested provider strings
and extension data remain intact. Reads perform no cache rewrites or ORM access.
Existing Stripe retained-reference evidence checks and whole-list preflight
remain independent, rejecting malformed dates/identities before provider GETs.
Custom mapper fallback is unchanged; no cached reference acquires mutation
authority. 24 focused actual-source cases, 353 configured regressions, 12 update
regressions, focused real-source typechecking, and 168 base/170 Stripe operations
pass: /private/tmp/fl-payment-link-cache-reads*.log. CI includes the new fixture.
Broad mapper/reconciliation gate855 remains open.

## Unversioned framed JSON parser receipt (2026-10-14)

Opt-in JsonRequestParsers registrations bind original unversioned request
declarations, parser options and verifier/reviver hooks. Matching framed JSON
is decoded before authentication and request callbacks, independently of schema
validation mode. Original wire bytes remain separate from ParsedJsonBody parsed
values/decompressed bytes. Auth callbacks observe the parsed body; framed empty
JSON becomes an object and unframed/nonmatching media remains absent. LocalRouter
provides an explicit execution API with identical framing rules. MCP is unchanged.

192 actual-source comparisons (384 HTTP observations), 47 HTTP and 92 Axum lib
tests, and strict all-target HTTP/Axum MCP/cookie-session Clippy pass. Eighteen
unsupported numeric/Unicode outcomes are asserted separately: integral magnitudes
outside JS safe range, nonfinite overflow and lone UTF-16 surrogates remain
explicit bounded-profile limits. Finite decimal/exponent/subnormal values use
correctly rounded parsing and comparisons inspect IEEE bits. JS response
serialization, including signed-zero spelling, is not claimed. Generic schema
diagnostic equality and broader parser/value profiles remain open. Existing
versioned tests pass with float_roundtrip enabled. Logs:
/private/tmp/fl-json-parser-{tests,clippy,parity}.log. CI includes the live oracle.
FrameworkRequest gains parsed_json; constructor-based callers preserve forward
compatibility. Registration metadata is validated before exposure and stale or
duplicate registrations fail. Checklist remains 660/682 (96.8%), 22 open.

## Checkout/portal cached identity receipt (2026-10-14)

Shared cachedSessionDto now accepts an optional expected identity. Base checkout
GET and portal GET/update supply the requested ID and reject corrupted cache-key
payloads before mapper, ORM or provider calls. This also protects Stripe portal
backup updates before selecting a cached customer for provider session creation.
Explicit blank/null/undefined expectations fail; omitted helper expectations
retain compatibility. Valid arbitrary ID formats and known-date restoration
remain supported. 72 actual base/Stripe/backup cases, 353 configured regressions,
focused real-source typechecking and 168 base/170 Stripe operations pass:
/private/tmp/fl-cached-session-identity*.log. CI includes the new fixture.
This strengthens cache identity checks without making cache rows mutation
ownership evidence. Broad billing mapper/provider reconciliation gates remain open.

## Generated billing checkpoint and main merge (2026-10-14)

After the invalid-options HTTP status correction, the coherent rebuilt CLI
passes the full generated core billing/Stripe mixed-module fixture: both
HTTP/MCP startup/initialization/discovery/Origin checks, both SDK artifact formats,
encrypted PostgreSQL persistence, relocation, preservation and rename/delete
lifecycle. Log: /private/tmp/fl-oct14-billing-corrected-generated.log; CLI build:
/private/tmp/fl-oct14-coherent-cli-build.log. The earlier failing log is retained
as evidence of the integration correction, not a completed receipt.
Owned failed/successful billing runs were removed only after role/process checks.
Generated auth validation is underway separately. This snapshot includes the
framed JSON parser and fixed JWKS-path profile, before the optional subject hook.

Latest origin/main e36cfd29c (preview model gateway client, #450) is now merged.
The public inventory is regenerated: 44 packages, 609 entry points, 1,667 declarations,
zero unresolved exports. The new preview client and its issued-ceiling catalog
are an explicit unmapped native capability within gate849; this merge does not
claim Rust support for that new feature. Existing broad gate counts remain
660/682 (96.8%), 22 open.

Generated auth checkpoint also passes from that coherent CLI: actual blueprint
and generated clients at default/custom/root/path-override namespaces, both
SDKs, HTTP/MCP shared/separate/disabled listeners, encrypted sessions, JWT/JWKS
and restart/preservation/rename/delete lifecycle. Log:
/private/tmp/fl-oct14-auth-generated.log. The owned run.yVg1mk was removed after
role/process checks. This verifies the optional fixed JWKS path/grace/header
production snapshot without enabling nondefault options in the preset; their
separate differential fixtures supply option-specific proofs. It predates the
new optional JWT subject callback and before-auth request-schema registration.

## Optional JWT subject callback receipt (2026-10-14)

AuthHttp::with_jwt_subject_resolver accepts a trusted async callback over an
owned read-only snapshot of a verified persisted user/session. None falls back
to the user ID and an empty string remains an empty subject. It runs before
local or external signing; anonymous requests/header-disabled get-session skip
it, while explicit token issuance retains it. Errors produce unexpected empty
500 responses without key minting/signing or new cookies. Cookie/session
identity and other claims are unchanged. Custom subjects can require separate
application authorization mapping. Snapshot access may include the secret
session token; Debug is redacted and there is no Serialize implementation.

24 actual TypeScript local/external route cases, 20 native route combinations
plus anonymous/context checks, four existing signer regressions and strict IAM
Clippy pass: /private/tmp/fl-iam-jwt-subject-{ts,native,regression,clippy}.log.
CI includes the actual-source fixture; configured preset defaults are unchanged.
This optional hook postdates the latest generated preset receipt. Other payload
hooks, independent key adapters, unsupported root/encoded namespaces and broad
Better Auth coverage remain in gate918.

## Registered request callback ordering receipt (2026-10-14)

RequestValidators::set_stage can opt an existing schema-bound callback into
BeforeAuthentication. Parsed JSON is available first; Error-mode callbacks run
once before async auth, while None/Warning and the default AfterAuthentication
order remain unchanged. Rejection metadata mismatches remain redacted500 before
auth. This moves only explicit callbacks, not native schema checks, generic Zod
semantics, request transformations or unregistered validation. Pre-auth diagnostic
text is trusted application policy and must be safe for anonymous callers.
96 Axum library tests, 30 actual TypeScript/native comparisons (60 observations)
and strict MCP/cookie-session all-target Clippy pass:
/private/tmp/fl-request-order-{tests,parity,clippy}.log. CI includes the oracle.
The latest generated-service proof predates this optional registration extension.
Broad validation/auth/error gates remain open.

## Durable subscription ownership foundation receipt (2026-10-14)

Migration6 extends administrative Stripe bindings with subscription-only customer
and local-party relation columns, including append-only audit records. Existing
migration5 aliases/history, storage IDs and ciphertext remain unchanged. An
opaque StripeSubscriptionOwnership and separate provision API require an existing
locked durable row matching externalId/partyId/type. Generic provisioning rejects
subscriptions without this relation. Read/revoke checks preserve sealed relation
and generation, tenant scope, explicit actor and atomic audit requirements.

This is an administrative assertion, not authenticated provider ownership proof.
No HTTP call, alias backfill, subscription mutation, webhook replay or receipt
completion is enabled. Future provider inspection must compare the asserted
customer exactly under separately verified account/mode context.
Restricted PostgreSQL tests cover migration compatibility, scoped ownership,
reassignment/stale generations, RLS and audit rollback; four existing direct/
managed checkout/PaymentLink PostgreSQL tests, base memory regressions and strict
base/Stripe Clippy also pass. Logs:
/private/tmp/fl-subscription-bindings-{pg,existing-pg,base,clippy,stripe-clippy}.log.
The existing PostgreSQL CI suite invokes the new foundation proof. Generated
startup receipts predate migration6; a new embedded snapshot remains required.
Billing reconciliation gates855/858 remain open.

## Finite JSON large-number normalization receipt (2026-10-14)

The unversioned JSON profile now admits finite large integer values with the
same IEEE-754 rounding as JavaScript, rather than rejecting all unsafe integers.
Normalization occurs before reviver callbacks, including nested/property-order
traversal; ordinary safe integers, callback-produced native Values and existing
versioned reviver behavior remain intact. 96 Axum plus48 HTTP library tests,
540 actual TypeScript/native comparisons (1080 observations) and strict all-target
MCP/cookie-session Clippy pass. Twenty-four overflow/lone-surrogate unsupported
cases remain separately asserted: /private/tmp/fl-json-numbers-{tests,parity,clippy}.log.

Numeric input/output comparisons inspect IEEE bits and do not claim JavaScript
response-serializer equivalence. Large values now require floating-point access
instead of exact integer DTO conversions. Caller-owned JSON Schema enum/const
literals are not normalized automatically. The oracle keeps nested numeric data
in its declared unknown-valued field: the separately observed TypeScript unknown
property stripping/schema transformations remain an explicit residual gate.
This supersedes the earlier profile's unsafe-integral rejection boundary.

## Latest-main preview model gateway receipt (2026-10-14)

The new main-branch preview capability now has a native PreviewModelGatewayClient
under blueprint-core's instance-gateway feature. It reuses existing instance HMAC,
TLS/loopback policy, fresh nonces and nonretrying transport. Fixed catalog/chat
paths, typed one-model issued-ceiling catalog validation, exact8192-byte catalog
bounds, strictly-future expiry within fifteen minutes, bounded chat JSON and preserved optional timestamps
are verified. Catalog fields make no remaining-usage/monthly-token/BAA claims.

64 actual TypeScript/native wire cases, eight all-feature core tests and strict
core Clippy pass: /private/tmp/fl-preview-model-{wire,core,clippy}.log.
The wire fixture checks fixed/chunked8192/8193 boundaries, HMAC bytes/path/fresh
nonces, HTTP status/retry metadata, closed-port classification, unchanged default
transport behavior and no retries. Review added browser UTF-8/BOM decoding and
finite large-integer rounding for opaque chat; IEEE bits are compared. Overflow
numbers/lone surrogates remain explicit Value representation limits, and byte-
identical JavaScript serialization is not claimed. Default provider transport
decoding/limits/error codes are unchanged; only the new bounded browser path
normalizes JSON source numbers.

CI includes the new fixture. The new explicit Cargo example required a CLI
template examples symlink; package listing confirms it, the preview module,
migration6, subject hook and request-order source:
/private/tmp/fl-oct14-preview-package-list.txt. This is not a new archive-install
receipt. The earlier newly-unmapped preview feature finding is addressed by
this native profile; managed model catalog/streaming APIs and broader public
symbol/DTO semantic coverage remain in gate849.

## Atomic unversioned JSON schema-transform receipt (2026-10-14)

A distinct RequestValidators JSON-transform registration requires matching
parser metadata and a declared400 rejection. Immutable parsed input transforms
once before auth in all modes; successful outputs commit atomically, including
absent defaults and root removal. None/Warning failures preserve the entire
original parsed body without partial coercion; Error rejects before auth.
Warning diagnostics are static and contain no callback/input values. A private
success marker suppresses only native body presence/media rechecks; unrelated
required headers and other field checks remain. Auth/local/Axum consumers use
ParsedJsonBody; original/decompressed bytes and raw Axum extractors remain intact.

100 Axum plus48 HTTP tests,57 actual-source transform pairs(114 observations),
30 ordering regression pairs and strict HTTP/Axum MCP/cookie-session Clippy pass:
/private/tmp/fl-request-transform-{tests,parity,order-regression,clippy}.log.
CI includes the new fixture. Callback code owns the actual schema transformation;
this does not implement a generic Zod interpreter, query/header transforms or
MCP argument transformation. ParsedJsonBody now has a private marker; downstream
struct literals must migrate to parser-produced values/trusted transformation.

## Already-satisfied paused-effect acknowledgement receipt (2026-10-14)

An explicit direct-client operator API authenticates account/mode and retrieves
the exact pending Stripe event plus current subscription. It requires paused
status, exact customer/sealed subscription binding/local-party relation and an
already-inactive local row. Only bookkeeping changes: audit+receipt+attempt removal
commit atomically under event/row/binding locks and exact attempt/xmin/generation
checks. No provider/local subscription mutation, replay, financial action or
public route is added. Managed transport is rejected before network I/O.

This is acknowledgement of an already-satisfied effect, not proof of callback
causality or uninterrupted pause history. Authenticated GET evidence is not the
original webhook HMAC signature. Event creation must precede attempt/inspection;
older-than30day/unavailable events remain manual. Fresh five-second context checks
follow awaited reads/locks and precede COMMIT. Late post-COMMIT expiry returns
RecordedEvidenceNoLongerCurrent; ambiguous COMMIT errors/cancellation require
explicit receipt inspection and do not imply rollback. Trusted operator UPDATE
on binding rows is additionally required for SELECT FOR UPDATE; no runtime
grant is installed automatically.

Restricted PostgreSQL/authenticated HTTP proof covers evidence denials, tenants,
row/binding ABA, stale attempts, audit/receipt/delete rollback, actual5.2second
lock expiry, precommit rollback, postcommit outcome, managed zero-I/O and exact
ciphertext preservation. Eight Stripe regressions and strict base/Stripe Clippy
pass: /private/tmp/fl-paused-effect-{pg,clippy,lib}.log. CI runs the ignored fixture.
Other webhook effects, absent ownership, managed paused inspection, capture/
financial interruptions and provider/public identity integration remain open.

## Optional JWT payload resolver receipt (2026-10-14)

AuthHttp::with_jwt_payload_resolver replaces the configured payload with trusted
async object claims. None means empty nullish spread; no installed callback keeps
the existing preset. Initial iat precedes callback claims, then subject resolution
overrides sub. Payload callback failure short-circuits subject/signing/key minting
and returns unexpected empty500. Existing local JOSE/external defaults remain;
invalid local claims can mint a key before rejection, matching upstream behavior.
Cookie/persisted ownership remains independent.

44 actual Better Auth local/external route cases,40 native route combinations,
the subject regression and strict IAM Clippy pass:
/private/tmp/fl-iam-jwt-payload-{native,regression,ts,clippy}.log. Tests include
actual JOSE null/invalid registered claims and separate external passthrough,
header suppression/anonymous paths, exact callback order and key side effects.
Primitive/array object spreading is outside this typed object interface. Trusted
snapshots remain read-only/Debug-redacted but can include session tokens; custom
claims may require separate application authorization mapping. CI includes the
new fixture. The archive checkpointa416 predates this optional hook.

## Atomic whole-request field projection receipt (2026-10-14)

A schema-bound whole-request callback now commits parameter/query/header fields
and optional JSON body atomically before auth in every mode. No-body routes need
no parser; body routes require matching JSON parsing. Failed None/Warning paths
retain all original sections. Successful projections use existing versioned value
maps and exact-key truthy header overlay: all original header keys remain and
absent defaults are not inserted. Raw maps/bytes and encoded repeated raw_query
remain separately available. ParsedRequestFields plus typed field extractors
expose successful values; permissive failures require optional access/wire fallback.

105 Axum plus48 HTTP tests,144 actual TypeScript/native pairs(288 observations),
57 body-transform and30 ordering regression pairs and strict Clippy pass:
/private/tmp/fl-request-fields-{tests,parity,clippy,body-regression,order-regression}.log.
Dedicated mode/default tests still enforce missing-auth401 and denied-role403.
Only native request-schema checks skip after success; auth/RBAC remain. Native
credentials use raw authorization/cookie headers. Generic Zod/complex query
parsing/credential rewriting/MCP remain outside this callback profile.
FrameworkRequest adds raw_query/parsed_fields; constructor callers remain valid.
CI includes the new oracle. The fixed archive checkpointa416 predates this API.

## Managed paused-evidence protocol receipt (2026-10-14)

Companion commit7cd2959f1712334826f58527e5c9ddb216426483 adds the dedicated signed
POST /payments/context/paused-subscription-read. Strict IDs/request keys, no query,
8KiB body/no-store/shared rate policy and exact HMAC path protect the paired read.
One token binds instance/organization/account/mode/API version/mapping generation
and a five-second window; fresh database checks surround provider awaits. Only
fixed balance/event/subscription GETs are issued; no raw proxy allowlist expands.
The aggregate response is bounded and preserves the exact scope envelope.

Native direct/managed paired reads verify that envelope, immutable client context
and event/subscription identity/mode before existing sealed local ownership/CAS
bookkeeping. Account metadata must match when present; absent/null account relies
on the authenticated SDK request pinned to the verified connected account, not
on assumptions about webhook-only metadata. Nonnull context remains unsupported.
This extends paused-effect acknowledgement to managed mode without provider or
subscription writes, original-HMAC claims or pause-epoch causality claims. Old
gateways fail closed.

37 actual platform tests plus focused TypeScript, native signed-wire/source
conformance, two restricted PostgreSQL direct/managed twins, eight Stripe library
regressions and strict base/Stripe Clippy pass:
/private/tmp/fl-managed-paused-{platform-full,types,wire,pg,lib,clippy}.log.
The packaged gateway mock follows actual companion protocol source. CI executes
its native wire fixture; cross-repository source comparison requires the explicit
companion checkout used for this receipt. No live Stripe/cloud deployment proof
is claimed. The archive checkpointa416 predates managed paired support.


## Root JWKS and full IAM regression receipt (2026-10-14)

A fixed JWKS path of `/` now mounts at the trailing auth root (`/api/auth/`)
without also serving the bare base (`/api/auth`). Ordinary token/organization
routes, external-signer JWKS suppression, relocated OpenAPI and protected business
routes retain their behavior. Root JWKS with root authentication remains rejected
in either builder order; dynamic/encoded paths remain outside this static profile.
Namespace collision checks still reject conflicting application routes.

The archive a416 all-module run exposed three failures from two stale assertions
expecting four signout cookies. The actual TypeScript successful HTTP/HTTPS oracle
confirms the three reference cookies already emitted by production. Assertions
now check exact names and flags rather than only counts; no signout production
behavior changed. The corrected current-source all-feature IAM suite passes
70 tests with two PostgreSQL tests explicitly ignored, plus strict all-target
Clippy. Actual JWKS45 and successful signout cookie oracles pass:
/private/tmp/fl-iam-jwks-root-{native,clippy,ts}.log and
/private/tmp/fl-iam-signout-cookie-ts.log.

The installed source archive at a416 separately passed complete generated billing
and auth fixtures, including HTTP/MCP listeners, both SDKs, encrypted persistence,
restart and relocation/lifecycle checks:
/private/tmp/fl-oct14-archive-{billing,auth}-generated.log. The broader archive
all-module suite stopped at the stale cookie assertions and is not claimed green;
checks after that point still require the corrected archive rerun. Finished owned
billing/auth fixture workspaces were removed after active-process checks.


## Default simple-query projection receipt (2026-10-14)

`forklaunch_http::parse_typescript_simple_query` supplies an explicit pure
projection matching Express's default Node querystring parser: repeated values
become arrays; brackets, dots and prototype-named keys remain literal; malformed
percent escapes and lossy UTF-8 match Node. The 1,000-segment budget counts empty
ampersand segments before skipping them. Native wire maps remain unchanged; the
whole-request transform example now uses the helper against retained raw_query.
Extended/custom query parsers, object prototypes/key enumeration and lone UTF-16
surrogates remain outside this API.

65,857 actual Node/native cases (including exhaustive encoded one/two-byte pairs),
23 actual default Express HTTP observations,50 HTTP tests,144 whole-request field
regression pairs and strict HTTP/Auth Clippy pass:
/private/tmp/fl-simple-query-{parity,tests,auth-clippy,fields-regression}.log.
The existing HMAC tampering regression also passes after replacing deprecated
GenericArray::as_slice with slice indexing; the constant-time comparison remains
unchanged: /private/tmp/fl-auth-hmac-warning-test.log.


## Managed model catalog/nonstreaming chat receipt (2026-10-14)

The blueprint-core managed ModelGatewayClient now exposes fixed signed GET
/models and POST /models/chat/completions routes. Catalogs and completions remain
opaque JSON, preserving extras, primitives and omitted timestamps like the actual
TypeScript unchecked casts. Monthly usage/budget and BAA fields remain platform
claims, not independently verified client guarantees. Preview catalog validation
is unchanged. Streaming, cancellation and typed chat DTOs remain open.

46 actual TypeScript/native signed-wire comparisons pass, covering unchecked
shapes, lossy UTF-8/BOM, large finite integers, malformed/empty JSON, non-2xx status
and retry hints, chunked responses, fixed paths, exact-body HMAC and fresh nonces.
Additional native checks verify the one-MiB advertised/chunked raw bound and
closed-port transport failure. Strict all-feature/all-target blueprint-core
Clippy passes: /private/tmp/fl-model-gateway-{wire,clippy}.log. Requests are not
retried; remote error text is excluded. Existing TLS/loopback settings apply.
Overflowing numbers/lone UTF-16 surrogates and JS serializer spelling stay outside
the finite native JSON profile. No live platform model deployment was exercised.


## Warning response observation receipt (2026-10-14)

An explicit ResponseValidators::enable_warning opt-in now invokes an existing
finite response callback in Warning mode while retaining the original response.
Successful parsed/coerced outputs are discarded, matching actual TypeScript;
None still skips. Sanitized static reasons report schema rejection, invalid UTF-8,
stream errors or collection bounds without sending/logging callback diagnostics.
Existing registrations still skip Warning unless enabled. HEAD/bodyless statuses
and versioned/multivalue inspection remain outside this finite callback profile.

Collection uses byte/time/1,024-frame bounds with replay of consumed frames and
the untouched remaining body. Original headers, extensions, trailers and stream
errors survive, including a once-polled error. Collection timeout does not preempt
trusted synchronous callbacks, which must return promptly.

130 all-feature Axum tests,24 actual TypeScript/native pairs(48 observations),
strict all-feature/all-target Clippy and canonical366 HTTP outcomes with zero
normalized OpenAPI differences pass:
/private/tmp/fl-response-warning-{tests,parity,clippy,canonical}.log.


## Configured Stripe update preflights receipt (2026-10-14)

Configured PaymentLink updates now validate the patch and normalized cached DTO
before Stripe mutation, including exact immutable public identity. The optional
hook uses the existing single normalization, does not mutate either input, and
never turns a cached provider reference into mutation authority. Both backup modes
and custom-mapper fallback retain target dispatch through dto.id. Public UUID to
provider identity resolution and duplicate mapping remain open.

Configured Plan updates now validate patches before ORM/provider reads and the
existing delete/create replacement sequence. Invalid patches therefore cannot
delete a valid provider plan before local validation. Removing a redundant
undefined name assignment preserves the name on price-only updates; explicitly
supplied names still update. Provider/local writes remain externally non-atomic.

30 PaymentLink and13 Plan actual-source cases,353 configured mapper regressions,
focused service TypeScript and170 actual TS/native/official-SDK service operations
pass. Final170 includes the Plan omitted-name fix:
/private/tmp/fl-payment-link-preflight{,-configured,-types,-legacy170}.log and
/private/tmp/fl-plan-preflight{,-configured,-types,-legacy170}.log. Cases assert
zero provider/ORM writes on invalid input, unchanged inputs, original valid
provider sequence and optional custom fallback. The public API inventory remains
44 packages/609 entrypoints/1,667 declaration records with no unresolved entries;
line/signature changes are regenerated and checked.


## Absolute JWT Date expiration receipt (2026-10-14)

AuthHttp::with_jwt_expiration_at accepts a trusted millisecond-precision UTC
instant, matching Better Auth's JavaScript Date option. It floors to Unix seconds
including negative dates, is independent of payload iat, and allows past dates.
Explicit payload exp wins; null exp falls back to the configured expiration.
Payload/subject callbacks retain their precedence. Default fifteen-minute JWT
lifetime, cookie identity and persisted session expiration are unchanged.
Submillisecond values are explicitly rejected. Chrono's representable year range
already fits the JavaScript Date range; native extremes are tested rather than
claiming an unreachable out-of-range typed Date test.

60 actual Better Auth local JOSE/external HTTP route comparisons, the native
60-route matrix and date boundaries, combined payload/subject/expiration
regressions and strict all-feature/all-target IAM Clippy pass:
/private/tmp/fl-iam-jwt-expiration-{ts,native,regression,clippy}.log.
Custom JWKS persistence/repository adapters remain separate unfinished work.


## Configured subscription update consistency receipt (2026-10-14)

Configured Stripe Subscription updates now validate the patch against the existing
row before provider reads/mutations. Supplied partyId/partyType must match the
existing ownership because the provider update does not move the Stripe customer.
Omitted or exact existing ownership remains accepted; reassignment needs a
separate trusted migration workflow. Wrong public identity, invalid fields and
blank/different owners fail without provider calls or database writes. The initial
existing-row read remains. Optional custom-mapper fallback is unchanged.

28 actual-source cases across user/organization ownership,353 configured mapper
regressions, focused service TypeScript and170 actual TS/native/official-SDK
operations pass: /private/tmp/fl-subscription-preflight{,-configured,-types,-legacy170}.log.
This is consistency preflight, not concurrency control, authorization or verified
provider-account binding. No cached reference becomes mutation authority. Product/
plan identity mapping, provider/local atomicity and broader mutation semantics
remain open. Public API line/signature changes are regenerated and checked.


## Response header array reference correction (2026-10-14)

The TypeScript response wrapper now passes safe-stringified header arrays to Node
as arrays instead of joining them with forbidden newline characters. Set-Cookie
retains separate fields; Accept/custom arrays use Node's normal wire behavior.
Scalar/object serialization is unchanged, empty arrays omit fields, and scalar/
array CRLF values still fail Node's ERR_INVALID_CHAR check. This corrects a real
TypeScript500 defect exposed by the Warning oracle; native HeaderMap behavior
already supports repeated fields.

54 actual ForkLaunch/raw-Node HTTP comparison pairs pass across three validation
modes, nine cases and repeat requests, plus focused source TypeScript checks:
/private/tmp/fl-response-header-arrays.log. No response schema acceptance or Node
header-injection checks are relaxed.


## Typed relative JWT lifetime and duration boundary receipt (2026-10-14)

AuthHttp::with_jwt_lifetime accepts whole-second nonnegative Duration values up to
JavaScript safe-integer seconds. It shares the absolute Date option's expiration
enum; the last Date/lifetime setter wins. Explicit payload exp still takes
precedence, null exp falls back, and defaults/cookie-session identity are unchanged.
This is a typed duration profile, not a complete JavaScript duration-string API.
Fractional or unsafe native durations fail configuration before JWKS writes.

The existing Better Auth duration parser now preserves upstream millisecond
multiplication/division order and Math.round behavior at safe-integer and negative
half boundaries. Adding0.5 before floor incorrectly incremented an already
integral MAX_SAFE value. The separate JOSE parser branch is unchanged; exact
negative-zero bits are verified.

144 actual local JOSE/external HTTP route cases plus four exact rounding-bit
checks, native JWT/Date/payload/subject regressions and strict all-feature/all-target
IAM Clippy pass: /private/tmp/fl-iam-jwt-lifetime-{ts,native,rounding,clippy}.log.
The immutable archive8f628 predates this optional lifetime slice; its service
verification must not be presented as a lifetime-feature archive receipt.


## Corrected installed-archive all-module receipt (2026-10-14)

Committed checkpoint8f62825bd958c6e81a096618ddfcea6014c6b779 was packaged and
installed offline/debug/locked from its extracted source archive. The archive's
VCS metadata matches that revision:2,451 files,19.6MiB unpacked/4.1MiB compressed;
crate SHA2568b6584706bd123381a7310b579297dce36e931b0b9ca55853f842454bb04d95d.
The unchanged post-install portion of rust_packaged_install.sh then passed in an
owned relocated generated application. Packaging and installation were separate
steps to reuse the verified installed binary; no tests were skipped in that
post-install suite.

All four generated worker families and auth/coding/messaging/Twilio/ecommerce/
billing/Stripe module sources compile portably. Native and TypeScript WebSocket
consumers, AsyncAPI import/export, CLI/native recursive SDK generation and
warning-denied empty-worker SDK builds pass. The archive's all-feature IAM suite
passes72 tests with its two PostgreSQL tests explicitly ignored. AWS S3 feature
and all requested module feature checks pass. Generated service HTTP/MCP checks
cover initialization/discovery/health/Origin rejection, enabled/disabled/custom
endpoints, shared/separate listeners, explicit/ephemeral ports, fallback ports,
atomic startup rejection and graceful shutdown. All five signed capability
callbacks pass forgery/freshness/replay/redelivery checks.

Logs: /private/tmp/fl-oct14-complete-checkpoint-{package,install}.log and
/private/tmp/fl-oct14-complete-archive-all-modules.log. The suite exited0 and
supersedes the earlier a416 stale-cookie failure. This is local generated service
and portable archive proof, not live AWS/provider deployment or public registry
publication. Checkpoint8f628 predates optional relative JWT lifetime and later
versioned-response/ecommerce-scope/managed-stream slices. CLI installation retains
four existing CLI warnings; this is not a zero-warning CLI-build claim. Framework
format check passed; two historical IAM formatting issues were separately fixed
in5f8ccab0e after this immutable checkpoint. Finished owned extraction/install
and detached verification workspaces are removed after process/cwd checks; the
crate artifact and logs are retained.


## Current IAM PostgreSQL follow-up receipt (2026-10-14)

Both ignored IAM PostgreSQL tests pass against the owned local test database
in256.78 seconds. The exact already-validated all-feature test executable was
copied and run directly, allowing the archive Cargo build to continue independently.
Binary SHA256710b5c6c51712d961fd15c19ce0a871fb479667c20f15f1e2d0aa24c212a88e5
corresponds to the current lifetime slice atd406df5617a20017a26498aeeafb6dd378239c59.
This includes encrypted durable storage, indexed large-record lookups, roles/
permissions/session behavior, tenant isolation, rotation, audits/transactions and
the corrected live HTTP/MCP signout assertions. The diagnostic-observer test uses
a transaction-local TEMP table and rollback; the long storage test owns a unique
restricted role and isolated tenants.

Post-run verification confirms the exact owned role is absent and its fixture
records/audit/unique_claims/lookup_claims are zero. The copied executable's hash was
rechecked before removal. Logs and metadata:
/private/tmp/fl-iam-pg-validated-{run,list,cleanup}.log and
/private/tmp/fl-iam-pg-validated-metadata.json. No cloud/vendor service was used;
this does not close broader Better Auth adapter/options coverage.


## Ecommerce durable provider-scope foundation receipt (2026-10-14)

Additive ecommerce migration9 records explicit administrative payment scope:
tenant, provider, deployment, merchant/account, mode, reference, linked order,
amount/currency and opaque generation. Provision compares the caller's payment
snapshot against a locked durable row and verifies linked order key/body identity.
Reads check payment/order key/body identity and the immutable local relation;
revocation compares exact generation/scope. No provider call, payment/reference
write, automatic backfill or public route is introduced. This is an administrator
assertion, not provider ownership proof or permission to mutate money.

Forced tenant RLS protects bindings; audit RLS permits only INSERT/SELECT even
under broad non-bypass table grants. Actor/correlation are required; audit failure
rolls back provisioning/revocation. Scoped reference uniqueness rejects duplicate
local bindings. Foreign-key deletion cascades remove bindings when payment/order
records are deleted, preventing delete/recreate revival while retaining audit.
Same-record relation changes restored later may match again; effect workflows
must independently compare current snapshots and generation under locks.

Restricted PostgreSQL foundation proof passes, including exact old migrations1–8
followed by9, opaque ciphertext/legacy nullable audit preservation, tenants,
key/body identity, grant rollback, append-only audits, uniqueness, revoke/reprovision
ABA and delete/recreate. Both existing reconciliation PG tests, seven base
regressions and strict all-feature/all-target Clippy pass:
/private/tmp/fl-ecommerce-bindings-{pg,reconciliation,base,clippy}.log. An initial
test-only reference collision was corrected and its exact owned role/tenant cleaned.
Default application routes receive no administrative grants. Short-lived
provider-account/mode verification, cancelled-order repair and reference retirement
remain open; retain existing references for review until that integration exists.


## Ordered matched-version response profile receipt (2026-10-14)

ResponseValidators::insert_versioned now opts finite routes into status admission
from every actually matched request version, with ordered duplicate-status schema
branches shared through Arc metadata. The last selected request/handler/auth view
is unchanged; default routes retain LastSelected behavior. Trusted callbacks own
union body interpretation and diagnostics; successful parsed values never replace
responses. Normal emitted registry/OpenAPI artifacts retain original declarations.
Standalone LocalRouter's explicit execution API admits statuses and returns
VersionedResponseOutcome; the Axum adapter executes registered finite callbacks
using metadata from that same local execution, without rerunning version hooks.

Private terminal provenance protects prehandler framework rejections and
framework-converted handler errors from callback replacement. A missing selection
after an executed handler remains guarded rather than fabricating matched branches.
Opt-in undocumented-status diagnostics preserve unrelated application headers/
extensions and remove stale representation headers. Empty/streaming/stale response
registrations are rejected. Warning observation remains separately opt-in.

108 exact actual TypeScript/native HTTP pairs across native/local handlers pass,
including duplicate203 union alternatives, earlier201/later202 status admission,
all modes and preserved request selection. Twenty additional prehandler comparisons
verify status/origin only; their twenty existing diagnostic-body differences are
explicit, not normalized into equality. Native baseline/profile tests cover early
query/parser/media/body/auth errors and zero handler/callback calls. A full run of
133 Axum+52 HTTP tests was followed by14 focused response regressions for the final
converted-handler origin guard. Strict all-feature/all-target Clippy, canonical366
outcomes/zero OpenAPI differences and Warning24 regression pairs pass:
/private/tmp/fl-version-response-{tests,final-focused,parity,canonical,warning-regression,clippy,registration}.log.
Generic versioned schema interpretation/error text and None/Warning request-mode
no-match handler outcomes remain outside this finite profile's evidence.


## Managed model streaming receipt (2026-10-14)

ModelGatewayClient now sends the fixed signed POST
/models/chat/completions/stream and returns an incremental ModelGatewayStream.
Its async next emits opaque JSON items, skips malformed/non-data frames and stops
on exact [DONE]. Framing matches actual TypeScript LF/CRLF/mixed delimiters,
multiline data, tail quirks, initial BOM, Unicode trim and incomplete final UTF-8
(the upstream decoder does not flush at EOF). Unsafe finite integers are actual
rounded f64 values with bit checks rather than serialization-only comparisons.
Overflowing numbers and lone UTF-16 surrogates remain outside native JSON.

The explicit native profile defaults to16MiB total raw bytes,256KiB per frame,
4,096 frames and120 seconds; smaller positive bounds are validated. Frame limits
apply per frame, including when a transport chunk contains many small frames.
Only one transport chunk and a bounded frame are retained, without a background
reader or complete-response buffer. Drop/cancel/[DONE] close the native response,
stronger than upstream releaseLock-only cleanup; cancel/drop semantics are
therefore documented separately. Stream request timeout overrides the existing
15-second JSON timeout without changing other methods. No redirect/retry or
provider credential fallback is introduced. Remote error text remains excluded.

41 actual TypeScript/native signed SSE comparisons plus12 native byte/frame/count/
time/cancel/drop/redirect checks pass, including15.2-second stream success and
unchanged15-second JSON timeout. Eleven core tests, strict all-feature/all-target
Clippy, preview64, managed nonstream46 and managed Stripe28 signed form/query
regressions pass: /private/tmp/fl-model-stream-{core,wire,clippy,preview-regression,nonstream-regression,billing-regression}.log.
The signing helper refactor moves existing exact body/path/query HMAC construction
without changing serialization. Typed chat DTOs and live hosted model deployment
remain separate work. The installed archive8f628 predates this streaming API.


## Ordered strict Zod string profile receipt (2026-10-14)

An explicit reusable strict-string helper now matches the repository's zod/v3
factory for ordered Trim/Min/Max/Length steps, UTF-16 code-unit lengths and default
strict JSON type/constraint diagnostics. Constraint ordering is preserved: trim
before min and min before trim intentionally differ, and multiple failures retain
declaration order. It reuses proven ECMAScript whitespace handling. Native JSON
Schema and TypeBox/AJV Unicode-scalar length policies remain unchanged.

31,510 actual-source/native outcomes,135 all-feature Axum plus13 validation tests
and strict all-feature/all-target Clippy pass:
/private/tmp/fl-zod-string-{parity,all-tests,clippy}.log. Whole-request callback tests
prove successful trimmed body/query projections commit atomically in all three
modes; failure preserves all original sections and markers. This is strict JSON
string input with u32 limits, not generic Zod, regex/case folding/custom messages,
JavaScript object coercion or lone-surrogate support.


## Header-array static typing follow-up (2026-10-14)

The expanded configured-mapper typecheck exposed an introduced error in the
header-array wrapper: its captured generic setter was narrowed to string-only.
The capture now uses the actual Express Response setHeader signature, preserving
array values without changing generic response interfaces or runtime behavior.
All54 header HTTP pairs pass again. The full eight-root check no longer reports
that error;13 separate Bun/SendFileOptions/Dict/Timer/swagger declaration dependency
errors remain recorded at this checkpoint. No clean full-typecheck claim is made:
/private/tmp/fl-response-header-types-oracle.log and
/private/tmp/fl-response-header-full-roots-types.log. The string-profile CI step
also explicitly selects its installed compiler and validator dependency roots.


## Managed gateway typed DTO receipt (2026-10-14)

Serde-only managed gateway DTOs now cover chat requests/messages, completions/
choices/messages/usage, chunks/choices/deltas and model catalogs. Presence explicitly
distinguishes missing fields from present null values. Required nullable fields
must be present; managed completion created is required while preview remains
unchanged. Unknown extras survive, and reserved-name collisions are rejected
before serialization. Numbers use f64 within the native JSON profile.

These DTOs opt into typed serialization/decoding through existing serde helpers;
they do not add network methods or pretend TypeScript's unchecked response casts
perform runtime validation. Default opaque client APIs remain unchanged. Fourteen
core tests,19 actual TypeScript/native JSON roundtrips and strict core all-feature/
all-target Clippy pass: /private/tmp/fl-model-dto-{ts,native,wire,clippy}.log.
The typed decoder is intentionally stricter than unchecked casts; arbitrary JS
values and byte-identical JS serializer spelling remain outside this layer.


## Bun fetch capability and complete focused typecheck receipt (2026-10-14)

The eight-root configured service/mapper check now passes with zero diagnostics
using the repository's actual TypeScript7.0.2 and Node/Bun/Vitest declarations.
Restoring real missing declarations and correct send/swagger resolution removed
13 environment errors without stubs or source-error suppression. It then exposed
one genuine Bun-aware fetch adapter issue: typeof fetch requires preconnect.

stripeGatewayFetch now conditionally forwards an actual underlying preconnect
capability to the configured gateway origin, preserving options, receiver and
return. It never warms a caller-supplied vendor origin or invents a no-op helper
when the underlying fetch lacks one. Normal signed requests are unchanged. Actual
source plus official Stripe22.6.2 SDK tests cover four signed requests, fresh
nonces, Node absence and gateway-only capability forwarding:
/private/tmp/fl-payments-fetch-capability.log.

The restored strict/skipLibCheck configuration retains exactly four changed
services and four configured mappers, with real pinned dependencies and no fake
ambient declarations: /private/tmp/fl-full-roots-types-deps/tsconfig.json and
/private/tmp/fl-full-roots-types-check.log. This supersedes the earlier focused
mapper environment blockage, not a claim of whole-repository TypeScript success.


## Configured cancelled-order resume guard receipt (2026-10-14)

Configured TypeScript payment mappers and the native concrete checkout preset now
check the durable payment identity/reference/order relation and linked order state
before and after retrieving an existing Stripe client secret. Missing, mismatched
or cancelled orders deny resume without provider mutation or reference clearing.
Native memory checks use one lock; PostgreSQL uses one tenant-scoped MVCC query
selecting relation/status only, without address decryption or PII-read audit.
The generic legacy resume method and custom mapper fallback remain available;
no generic HTTP-router trait bound or route is changed.

Configured ORM checks explicitly refresh selected fields and reject an active
outer transaction so repeatable-read snapshots cannot span provider I/O. They are
sequential consistency checks rather than one atomic transaction. Both profiles
retain a race after the final check and do not revoke previously issued secrets,
prove provider ownership, or cancel/refund a payment. Provider references remain
for audit. Noncancelled typed states retain existing eligibility.

20 actual source/real MikroORM cases, memory/provider and restricted PostgreSQL
checks, full native checkout HTTP/MCP plus inventory tests,15 legacy provider cases
and strict base/Stripe/Axum Clippy pass:
/private/tmp/fl-ecommerce-resume-{ts,native,pg,http,clippy,legacy}.log. The fixture
also passes with the consistent dependency-root convention used by CI:
/private/tmp/fl-ecommerce-resume-root-env.log. Four-service TypeScript checking
passed; after restoring real declarations and fixing the Bun fetch capability,
all eight changed service/configured-mapper roots now typecheck cleanly. Public
inventory line/signature changes are regenerated and checked. Fresh committed CLI
preset/archive validation remains a separate follow-up to this source receipt.


## Ordered Zod number and reproducible configured source types (2026-10-14)

The opt-in TypeScript coercion bridge now supports ordered Zod v3 number Int,
Finite, Safe, and inclusive Min/Max checks with safe integer bounds. Safe checks
magnitude and accepts fractions, matching actual Zod. Successful values retain
IEEE-754 bits, including negative zero and unconstrained infinities. Coercion
failures stop checking; constraint diagnostics accumulate in declaration order.
This bounded helper does not implement multipleOf, fractional/nonfinite bounds,
exclusive bounds, custom messages or generic schema interpretation.

15,640 actual ForkLaunch Zod/native comparisons pass with exact output bits and
ordered diagnostics. All 136 Axum and 15 validation tests and strict all-feature,
all-target Clippy pass. Whole-request callback tests verify successful decimal query
coercion and atomic rollback on Infinity rejection in all three validation modes.
Evidence: /private/tmp/fl-zod-number-{parity,tests,clippy}.log.

The eight changed billing/ecommerce services and configured mapper roots now have
a reproducible strict source type check using 609 inventory export mappings, the
manifest-declared private blueprint-core root, and real pinned Node/Bun/Vitest/
send/Swagger declarations. The isolated fixture uses the repository-declared
TypeScript 7.0.2, preserving existing oracle compiler installations. A fresh locked
npm install and all eight roots pass without ambient stubs or source suppressions:
/private/tmp/fl-configured-types-{install,repro,minimal}.log. Both fixtures are
wired into CI. These finite checks do not close the broad validation or billing
mapper gates; the checklist remains 660/682 with 22 gates open.


## Strict Zod array container receipt (2026-10-14)

The opt-in zod_array_shape helper supports strict JSON array type and container
Min/Max/Length/Nonempty constraints. Like actual Zod v3, later checks of the same
kind replace earlier ones; diagnostics appear exact-length, minimum, maximum
regardless of declaration order. Nonempty replaces minimum with one. Accepted
elements are borrowed unchanged. Bounds are unsigned 32-bit integers. Element
schemas/transforms, custom messages and sparse JavaScript arrays are outside this
helper; callers must still run element checks after container length failures to
aggregate diagnostics and commit transformed output atomically.

117,963 actual ForkLaunch factory/native comparisons, all 17 validation tests and
strict all-feature/all-target Clippy pass:
/private/tmp/fl-zod-array-{tests,parity,clippy}.log. The differential is wired into
CI. Native JSON Schema behavior is unchanged; broader validation stays open.


## Better Auth numeric absolute JWT expiration (2026-10-14)

AuthHttp.with_jwt_expiration_seconds now accepts finite absolute seconds, including
fractional, negative, zero and large finite values. Payload nonnull exp still wins;
null falls back to configured expiry. Numeric, Date and relative lifetime setters
replace one another, with the last call winning. Subject resolution and cookie/
stored-session expiry retain their independent behavior. Nonfinite configuration
is rejected before storage; this is an explicit finite JSON NumericDate profile.

60 actual installed Better Auth local JOSE/external-signer route cases, six native
expiration tests covering the same matrix and setter/config boundaries, and strict
IAM all-feature/all-target Clippy pass:
/private/tmp/fl-iam-jwt-numeric-{ts,native,clippy}.log. The actual-source fixture is
wired into CI. Broader custom adapters and arbitrary JavaScript options stay open.

The reproducible configured source type check also passes with a fresh installation
of the exact canonical runtime manifest used by CI, plus Stripe:
/private/tmp/fl-oct14-configured-ci-{install,types}.log. CI installs this isolated
runtime prefix as well as its locked compiler/declaration prefix.


## Exact Stripe resume identity and fresh generated ecommerce proof (2026-10-14)

TypeScript StripePaymentService.resumePayment now requires the retrieved intent's
id to exactly match the retained provider reference before returning a secret,
including custom mapper fallback. Wrong, missing, nonstring and null response IDs
fail with a fixed diagnostic that does not disclose provider content. Native
retrieval already enforced this invariant; its regression now covers the same
malformed/substituted response shapes. 32 actual configured/custom mapper cases,
two native provider/guard tests and focused four-service type checking pass:
/private/tmp/fl-ecommerce-response-id-{ts-parent,native}.log and
/private/tmp/fl-ecommerce-resume-response-id-types.log. The public API inventory
is regenerated and its coverage checks pass (44 packages,609 exports,1667
records,zero unresolved).

A fresh detached CLI source checkpoint24a6250c5b45856b383ed0d0cb88be6a0999f010
was built offline, embedding that committed module source. Its generated mixed
ecommerce application passed dry-run, relocation, preservation/sync, provider
cases, restricted PostgreSQL reconciliation, both contract/OpenAPI SDK generation
and compilation,32-route HTTP/MCP startup/discovery, two-process checkout races,
10 concurrent carts, signed Stripe/PayPal callbacks, durable inventory delivery,
encryption/tenant isolation, restart persistence and rename/delete lifecycle.
An expanded test overlay additionally cancels the order during the provider GET
and proves both generated HTTP and MCP withhold a new secret without creating
another payment or clearing the retained provider reference.

The first expanded test run exposed a fixture-only text-as-JSON mistake; after
correction a second reached the guard but lacked the explicit Stripe SDK fixture
path. The final resumed run passes all runtime/lifecycle steps using the same
unchanged generated source. Logs are retained separately:
/private/tmp/fl-oct14-ecommerce-checkpoint-cli-build.log,
/private/tmp/fl-oct14-ecommerce-checkpoint-runtime.log,
/private/tmp/fl-oct14-ecommerce-checkpoint-runtime-corrected.log,
/private/tmp/fl-oct14-ecommerce-checkpoint-runtime-final.log.
The CLI has four existing unrelated warnings. This checkpoint predates the newer
number/array/numeric-expiry and TS response-ID slices; those have separate source
receipts and are not claimed as included in this generated proof. Final-check
races, previously issued secrets and provider cancellation/refund remain outside
the guard. Broad gates remain open at660/682.


Completed owned ecommerce checkpoint/generated trees (63MiB) and superseded
ad-hoc type-check prefix (42MiB) were removed after verifying no active process
used them. Restricted test roles were absent after cleanup. Logs and checkpoint
metadata remain in /private/tmp/fl-oct14-ecommerce-checkpoint-proof.json; the
permanent source type fixture replaces the deleted ad-hoc configuration.


## Read-only direct Stripe ecommerce context (2026-10-14)

An explicit opt-in ecommerce client profile now authenticates direct account and
mode with GET /v1/account and /v1/balance under the same immutable credentials and
transport. Its opaque observation binds one client generation and expires five
seconds after verification starts. No key-prefix inference or Connect header is
used. New postgres-feature inspect_bound_payment_intent rejects Connect before
all I/O, resolves the provisioned tenant administrative binding, checks exact
intent ID/type/mode/amount/canonical currency/order metadata and rechecks the
entire binding, generation and deadline after the final database wait. Its small
result omits client secrets, raw provider data and customer/address fields.

The direct-context native test, restricted PostgreSQL test, official pinned Stripe
22.6.2/native three-GET wire comparison, and strict base/Stripe all-feature,
all-target Clippy pass: /private/tmp/fl-ecommerce-direct-context-{unit,pg,wire,clippy}.log.
The database fixture proves tenant/identity/shape rejection, binding replacement
ABA rejection, expiry while waiting for final database access, Connect zero I/O,
SELECT-only successful inspection and unchanged payment. CI runs the official SDK
comparison and explicit ignored PostgreSQL test. Source/template documentation
records trusted expectations and the sensitive low-level raw-read boundary.

This read-only API has no public route or write capability. Administrative tenant
assignment remains an operator assertion. Neither a status observation nor this
context authorizes capture/refund/cancellation/reference clearing or cancelled-
order repair. Existing reconciliation APIs keep their prior evidence requirements
and do not automatically use it. Connect, managed gateway and PayPal integration
remain separate work; remote provider changes can race after the final check.


## Version selection from repeated HTTP query values (2026-10-14)

VersionBodyHooksRegistry.enable_typescript_simple_query now explicitly parses the
retained raw HTTP query once per version selection using the proven Node/Express
simple parser. Per-version field schemas/hooks receive duplicate-value arrays or
scalar strings before selection, preserving the last-matched version rule and
typed projection. Raw input and the default collapsed native map remain unchanged.
Registration rejects invalid/unversioned, duplicate or stale request contracts.
Missing raw_query means empty input, even if a caller populated the native map;
this does not invent HTTP wire input from MCP JSON arguments. Extended/custom
query parsers and generic no-match error diagnostics remain outside the profile.

Live actual Express comparisons pass56 exact success/projection pairs plus16
rejection-status pairs with known diagnostic differences retained. Cases include
repeated empty/malformed values, literal bracket/prototype names, unknown-key
stripping, overlapping versions and the1000-segment limit counting empty segments.
HTTP57 and Axum136 all-feature tests, strict all-feature/all-target Clippy and the
existing108versioned-response/20early-error-origin regression pass:
/private/tmp/fl-version-query-{tests,parity,clippy,response-regression}.log.
Focused tests also prove absent-input defaults receive None once, no-match invokes
no handler, each field callback runs once, and raw/default maps stay intact. The
actual-source differential is wired into CI; versioned error/body diagnostics
remain a separate gate.


## Coherent internal JWKS repository foundation (2026-10-14)

Default key issuance/signing, verification, publication and retirement now share
one private tenant-transaction repository boundary. Creation restores/validates
the returned persisted record before signing. Rotation expiry is calculated before
creation and included in the initial encrypted insert, eliminating the prior
expiry-only update/audit mutation. The default encrypted backend,100-key read
window, transactional rollback and post-create claim-failure persistence remain.
No public custom adapter hook is exposed yet; this foundation avoids a misleading
publication-only callback that would leave signing/verification on another source.

An actual installed Better Auth oracle validates28 independent default/custom
getJwks/createJwk lookup cases and proves returned replacement key/id determine a
verified signature, expiry reaches creation, and callback/invalid-material failure
propagates. This records upstream requirements for later native hooks, not native
custom-adapter parity. All80 native IAM tests pass (two PG cases ignored by that
command), including returned record/expiry/rollback/claim-failure checks; strict
all-feature/all-target Clippy passes:
/private/tmp/fl-jwks-repository-{ts,native,clippy}.log. The oracle is wired into CI.

The exact validated all-feature binary is copied for an independent direct run
of both ignored PostgreSQL cases, both PASS in271.77seconds. Its SHA256 is
fb31b75a53d411b56d9f98b3f6860d9da7dd34ca1c739552ecab51a3e15d0875;
metadata /private/tmp/fl-jwks-repository-pg-binary.json and run log
/private/tmp/fl-jwks-repository-pg-run.log retain the final result. Encryption,
RLS, rotation, audit atomicity,32k indexed lookups and live HTTP/MCP sessions pass.
Restricted fixture roles are absent; matching records/audits/unique/lookup claims
are all zero (/private/tmp/fl-jwks-repository-pg-cleanup.log). The copied binary
was hash-verified again and removed; later source work is outside this receipt.


## Framework fixture executable discovery correction (2026-10-14)

Eleven framework-manifest HTTP differential fixtures now default to the executable
under framework-rs/target, matching Cargo metadata. They previously looked under
blueprint-rs/target unless an explicit shared CARGO_TARGET_DIR was set, which local
parity runs provided but clean CI did not. Explicit target overrides still work.
The framework CI job now sets its absolute cached framework target consistently,
including canonical comparisons that build a blueprint example from that job.
All eleven scripts pass syntax checks and their defaults match actual offline
Cargo metadata. Their existing semantic receipts used the explicit shared target
and remain valid; this fixes clean-environment artifact discovery.


## Scoped cancelled-order terminal no-charge repair (2026-10-14)

A separate direct-only operator flow now verifies authenticated Stripe account/mode,
a bound intent's exact fields and explicit terminal canceled/no-charge evidence,
then repairs the linked local payment under locked payment/order/binding snapshots.
It requires a cancelled order, unchanged order xmin, complete payment identity and
binding generation/scope, no succeeded local payment and no captured-history audit.
The initial profile requires latest_charge explicitly null and zero received/
capturable amounts; paid, authorized, failed/refunded charge histories, omitted
fields and nonterminal/ambiguous provider states remain manual. Existing legacy
reconciliation APIs are unchanged. No provider POST, order transition, cancellation,
refund or reference clearing occurs.

Additive migration10 records an append-only tenant receipt alongside the existing
cancellation audit and failed payment in the same transaction. The operator retains
a receipt UUID before invocation and can inspect it after ambiguous COMMIT errors or
cancellation; absence never authorizes financial retries. Freshness is checked after
locks/writes before COMMIT. If the deadline elapses after successful COMMIT, a typed
recorded-but-no-longer-current outcome preserves the receipt instead of claiming
rollback. Trusted store-level evidence/freshness callbacks are not public request
capabilities. Explicit operator grants and lock order are documented.

Restricted PG tests pass terminal/shape/amount denial, order/reference/binding ABA
and raw-scope changes, same-status order xmin changes, succeeded/captured-state
denial, five-second lock-wait expiry, receipt UUID collision rollback, audit/receipt
INSERT failure rollback, precommit expiry rollback, postcommit-expired inspection,
reference retention and receipt tenant isolation/append-only RLS. Existing three PG
binding/reconciliation regressions also pass, including migration1–9→10 preserving
ciphertext and historic nullable cancellation timestamps. Strict base/Stripe
all-feature/all-target Clippy passes:
/private/tmp/fl-ecommerce-scoped-cancellation-{pg,regression,clippy}.log. The explicit
ignored PG case is wired into CI. Existing template directory/README symlinks expose
the new migration/source. All owned PG fixtures clean up their exact roles and rows.
This finite direct no-charge repair does not close the broad interrupted-capture/
cancelled-reference gate or prove live provider/cloud behavior.


## TypeScript versioned request-diagnostic correction (2026-10-14)

Actual Express auditing found that a request-version rejection could itself enter
application response validation before selected-version metadata existed, collapsing
intended diagnostics to generic400 or raising500 even with response validation
None. The request parser now sends its framework-created text diagnostic within a
synchronous per-response WeakSet scope; the enriched sender bypasses application
response validation only for marked string diagnostics and cleans the scope in
finally, preserving nested sends and exception cleanup. Genuine handler responses
still validate normally. Missing selected version uses empty response metadata,
allowing None/Warning handler sends and preserving Error-mode rejection of an
undocumented response. No route metadata or request-controlled bypass flag is set.

54 actual-source cases pass repeated failure/success requests, all validation modes,
authentication preservation and genuine handler schema rejection; an instrumented
validator rejects any accidental diagnostic validation. Native characterization
checks90 requests:52 exact successes and38 existing diagnostic differences,
including six auth-before-version status differences. Those differences stay
explicitly open; this TS correctness fix is not native no-match parity closure.
Prior36 observations: /private/tmp/fl-version-request-errors-audit.log. Final proof:
/private/tmp/fl-version-diagnostics-{native,configured-types,headers,clippy}.log.
All eight permanent configured service/mapper roots typecheck with zero errors;
the obsolete ad-hoc13-ambient-error log is not a successful type-check receipt.

Canonical366 live outcomes and zero normalized OpenAPI differences,144 whole-request
field transforms,57 JSON body transforms,24 Warning-mode pairs,108 version-response
pairs/20 known early-origin differences and54 raw-header pairs all pass again:
/private/tmp/fl-version-diagnostics-rust_*.log and the headers log. CI includes the
actual diagnostic regression and explicit native characterization. Public inventory
is regenerated/checked at44 packages,610 entrypoints,1669 declaration records,zero
unresolved; the additional entry is the shared diagnostic helper module covered by
the existing package wildcard. Auth-ordering/no-match envelopes remain further work.


## Explicit Stripe Connect scoped inspection and no-charge repair (2026-10-14)

Distinct connected-account APIs now verify GET /v1/account and /v1/balance with the
same exact Stripe-Account header as the subsequent intent GET. Their sealed context
binds the immutable client generation, copied Connect account/fee configuration,
expected account/mode and five-second deadline. Wrong scope/configuration rejects
before I/O. Existing direct APIs still reject Connect; callers explicitly choose
the connected inspector or terminal-no-charge cancelled-order repair counterpart.
Both profiles share the same private provider field checks, complete durable binding
recheck, locked order/payment CAS, audit/receipt transaction and COMMIT ambiguity
contract. No migration, provider write, reference clearing or monetary action is
added. Managed-gateway and PayPal scope integration remain separate work.

Two native context tests, four restricted PostgreSQL direct/connected twins,
official Stripe22.6.2/native wire comparisons for both three-GET profiles and strict
Stripe all-feature/all-target Clippy pass:
/private/tmp/fl-ecommerce-connected-context-{unit,pg,wire,clippy}.log. The fixtures
verify exact account headers on every read, foreign client/configuration rejection,
wrong-profile zero I/O, binding/order ABA, final database/lock deadlines, terminal
evidence denials, rollback, receipts and unchanged references. Existing CI fixture
and ignored tests now exercise both profiles. All owned PG cases cleaned up their
exact roles/rows. This is scoped local failure repair, not provider cancellation,
refund/capture authority, broader charge-history reconciliation or live Stripe proof.


Owned stale compiled-artifact cleanup removed102 hashed debug/deps groups (1.14GiB)
whose dep-info referenced only known deleted temporary checkpoints/archive runs.
No Cargo/rustc process was active; source roots remained absent, file metadata was
unchanged and no candidate was open according to lsof. Current source artifacts,
registry caches, fingerprints, example binaries and unrelated workspaces were
preserved. Exact dry-run/removal metadata:
/private/tmp/fl-oct14-owned-stale-artifacts.json. This follows the earlier105MiB
workspace cleanup and does not use blanket cargo clean.


## Typed independent native JWKS read/create hooks (2026-10-14)

KeyRing/AuthHttp now accept an explicit JwksAdapter with independent trusted read
and create callbacks, consistently used for session JWTs, explicit signing,
verification by ID, live/algorithm selection and JWKS publication. A custom reader
is called again for latest-any fallback when primary selection is empty, allowing
its second snapshot to be authoritative. Creation receives full creation/rotation
expiry and returns the authoritative ID/key record. An empty publication source
re-read after independent default creation fails, matching actual Better Auth.
Absent hooks retain the encrypted store fallback; default-only operations retain
one atomic transaction. Fallback locks are released before invoking callbacks,
preventing same-store callback deadlock. Mixed repositories are not cross-store
atomic; callbacks own persistence, synchronization and encryption. Active custom
hooks explicitly disable administrative rotate rather than mutating another source.
External signer mode bypasses these local callbacks.

The fixed server-owned tenant label must be bound by the application to its backend;
it is not backend isolation by itself. Native callback records use validated plaintext
PKCS#8 with explicit algorithm and millisecond times, not upstream encrypted JWK
wire strings. Byte buffers zeroize on drop; secret types expose neither Debug nor
serde. Public-only/legacy missing-algorithm records, arbitrary JS contexts and
per-call pinning remain outside this typed subset. Custom sets above4096 reject
explicitly rather than truncating to the default100-record window.

Actual installed Better Auth oracle covers28 independent hook/default cases,
returned-key JOSE verification, expiry/failure, empty publication and second-read
fallback. All87 native IAM tests pass (two PG cases ignored by that command),
including seven adapter tests and HTTP token/header/JWKS consistency. Strict
all-feature/all-target Clippy passes:
/private/tmp/fl-jwks-hooks-{ts,native,clippy}.log. The prior observer fixture's
thread-local tracing subscriber failed in parallel while passing in isolation;
a single Dispatch bound to every request-future poll stabilizes the exact-two-event
assertion. Production logging is unchanged, and the failed run is not claimed green.
The full parallel final run passes. Existing CI oracle inherits real TSC,validator
and Better Auth fixture dependencies.

A fresh copy of that validated binary passed both ignored PostgreSQL cases
independently in315.94seconds. Hash
5238e25eea0deab77edebf6fe46cbe9090a2b9fe96f2920df691e71744ad99f0;
/private/tmp/fl-jwks-hooks-pg-binary.json records exact provenance. Source-ready
uncommitted opt-in version-preauth framework dependencies were present at compile;
the profile is disabled in this suite, and their snapshot patch/hash is retained
explicitly. This is not claimed as a pure committed whole-worktree artifact.


## Opt-in version selection before authentication (2026-10-14)

VersionBodyHooksRegistry.enable_selection_before_authentication now explicitly
selects/rejects request versions before authentication, matching actual TypeScript
ordering. Default post-auth behavior is unchanged. Authenticated handlers/verifiers
observe the selected projection while credentials remain raw. Local adapters use
the actual LocalRouter compiled plan, not a potentially different RouterConfig
callback registry. A private, request-bound prepared outcome carries success or
no-match through dispatch without running parser/schema/field callbacks twice.
Plan identity and context/method/params/query/rawquery/headers/body/parsed inputs
must match; only trusted mount-prefix normalization and added authenticated session
are allowed. Caller-supplied derived versions are cleared before cached reuse.
Preparation holds one bounded request snapshot and does not clone full registries.
The opt-in callbacks deliberately run on unauthenticated input; authorization/RBAC
still follows and is not bypassed.

HTTP59 and Axum140 all-feature tests, strict all-feature/all-target Clippy and30
native/local security/mode cases pass. The actual90-request matrix still retains
six known ordering differences under the default profile; enabling this profile
removes all six. Each matrix has52 exact successes and38 explicit diagnostic-body
differences (the six default status differences are included in those38). This
fixes ordering, not generic schema-diagnostic wording. Wrong-plan/input and forged
version tests, malformed body before auth, valid authentication, RBAC403, callback
once-only, stale registration and preserved default ordering all pass.

Canonical366/zero OpenAPI differences,108 versioned responses/20 known origins,
56 exact version-query projections/16 rejection statuses,30 unversioned request
ordering pairs and144 whole-request field transforms pass again:
/private/tmp/fl-version-stage-*.log. CI runs both default and enabled matrices.
Broader versioned diagnostics and default-error artifact support remain open.


## Configured Plan creation preflight before external effects (2026-10-14)

StripePlanService now invokes an optional pure configured CreatePlanMapper
validateCreate hook before provider creation. The configured hook validates the
actual creation schema except externalId/billingProvider, which the service replaces
with provider-derived values. Invalid configured fields fail before Stripe or ORM
I/O. Valid POST→expanded-product retrieval→mapper/persistence order, caller input
immutability and custom mapper fallback remain unchanged. This does not resolve
public IDs/name to provider products, prove provider ownership or make provider/
local creation atomic.

14 actual creation cases,13 existing update cases,353 configured mapper cases,
all eight permanent TypeScript7.0.2 service/mapper roots with zero diagnostics,
and170 actual TS/native official Stripe SDK service regressions pass:
/private/tmp/fl-plan-create-preflight.log,
/private/tmp/fl-plan-create-update-regression.log,
/private/tmp/fl-plan-create-configured-regression.log,
/private/tmp/fl-plan-create-preflight-types.log,
/private/tmp/fl-plan-create-legacy170.log. The public inventory is regenerated and
checked; CI runs the new fixture. Interrupted creation and trusted provider/local
identity mapping remain separate open work.


## Public-only custom JWKS read records (2026-10-14)

An additive with_get_jwks_records callback accepts mixed private/public read
snapshots while preserving the existing private reader/material accessor/create
callback. JwksPublicKeyRecord validates the exact native public-JWK shape, matching
ID/algorithm, millisecond timestamps and field/string bounds before serialization;
private/unknown fields reject. Public records publish and verify signatures, but
selection for signing fails without fallback minting or silent alternate selection.
The default encrypted backend is unchanged. Public-only create returns remain
unrepresentable by the typed private create API; actual upstream structurally
accepts them but fails later while signing. Arbitrary/legacy JWK wire import stays
outside this explicit profile.

Actual Better Auth publication and server verification succeed with public-only
records; actual selected/create-returned public-only signing fails. All nine native
adapter tests and strict IAM all-feature/all-target Clippy pass, including real HTTP
JWKS, signature verification, sign failure with UnavailableStore proving no fallback,
empty default table and malformed/private/oversized field rejection:
/private/tmp/fl-jwks-public-only-{ts,native,clippy}.log. No broad custom-adapter or
Better Auth gate is closed by this bounded additive source receipt.


The current custom-hook default-backend PG recheck passes encryption/RLS/rotation/
audit atomicity,32k indexed lookups and live HTTP/MCP sessions:
/private/tmp/fl-jwks-hooks-pg-run.log. Restricted fixture roles and matching records,
audits, unique and lookup claims are all zero after cleanup:
/private/tmp/fl-jwks-hooks-pg-cleanup.log. The copied binary was hash-verified again
and removed; its metadata and source-ready preauth dependency patch remain for
precise provenance. The subsequent additive public-only slice has separate focused
proof and is not claimed inside that earlier binary.


The permanent strict configured source fixture now covers12 roots, adding checkout
and portal services/mappers to the prior eight. Real pinned compiler/declarations
still produce zero diagnostics: /private/tmp/fl-configured-twelve-roots.log. The
packaged-install integration fixture additionally checks presence of the newer
query/version/validation runtime files, managed model stream/DTO sources, JWKS/
expiry modules and scoped ecommerce source/migrations9–10. Full fresh archive
execution is the next proof; syntax/source assertions alone do not claim it passed.


## Bounded versioned default-error declarations (2026-10-14)

DefaultErrorResponses.insert_versioned adds selected missing default string errors
to every exported version only when absent from the root and all branches. It
requires500 selected and initially rejects400, whose generic version-selection
diagnostics remain outside the exact TS text profile. Existing auth/parser-format
guards apply, including a TS parser requirement when any version parses a body.
Registration snapshots original version declarations and rejects stale/ambiguous
profiles. The additive registry projection changes exported declarations; runtime
selection and caller response callbacks retain original compiled schemas. This
prevents projected default statuses from leaking into matched callback snapshots.

Actual144 TS version/default outcomes and120 native/local Error-mode comparison
pairs pass, including24 HEAD pairs, finite text/JSON-string/empty behavior and
safe rejection of object responses. None/Warning retain their existing behavior
and are not normalized by this Error-only profile. Callback/header preservation,
original matched snapshots, local selected errors, stale declarations and metadata
projection are tested. All-feature Axum140/contract24/HTTP61 tests and strict
HTTP/Axum all-feature/all-target Clippy pass; canonical366/zero OpenAPI differences
and108 versioned-response pairs/20 retained early-origin diagnostic differences
pass again: /private/tmp/fl-version-defaults-*.log. A first oracle caught projected
snapshot leakage and was corrected before final proof. CI runs the live fixture.
This supports bounded missing404/500 and guarded401/403 declarations; it does not
claim arbitrary explicit-schema unions, versioned400 or generic diagnostic parity.


## Parallel tracing fixture isolation (2026-10-14)

Fresh immutable CLI archive15e420bed passed generation, relocation, source-presence,
SDK and initial module checks, but its IAM suite failed the exact observer trace
assertion (88 pass,1 fail,2 ignored): /private/tmp/fl-oct14-full-archive-runtime.log.
That archive is not a successful full-runtime receipt. The fixture now runs its
exact-count assertion in a child of the same test binary, explicitly checking
that one test ran and passed. Process-global tracing callsite state is isolated;
production logging and the exact two-event/filter/provenance assertions remain
unchanged.100 parallel observer-suite repetitions pass, followed by the full IAM
suite91 pass/2 ignored and strict all-feature/all-target Clippy:
/private/tmp/fl-observer-isolated-{stress,full}.log and
/private/tmp/fl-observer-pin-clippy.log. This full suite also includes the pending
key-ID signing constraint slice. A corrected immutable archive rerun is required.


## Trusted signing-key ID constraints (2026-10-14)

Additive server-only KeyRing.sign_jwt_with_key_id selects an exact tenant-bound
default-store ID beyond the100-row window, or reads a custom source once. Missing,
expired, algorithm-mismatched and public-only selections fail without minting or
fallback. Optional algorithm assertions require explicit stored metadata;512-byte
IDs and16-byte algorithm constraints are bounded. Expiry equality at the current
millisecond is accepted, matching actual Better Auth's strict-less-than pin check.
Seven actual JOSE cases pass: /private/tmp/fl-jwks-pinning-ts.log. Two new native
tests plus public-only rejection pass inside the91-test IAM suite and strict
Clippy noted above. This adds no HTTP route or client-controlled signing authority.
Legacy missing-algorithm inheritance, algorithm-only provisioning, resolved-key
input and external-signer pin metadata remain separate, unclaimed behavior.


## Configured billing creation preflight batch (2026-10-14)

Subscription, checkout, portal and payment-link configured mappers now expose an
optional pure validateCreate hook invoked before provider POST. It checks the
actual configured schema and its coercions while omitting only fields replaced
by the service. Nullish payment-link amounts remain provider-derived; supplied
amounts are validated. Custom mapper fallback, provider options, backup/cache
behavior, overwritten caller fields and input immutability are retained.52 actual
source cases,353 configured regressions, permanent12-root strict typecheck with
zero diagnostics and170 legacy comparisons pass:
/private/tmp/fl-billing-creation-{preflight,configured,preflight-types,legacy170}.log.
CI runs the new source fixture. Provider/local atomicity, arbitrary SDK models and
tenant ownership are not claimed by this pure input-validation slice.


The versioned default-error profile now also requires request Error validation at
registration, export and execution.24 additional actual TS observations expose
no-match final-error redaction in None/Warning modes; those modes are rejected
rather than preserving a handler message upstream would redact. Six focused
native tests cover stale mode changes and rejection before handler execution.
120 native comparisons,168 TS observations, canonical366/zero OpenAPI differences
and strict HTTP/Axum Clippy pass:
/private/tmp/fl-version-default-guard-{tests,clippy,rust_version_default_errors_parity,rust_canonical_http_parity}.log.
Unversioned profiles retain their existing behavior.


Owned failed15e archive/checkpoint roots were removed after process and open-file
checks, reclaiming209 MiB. The exact source archive, failed runtime log and source
SHA metadata remain, with cleanup receipt:
/private/tmp/fl-oct14-failed-archive-cleanup.json. Corrected174594c48 archive
validation is running independently; no failed receipt is relabeled successful.


## Portable CLI repackaging without Git metadata (2026-10-14)

A Git-free source snapshot exposed Cargo's default omission of embedded dotfiles,
causing include_str(application/.gitignore) compilation failure. Explicit package
paths now retain source, assets, docs, scripts and fixtures, including hidden
templates, while excluding generated output/dependencies. A Git-free repack of
174594c48 plus only this manifest overlay retains all2510 original package paths
except Git provenance metadata; offline locked installation and Rust application
initialization pass with the generated .gitignore and embedded framework sources.
The permanent archive fixture checks Git-free package listing and leakage. Logs:
/private/tmp/fl-cli-untracked-{package,install,init}.log; metadata:
/private/tmp/fl-cli-untracked-package.json. Retained archive SHA256:
94bae1cd1647620859a129ade92896033d2c2bc58a14b8ca3f9c2055c76e37f4. This is a focused repack/install/init proof; the separate
full all-module runtime archive is unchanged and still running.


## Fresh immutable CLI all-module runtime checkpoint (2026-10-14)

Committed174594c48 sources now pass the full portable CLI archive test after the
observer fixture fix.2511 archive files/20.1 MiB source,4,435,233 compressed bytes;
retained SHA256 d02939d7e08decbec69ad52b895dd77a300c8a1b08deebad4547553e032826d6. Offline package and locked debug installation
pass (four existing CLI warnings). All extracted source bytes and retained archive
hash were verified again after the run. Git packaging tracking snapshot metadata
is distinct from the original source SHA and recorded in the checkpoint JSON.

The mixed application includes service, PostgreSQL/Redis/BullMQ/Kafka workers,
Better Auth, CAC, base/Twilio messaging, ecommerce and both billing variants. It
passes relocation, no symlink/checkout-path leakage and newer source-presence
checks, workspace and provider-feature compilation, native/TypeScript WebSocket
consumers, socket/AsyncAPI SDKs, AsyncAPI18, WebSocket/MCP transports and IAM91
tests with2 explicit ignored PG tests. HTTP/MCP startup validates shared/separate
listeners, disabled/custom endpoints, ephemeral/explicit ports, discovery/health,
Origin rejection, atomic startup failures and graceful shutdown. Five native
capability callbacks pass actual TS signatures and forgery/staleness/replay/
redelivery checks. Contract/OpenAPI exports, both SDK imports and check mode,
empty worker warning-denial and recursive native generators/strict SDK Clippy pass.

Logs: /private/tmp/fl-oct14-corrected-{package,install,runtime}.log; exact provenance:
/private/tmp/fl-oct14-corrected-checkpoint.json. Later Git-free packaging and draft
legacy-key/read-identity/JSON-wire slices have separate proof; they are not silently
included in this immutable checkpoint. This is local source-archive/runtime proof,
not public registry publication or an AWS deployment. The failed15e tracing receipt
remains retained. Checklist stays660/682 (96.8%),22 broad gates open.


## Billing provider read identity (2026-10-14)

Configured Plan and Subscription mappers expose optional validateProviderRead
hooks; getters validate exact stored external ID and provider object kind before
attaching data. Native read enrichment checks the same identity. User/organization
subscription getters and lists are covered; lists retain skip-on-provider-error
semantics. Generic custom mappers without the hook retain their fallback. Local
UUIDs/provider fields are preserved and malformed responses produce fixed errors
without local writes or provider mutations; customer IDs are not inferred from
local party UUIDs. Legacy transport fixtures now use official singular Stripe
object tags.64 actual configured TS cases,353 mapper regressions,52 creation
cases, permanent12-root types with zero diagnostics,2 native loopback tests,170
legacy operations and strict all-feature/all-target billing Clippy pass:
/private/tmp/fl-billing-read-identity{,-configured,-creation,-types,-native,-legacy170,-clippy}.log.
Native proof uses current working dependency sources, including frozen draft
versioned-JSON/MCP changes; the earlier immutable archive excludes this slice.
Provider update preflight and post-effect response validation remain separate.
CI now runs the focused configured read fixture.


## Shared local key configuration and legacy metadata inheritance (2026-10-14)

Validated KeyPairConfig on KeyRing/AuthHttp configures supported algorithms and
2048–4096-bit RSA generation, retaining EdDSA by default. Missing/null native
stored algorithm metadata inherits that trusted profile consistently during
signing, explicit ID pinning, publication and verification. Missing public alg/kid
and row curve normalize in memory only; coordinates and public curve are required,
public JSON is bounded before parsing, conflicts/private/unknown fields reject.
Explicit records preserve existing restoration/publication/verification semantics;
malformed verification remains null rather than changing internal failure class.
Issuance, empty-store initialization and rotation use the same configuration.
Per-call jwks overrides preserve upstream's shallow merge: empty jwks resets to
EdDSA/2048 rather than inheriting the server profile. No row rewrite/audit or HTTP
pinning endpoint is introduced. Arbitrary JWK override spreads and encrypted
Better Auth private-JWK import remain outside the typed native profile.

16 actual Better Auth/JOSE cases,5 focused native config tests,96 all-feature IAM
tests/2 ignored PG tests, existing hooks/public-only/pinning oracle regressions
and strict all-target Clippy pass:
/private/tmp/fl-jwks-legacy-{ts,focused,native,adapter-regression,clippy}.log.
Clippy initially caught a new doc backtick warning; only that comment changed
after the full suite, and the final strict run passed. This working-tree receipt
includes frozen draft HTTP/MCP/validation dependencies recorded in
/private/tmp/fl-iam-legacy-framework-dependency-hashes.json; it is not the earlier
immutable archive checkpoint. CI runs the new actual legacy oracle.


## Exclusive ordered Zod number bounds (2026-10-14)

The bounded opt-in number helper now supports ordered gt/lt alongside inclusive
min/max, using exact JavaScript-safe integer bounds. Equality rejects with the
actual default exclusive diagnostic; successful IEEE-754 values, signed zero and
infinities retain their bits.47,090 actual ForkLaunch Zod v3/native comparisons
pass across single and chained constraints, boundary fractions and safe-integer
edges. All 17 validation tests and strict all-feature/all-target Clippy pass:
/private/tmp/fl-zod-exclusive-{ts,native,parity,clippy}.log. Existing native/default
schemas remain unchanged. Fractional/nonfinite bounds, multiple-of and custom
messages remain outside this helper.


## Guarded versioned JSON wire parsing and MCP bytes (2026-10-14)

An explicit versioned JSON wire profile reuses JsonRequestParsers once before
version callbacks, retaining original bytes and selected parsed values. Framing,
media skips, empty input, strict null, charset, gzip/limits and verifier failures
are characterized against actual TS. The first version owns parser rules. The
profile requires request Error mode, including stale-mode rejection: unmatched
None/Warning parsed-value projection requires broader outcome state, so those
modes are explicitly rejected. Consuming parser/body/field hook builders reset
private plan identity, preventing reuse of prepared outcomes with changed hooks.

MCP now derives trusted framing from encoded bytes, with zero length for bodyless
POST/PUT/PATCH and unframed empty DELETE/OPTIONS. Caller framing remains forbidden.
JSON strings preserve TS safeStringify wire text instead of adding quotes. TS MCP
also omits undefined media headers on bodyless routes while retaining supplied
headers. Real fetch/body tests cover bytes, Unicode, strings/null/arrays, forms,
files, multipart, empty methods and unchanged caller headers. Omitted declared
MCP bodies retain existing Missing body admission; six actual TS boundary cases
are recorded separately rather than claimed equivalent.

All-feature HTTP66/Axum140/MCP21 tests and strict all-target Clippy pass.120 actual
TS wire observations support80 Error-mode native/local comparisons:56 exact and
24 retained generic schema-diagnostic differences, with all statuses and callback
counts matching. Eight real MCP invocations preserve supplied bodies with one
verifier/handler;51 actual body/framing comparisons pass. Canonical366 retains
zero OpenAPI differences; version responses retain108 exact pairs/20 diagnostic
differences; pre-auth54 actual cases/90 native requests retain52 exact/38 known
diagnostic differences and zero ordering differences. Permanent12-root types
have zero diagnostics. Logs:
/private/tmp/fl-version-json-wire-{allfeatures,clippy,parity,mcp-body,canonical,response,diagnostics,types}.log.
CI runs the new live wire oracle. Finite Unicode/nonfinite JSON and broader
None/Warning/omitted-body semantics remain open; no broad gate is closed.


## Billing identity validation before provider mutations (2026-10-14)

Plan replacement and Subscription updates now validate the initial provider GET
identity/kind before deriving product/item fields or issuing DELETE/POST. Optional
configured mapper hooks preserve custom fallback; native default services enforce
the same read identity. Malformed/wrong provider results cause exactly one GET,
zero provider mutations and unchanged local records. Valid mutation sequences
remain intact. Existing preflight fixtures now use official provider object tags.

22 focused configured TS cases, Plan13/Subscription28 regressions,353 configured
mapper cases, permanent12-root strict types with zero diagnostics,4 native read/
update loopback tests,170 legacy operations and strict all-target/all-feature
billing Clippy pass: /private/tmp/fl-billing-update-identity{,-plan,-subscription,
-configured,-types,-native,-legacy170,-clippy}.log. CI runs the new focused fixture.
This prevents unchecked reads from selecting mutation inputs; validation of
post-effect responses and interrupted provider/local atomicity remain separate.


## Configured local JWKS rotation lifecycle (2026-10-14)

KeyPairConfig optionally stamps newly minted local keys with a validated whole-
millisecond rotation interval. Zero/default disables expiry; issuance, explicit
local signing, empty JWKS initialization and operator rotation share the policy.
Shallow jwks overrides replace the configured interval. Expired unpinned keys
replace lazily; explicit ID pins never mint a replacement. Existing key rows, JWT/
cookie expiry and custom persistence ownership remain independent. Unsupported
intervals and expiration overflow reject before repository I/O. No timer is added.

Actual clock-controlled Better Auth observations cover absent/zero,1ms/2s expiry,
unpinned equality replacement, pinned equality admission then rejection, strict
2ms publication grace and empty-override reset. Four focused native tests include
those lifecycle boundaries, pre-I/O bounds and real HTTP ES256 token/session-
header propagation with payload and lifetime callbacks independently enabled.
All 100 IAM tests pass with two explicit ignored PG tests; strict all-feature/
all-target Clippy passes: /private/tmp/fl-jwks-rotation-{ts,focused,native,clippy}.log.
CI runs the new clock-controlled oracle. This working-tree proof includes the
HTTP/MCP changes now committed as91ddee904; it is separate from the174594c48
immutable archive. Default backend transactions/storage were unchanged; earlier
encrypted expiry-first insertion PG proof remains separate and was not rerun.


## Unmatched versioned JSON parser state (2026-10-14)

The JSON wire profile now supports request None/Warning as well as Error. A
private selection outcome retains first-parser/reviver output independently of
version matching. Opaque VersionParsedBody exposes parsed value and decompressed
parser input through read-only accessors; original wire bytes remain unchanged.
JSON null, skipped parsing and reviver root removal retain distinct projections.
Unmatched requests keep version absent and never use a failed candidate transform.
Trusted application of direct/prepared outcomes replaces supplied derived state;
input-binding and hook-identity checks remain intact. Ordinary direct dispatch
avoids an extra request/body snapshot; only prepared pre-auth reuse retains one.

The former48 unmatched-value differences are resolved:120 actual TS observations
now produce240 native/local comparisons across all modes,216 exact plus24 retained
generic schema-diagnostic differences.24 real MCP tools preserve bodies/callback
counts. Protected routes cover60 default/profile/local/native/mode cases, including
denied unmatched requests. Gzip tests preserve original/decompressed separation.
HTTP69/Axum140 tests, strict all-feature/all-target Clippy, canonical366/zero OpenAPI
differences and pre-auth90 regressions pass:
/private/tmp/fl-version-json-unmatched-{allfeatures,clippy,parity,canonical,diagnostics}.log.
The prior Error-only guard is removed with this broader state proof; omitted
declared MCP-body admission and24 diagnostic differences remain explicit gaps.


## Billing post-effect local identity protection (2026-10-14)

Plan creation/replacement now compares the expanded provider GET against the
just-created plan ID/kind before local persistence. Subscription updates compare
the POST response against the stored reference/kind. Configured optional hooks
preserve custom fallback; native default services enforce the same checks. Local
mapper/assign/upsert/persist are not called for mismatches, and local snapshots,
input objects and public UUIDs remain intact. These checks run after provider
effects may have occurred: errors do not mean provider no-op. No retries, refunds,
compensation or reconciliation guarantees are added.

33 focused actual TS cases, Plan-create14/Plan-update13/Subscription-update28,
353 configured cases, prior22 update/64 read cases and permanent12-root strict
types pass. Three native loopback tests prove exact effect sequences and no local
write/retry;170 legacy comparisons and strict all-feature/all-target billing
Clippy pass: /private/tmp/fl-billing-post-effect-identity{,-types,-native,-legacy170,
-clippy}.log and corresponding named regression logs. CI runs the focused fixture.
Interrupted effects and broader configured provider response semantics remain open.


## Trusted signing algorithm selection (2026-10-14)

KeyRing.sign_jwt_with_algorithm selects the newest live matching effective
algorithm with one reader invocation and no latest-other-algorithm fallback.
Existing matching keys may be used without advertisement; only missing-key
creation requires the configured primary or a trusted additional profile. Typed
extras are bounded to16, unique supported algorithms, no nested lists or per-extra
rotation. Shared rotation applies. Shallow jwks overrides clear inherited extras;
the constrained API rejects raw keyPairConfigs rather than accepting new mint
authority from JSON. Expired/public-only/unknown profile boundaries are tested.

Trusted create callbacks remain authoritative, including a returned algorithm
different from the requested generation profile. This is explicitly tested and
documented, rather than falsely guaranteeing an algorithm after custom creation.
Existing exact key-ID assertions stay strict and never mint replacements. No HTTP
algorithm-selection authority or default backend transaction changes are added.
Ten actual JOSE cases, five focused native tests,105 all-feature IAM tests with
two explicit ignored PG tests, and strict all-target Clippy pass:
/private/tmp/fl-jwks-algorithm-{ts,focused,native,clippy}.log. The source receipt is
52288e9ca plus the six-path IAM delta, distinct from immutable archive proof.
CI runs the actual oracle. Resolved-key/external-signer metadata and malformed
legacy formats beyond the typed profile remain separately unclaimed.


## Provider creation response shape before local billing writes (2026-10-14)

All five configured billing CreateMappers now expose an optional typed provider-
creation validator, called after POST and before consuming data or local mapping/
cache/persistence. Plan/Portal replacement creation uses the same check. Shared
private blueprint validation checks nonblank ID,1024 UTF-8 byte bound, Unicode
control exclusion and exact Stripe discriminator, with native default checks at
the same boundary. JS whitespace/BOM-only IDs reject in both implementations.
No prefix, account/customer ownership or prior-ID equality is inferred. Generic
custom mappers retain fallback. Provider effects can already exist on failure;
no compensation, retry or rollback promise is added.

An actual-source probe reproduced the original Checkout/PaymentLink/Portal defect:
missing provider IDs still persisted/cached local UUID records.186 focused cases
now reject malformed responses across the five families/replacements, preserving
valid options/sequences and local snapshots.353 configured regressions, permanent
12-root types/610 exports with zero diagnostics and related creation/update/read
fixtures pass. A native seven-operation matrix,170 legacy operations and strict
all-feature/all-target billing Clippy pass:
/private/tmp/fl-billing-creation-response-shape{,-native,-legacy170,-clippy,-types}.log.
Two final TS replacement BOM cases extended184 to186 after the native proof;
native BOM handling/tests were already present and unchanged. The shared helper
is internal to the private configured blueprint, not an implementation API export.
CI runs the new fixture. Public UUID/provider alias integration and remaining
consumed-field/update/cache semantics keep gate855 open.


## Explicit MCP body admission and root defaults (2026-10-14)

Opt-in McpBodyAdmissions binds finite required/root-default/null policies to exact
projected HTTP tool contracts. It validates inline body schema and bounded input
before transport, injects only explicit trusted root defaults, and projects the
same required/default/null policy into tools/list. Missing/stale/duplicate routes
and invalid defaults/bounds reject at registration. Local references preserve
their scope in emitted schemas; annotations alone never inject defaults. Nullable
text is admitted as null before encoding zero bytes, matching fetch. Existing
unregistered tools retain native behavior and forbidden caller transport headers.

A real registered FastMCP audit first characterized80 direct and80 Express-backed
calls, separating admission from downstream parsing. Final native proof passes48
comparisons (26 admitted exact-wire,22 rejected admission),12 metadata cases,24 MCP
tests and strict all-feature/all-target Clippy. Axum140 and legacy51 MCP body
comparisons/6 boundary observations passed before the isolated final nullable-
export correction. That correction removed invalid empty allOf and was rechecked
by the full MCP suite, Clippy and registered oracle. Logs:
/private/tmp/fl-mcp-admission-{focused,allfeatures,clippy,parity,legacy-body}.log.
All28 option references use constructors/mutation; no caller struct-literal updates
were necessary. CI runs the oracle after installing its canonical dependencies.
Diagnostic text, optional omission, recursive defaults, arbitrary Zod transforms/
unknown-key stripping and downstream text projection remain explicit boundaries.


## External signer metadata and default key window audit (2026-10-14)

Additive JwtSigningContext forwards bounded trusted JSON-object headers and key/
algorithm hints verbatim to context-aware external signers. Absent/empty headers
and explicit nulls retain their distinctions. Existing JwtSigner implementations
delegate through the provided method and intentionally ignore metadata; wrappers
explicitly forward it when supported. The new KeyRing entrypoint bypasses local
key lookup/minting/algorithm checks, matching actual external-signing behavior.
The trusted callback owns enforcement and publication. Authenticated HTTP calls
supply empty context; no client-controlled key authority is added. Context has no
Debug/Serialize and bounds header depth/nodes/raw/encoded size and hint bytes.

Six actual Better Auth/JOSE cases, three focused context tests within five native
custom-signer tests, four HTTP external-signer regressions and strict all-target/
all-feature Clippy pass: /private/tmp/fl-external-signing-context-{ts,native,
regression,clippy}.log. Callback errors remain sanitized; arbitrary JavaScript
undefined/prototypes and caller-resolved keys remain outside this typed contract.
The105-test IAM suite/PG were not unnecessarily repeated: storage/default signing
were unchanged and the modified custom/external dispatch paths had focused proof.
The95f1b203b immutable runtime snapshot excludes this later additive slice.

A separate actual default-adapter audit proves algorithm lookup reads100 rows
without sortBy, then selects newest createdAt from that page; row101 is invisible
unless advanced.database.defaultFindManyLimit is raised. A custom reader sees its
full list in one call: /private/tmp/fl-jwks-window-audit.{mjs,log}. Native keeps the
default window; an unlimited indexed query would incorrectly change behavior.
Cross-database membership of an unordered page and optional global-limit settings
remain explicit boundaries rather than claimed deterministic equivalence.


## Immutable packaged checkpoint with MCP admission (2026-10-14)

The Git-free source archive at95f1b203b80ab979850117680925e969c9775334
passes the complete packaged installation/runtime harness: portable generated
services and workers, feature checks, WebSocket/AsyncAPI contracts, both SDKs,
recursive clients/Clippy, the105-test IAM suite with two explicit ignored
PostgreSQL tests, and generated accounts HTTP/MCP startup and callbacks. This
checkpoint includes creation-response validation, algorithm selection and MCP
admission; it excludes the later2a09c5bcd external-signing-context slice and
subsequent text-parser/SDK drafts. Billing feature compilation in this harness
is distinct from the separate generated billing runtime proof.

Offline locked installation and source repackaging pass with2531 retained files,
including embedded dotfiles and excluding dependency/output directories. The
retained archive cli/target/parity-artifacts/forklaunch-95f1b203b-4067ee2d.crate
has SHA2564067ee2dcc155ba9476a4790f177748ea8e9353691b5ef58a2799f256f4e2848.
After runtime, the hash and every extracted file were compared with the archive
and pass. Logs: /private/tmp/fl-oct14-latest-{package,install,runtime}.log;
metadata: /private/tmp/fl-oct14-latest-checkpoint.json. After the billing fixture completed, both owned temporary roots were removed
with no process/openfile references; about211MiB was reclaimed. Broad release,
publication, cloud and conformance gates remain open; checklist660/682 (96.8%).


## Generated billing runtime with official response kinds (2026-10-14)

The generated billing runtime fixture now returns official Stripe object kinds
with realistic plan_/sub_/cs_/plink_/bps_ identifiers for all five families. The
full harness passes using the immutable95f1b203b installed CLI; generated native
Stripe services bytes exactly match that checkpoint, with only the provider-mock
fixture overlaid. Base and Stripe HTTP/MCP startup/discovery, five-family lifecycle,
direct/managed transports, signed webhook/platform events, tenant isolation,
persistence/reopen, both SDKs, encrypted PostgreSQL actor audit and CLI rename/
sync/delete pass: /private/tmp/fl-billing-generated-identity-runtime.log. The
owned runtime database role was removed successfully.

A separate actual MikroORM7.2.3 connect:false probe of all five configured entity
definitions proves local UUID allocation immediately at em.create, rather than
in a service stub: /private/tmp/fl-billing-actual-orm-id-audit.log. Native session/
link/portal public provider IDs therefore still differ from configured TS UUIDs.
No UUID-required native route failure appeared, but mock success proves neither
provider ownership nor safe mutation aliases. Trusted ownership-bound reference
resolution and remaining mutation/consumed-field semantics keep gate855 open.


## First-parser versioned text wire semantics (2026-10-14)

An additive enable_typescript_text_wire profile parses framed, exact-media text
once before ordered version selection, preserving absence versus framed empty
text and unmatched parsed values in Error/Warning/None modes. It reuses charset,
inflation, byte limits and trusted verifiers, keeps original compressed request
bytes separate from decompressed raw bytes, and shares cached pre-authentication
outcomes. Registration rejects stale/duplicate/conflicting parser profiles and
JSON revivers for text. Existing JSON wire behavior remains covered.

The real TypeScript oracle observes264 inputs;528 native/local comparisons pass
with456 exact results and72 explicit existing schema-diagnostic differences. All
status, verifier and handler counts and unmatched projections agree.48 supplied-
text MCP calls and48 protected text/auth cases pass, alongsideHTTP72, Axum141
and strict all-target/all-feature Clippy. JSON regression240 comparisons/24 MCP
calls and canonical IAM366 outcomes/zero normalized OpenAPI differences pass.
Logs: /private/tmp/fl-version-text-wire-{allfeatures,clippy,parity,json,canonical}.log.
CI runs the oracle after canonical dependencies. Unversioned text transforms,
optional MCP omission and generic validation diagnostics remain open.


## Real mixed SDK and Better Auth client composition (2026-10-14)

Shared/generated Better Auth factories now accept optional coreHost so a host
containing an auth path can still call application APIs at an explicitly chosen
origin. The default remains host, preserving intentional reverse-proxy prefixes.
UniversalSdk validates referenced JSON/SSE responses in their original OpenAPI
document scope. Standard factories isolate AJV instances per document; direct
constructor callers retain their complete supplied validator/custom keywords,
with an additive optional factory for document isolation. Conflicting explicit
IDs fail rather than replacing schemas; missing/external references never fetch.
Boolean component schemas, nested refs, component IDs and cross-client/version
isolation have real validator tests. Referenced date/binary coercion remains an
explicit separate boundary.

The previous universal-SDK construction stub is removed from the frontend fixture.
Actual shared and freshly generated factories each pass21 owned HTTP requests;
strict TypeScript7.0.2 checks the SDK root against611 source export paths and real
declarations with zero diagnostics. The permanent configured-types fixture now
has an additive --sdk profile, run in CI alongside its existing12 roots. Body/
form13 and SSE35 comparisons pass. A newly source-built CLI-generated IAM app
passes eight shared/generated mixed-client runs with14 native HTTP actions each
(112 total), including core discovery/current grants and actual Better Auth
session/profile/JWT/signout/signin behavior for default/custom/root/URL-host paths.
HTTP/MCP shared/separate/disabled listeners, both SDKs, encrypted sessions, tenant
isolation/restart and rename/sync/delete pass; the owned PG role was cleaned up.

Logs: /private/tmp/fl-real-frontend-native-runtime.log,
/private/tmp/fl-frontend-{composition-fixed,generated-composition}.log,
/private/tmp/fl-real-frontend-sse.log and /private/tmp/fl-sdk-reference-types-permanent.log.
Source/CLI hashes: /private/tmp/fl-real-frontend-source-hashes.log. This is a
source-built checkpoint, distinct from immutable95f1b203b. DNS forwarding, a Node
cookie jar and scoped fetch forwarding are owned fixture components; actual SDK,
AJV and Better Auth clients execute against live native HTTP. Neither browser
coverage nor HTTP SDK access to MCP is implied. CI installs pinned bundling/
validation dependencies and runs the composition fixture.


## PaymentLink provider update result and single local mapping (2026-10-14)

Configured Stripe PaymentLink updates now expose an optional toLocalUpdate hook
which validates the already-applied provider result before local persistence:
exact dispatched ID, payment_link object kind and numeric finite/nonoverflowing
consumed totals. The wrapper preserves the caller patch and public ID and invokes
Base.update once. A schema-checked configured toCachedDto hook performs cache-only
updates with zero ORM calls; database backup maps once. Generic hook-absent mappers
retain their historical sequence. Native rejects mismatched returned IDs/kinds
instead of redirecting the local update to an arbitrary provider response ID.

36 actual-source cases,30 preflight cases,353 configured regressions and12-root
TypeScript7 strict types pass. The native loopback covers10 invalid results and
a valid boundary, preserving exactly one provider POST and zero local writes on
invalid results;170 legacy operations and strict all-feature/all-target billing
Clippy pass. Logs: /private/tmp/fl-payment-link-update-result{,-preflight,
-configured,-types,-native,-legacy170,-clippy}.log. CI runs the new source fixture.
Formatting changed no semantics. Provider effects already exist at rejection;
no retry/compensation is promised. Original dispatch IDs remain unchanged, so
UUID/provider mutation resolution and account ownership still keep gate855 open.


## Trusted optional MCP body omission (2026-10-14)

The TypeScript MCP JSON encoder now leaves admitted undefined bodies absent
instead of emitting literal invalid JSON undefined. Native MissingToolBody::Omit
explicitly authorizes absence for an exact schema-bound route and removes body
requiredness from tools/list without injecting null/default values. Supplied
bodies still validate; legacy unregistered missing bodies still reject. Only
trusted admission produces the private encoder omission outcome. Caller framing
headers stay forbidden. POST omits wire bytes with length0; empty DELETE remains
unframed, preserving downstream parser differences. Explicit body:null remains
distinct. Route-owned media and explicit incompatible contentType guards remain.

120 registered FastMCP/native comparisons pass (74 exact admitted wire,46 rejected
admission), with20 metadata declarations and140 actual registered FastMCP/Express
omission/framing/downstream assertions. Native MCP25, strict all-target Clippy,
permanent12-root strict types, legacy MCP51+6, JSON240 comparisons/24 MCP, text528
comparisons/48 MCP and canonical IAM366 outcomes/zero OpenAPI differences pass.
Logs: /private/tmp/fl-mcp-omission-{native,clippy,parity,body,json,text,canonical,types}.log.
Two initial new test expectations were corrected: route media owns Content-Type,
and unframed empty text permits the downstream default. Production guards were
not relaxed.20 observed TS whole-arguments-null protocol rejections remain outside
the native body policy; explicit optional absence during native version-schema
selection is separate work. No generic Zod/default-transform equivalence is claimed.


## Required Portal provider URL before local writes (2026-10-14)

Configured Portal create/replacement validation now requires the provider's
nonblank string URL before consuming it or writing local/cache state. Native
uses a dedicated portal_uri helper; missing/null/nonstring/blank/BOM-only URLs
reject. This prevents replacing provider references while silently retaining an
old session URL. Valid strings remain byte-for-byte unchanged; generic custom
hook fallback, optional DTO input and Checkout nullable URL handling are retained.
This is shape validation, without a new host/scheme authorization policy.

56 actual-source cases,353 configured regressions,186 creation-shape cases and
12-root strict types pass. Native create/replacement covers20 invalid and6 valid
operations; the seven-operation creation matrix,170 legacy operations and strict
all-target/all-feature billing Clippy also pass. Invalid responses follow exactly
one POST with no local/cache writes and retain the previous URL/reference. Logs:
/private/tmp/fl-billing-portal-url-{shape,native,configured,creation,types,legacy170,clippy}.log.
The focused TS log is /private/tmp/fl-billing-portal-url-shape.log. CI runs it.

Provenance correction: concurrent staging included the native helper/call sites
in2ba036a87 alongside PaymentLink before this dedicated proof completed. This
receipt records the later successful native proof; the configured TS hook and
dedicated tests are integrated here. No earlier immutable/runtime checkpoint is
claimed to include this slice. Provider effects are already applied on rejection;
no compensation is promised.


## Referenced shared-SDK date and binary conversion (2026-10-14)

Actual UniversalSdk previously returned Date/ArrayBuffer for inline properties
but strings for identical validated component references. It now resolves the
supported conversion subset in the original AJV document/resource scope without
remote fetch, schema expansion or shared-registry mutation. Finite response-value
traversal supports nested properties/items and referenced root objects/scalars/
arrays, with escaped pointers, nested relative IDs and per-version isolation.
The response document/pointer is captured for an entire stream, so a concurrent
registry refresh cannot change conversion after the first event. Ambiguous new
string unions convert only when branches agree; arbitrary structural/conditional
compositions and anchors remain explicit unsupported boundaries. The old public
two-argument coercion helper retains its behavior.

New schema inspection is deliberately bounded to10000 visited nodes/depth64,
including inline schemas. Referenced conversion bounds reference chains256, total
references10000 and traversal depth64/nodes10000. Budget failures return no
partially converted value; cycle/direct immutable-failure checks pass.18 actual
HTTP cases,21 composition and13 body/form regressions,35 SSE comparisons and
permanent TypeScript7 strict SDK checks pass. Logs: /private/tmp/fl-sdk-reference-
coercion.log, /private/tmp/fl-sdk-coercion-sse.log and the focused SDK type receipts.
CI runs the real-source coercion fixture.

This is shared UniversalSdk behavior, also used by mixed generated client
factories; the independent generated contract SDK still uses JSON.parse. Native
IAM client112-action proof was not unnecessarily repeated for this conversion-
only slice; focused response/SSE/validator tests cover the changed paths. The
source inventory now records612 entrypoints/1674 declarations across44 packages,
with zero unresolved entries; syntax coverage does not close semantic gate849.


## Consumed Plan and Subscription references before mutations (2026-10-14)

Configured optional validateProviderUpdate hooks now reject unusable Plan product
references before DELETE/replacement and Subscription item IDs/consumed fallback
plan IDs before POST. Native mirrors the guards. A shared private reference
validator preserves nonblank/control/UTF-8 byte rules used by creation shape
checks. String/expanded product IDs are supported. Subscription absent fallback
plans remain omitted, empty items remain valid, and truthy caller product
overrides bypass unused fallback validation. Generic custom hook-absent sequences
and valid provider-effect ordering are unchanged. No alias/account authority is
inferred from provider fields.

81 actual-source cases,353 configured regressions,22 update-identity cases,186
creation-shape cases and12-root strict TypeScript checks pass. Two focused native
tests and the creation matrix,170 legacy operations and strict billing all-target/
all-feature Clippy pass. Invalid references cause one provider GET and zero
mutating requests/local writes, preserving caller input and local snapshots.
Logs: /private/tmp/fl-billing-consumed-references{,-configured,-update,-creation,
-types,-native,-legacy170,-clippy}.log. CI runs the new fixture. Broader consumed
provider fields, complete validators and public UUID mutation mapping remain open.


## Trusted caller-resolved signing capability (2026-10-14)

Opaque ResolvedSigningKey imports validated private record material once and has
no Debug/Serialize/Clone interface. KeyRing.sign_jwt_with_resolved_key reuses its
ID/algorithm and existing claim builders without storage reads, adapter callbacks
or minting. Record expiry/removal is deliberately not rechecked, matching actual
caller-resolved upstream key reuse; the trusted caller owns capability lifetime.
Existing strict key-ID selection and external HTTP/context signing remain
unchanged. This local API does not select or override an HTTP signer. Raw resolved
JSON/public-only records cannot construct the typed capability; arbitrary local
JOSE header/crit/b64 options remain outside this entrypoint.

The actual Better Auth/JOSE oracle proves initial resolution versus zero reuse
reads/creates, resolved ID/algorithm authority, expiration/removal behavior,
public/mismatched key failures and external-signer precedence. Native resolved2,
custom signer5, HTTP external4 and strict ID2 tests plus strict all-target/
all-feature IAM Clippy pass. Native signatures use the independent public ECDSA
verifier; upstream tokens use JOSE. No claim of a new native-token-to-JS verifier
roundtrip is made. Existing signing-protocol interoperability was not repeated.
Logs: /private/tmp/fl-resolved-key-{ts,native,custom,external,pin,clippy}.log.
Record bounds remain ID512 bytes/PKCS8 6144 bytes; existing claim validation is
reused without inventing a new claim-size guarantee. CI runs the actual oracle.


## Explicit optional request-body contract and safe root documentation (2026-10-14)

RequestContract.with_optional_body explicitly admits absent parsed values while
supplied null still validates normally. Required builders reset the flag; false
is omitted from serialized artifacts and version projection preserves it. Native
version selection/defaults/transforms, OpenAPI requiredness, SDK plans and typed
TS optional bodies read the same declaration. Parameter/query-free TS operations
permit omitted input; supplied-body types remain checked. Rust's historically
permissive Option request representation is retained. Implicit MCP omission uses
bounded inline JSON/text schemas; explicit admission policies remain overrides,
including stricter Reject and independent default/Omit behavior. Exact wire
profiles preserve framed empty versus skipped/absent parser results.

The TS OpenAPI hardcoded requestBody.required defect is fixed with an additive
optional adapter.openapiRequestBody projection. Root Zod Optional/Default/input
Effects/Nullable wrappers are inspected without running root user callbacks;
TypeBox uses optional markers, without inferring presence from default annotations.
Unknown preprocess/refinement optionality remains conservative. Dynamic root
default annotations are omitted; nested/general exporter behavior is unchanged.
Custom validators without the capability preserve previous required/export rules.
16 throwing-root callback cases keep counters at zero, alongside6 presence/null
cases,2 default-annotation distinctions and custom fallback checks.

240 actual TS inputs produce480 native/local pairs (416 exact,64 existing schema-
diagnostic differences), with16 MCP checks, artifact/version/OpenAPI/SDK metadata,
strict TS positive/negative consumers and live omitted/supplied SDK calls. Native
Axum142/HTTP72/MCP26/contract25/OpenAPI10/codegen43/RustSDK5/TSsdk8 and strict
eight-crate all-feature/all-target Clippy pass after required-builder reset and
stale/null tests. Canonical IAM366/zero OpenAPI differences, MCP omission120 plus
140 Express outcomes and permanent strict types pass. CLI sdk::generate32 tests
include the final artifact/body-type/optional-input regression; the earlier build
passed with four existing warnings. Parent moved only adapter documentation
comments after source freeze, preserving semantics.

Logs: /private/tmp/fl-version-optional-{native,final-tests,clippy,parity,openapi,
canonical,admission,types,sdk}.log and /private/tmp/fl-optional-body-cli-final32-tests.log.
CI runs the oracle. Source inventory613 entrypoints/1675 declarations remains
syntactic; arbitrary schemas/callbacks, other wire profiles and complete artifact
equivalence stay open. This source proof is separate from the next immutable
packaged checkpoint.


## Configured diagnostics and caught session-list query failures (2026-10-14)

AuthApiErrorTraceMode::Configured is additive and independent of callback mode.
Historical library AllServerErrors tracing remains default; generated presets
select Configured plus UnexpectedOnly callbacks. Private internal-catch provenance
retains one safe diagnostic for characterized get-session/revoke/list catches,
while silent handled API failures produce none. Session-list find_page failures
now return handled empty500 with unexpected callback0, matching the actual catch;
auth/freshness/commit errors are not blanket-converted. No freeform exception/
stack logging is added and private response markers are removed before delivery.

The actual14-case/seven-route fault oracle distinguishes unexpected callback,
internal catch and explicit APIError behavior.10 focused native observer/fault
tests include subprocess-isolated trace counts1/0/1 and callbacks1/0/0, historical
default behavior and authenticated list query failure. Existing actual observer
regression, strict all-target/all-feature and minimal-feature Clippy pass. Logs:
/private/tmp/fl-configured-diagnostics-{native,ts,observer-oracle,clippy,minimal}.log.
No full IAM/PG rerun was needed for the new diagnostic mode and bounded catch;
storage semantics are unchanged. Uncharacterized endpoints/plugins, native typed
error-representation boundaries and logger payload bytes remain explicit gaps.
CI runs the actual matrix; this is not blanket logging parity.


## Immutable optional-body/configured-diagnostics release checkpoint (2026-10-14)

Committed source 5a8eba9a8751a40764db62b3994245b6308790e8 passes Git-free
packaging, offline locked installation, full portable service/worker generation,
WebSocket/AsyncAPI, recursive SDK checks and live HTTP/MCP surfaces. All 2555
regular archive files match extracted bytes. Retained artifact:
cli/target/parity-artifacts/forklaunch-5a8eba9a8-70f69f3d.crate;
SHA256 70f69f3d0567ce8f399fcb03a7074e3aa59755ef4b0b1ad69b8b701c2c7bfe09.
Metadata /private/tmp/fl-oct14-optional-checkpoint.json; logs
/private/tmp/fl-oct14-optional-{package,install,runtime}.log. This immutable
checkpoint excludes later bound creation, two-factor and multipleOf changes.
Public publication and cloud deployment remain separate unverified gates.

## Direct bound PaymentLink creation journal (2026-10-14)

An opt-in trusted server profile creates a fresh native UUID and encrypted,
immutable intent before one direct Stripe POST. Finite approved price references,
strict line items and provider scope/version form the authorized profile. The
local declared amount is not proof of the provider price total or price ownership.
Managed accounts, fees/delegation and unsupported options reject before dispatch.
Duplicate operations inspect pending/historical completed evidence; they never
regain dispatch capability. Changed actor/profile/payload/version are denied.

A single local transaction binds the UUID record, provider alias generation,
audit and encrypted completion receipt, with scope checks after lock waits and
writes. Historical completion does not assert current provider authority. Lost
reservation COMMIT acknowledgement yields unresolved with zero POSTs; lost final
COMMIT acknowledgement yields unresolved but inspection finds completed evidence,
and replay sends no additional POST. Pending recovery is deliberately not an
automatic retry. Existing routes and legacy local/provider identity stay unchanged.

Real restricted PostgreSQL journal matrix passes (17.81s), including concurrency,
RLS, immutable journals, rollback faults, stale contexts, timeouts/malformed
responses and real wire-proxy lost acknowledgements. Bound-read twins pass2/2,
legacy billing170 and strict base/Stripe all-feature/all-target Clippy pass.
Official Stripe22.6.2 wire oracle independently verifies pinned version/auth,
idempotency, form values and no Connect header/retry. Logs:
/private/tmp/fl-billing-bound-creation-{pg-final,read-regression,legacy170,clippy,sdk}.log.
CI runs both the SDK oracle and ignored PostgreSQL matrix. Source hashes are
retained at /private/tmp/fl-billing-bound-creation-source-sha256.json; validation's
subsequent ryu-js lock edge is excluded from this billing receipt. Managed creation,
full mapper semantics and interruption recovery remain open broad gates.


## Zod v3 multipleOf runtime constraints (2026-10-14)

The opt-in ordered number helper now supports bit-preserving ZodMultipleOf for
all IEEE divisors, including negative, signed zero and nonfinite registrations.
This runtime capability is separate from JSON Schema mathematical multipleOf.
Actual Zod uses decimal formatting, fixed formatting and decimal parseInt prefix
remainder; scientific spellings have deliberately preserved quirks (1e-7 rejects
even zero/self, while 1e21 can admit small fractions). Checks preserve order,
coercion/NaN early rejection, diagnostic spellings and accepted IEEE output bits.

192816 actual ForkLaunch Zod v3/native comparisons and seven unusual constructor
cases pass, including seeded IEEE values, adjacent rounding and ordered checks.
The oracle caught a ryu-js1.0.3 fixed-format discrepancy for a tiny value; that
formatter is not used. Only its ECMAScript shortest formatting is retained. Exact
Rust fixed formatting is corrected at exact binary halfway cases using checked
u128 IEEE scaling for JavaScript magnitude-up ties, without a big-integer package.
Focused tests retain the discovered tiny-value, signed tie and exponent examples.
Validation19 library tests, previous47090 number comparisons and strict
all-feature/all-target validation Clippy pass. Logs:
/private/tmp/fl-zod-multiple-{parity,tests,number-regression,clippy}.log;
frozen hashes /private/tmp/fl-zod-multiple-source-sha256.json. CI runs the oracle.
Fractional/nonfinite comparison bounds, arbitrary callbacks/custom messages and
complete schema/emitter parity remain separate work. The 5a8 packaged checkpoint
predates this source change.


## Fractional and nonfinite Zod runtime bounds (2026-10-14)

Opaque bit-preserving ZodFloatBound adds MinFloat/MaxFloat/GtFloat/LtFloat to the
ordered runtime number helper, preserving existing exact integer constructors.
Fractional, subnormal, huge, signed-zero and infinite values use actual IEEE
comparisons and ECMAScript diagnostic spelling; NaN bounds never reject.
These are runtime checks, not JSON Schema declarations of nonfinite numbers.
63240 actual ForkLaunch Zod v3/native comparisons pass, including adjacent
values and ordered diagnostics/output bits. Full validation20 tests, previous
47090 integer-bound comparisons and strict all-feature/all-target Clippy pass.
No dependency/lock changes. Logs /private/tmp/fl-zod-float-bound-{parity,tests,
number-regression,clippy}.log; frozen hashes
/private/tmp/fl-zod-float-bound-source-sha256.json. CI runs the oracle. Arbitrary
schema/emitter coverage and custom messages/refinements remain open; this source
slice is later than the immutable5a8 packaged runtime checkpoint.


## Opt-in fixed TOTP, recovery and trusted-device lifecycle (2026-10-14)

The isolated Better Auth implementation now exposes TwoFactorService and an
explicit authenticated-versus-challenge credential outcome. Generated services
can opt in with BETTER_AUTH_TWO_FACTOR_ENABLED=true; default behavior remains
disabled. The fixed profile protects password sign-in with SHA1/6digits/30seconds,
plus-or-minus one step, ten single-use backup codes and rotating thirty-day
trusted browser cookies. Existing sessions and social callbacks retain upstream
independent behavior; this is not all-signin MFA or complete plugin parity.

An encrypted IAM-only factor model/migration holds secrets, recovery codes and
failure state. Tenant transactions atomically consume challenges/recovery/trust
and issue replacement sessions, rechecking password/session authority after
crypto work. Pending password login never creates an authenticated grant or JWT.
Challenge lifetime600s/5 attempts and aggregate10 failures/900s lockout match the
characterized defaults; route rate3/10s is independent. Timestep reuse across
distinct challenges follows upstream. Enable/get-URI/initial verify/disable and
recovery regeneration are present. The initial enrollment body retains the
actual upstream old-token/user snapshot while only the rotated cookie session
is persisted; get-session subsequently signs the authoritative replacement.
Session rotations preserve signed dont_remember behavior. Secret projections
have no Debug/Serde; authorized recovery disclosure must not be logged.

Actual Better Auth reference68 HTTP calls characterize lifecycle/client/rate
behavior. Real twoFactorClient/native handler bridge20 calls passes, with JOSE-
verified authoritative session JWT, rotation/challenge/trust/recovery/disable.
This bridge is owned JSON-line HTTP/cookie transport, not browser/TCP evidence.
Focused native5 tests plus finalHTTP2, restricted PostgreSQL1 (44.51s; owned roles
and tenant rows cleaned), full all-feature IAM117 passed/3 ignored (66.18s), and
strict all-feature/all-target plus minimal-library Clippy pass. Logs:
/private/tmp/fl-two-factor-{oracle,native,http-final,client-native,postgres,
full-iam,clippy}.log. Frozen25-path hashes/aggregate:
/private/tmp/fl-two-factor-source-freeze.json. The final cookie fix affects HTTP;
the earlier restricted PG domain proof remains unchanged. CI runs both actual
fixtures and the ignored PG lifecycle test. Fresh CLI build passes with four
existing warnings; generated opt-in liveTCP HTTP/MCP proof is the next checkpoint.
OTP delivery, custom parameters/storage hooks, backup disableSession, passkeys,
admin/SSO/provider plugins and broader client/options coverage remain open.

## Managed creation platform journal checkpoint (2026-10-14)

Companion forklaunch-platform-rust-parity commit
 a34ec3b7ceff8d3c029bc7222ab2b2700e90b138 adds explicit signed create/inspect,
strict canonical form bytes, copied adapter/profile fingerprint and encrypted
immutable claim/completion journal. Default bootstrap is disabled. Real restricted
PostgreSQL tests cover account/generation/delete/ABA barriers, races, RLS,
non-superuser/non-BYPASSRLS function ownership, same-org receipt swaps and both
lost COMMIT acknowledgements. An explicit SELECT-only function-owner policy
requires distinct restricted runtime roles without inheritance. Receipt AAD
binds org/kind/envelope hash; claims never automatically expire or retry.
56 tests and focused strict types pass through the checked-in reproducible
harness. Logs /private/tmp/fl-managed-creation-reproducible.log; hashes
/private/tmp/fl-managed-creation-source-sha256.json. Native UUID publication,
current creation-configuration pinning, managed recovery and full production
bootstrap are subsequent work, not claimed complete by this platform proof.


## Ordered Zod literal string constraints (2026-10-14)

Borrowed ZodStringRule and zod_string_rules add startsWith/endsWith/includes to
existing strict-string operations without changing the original Copy step API.
Includes positions follow JavaScript UTF16 indexing, numeric truncation/clamping,
NaN/infinity and empty-needle behavior. Literal matching returns borrowed output;
checks and trim preserve declaration order and failures expose no partial result.
7140 actual ForkLaunch Zod/native comparisons pass with complete diagnostics and
transformed values, including astral/combining Unicode and strict nonstrings.
Full validation21 tests, prior31510 string comparisons and strict all-feature/
all-target Clippy pass. No dependency/lock changes. Logs
/private/tmp/fl-zod-string-rules-{parity,tests,regression,clippy}.log; frozen hashes
/private/tmp/fl-zod-string-rules-source-sha256.json. CI runs the oracle. Regex
flags/state, custom messages, case conversions and lone-surrogate/rawJSON domain
boundaries remain explicit gaps; literal matching does not imply regex parity.


## Generated opt-in TOTP HTTP/MCP checkpoint (2026-10-14)

A fresh source-built CLI passes the full generated Better Auth runtime, including
112 existing real frontend actions and20 actual twoFactorClient calls against
an opt-in generated service over realTCP HTTP. Default discovery remainsfalse;
opt-in HTTP and MCP discovery both reporttrue with equal projections. Both SDKs,
relocation, contracts, encrypted tenant sessions, independent JWT verification,
listener modes, restart, preserved configuration, rename and deletion pass.
The helper uses an owned cookie jar and actual client/plugin; browser enforcement
is not claimed. Seed cleanup now enumerates exactly the existing two tenant
scopes plus the new -totp scope. Role fl_auth_gen_1792017605_12346 and all three
record/audit scopes are verified absent after exit0.

Logs /private/tmp/fl-two-factor-generated-runtime.log,
/private/tmp/fl-two-factor-client-refactor.log and
/private/tmp/fl-oct14-totp-cli-build.log; frozen four-file hashes
/private/tmp/fl-two-factor-generated-source-sha256.json. Build passes with four
existing warnings. This is a source-built generated runtime, not a new immutable
archive/public-registry checkpoint. Existing e2e matrix runs runtime.sh; native
bridge remains in parity CI and helper/seed/runtime paths trigger it.
Read-only elapsed-clock audits confirm current one-day rotation/reset and signed
dont_remember inheritance; no speculative expiry change was made. Broader OTP,
backup disableSession and plugin/options behavior remains open.

Managed platform prerequisite follow-up54cd3375c adds signed creation-context
before local reservation, required current configurationId on create, and fresh
inspect context plus current versus historical profile/config identity.61 tests
and focused strict types pass (/private/tmp/fl-managed-creation-context.log;
hashes /private/tmp/fl-managed-creation-context-source-sha256.json). Native
publication must require current==historical==sealed configuration/generation;
that native integration is in progress, not covered by the platform receipt.


## Managed UUID creation and inspect-only native publication (2026-10-14)

An explicit ManagedBoundPaymentLinkCreationProfile pins current gateway creation
configuration before sealing a local UUID intent. Canonical form bytes and an
opaque native hash bind the authenticated instance/org, tenant, operation/public
UUID, account/mode/APIversion/generation/profile and copied gateway configuration.
Local semantic scope excludes ephemeral issuedAt/expiresAt; clock-advanced exact
duplicates never regain dispatch. Only the confirmed original reservation ticket
can send one signed create RPC. Recovery obtains a separate opaque publication
capability from encrypted actor-owned intent and calls remote inspect only.
Current==historical==sealed remote configuration/generation and exact envelopes
are checked; pending publication also requires unchanged normalized local approved
prices. A single native transaction publishes UUID record/alias/audit/receipt,
with deadline checks after locks/writes. Direct APIs/default routes stay compatible.

Actual companion protocol/HMAC/canonical-byte harness plus restricted native PG
matrix pass (5.80s), including lost acknowledgements, provider/result faults,
cancellation, stale generation/config, local price changes, rollback and replay.
The remote journal in this wire fixture is an owned memory boundary; actual remote
PostgreSQL/barrier/lost-ack proof is separately recorded in the companion61 tests.
This is not a cross-database atomic transaction. Canonical unit, direct creation
regression17.75s, legacy170 and strict base/Stripe all-feature/all-target Clippy pass.
Zero owned fixture roles remain. Logs:
/private/tmp/fl-managed-creation-native-wire-pg-final.log,
/private/tmp/fl-managed-creation-native-unit.log,
/private/tmp/fl-managed-creation-direct-regression.log,
/private/tmp/fl-managed-creation-native-{legacy170,clippy}.log.
Exact11-path hashes /private/tmp/fl-managed-creation-native-source-sha256.json.
The checked-in wire fixture requires explicit FORKLAUNCH_PLATFORM_ROOT, TypeScript
and an owned PostgreSQL URL; its ignored native test is not silently presented as
an automatically executed cross-repository CI test. Normal all-target checks still
compile it. Full production bootstrap, generated default UUID ownership across all
families, managed mutations and unresolved-provider administrative recovery remain
open. No live Stripe/cloud operation was performed.


## Generated TOTP configuration propagation (2026-10-14)

Native IAM scaffolds now document BETTER_AUTH_TWO_FACTOR_ENABLED=false and pass
${IAM_BETTER_AUTH_TWO_FACTOR_ENABLED:-false} through generated Compose. CLI
default/release metadata recognizes the optional false default; user configuration
can enable it explicitly. Generated README/user guide explains password-login
scope, actual twoFactorClient, discovery and independent social/existing sessions.
README configured tracing description now matches the tested generated preset.
31 CLI env-default tests and locked CLI build pass (four existing warnings).
A fresh generated app retains the setting through sync/reinit; actual docker
compose config resolves bothfalse/true under explicit fixture settings without
starting services. Metadata /private/tmp/fl-totp-cli-config-checkpoint.json;
logs /private/tmp/fl-totp-cli-config-{tests,build}.log. Runtime shell assertions
retain default/Compose propagation in the existing e2e flow. This local Compose
proof does not verify AWS deployment or a production secret/configuration setup.


## Backup verification without session issuance (2026-10-14)

Additive BackupVerificationOptions and verify_backup_with_options support actual
backup disableSession:true while preserving existing verify defaults. Pending
success atomically consumes recovery code and challenge authority, returns user
without token, and emits no session/trust/cookie. Authenticated success retains
the current token and emits no replacement cookie. The inert challenge cookie
can remain, matching upstream's separate consumed attempt capability behavior;
subsequent verification cannot issue a session. Failures retain committed attempt
counts; cancellation/commit faults retain transactional rollback.

Permanent actual-source oracle now includes elapsed enrollment/disable rotations,
stale signed dont_remember inheritance and trust clearing; no unneeded lifetime
change was made after characterization. Actual-client/native29 calls, focused6
with onePG ignored, full IAM118 passed/3PG ignored (64.48s), and strict all-feature/
all-target Clippy pass. Logs /private/tmp/fl-two-factor-options-permanent.log and
/private/tmp/fl-backup-options-{native,client,full,clippy}.log; frozen six-path
hashes /private/tmp/fl-backup-options-freeze.json. Existing shared transaction
lifecycle exercises failed-commit rollback/recovery single-use. Restricted PG was
not repeated for this branch; no backend/schema/migration changed. GeneratedTCP
proof above covers the earlier20-call profile; expanded29-call generated proof
is pending the next fresh CLI checkpoint. OTP/custom parameters/other plugins
remain open, and the immutable5a8 archive predates this change.


## Per-constraint Zod custom diagnostics (2026-10-14)

Borrowed ZodNumberCheck/ZodStringCheck and zod_number_checks/zod_string_checks
add optional per-check messages without changing existing enums/functions.
Only diagnostics appended by that constraint are overridden, including Safe's
underlying bounds. Empty messages and quotes/newlines/Unicode/NUL remain exact;
coercion/strict type failures retain defaults. Ordered transformations and atomic
failure behavior are shared with existing evaluators. Arbitrary errorMap callbacks
and schema-level type messages are not implied.
2352 actual-source/native comparisons, validation23 tests, number47090, string31510,
literal rules7140 and strict all-feature/all-target Clippy pass. No dependency or
lock changes. Logs /private/tmp/fl-zod-check-messages-{parity,tests,clippy,number,
string,rules}.log; hashes /private/tmp/fl-zod-check-messages-source-sha256.json.
CI runs the oracle. Full schema/emitter and arbitrary callback parity remain open.


## Verified direct Plan creation and exposed Subscription target gap (2026-10-14)

Plan/Subscription already use local UUIDs with separate externalId; their remaining
ownership/reference gaps differ from Checkout/PaymentLink/Portal provider IDs.
A dedicated opt-in BoundPlanCreateRequest/Profile now verifies an allowlisted
Product under authenticated account/mode, reserves an encrypted intent, issues
one idempotent Plan POST, and verifies exact Plan plus expanded Product readback.
The finite profile supports integer per-unit licensed pricing, one cadence,
uppercase local currency, safe custom alphanumeric/hyphen/underscore IDs and no
caller-generated provider/local IDs or nested creation. Unit amount is a declared
price, not an invoice/charged total. Journal canonical payloads are bounded64KiB.

Separate additive migration8 adds Plan binding kind and Plan intent/audit journal;
migration7 is unchanged. Confirmed commit alone issues dispatch capability. Local
UUID/externalId/Product fields, immutable Plan alias/audit and encrypted historical
receipt publish together with post-lock/write deadline checks. Failed or ambiguous
operations inspect historical state and never automatically repeat POST. Trusted
store primitives are not caller-controlled evidence ingestion. Legacy Plan CRUD
is not newly binding-aware, administrative Plan bindings remain assertions, and
no default generated route silently mints an operation UUID.

Restricted PG matrix12.68s passes Product/Plan/mode/amount/cadence evidence, custom
IDs, wrong scopes, actor/RLS, ciphertext/immutability, concurrency/cancellation,
timeout, rollback, late waits and real lost reservation/final COMMIT acknowledgments.
Official Stripe22.6.2 wire, finite unit, old journal17.79s, legacy170 and strict
base/Stripe all-feature/all-target Clippy pass. Zero fixture roles remain. Logs:
/private/tmp/fl-bound-plan-{pg-final,clippy,unit,sdk,native-target-gap,
legacy-journal,legacy170}.log.13 frozen hashes:
/private/tmp/fl-bound-plan-source-sha256.json. CI runs SDK and restricted PG cases.

A separate actual configured TS/native strict provider-target fixture confirms
both legacy Subscription services send local partyId/productId UUIDs directly as
provider customer/plan, then fail without a local write when a strict provider
rejects them (/private/tmp/fl-billing-subscription-target-gap.log). The older
generated fake accepted these UUIDs; that fixture proves lifecycle/transport,
not usable Stripe references. Trusted customer registration, scoped customer/Plan
resolution and an explicit generated operation/outcome adapter remain work.
The immutable416d checkpoint excludes this subsequently validated Plan source.


## Exact native UUID/email string formats (2026-10-14)

ZodStringFormat adds bounded UUID/email runtime grammars to ordered string rules
and per-check messages. UUID accepts hyphenated hexadecimal groups without a
version/variant restriction. Email follows actual Zod v3 ASCII behavior, including
internal apostrophes and domain trailing hyphens; it rejects percent signs,
Unicode and repeated dots. These are runtime grammars, not DNS/delivery checks
or arbitrary regex/schema-emitter parity.

81036 actual-source/native comparisons, validation24, strict all-feature/all-target
Clippy, literal string7140 and custom-message2352 regressions pass. CI runs the
format oracle. Logs /private/tmp/fl-zod-string-formats-{parity,tests,clippy,rules,
messages}.log; frozen hashes /private/tmp/fl-zod-string-formats-source-sha256.json.
The separate TS email OpenAPI projection correction awaits canonical366 validation.
No broad checklist gate is closed by this slice.


## Two-factor default OTP route behavior (2026-10-14)

The opt-in two-factor profile now exposes upstream send-otp/verify-otp routes
without a delivery hook. Send validates its optional body then returns400
OTP_NOT_CONFIGURED before authentication. Verify validates the code, resolves
an authoritative persisted session or signed durable challenge, and returns400
OTP_HAS_EXPIRED; pending-factor lock checks and lazy lock reset are preserved.
The default-disabled profile still omits two-factor routes. Schema operations
and status mapping are additive. No delivery hook or OTP enrollment is claimed.

Focused native1, MFA HTTP3, actual-source lifecycle61 status checks plus client,
rate, elapsed-cookie and backup-option regressions, and strict all-feature/
all-target Clippy pass. Logs /private/tmp/fl-otp-disabled-{native,http,oracle,
clippy}.log; exact five hashes /private/tmp/fl-otp-disabled-freeze.json. Full
OTP delivery remains a separate unreferenced draft and is excluded here.


## Immutable416d installation and generated MFA29 checkpoint (2026-10-14)

Source416d6e5b4ff5b0f3d9bce0e9fdb49bfcb9af1157 was archived independently
of later Plan, UUID/email, OTP and Customer changes. All2590 regular packaged
files were byte-verified after extraction and after runtime validation. The retained
4,557,317-byte crate has SHA256
c36dea8df3dc97ca61f8c34eeae5c619738785ae7e74723df582a16c277e2092 at
cli/target/parity-artifacts/forklaunch-416d6e5b4-c36dea8d.crate.
Offline locked installation, repackage listing and portable generated runtime
pass, including services/workers, both SDK targets, recursive exports and
HTTP/MCP listeners. That installed CLI also passes generated Better Auth29 actual
twoFactorClient calls over real TCP,112 existing frontend actions, shared/separate/
disabled HTTP/MCP listeners, relocation, restart and rename/delete lifecycle.
Post-run database fixture roles, records and audit counts are all zero.

The package contains three intentional PEM certificate/test-key fixtures with
bytes identical to the previous verified5a8 archive. Source symlinks to unrelated
local IAM PEM paths were not followed or read. No generated output, node_modules
or Cargo target directories were packaged. Metadata and logs are retained in
/private/tmp/fl-oct14-complete-checkpoint.json and
/private/tmp/fl-oct14-complete-{package,install,repackaged-files,runtime,
totp-runtime}.log. Public registry publication remains unverified.

## Pinned private platform/framework CI pair (2026-10-14)

Companion commit12d8cedda adds a private-only manual/callable workflow with
read-only contents access, fixed reviewed platform54cd3375c/frameworkd2597896c
commits and exact source hashes. The verifier rejects modified files and wrong
commits before executing fixtures. Dependencies are npm-lock pinned; an owned
PostgreSQL service supplies the restricted journal/barrier tests. Successful runs
produce a receipt including source pair, workflow commit and run identity.
Local pinned-source positive and tamper/wrong-commit negative checks pass, as do
locked platform61 tests and focused strict types. Logs are
/private/tmp/fl-managed-pair-source-verification.log and
/private/tmp/fl-managed-pair-locked-platform.log. Hosted CI and full platform
application bootstrap have not been run; native protocol/PG proof remains the
separately recorded d2597896c receipt. No source push or deployment is claimed.


## Verified direct Customer association registration (2026-10-14)

The opt-in direct Stripe registration method accepts a server-authorized local
Customer/Party association, verifies current account/mode and an exact fixed
Customer GET, and publishes an immutable scoped association generation with audit.
It rechecks the tenant-owned encrypted local Customer body and xmin under lock;
no decrypted email/name or provider metadata establishes Party ownership. Missing,
deleted, malformed or wrong-mode Customer responses fail closed. The registration
uses provider reads only and exposes no default route or Subscription dispatch.

Additive migration9 enforces tenant RLS, immutable associations, scoped provider/
Party uniqueness and RESTRICT deletion of the local Customer. Exact-generation
CAS revocation preserves audit and rejects ABA. Party existence/authorization is
an explicit trusted-server assertion: there is no cross-service Party FK or IAM
lifecycle guarantee. Future Subscription creation needs fresh Customer/Plan
proof, locked dependencies and a durable claimed-dependency barrier.

Compile5.46s, restricted PostgreSQL10.60s, strict base/Stripe all-feature/all-target
Clippy4.70s, malformed/deleted unit, official SDK fixedGET, migration9 Plan
regression12.77s and legacy170 pass. The PG matrix includes changed encrypted
snapshots, wrong provider evidence, actor/RLS, immutable/CAS/ABA checks, waits,
post-audit expiry rollback and real lost snapshot/final COMMIT acknowledgements.
Ambiguous COMMIT requires scoped historical inspection, never a retry assumption.
Fixture roles are removed. Logs /private/tmp/fl-customer-registration-{compile,pg,
clippy,unit,sdk,plan-regression,legacy170}.log; ten exact hashes
/private/tmp/fl-customer-registration-source-checkpoint.json. CI runs SDK/PG.


## Zod factory email OpenAPI admission correction (2026-10-14)

The actual Zod factory previously exported an explicit pattern admitting percent
signs and rejecting apostrophes, while asserted format=email rejected Zod-valid
domain trailing hyphens. A private definition Symbol now identifies unchanged
factory-email checks through ordinary wrappers. The shim emits the exact ASCII
input pattern without assertive format=email for that marked single-check leaf.
Direct/nested optional/nullable/object/array/union exports recurse correctly;
explicit user OpenAPI overrides, unmarked raw Zod email, modified multicheck
strings and TypeBox retain their existing behavior. Export executes no callbacks.
This corrects declared admission for the supported factory, not arbitrary schemas.

1799 actual parse/export/AJV comparisons pass. Canonical366 HTTP outcomes pass
with zero normalized OpenAPI differences and unchanged snapshots. Strict12-root
TypeScript checks and public inventory tests pass with614 entry points,
1678 declarations and zero unresolved entries (the existing package wildcard
exposes the three helper declarations). Logs /private/tmp/fl-zod-email-{canonical,
inventory,inventory-tests,types-final}.log. CI runs the export oracle.


## Fixed cross-channel AsyncAPI WebSocket replies (2026-10-14)

The checked server-view importer now assigns fixed reply messages to the declared
root reply channel, including a separate absolute WebSocket path. Direction
reverses relative to the initiating operation; missing directions remain forbidden
schemas. Explicit message membership, disjointness, JSON/binary restrictions,
local-reference budgets and fixed-path requirements are preserved. Each path
produces an independent client. Applications open both connections and own routing,
correlation and deadlines; no implicit request/reply connection manager is claimed.
This follows the fixed-channel reply model in the AsyncAPI3.0 specification:
https://www.asyncapi.com/docs/reference/specification/v3.0.0#operationReplyObject
Dynamic reply addresses, upgrade auth/parameters and broader event imports remain open.

All32 AsyncAPI tests pass, including send/receive, inline/referenced, JSON/binary
separate-path cases and foreign-message rejection. Strict all-feature/all-target
Clippy passes after fixing eight existing missing-semicolon test warnings.
Fresh relocated CLI generation/check/watch-once, strict TypeScript/Rust SDK
consumers, official parser, live two-path clients and generated HTTP/MCP shared/
separate/disabled/custom listener lifecycle pass. The first live fixture's cleanup
attempted to close an already-closed client; after fixing cleanup, the affected
live-peer and surface checks pass independently. No product behavior changed for
that harness correction. CI includes the new runtime fixture.
Logs /private/tmp/fl-cross-reply-{cli-build,runtime,live-final,surfaces,unit,
clippy-final}.log; three source hashes /private/tmp/fl-cross-reply-source-sha256.json.


## Exact native Zod binary factory (2026-10-14)

zod_binary returns explicit Vec<u8> from a strict JSON string, matching the actual
Zod factory's Buffer.from(base64) conversion. Both alphabets, skipped invalid
characters, first-padding termination and incomplete groups are preserved.
UTF-16 code units contribute their low byte, including non-ASCII/astral cases;
this is not UTF-8 base64 normalization or canonical-base64 validation. Raw input
and existing native codecs remain unchanged. Lone surrogates and JavaScript
Buffer objects remain outside the input Value string domain. No new dependency.

259118 actual-source/native comparisons, validation25 tests, strict all-feature/
all-target Clippy and UUID/email81036 regressions pass. CI runs the binary oracle.
Logs /private/tmp/fl-zod-binary-{parity,tests,clippy,formats}.log; four source hashes
/private/tmp/fl-zod-binary-source-sha256.json. Generic schema composition, remaining
scalar/emitter behavior and broader media validation gates remain open.


## Trusted fixed delivered OTP lifecycle (2026-10-14)

The optional trusted server-owned OtpDelivery hook now wires domain/HTTP send,
verify and OTP enrollment. Its owned context provides only public user projection
and Zeroizing code, with no Debug/Serde/session token. Default dispatch commits an
encrypted tenant/purpose/session-or-challenge-bound code before awaiting delivery,
without retaining storage locks. Unambiguous session/user framing is hashed.
Six digits use unbiased rejection sampling, validity180s (expiry equality valid),
and five invalid attempts; the next attempt consumes the exhausted code. Resend
replaces the bound code. Success atomically consumes OTP/challenge and rotates or
creates sessions/trust; failed commits preserve reusable proofs. Invalid attempts
commit counters, sharing an existing factor's account lockout. OTP-only users have
no TOTP row/account-wide counter, matching upstream. Sign-in captures advertised
methods in the same transaction as its challenge. Removing the delivery hook
prevents new send/enrollment but preserves verification of stored codes.

Explicit fallible preparation failure yields sanitized empty500; asynchronous
future failure retains200 and the already stored code. Rust panics are outside
the callback contract. Cancellation after commit may leave an expiring code and
never grants authentication; this native cancellation policy is stated separately
from upstream proof. No background dispatcher or automatic delivery channel is
implemented. Default two-factor remains off; no-hook routes preserve the
characterized default outcomes. No-hook OTP enrollment now correctly checks
session then password before reporting unavailable delivery.

Actual upstream5 delivery profiles, actual twoFactorClient/native21 handler-bridge
calls, full IAM120 tests/3ignored, final focusedOTP2, strict all-feature/all-target
and minimal-profile Clippy pass. Final copied-binary restricted PG55.84s proves
atomic consumption, rollback, concurrency, attempts, expiry, hook-off verification,
shared locks and cancellation/no retained lock. Roles/records/audits all0; copied
binary removed with provenance receipt retained. The client bridge is not a
TCP/browser delivery or generated MCP proof; immutable416d generated29 remains a
separate receipt. Existing restricted PG CI now includes this OTP matrix.
Logs /private/tmp/fl-otp-{delivery-oracle,client-native,full-iam,clippy,
final-postgres}.log;16 exact frozen hashes /private/tmp/fl-otp-full-freeze.json;
final binary provenance /private/tmp/fl-otp-final-pg-zxq1rysi/receipt.json.
Custom digits/periods/storage, background dispatch and other plugins remain open.


## Subscription reservation and durable dependency barriers (2026-10-14)

Stage A adds an immutable encrypted Subscription reservation with persistent
claims on exact Customer/Plan local bodies, binding generations and completed
sealed Plan creation provenance. Confirmed commit returns a nonclone reservation;
inspection/replay never reconstruct dispatch authority. There is no provider
POST, dispatcher, completion or release API yet. Claims have no TTL and survive
unknown COMMIT outcomes. The external Party association remains administrative.
Whole envelopes are bounded before cloning/canonicalization: depth32,8192nodes,
conservative escaped256KiB (larger than Plan64KiB because two encrypted snapshots
and sealed provenance are included). Invalid inputs perform no reservation/audit.

Migration10 preinitializes private claim counters for old/new dependencies.
Mutation triggers lock/read the current counter through UPDATE RETURNING,
preventing predating statement snapshots from missing a committed reservation.
Record update/delete, binding revoke/rebind/delete and table truncation are
barred while claimed. Guards use the actual OLD tenant, not caller GUC visibility.
Counter SECURITY DEFINER functions have fixed search_path and no PUBLIC EXECUTE;
private FORCE-RLS policy belongs only the migration owner. Runtime must be a
distinct non-inheriting non-owner; journal/claim mutations and truncation are denied.

Two isolated PostgreSQL matrices pass with non-superuser/non-BYPASSRLS migration
owner and restricted runtime even with ALL TABLES grants. They verify backfill
retains ciphertext, READ COMMITTED/REPEATABLE READ mutation/claim races, one-ticket
concurrent reserve, immutable/private guards, wrong/empty tenant visibility, ABA,
stale Plan provenance, foreign actor/tenant, actually blocked-lock expiry, audit/
freshness rollback and real lost reservation COMMIT acknowledgment. Bounded-input
unit, strict base/Stripe all-feature/all-target Clippy5.60s, Customer PG10.64s and
Plan PG12.73s regressions pass. Fixture databases/roles removed. Logs
/private/tmp/fl-subscription-foundation-{pg-final,unit,clippy-second,
customer-regression,plan-regression}.log; six hashes
/private/tmp/fl-subscription-foundation-source-sha256.json. CI runs both PG tests.
Stage B still needs reviewed finite collection/readback policy and atomic
publication with validated claim release; this foundation does not close billing.


## Trusted OTP digit, validity and attempt options (2026-10-14)

Validated OtpOptions admits 1..32 digits, positive whole-millisecond Duration and
1..100 invalid attempts, preserving defaults6/180s/5. Existing delivery builders
retain defaults; an additive HTTP builder accepts server-owned options. This
explicit native subset does not claim upstream negative/nonfinite/zero coercion.
Expiry persists at send and verification reads current attempt policy; changing
digits never invalidates already issued bounded codes. Resend resets counters.
Constructor and timestamp overflow fail before store work. No storage/schema or
dependency changes. Preparation/delivery/default-off semantics are unchanged.

Actual-source five callback/five option profiles and three no-hook ordering cases
pass, including admitted endpoints1/32digits/100attempts, fractional-minute source
validity, upstreamzero fallback, expiry equality/+1ms and changed attempt policy.
Focused native6, final options2 and strict all-feature/all-target Clippy6.80s pass;
existing TOTP lifecycle also passes. Tests verify invalid constructors, overflow,
resend, changed policies and failed-commit reuse. No-hook enrollment correctly
returns401 without session,400 invalid password, then400 unavailable OTP after a
valid password. Storage backend/full IAM120 are not rerun for this bounded slice;
the separately recorded fixed-profile PG/encryption/full-suite proof remains.
An initial focused run caught a leftover six-digit stored-code assumption; the
final code/tests preserve previously issued lengths across configuration changes.
Logs /private/tmp/fl-otp-options-{native-final,final-source,clippy,actual}.log;
ten frozen hashes /private/tmp/fl-otp-options-freeze.json. Existing CI oracle
imports the new options helper. Custom storage, background and TOTP options remain open.


## Trusted TOTP digit and period options (2026-10-14)

TotpDigits selects six/eight digits; validated TotpOptions selects positive whole-
millisecond periods1ms..24h with unchanged6/30s defaults. SHA1 and fixed±1window
remain. URI/enrollment/verification share current policy, including existing
secrets, and HTTP builders preserve independently configured OTP in both orders.
No request selects parameters; no factor/backend/schema rewrite is introduced.
Milliseconds map to source numeric seconds, then binary64 period*1000/floor
counter arithmetic. At2.007sec, integer division would give a different boundary;
actual vectors cover that case. Admitted clocks are nonnegative bounded JS-Date
integers. The prior negative window at epoch0 is skipped rather than unsigned-
wrapped as upstream; historical/negative clocks remain an explicit boundary.
JavaScript zero/negative/sub-ms options and remaining lifecycle flags are not claimed.

Native two_factor16/1ignored PG, strict all-feature/all-target Clippy7.48s and
minimal-profile Clippy2.39s pass. Actual-source six option/lifecycle profiles and
96 SHA1/counter vectors pass alongside existing61-status/client7/rate/cookie/
backup regressions. A new seeded test initially used20 bytes against the existing
32-byte persisted-secret constraint; only its fixture was corrected. Backend/PG
proof is not rerun/relabelled for this configuration-only change. Logs
/private/tmp/fl-totp-options-{native-final,clippy,minimal-clippy,actual-full}.log;
13 hashes /private/tmp/fl-totp-options-freeze.json. Existing CI oracle imports
new helper/vectors. Disable, skip-verification, issuer and passwordless flags remain open.


## Bounded reusable Zod JSON schema composition (2026-10-14)

ZodJsonSchema composes exact scalar operations with Unknown, optional, nullable,
static defaults, arrays and ordered strip-mode objects. Missing values remain
distinct from null; field/index diagnostics and transformed output are atomic.
Integer-index keys follow ECMAScript order. Private dirty/aborted status is retained
for future unions. This is a programmatic evaluator, not a Zod/JSON Schema
interpreter, arbitrary callback engine or new live route-mode adapter.

Admission bounds schema/input/default depth64,100000 nodes/checks and aggregate
logical16MiB bytes. Evaluation debits repeated checks and32MiB conservative byte
work before transformations/defaults/Unknown clones, messages and diagnostic paths;
returned output is admitted too. Review caught product-sized repeated checks and
large repeated defaults/diagnostics; regression cases now reject them boundedly.
Logical byte bounds exclude allocator overhead. Duplicate/prototype-sensitive
schema fields and nonfinite numeric inputs/defaults anywhere are explicit typed
Unsupported boundaries, never disguised validation failures. Finite Unknown
number clones normalize to IEEE values (including unsafe integer rounding and
preserved negative-zero bits), with raw input untouched. This does not claim
native exact-integer accessors or literal JSON numeric spelling equality.

3486 actual-source/native comparisons plus2 explicit nonfinite boundaries,
validation35 tests both normally and with unified arbitrary_precision, and strict
all-feature/all-target Clippy pass. Scalar6498/array117963/custom-message2352
regressions also passed before final resource/numeric additions. Tests cover
wrapper order, default/null/absence, unknown stripping, field/index ordering,
atomicity, depth/width/check/byte/default/message bounds, precision and raw input.
Logs /private/tmp/fl-zod-json-{final-parity,resource-tests,arbitrary-tests,
final-clippy}.log; six exact hashes /private/tmp/fl-zod-json-source-sha256.json.
CI runs the evaluator oracle. Union/record/strict/callbacks, live composition
integration and broader schema/emitter gates remain open.


## Immutable d835 archive, portable runtime and generated MFA29 (2026-10-14)

Source d835988ca7e2a636e53b08861c6a8ca5977cce7a plus the recorded CLI lock
installs offline/locked from its Git-free Cargo archive. All2622 regular packaged
files are byte-verified after extraction and after runtime. Retained archive:
cli/target/parity-artifacts/forklaunch-d835988ca-f54de962.crate
SHA256 f54de9621571a5a24e73badc6ce8872cfbc7689b603dec10cb37d76d3f20881a, 4599368 bytes.
CLI lock SHA256 59f7db815712b329afa16484d18c79c127862170af830cb2cd3abc572a36afa7.
The three intentional certificate/test-key PEM fixtures match previous verified
archive bytes; unrelated absolute source IAM PEM symlinks were not followed/read.
No node_modules, target or generated output was packaged.

Portable relocated services, all four worker kinds, modules/capabilities, native/
CLI SDK generation, recursive clients and HTTP/MCP shared/separate/disabled/custom
surfaces pass. Packaged IAM120 tests and typed WebSocket consumers also pass.
The installed CLI separately passes generated realTCP twoFactorClient29 calls,
112 existing frontend actions, HTTP/MCP discovery/listeners, encrypted tenant
sessions, JWT, SDKs, relocation, restart and rename/delete. All generated fixture
roles/records/audits are zero afterward. This includes fixed delivered-OTP library
source, Customer registration and Subscription reservations, but predates the
later OTP/TOTP option, generic composition and Subscription publication drafts.
The generated preset still selects no OTP delivery channel; no browser/mail
transport proof is implied. Public registry/cloud validation remain separate.
Metadata /private/tmp/fl-oct14-next-checkpoint.json; logs
/private/tmp/fl-oct14-next-{package,install,repackaged-files,runtime,
totp-runtime}.log. Source/hash receipts remain after owned workspace cleanup.


## Direct verified Subscription creation and trusted atomic publication (2026-10-14)

The opt-in direct-only profile resolves a verified Customer association and a
locally unchanged Plan with allowlisted completed sealed creation provenance.
Fresh pinned account/mode, Customer and expanded Plan GETs precede confirmed
reservation; one idempotent Subscription POST and fixed exact-ID GET then verify
customer/mode, complete single item, legacy Plan/Price/product/amount/cadence/
quantity, stable item/start_date and supported active/incomplete states. Explicit
AutomaticDefaultIncomplete selects charge_automatically/default_incomplete;
unsupported discounts/taxes/trials/delegation/evidence remain unresolved.
Customer defaults can affect invoices. Active is service state, never payment or
charged-total proof; an active/unpaid invoice case tests this boundary. External
Party lifecycle/authorization remains the trusted server caller's assertion.
Default routes/legacy creation and managed transport are unchanged.

Additive migration11 and an explicit separate trusted publisher connection
atomically write local UUID/ownership binding/audit/encrypted receipt/completed
journal and release only that operation's four claims. Runtime cannot inherit
publisher/migration owner, update completed state, delete claims or execute
publication/decrement helpers even with broad table grants. Publisher is trusted
authority, not a defense against stolen publisher credentials; SQL does not
pretend to authenticate application ciphertext as Stripe evidence. Constructor
checks configured host/port/database, observed server/database tuple and immutable
migration UUID, explicitly not cryptographic cloned-cluster equivalence. Native
owns the transaction and rechecks freshness after awaited SQL before COMMIT.
Unknown POST/expiry/COMMIT outcomes retain inspectable pending/completed state,
never automatic retry/refund/delete. Historical inspection grants no new dispatch.

All-feature compile, isolated basePG3, native StripePG1, unit17, strict base/Stripe
all-feature/all-target Clippy, official SDK22.6.2 wire, Plan PG12.76s and Customer
PG10.60s regressions pass. Matrices verify distinct non-BYPASS roles, runtime
forgery/helper denial, claims during POST/readback, duplicate/concurrent calls,
malformed/mismatched evidence, timeout/no retry, failed audit/release/freshness
rollback and actual lost final COMMIT acknowledgment. Fixture databases/roles0.
Only lint corrections were needed. Logs /private/tmp/fl-subscription-stage-b-
{compile,base-pg,stripe-pg,unit,clippy-pass,prerequisite-regression,cleanup}.log
and /private/tmp/fl-bound-subscription-sdk.log;14 exact hashes
/private/tmp/fl-bound-subscription-stage-b-source.json. CI runs SDK and PG.
General interrupted-effect recovery, mutation ownership, generated operation
adapters and managed Subscription creation remain open.


## Working-tree CLI regression (2026-10-14)

All1121 CLI unit tests pass with locked dependencies and incremental compilation
disabled (/private/tmp/fl-oct14-cli-full-tests.log). This is the current source
build, including coherent working source drafts during active integration; it
is not relabelled an immutable archive proof. The separate d835 archive receipt
retains exact source/lock/package hashes. Four existing CLI compiler warnings
remain visible. Completed owned d835 source/archive/generated roots were removed
after committed receipts and process/open-file/checksum guards; retained crate,
logs/metadata and build caches remain. Missing old owned worktree registration
fl-rust-full-checkpoint-xk4xb_by was pruned; unrelated worktrees/watchers retained.


## Trusted two-factor lifecycle options (2026-10-14)

Trusted typed lifecycle options now support disabling TOTP, skip-verification
enrollment, independent enrollment/URI issuers, and passwordless policy with
optional URI/backup overrides. Defaults remain unchanged. Passwordless operation
requires an authoritative session and an absent/null/empty credential password;
existing passwords remain mandatory. A transactional snapshot check rejects a
credential created or changed during mutation. This native atomic guard is not
a claim about upstream race handling. Skip enrollment atomically verifies the
factor, enables the user and rotates only the current session. Disabled TOTP
sign-in retains an empty-method challenge usable by backup codes; it grants no
session/JWT. Existing trusted-device renewal remains independent. Request bodies
cannot override trusted configuration.

All19 native two-factor tests, restricted PostgreSQL lifecycle47.30s, final
focused3 and HTTP helper1, actual Better Auth lifecycle oracle, all-feature/all-
target and minimal-library strict Clippy pass. PostgreSQL owned roles/records/
audits are zero. Exact17 hashes: /private/tmp/fl-lifecycle-options-freeze.json;
logs /private/tmp/fl-lifecycle-options-{native,postgres,final-focused,final-helper,
actual-final,clippy-final,minimal-clippy}.log. Evidence is working-tree source
with the frozen checked-transform dependency, not an immutable package receipt.
CI tracks the imported lifecycle oracle helper and existing PostgreSQL test.
Custom storage, backup generators, trust TTL and broader plugins remain open.


## Live checked JSON composition bridge (2026-10-14)

Axum now offers additive insert_json_transform_checked with explicit validation
versus operational failure. Validation retains Error/Warning/None behavior;
operational/resource failure always stops with the existing safe correlated500.
The old callback API retains its behavior. Registration still requires the
JSON parser and declared400 rejection; parser/auth ordering and original bytes
remain intact. This is a trusted callback API, not a schema interpreter.

A shared native example contract, both SDK generators and actual configured
TypeScript routes exercise nested coercion/defaults/optional arrays/strip and
atomic failure. All143 Axum tests,69 paired HTTP cases,30 registered MCP calls,
three operational-mode cases, six calls per generated TypeScript/Rust SDK,
strict all-feature/all-target Axum/SDK Clippy, and201 existing transform pairs
pass. Callback-once/auth/handler counts and raw bytes are asserted. Public
error envelopes prove rejection/body diagnostics; raw object key order is
separate. MCP transport admission is permissive to reach downstream HTTP
validation; this does not prove direct MCP schema interpreter equivalence.
Logs /private/tmp/fl-composition-bridge-{tests,live,clippy}.log and
/private/tmp/fl-composition-legacy-{transform,fields}.log. CI runs the fixture
with pinned canonical dependencies. Broader unions/records/strict/schema gates
remain open. Evidence is working source, not an immutable package proof.


## GET-only interrupted Subscription recovery (2026-10-14)

Opaque same-actor recovery snapshots restore publication authority only, with
no dispatch/key getter/Clone/Debug/Serde. Snapshot checks immutable encrypted
intent/hash, exact current encrypted Customer/Plan dependencies, binder/provenance
and all four durable claims. Trusted adapter verifies account/mode, exact creation
Event request-key linkage, static rendering API version, event/object timestamps
within reservation -5s/+5min and nonfuture bounds, fresh Customer/expanded Plan
and exact Subscription. Incomplete-to-active is admitted; active-to-incomplete
is rejected. Key linkage is not process-origin proof; the narrow clock/version
policy is not a provider guarantee. Null/automatic keys and ambiguous evidence
require manual review. Provider traffic is GET-only; no retry/refund/renewal.

Separate-role publisher shares the existing locked atomic publication, encrypted
receipt/audit/claim release and post-write freshness check. Historical recovery
evidence is bound to operation/public UUID/payload hash plus event hash. Lost
COMMIT acknowledgements remain inspectable without dispatch. Runtime journal
SELECT FOR UPDATE initially failed because runtime deliberately has no UPDATE
policy; exact dependency locks already dominate publisher completion, so immutable
journal read/reread now uses those locks without widening permissions.

Final isolated base PG3, native Stripe creation/recovery matrix3.37s, unit17,
strict all-feature/all-target Clippy and official Stripe22.6.2 Event GET5 pass.
Matrices cover timestamp/version/key/scope/terms rejection, status regression,
concurrent single publication, audit/release/freshness rollback, lost COMMIT ack
and ciphertext-bound receipt tampering. Owned databases/roles0. Final11 hashes:
/private/tmp/fl-subscription-recovery-source.json; logs
/private/tmp/fl-subscription-recovery-{base-pg-second,stripe-pg-final,
clippy-second,unit,sdk,cleanup}.log. CI runs new SDK fixture and existing expanded
PG matrices. No migration/default route change. Broader interruption, mapper,
managed Subscription and mutation gates remain open.


## Trusted challenge and device lifetime options (2026-10-14)

TwoFactorLifetimeOptions supports independently configured whole-second challenge
and trusted-device lifetimes0..365days, preserving600s/30day defaults. New and
renewed records/cookies use current trusted configuration; existing durable expiry
remains unchanged. Builders preserve lifecycle/TOTP/OTP policies. The actual
clock-controlled oracle exposed challenge expiry equality: native backup/TOTP/OTP
now accept at exact challenge expiry, reject after; trust still requires strict
future expiry. Zero duration follows those distinct boundaries. Session and
delivered-code lifetimes remain separate. No schema/storage change.

Actual nine-case clock oracle and integrated two-factor fixture pass; focused
native4, full two-factor22/1PGignored, restricted PostgreSQL1 in49.56s, strict
all-feature/all-target and minimal-library Clippy pass. HTTP tests compare issued
challenge/proof-trust/credential-renewal cookie Max-Age with stored expiry; generic
memory/PG cases verify equality/+1ms, current-policy renewal, one-use challenges
and failed renewal insertion rollback. Owned roles/records/audits0. Final15 hashes:
/private/tmp/fl-lifetime-options-freeze.json; logs
/private/tmp/fl-lifetime-options-{native,regression,postgres,clippy,minimal-clippy,
cleanup,actual,actual-full}.log. CI tracks the imported helper and expanded PG
fixture. Working-source proof includes the frozen union dependency draft; no
immutable package/browser-cookie claim. Fractional/negative/nonfinite option
values and broader storage/generator/lockout/plugin policies remain open.


## Ordered reusable Zod unions and raw diagnostic paths (2026-10-14)

ZodJsonSchema::Union evaluates in declared order: first valid wins, otherwise
first dirty diagnostics win; only all-aborted candidates produce invalid_union.
Empty/single unions follow actual source runtime. A private raw issue tree
preserves nested union paths until public formatting. The repository adapter
flattens exactly one union level, prefixes VariantN and duplicates outer paths;
nested branch unions retain Invalid input. No recursive flattening is invented.

Additive zod_json_at_path applies a trusted path before formatting without
cloning input; successful output is unchanged and zod_json delegates with empty
path. Admission bounds64 segments and aggregate schema/input/path16MiB before
evaluation; branch work, retained raw paths and final duplicated prefixes debit
the existing100,000 node/32MiB logical work budgets. Tests include multiplicative
branches, repeated1MiB paths and individually admissible9MiB default+8MiB path
rejected together even with absent input. No allocator-overhead guarantee.

All41 validation tests with arbitrary_precision,13,398 actual/native observations
plus2 explicit nonfinite boundaries,99 paired HTTP cases,45 registered MCP calls,
3 safe operational modes and6 calls per generated SDK pass. Strict validation/
Axum all-feature/all-target Clippy with arbitrary_precision also passes. Logs
/private/tmp/fl-zod-union-{tests,parity,live,clippy}.log; frozen8 hashes
/private/tmp/fl-zod-union-source.json. Existing CI runs expanded fixtures.
Success projection and ordered public diagnostics are tested; direct MCP schema
interpretation/raw JSON key order remain separate. Record/strict own-key-order,
literals/enums and broader arbitrary schemas/callbacks remain open.


## Embedded MCP example packaging correction (2026-10-14)

The current working-tree recursive package-source check found the tracked MCP
body_admission example absent from CLI templates. Added the missing examples
directory link using the same relative source-link layout as other crates.
The generic package-source check now passes all1210 embedded Rust source/template
paths (/private/tmp/fl-oct14-current-package-sources-fixed.log); initial failure
retained at /private/tmp/fl-oct14-current-package-sources.log. This is a current
working-tree source-list proof including coherent active drafts, not another
immutable archive installation/startup receipt. Earlier d835 receipts retain
their exact source/package hashes and scope.


## Scoped adoption after interrupted captured-payment confirmation (2026-10-14)

Direct-only operator recovery authenticates account/mode and exact expanded
PaymentIntent→Charge→balance evidence, then publishes against sealed local
payment/noncancelled order xmin/administrative binding generation. Full amount,
canonical currency, captured/paid, zero refunded amount and no dispute are
required. The explicit non-FX balance subset admits pending/available bookkeeping
with exact charge source, not settled/available funds or protection against later
refunds/disputes. Metadata is consistency only; ownership comes from the trusted
binding. No provider financial write, order transition or reference clearing.

Additive migration11 retains tenant SELECT/INSERT-only receipts; payment→order→
binding locks match provisioning/cancellation. Exact CAS, retained cancellation
history, audit+receipt+payment atomicity, post-await/preCOMMIT freshness and
unknown COMMIT inspection preserve authority. Recorded evidence expiry after
COMMIT is a typed committed outcome. Connect/managed adoption and ambiguous
provider histories remain open. Default routes and legacy reconciliation retain
their behavior.

Initial compile, restricted PG matrix5.60s, native Stripe unit4, strict base/Stripe
all-feature/all-target Clippy6.79s, existing direct/Connect cancellation PG2 in
5.47s and actual configured TS confirmation+official Stripe22.6.2 GET wire pass.
The TS confirmation oracle executes repository service/mapper code over an owned
transaction boundary, not real ORM database confirmation; its header was corrected
and rerun by parent (/private/tmp/fl-scoped-capture-ts-sdk-root.log). Native PG
covers order-xmin/CAS/bindingABA, tenant/identity, cancellation history, deadline
waits, concurrent single publication, audit/receipt-collision rollback and actual
lost COMMIT ack. Existing records_check denies raw body-ID tampering; initial
fixture setup failure retained and strengthened to prove denial. Owned roles/
records0. Final10 hashes /private/tmp/fl-scoped-capture-source.json; logs
/private/tmp/fl-scoped-capture-{initial-check,ts-sdk,pg-second,unit,clippy,
cancellation-regression,cleanup}.log. CI adds SDK/source oracle and isolated PG.


## Trusted pending-account lockout options (2026-10-14)

AccountLockoutOptions provides enabled flag, threshold1..100 and whole-ms
duration0..365days, preserving true/10/900s defaults. Pending TOTP/backup/OTP
share the factor account budget; authenticated recovery and OTP-only accounts
without a factor record bypass it. Independent challenge/OTP attempt limits
remain enforced. Disabled policy neither checks nor mutates historical failure
fields; re-enabling honors stored deadlines, and current threshold/duration
applies to the next failure. Exact expiry clears the lock. Lazy expired-lock
reset now persists before deliberate exhausted-challenge deletion errors.

Deadline arithmetic matches source binary64 seconds-to-ms addition and Date
TimeClip on bounded JS-Date/Chrono clocks, including1001ms at epoch0/negative
clocks; unrepresentable deadlines fail before counter mutation. Corrupt counters
and increment overflow fail closed. Review caught redundant disabled-success
factor writes: TOTP/OTP now issue zero factor UPDATEs, while backup consumption
still requires one and rolls back atomically if denied. Actual adapter-denial
oracle and generic memory/PG fail-on-replace wrapper prove that boundary.

Actual integrated fixture/routes and16 direct helper clock cases pass. Full
two-factor25/1ignored53.26s precedes only the final write guard; final affected
focused3, restrictedPG1/47.23s and strict all-feature/all-target/minimal Clippy
pass on the frozen final source. Owned roles/records/audits0. Final15 hashes:
/private/tmp/fl-lockout-options-freeze.json; final logs
/private/tmp/fl-lockout-options-readonly-{native,postgres,clippy,minimal}.log,
/private/tmp/fl-lockout-options-final-actual.log and cleanup.log. CI tracks
imported helper and expanded PG fixture. Working-source proof includes frozen
literal/resource dependency draft, not an immutable archive receipt. Broader
backup generation/storage/OTP codecs/plugins remain open.


## Strict JSON literals, ordered enum lowering and counted rescans (2026-10-14)

ZodJsonLiteral supports borrowed String/Number(f64)/Boolean with strict scalar
equality; signed zeros compare equal while successful output preserves input
sign. NaN never matches; nonfinite expected values render null in actual literal
diagnostics while input remains the finite JSON subset. String diagnostics match
JSON.stringify escaping; finite number messages use ryu_js spelling. Ordered
enums lower to literal Union branches in source Object.values order, including
integer-key ordering, duplicates and empty/single members. Null/object/symbol
literals and arbitrary JavaScript reflection are outside the typed factory.
Schema/evaluation byte budgets cover worst-case diagnostic escaping before
allocation.

Resource review found repeated scalar-union metric scans could traverse large
zero-string-byte containers without spending evaluation nodes. The evaluator
now debits base schema/check work before metrics and each revisited descendant
before inspection, sharing the existing100,000-node budget. Unknown cloning
retains bounded admission without the redundant prior descendant debit. The
adversarial repeated-scan test and oversized literal diagnostic tests pass.
This extends the earlier union resource proof rather than silently relabeling it.

All45 validation tests with arbitrary_precision,118,132 exact source/native
observations+2 nonfinite boundaries,117 paired HTTP cases,54 registered MCP calls,
3 operational-mode cases,6 calls per generated TypeScript/Rust SDK and strict
validation/Axum all-feature/all-target Clippy pass. Fixture covers Latin1/control/
Unicode2028/2029 escaping, unsafe integers, nonfinite configs, signed-zero raw
inputs, ordered enum diagnostics and nested union/path contexts. Final resource
regression reran after test-only lint correction; production unchanged. Logs
/private/tmp/fl-zod-literal-{tests,parity,live,clippy,final-resource}.log; final9
hashes /private/tmp/fl-zod-literal-source.json. Existing CI runs expanded fixtures.
Record/strict own-key order, broader schema/callback and wire key-order gates
remain open. No immutable archive claim.


## Explicit Connected captured-payment adoption (2026-10-14)

An explicit Connected counterpart reuses the same immutable payment/order/binding
CAS and atomic audit/receipt, with base migration11 unchanged. Provider context
binds client generation, copied account/fee configuration, mode and five-second
observation; every account/balance/expanded intent GET carries the exact sealed
Stripe-Account. Direct still rejects Connect before I/O. Connected direct-charge
profile requires explicit null critical delegation/source-transfer fields and
rejects destination/transfer ambiguity or missing fields. Coherent historical
PI/Charge application fees0..gross are admitted independently of current fee_bps;
gross capture evidence does not claim net payout/fee ownership/settlement. Same
non-FX balance bookkeeping and refund/dispute guards apply.

Actual TS/official Stripe22.6.2 direct/Connected three-GET header/expansion checks,
restricted PG twins2/5.62s, native unit5 and strict Stripe all-feature/all-target
Clippy5.15s pass. Twins cover zero-I/O wrong profile/account, missing/transfer/fee
rejections, historicalfee5/currentfee0/net97/gross105 acceptance, client/config/
account generations, CAS/expiry/concurrent single publication/lost COMMIT ack.
Owned roles/records0. Final4 hashes /private/tmp/fl-connected-capture-source.json;
logs /private/tmp/fl-connected-capture-{ts-sdk,pg,unit,clippy,cleanup}.log. Existing
CI fixture/PG entries run both profiles. No new schema/default route/provider
write/order transition/reference clearing. Broader ambiguous/managed/PayPal
reconciliation remains open.


## Immutable dda838 archive and portable HTTP/MCP checkpoint (2026-10-14)

Exact source dda838bd156db9bd692cabcc411868af27ba5d24 plus recorded CLI lock
59f7db815712b329afa16484d18c79c127862170af830cb2cd3abc572a36afa7 packages2666 regular
files into4,683,521bytes. Retained archive
cli/target/parity-artifacts/forklaunch-dda838bd1-503464bf.crate, SHA256
503464bf966940d6466edbbb9c8a1b3176a8d2516d7a56525aa715068dd1dca8.
Git-free extraction, offline locked debug installation, repackaged dotfile list
and all2666 extracted file bytes pass before/after runtime. Three intentional
public/test PEM fixtures match the earlier416d archive byte-for-byte; two
baseline absolute source key symlinks were not followed/read. No node_modules,
target or output files enter the package.

Relocated generated services, all four worker families, modules, infrastructure
feature checks, recursive/shared SDKs, CLI/native generation from contract and
OpenAPI, strict TypeScript/WebSocket clients and Rust consumer checks pass. Live
generic-service HTTP/MCP passes initialization/discovery/health/Origin checks,
enabled/disabled/custom endpoints, shared/separate listeners, explicit/ephemeral
ports, fallback ports, atomic startup rejection and graceful shutdown. All five
capability callbacks pass signature/freshness/replay/redelivery checks. Packaged
IAM passes137 tests/3PGignored, including live auth HTTP/MCP. Separate restricted
PG receipts remain distinct. This does not relabel prior generated actual-client
MFA29 as rerun here; that exact earlier source receipt remains recorded.

This archive includes committed lifecycle/lifetime/lockout, ordered unions and
literals, checked live composition, direct Subscription publication/recovery,
direct/Connected captured-payment adoption and MCP example packaging correction.
It excludes active backup generation, ordered strict objects and scoped PayPal
read drafts. Generated app Cargo lock SHA256
045644641c3220b165d7c48db55c5f07527ddbb1bf419063b2d7ee61fde5f118.
Logs /private/tmp/fl-oct14-options-{package,install,repackaged-files,runtime}.log;
metadata /private/tmp/fl-oct14-options-checkpoint.json. Four existing CLI warnings
remain visible. No hosted deployment/public registry validation is inferred.


Owned dda838 checkpoint cleanup (2026-10-14): after the committed immutable
receipt, final retained-archive checksum, process and open-file guards, removed
/private/tmp/fl-rust-options-checkpoint-2_20rhaa (32,361,353 logical bytes) and
/private/tmp/fl-rust-options-archive-4segrwr2 (173,576,426 logical bytes). Retained
crate/logs/metadata and shared build caches remain. Cleanup is recorded in
/private/tmp/fl-oct14-options-checkpoint.json; unrelated worktrees/watchers and
the owned reusable PostgreSQL fixture were retained.


## Trusted backup-code generation options and hook (2026-10-14)

BackupCodeOptions supports amount0..100 and random-character length1..127,
preserving10/10 defaults and dash insertion after min(5,length). Optional trusted
synchronous BackupCodeGenerator matches the installed typed () => string[] hook,
overrides amount/length and returns bounded≤100 strings/≤128UTF8bytes each. Empty
list, duplicate/empty/Unicode codes remain supported. Verification uses stored
proofs under fixed supported bounds, independently of current generation policy;
a successful duplicate match consumes every equal code. No HTTP body configures
server policy.

Generation follows authoritative session/password/factor checks inside the
mutation transaction. Callback errors/invalid output become empty500 with no
callback/error details or cookies; factor/session changes roll back. Sync hooks
must not reenter their store; Rust panic/JavaScript throw equivalence is not
claimed. Stored factors retain native record encryption; arbitrary field-storage
codecs remain separate. Bounded custom generators can deliberately return weak
proofs, matching source semantics rather than silently rewriting them.

Actual full two-factor oracle including six numeric profiles, ordering/failure,
custom override and exact128byte output cases passes. Focused native4/18.43s,
affected two-factor26/1ignored56.18s, restrictedPG1/47.16s, strict all-feature/
all-target Clippy9.67s and minimal Clippy2.36s pass. Memory/PG prove concurrent
consumption, policy-change old-code validity, generator failure and failed session
INSERT after enrollment factor/user mutation rollback. Owned roles/records/audits0.
Final16 hashes /private/tmp/fl-backup-options-freeze.json; logs
/private/tmp/fl-backup-options-{native,regression,postgres,clippy,minimal,actual,
full-actual,cleanup}.log. CI tracks helper and expanded PG. Working-source proof
includes frozen ordered-validator dependency draft, not immutable archive dda838.


## Immutable ordered input and strict JSON objects (2026-10-14)

OrderedJsonDocument binds immutable Value to a local object-order tree, preserving
ECMAScript integer-index ordering, duplicate first position/last value and string
insertion order without repeated ancestor-path sidecars. Parsing checks raw16MiB
before decode and depth64/node100,000/logical value+metadata16MiB during building;
logical metadata includes32 units/node and retained keys/strings, not allocator
overhead. Vendored literal-key discrimination separates real private-marker
object keys from serde numeric protocol without vendor/global feature changes.

StrictObject emits declared-field errors then one dirty unknown-key message in
actual own-key order. Ordinary Value can infer numeric keys plus at most one
non-index unknown key. Missing order remains a deferred raw issue: later valid
union branches win, and hidden nested-union children do not demand unobservable
order; only retained formatted diagnostics raise ObjectOrderUnavailable. Trusted
default Values gain no invented order. No source mutation/stale-sidecar access.

Arbitrary_precision testing found bare -0 reaches a custom serde visitor as
integer0. A bounded lexical numeric-token queue now preserves finite IEEE/-0
across both modes while serde alone validates full syntax; strings/escapes/keys
are skipped and successful decode must consume exactly the queue. Token count/
metadata budgets apply before push/build. Malformed/nonfinite/lone-surrogate
inputs remain outside this declared parser domain.

All52 validation tests pass in default and arbitrary_precision;2018 exact actual/
native ordered pairs pass per mode (4036 total), alongside118,132 prior composition
pairs+2 nonfinite boundaries. Live138 HTTP/63 registered MCP/3 operational cases
and6 calls per generated SDK pass; SDK calls exercise strict success/failure.
Strict validation/Axum all-feature/all-target Clippy passes. The live bridge uses
a declared no-reviver boundary and ordinary unambiguous synthetic{} for framed
empty JSON; it never reparses stale transformed bytes. Outputs remain semantic
JSON values, not serialized-key-order equivalence; record transforms remain open.
Final10 hashes /private/tmp/fl-zod-ordered-source.json; logs
/private/tmp/fl-zod-ordered-{tests,arbitrary-tests,parity,live,legacy,clippy}.log.
CI tracks/runs new two-mode oracle plus expanded live fixture. Working-source
proof, not immutable dda838 archive scope.


## Scoped PayPal ORDER-read foundation (2026-10-14)

Explicit Sandbox/Live constructor pins documented HTTPS origins; separately
named trusted proxy constructor accepts a server environment attestation. Legacy
constructors remain unprofiled. Immutable client generation/environment and5s
resource context bind authenticated OAuth→exact order GET to a trusted merchant
and requested order. One CAPTURE unit must carry exact13char primary-schema
merchant ID, local reference, positive exact cents and canonical currency.
Missing/email-only payee is denied. OAuth app_id is not merchant ownership and
Orders does not attest livemode. No partner headers or financial API actions.
Redacted observation contains no token/payer data; COMPLETED is not capture,
settlement, refund/dispute or monetary authority.

Opt-in postgres inspect_bound_order rechecks the complete tenant administrative
binding/current relation after I/O and deadline after final DB wait. SELECT-only
execution passes; no local writes/migration/receipt/default route introduced.
Captured/cancelled PayPal repair remains a separate gate.

Final context units2, restrictedPG1/5.46s,143 primary-schema/native merchant+wire
cases, strict all-feature/all-target and minimal-library Clippy, and existing
actual TS22client+19wrapper regression pass. PG covers tenant/ref/generationABA,
SELECT-only and5.1s final DB wait. Primary schema fragment retains full-source
SHA256 in fixture; official account_id/payee definitions were used, not a guessed
OAuth ownership protocol. Owned roles/records0. Final11 hashes
/private/tmp/fl-paypal-scoped-read-source.json; logs
/private/tmp/fl-paypal-scoped-{unit-final,pg,schema-wire,clippy-final,
minimal-clippy,legacy,cleanup}.log. Cargo.lock adds only3 PayPal dev-dependency
edges, no version updates. README is embedded with matching relative template
link. CI runs schema/wire and isolated PG. Working-source proof, not dda838
archive scope.


## Trusted plain/hashed OTP field storage (2026-10-14)

OtpStorage::{Plain,Hashed} is trusted service/AuthHttp policy, preserving Plain
default and existing delivery/digit/attempt/lifetime/lockout configuration. Hashed
uses source SHA256(UTF8)→base64url without padding through existing ring/base64;
no dependency change. Whole-record tenant encryption remains independent.
Persisted plain digits or canonical43byte hash are admitted independently of
CURRENT comparison policy. Plain↔hashed switches yield INVALID_CODE+attempt,
not corrupt-store500; restoring policy verifies the same remaining proof. Old
digit widths1/32 remain valid after generation policy changes. Request bodies
cannot choose storeOTP.

Actual full fixture plus11 storage/error/callback groups pass. Focused native3,
affected two-factor29/1ignored56.24s, restrictedPG1/47.49s, strict all-feature/
all-target Clippy9.05s and minimal2.33s pass. Tests cover canonical hash vectors,
Unicode/empty401 versus nonstring400/no attempt, policy restoration, expiry
equality/+1ms, commit rollback, one-use race and pending account lockout. Owned
roles/records/audits0. Final13 hashes /private/tmp/fl-otp-storage-freeze.json;
logs /private/tmp/fl-otp-storage-{native,regression,postgres,clippy,minimal,
full-actual,actual,cleanup}.log. CI tracks imported helper and expanded PG.
Encrypted/custom OTP and backup field codecs in the oracle are audit-only
follow-ons, not implemented by these two modes. Actual custom callbacks are
awaited Promise<string>; OTP callback failure after destructive consume differs
from backup codec failure/CAS preservation and requires separate transaction
design. Working-source proof, not immutable/generated-service receipt.

### 2026-10-14: ordered record validation (working source)

Record key/value composition now preserves own-key enumeration, key-before-value
issues, value validation after key failure, transformed collision last-wins and
source __proto__ omission. Ordered documents retain necessary insertion evidence;
ordinary Values defer unavailable diagnostic order until selected union errors,
and reject ambiguous successful collisions. No serialized property-order claim.

Frozen8 hashes: /private/tmp/fl-zod-record-source.json. Native58 tests pass in
each default/arbitrary_precision mode; actual Zod7844 exact comparisons plus4
explicit nonfinite unsupported boundaries pass per mode. Ordered regression2018
per mode and legacy118132+2 pass. Live159 HTTP/72 MCP/3 safe500 and6 calls through
each SDK pass; strict validation/Axum all-feature/all-target Clippy passes. Logs:
/private/tmp/fl-zod-record-{tests,default-tests,parity,ordered-regression,legacy,live,clippy}.log.
CI includes the record oracle. Broader schema gate remains open;660/682 unchanged.

### 2026-10-14: scoped PayPal VOIDED operator repair (working source)

Authenticated exact order/payee/environment evidence with explicitly empty
captures/authorizations/refunds histories now permits atomic Failed bookkeeping
for a nonsucceeded payment on an already-cancelled local order. Exact payment,
order xmin and full binding generation are rechecked under ordered locks; prior
capture audit denies repair. Migration12 adds a scope-unique tenant receipt,
preventing repeated publication even after binding ABA or an already-Failed
snapshot. Freshness is checked after waits/writes and before COMMIT; expired
post-COMMIT evidence has a recorded outcome, and lost acknowledgement requires
receipt inspection. References/orders stay unchanged, canceled_at stays None,
and provider I/O remains OAuth authentication plus GET, without financial writes.

Frozen12 hashes: /private/tmp/fl-paypal-void-source.json. Unit3, restricted PG
matrix1/5.62s, bound-read regression1/5.47s, primary-schema/native wire152, actual
TS legacy22 client+19 wrapper comparisons and strict base/PayPal all-feature/
all-target plus minimal-lib Clippy pass. Matrix covers7 CAS/deadline races,
concurrent one-publication, rollback and true lost-COMMIT acknowledgement.
Owned roles/records0. Logs /private/tmp/fl-paypal-void-{unit,pg,read-regression,
schema-wire,legacy,clippy,minimal-clippy,cleanup}.log. CI runs restricted PG and
tracks the history schema fixture. No live provider/deployment or immutable
package claim; broader reconciliation remains open and660/682 unchanged.

### 2026-10-14: built-in encrypted OTP field storage (working source)

Trusted Encrypted mode uses SHA256(secret UTF8)→XChaCha20Poly1305, fresh24-byte
nonce and lowercase nonce/ciphertext/tag hex. Independent validated field keys
support bounded version envelopes and explicit legacy fallback; only zeroized
derived keys are retained and key configuration has no Debug/Serde. Existing
cookie keys/row encryption remain independent. Unknown/retired keys, malformed
encoding and bad tags commit destructive OTP consumption before empty500, with
no session/trust or new attempt increment; prior authorized expired-lock cleanup
commits too. Commit failure rolls back. Plain/hash→encrypted errors consume;
encrypted→plain/hash wrong proofs retain/increment. Existing128-byte supplied
proof bound explicitly excludes upstream144-character ciphertext-as-Plain-proof
behavior. Decimal-prefix version parsing and replacement UTF8 decoding match the
admitted actual-source subset. Async custom/backup field codecs remain open.

Frozen19 hashes: /private/tmp/fl-otp-encrypted-freeze.json. Five deterministic
Node/Rust vectors match encrypt/decrypt bytes. Actual full fixture plus8 encrypted
groups pass; focused4, affected35/1ignored56.14s, restrictedPG1/47.80s, strict
all-feature/all-target Clippy7.50s and minimal3.36s pass. Cleanup0/0/0. Logs:
/private/tmp/fl-otp-encrypted-{native,regression,postgres,clippy,minimal,actual,
full-actual,cleanup}.log. Pinned RustCrypto chacha20poly1305 0.11.0 and additive
lock edges are included; owned fake-secret JSON vectors are source assets. No
new generated-TCP or immutable archive claim;660/682 stays unchanged.

### 2026-10-14: checked AsyncAPI correlation locations (working source)

RuntimeExpression parses bounded message.header/message.payload locations and
RFC6901 URI-fragment pointers (4096bytes/64segments), decodes percentUTF8 once,
validates pointer escapes and resolves borrowed values without coercion. Missing
source/path differs from present null. MessageContract exports optional checked
correlationId metadata; differing correlation declarations fail message conflict
checks. The metadata importer retains expressions keyed by channel-message
pointers alongside identical SocketContractArtifact values. JSON payload locations
are supported; application-header and binary-payload correlations fail checked
import rather than silently disappear. Ordinary import validates the same profile
and returns contracts only; applications own matching/deadlines/connections.

The new referenced-correlation plus trait test initially failed: merging before
resolving created forbidden $ref+location siblings. Official @asyncapi/parser
confirmed reference targets are resolved before target precedence. Correlation
references are now resolved before trait merge within existing bounded local
reference/output budgets. Native38 all-feature tests, official parser import/
export/trait precedence, existing official trait/envelope regression, minimal22
tests and strict all-feature/all-target Clippy5.13s pass. No dependency/vendor/
lock changes. Frozen5 source hashes: /private/tmp/fl-asyncapi-correlation-source.json;
logs /private/tmp/fl-asyncapi-correlation{,-legacy,-minimal,-clippy}.log. CI adds
the parser fixture and encryptedOTP helper path filters. No new live HTTP/MCP
or generated/immutable receipt is implied by this metadata change. Dynamic reply
addresses, broader event imports and overall AsyncAPI gate remain open;660/682
unchanged. Primary grammar: https://www.asyncapi.com/docs/reference/specification/v3.0.0#runtimeExpression.

### 2026-10-14: JSON-domain absence factories (working source)

Null/Undefined/Void/Never are distinct reusable schemas with actual missing/null
diagnostics and object omission. Nullish lowers to ordered[Void,Null,Undefined];
existing first-valid unions, path-prefix behavior and record key-before-value
validation remain intact. No Any alias, undefined-valued JSON properties or sparse
array claim. Frozen7 hashes: /private/tmp/fl-zod-absence-source.json.

Native62 and actual4212 exact comparisons pass EACH default/arbitrary_precision
mode; Record7844+4 explicit nonfinite boundaries pass each mode. Live195HTTP/
90registeredMCP/3safe500/6TS SDK/6Rust SDK pass. Strict validation+Axum all-feature/
all-target Clippy passes. Initial example too-many-lines lint was corrected by
extracting unchanged string_schema; final oracle/Clippy reran. Logs:
/private/tmp/fl-zod-absence-{default-tests,tests,parity,record-regression,live,clippy}.log.
CI adds actual absence oracle. Broader schema gates remain open;660/682 unchanged.

### 2026-10-14: direct bound BillingPortal creation (working source)

Opt-in finite direct profile verifies Account/Balance, registered Customer and
explicit active Portal configuration, then reserves immutable encrypted intent/
idempotency key before a single sessions POST. Exact response scope/customer/
configuration/return URL is checked; separate trusted publisher atomically writes
local public UUID, durable record, provider binding, encrypted receipt and audits.
Additive billing12/13 retain distinct Portal journals/claims/completion/release
while reusing private Customer dependency counters. Ordinary runtime cannot
complete/release; Subscription publication cannot release Portal claims. Current
Customer body+xmin/full binding are checked with fresh context after locks and
before COMMIT. Uncertain reserve/dispatch/final COMMIT remains inspectable, with
no automatic POST retry or regenerated dispatch ticket. Historical completion
returns IDs, not an expired bearer URL. Configuration can change remotely after
GET; local5-minute expiry is not Stripe lifetime proof. Caller authorizes Party/
Customer access; a session is not payment/entitlement evidence. Generic routes,
provider-ID behavior and managed transport remain unchanged/separate.

Frozen13 hashes: /private/tmp/fl-bound-portal-source-final.json. Shape unit1, final
restrictedPG matrix1/6.33s, existing Subscription barrier/publication/race3, strict
base+Stripe all-feature/all-target Clippy4.65s, actual legacy170 operations,
official Stripe22.6.2 wire and configured creation186 comparisons pass. PG covers
separate roles/cross-journal isolation,13 malformed post-effect states, lock
deadline, audit rollback and true dropped reserve/final COMMIT acknowledgement.
Only lint/style changes followed initial PG; finalPG reran exact source. Cleanup
zero owned roles/databases. Logs /private/tmp/fl-bound-portal-{unit,pg-final,
subscription-regression,clippy,legacy170,sdk,configured-uuid,cleanup}.log.
CI tracks SDK fixture and restricted PG. Broader mapper/recovery gate remains
open;660/682 unchanged. No live provider/generated/immutable receipt claimed.

### 2026-10-14: CLI/package and added-dependency review

Current coherent working sources pass1121 CLI unit tests
(/private/tmp/fl-oct14-current-cli-tests.log) and1243 embedded Rust/template paths
(/private/tmp/fl-oct14-latest-package-sources.log). Includes Portal drafts through
its final13 and encryptedOTP vectors; this is package-list proof, not a newly
installed immutable archive. Workflow YAML parses. New encryptedOTP lock edges
were checked with cargo-audit against local1310-entry RustSec data
(/private/tmp/fl-oct14-encrypted-dependency-audit.json). Audit exits1 for the
existing exactly reviewed rsa0.9.10 RUSTSEC-2023-0071; no new package finding or
warning was reported. No fresh-database/zero-advisory claim.

### 2026-10-14: object passthrough/catchall policies (working source)

ObjectWithPolicy adds Strip/Strict/Passthrough and optional catchall, preserving
existing Object/StrictObject behavior via shared evaluator. Active catchall
overrides unknown policy; unwrapped Never alone is the source sentinel. Declared
diagnostics precede own-key-ordered extras, all values run after earlier aborts,
and __proto__ validates then omits output. Unavailable extra diagnostic order is
deferred to selected errors so a later valid union branch can win. Bounded
traversal/cloning shares existing schema/input/work budgets. No prototype
inheritance/callback/nonJSON or serialized-order claim.

Frozen9 hashes: /private/tmp/fl-zod-object-policy-source.json. Native67 per
default/arbitrary_precision mode; actual8910 exact+3 explicit nonfinite boundaries
per mode; live219HTTP/102MCP/3safe500/6TS SDK+6Rust SDK; strict validation+Axum
all-feature/all-target Clippy all pass. Absence4212 and Record7844+4 regressions
pass each feature mode. Production behavior needed no correction; oracle fixed
nested -0 serialization and separated Infinity coercion from finite-domain
controls. Logs /private/tmp/fl-zod-object-policy-{tests,default-tests,parity,live,
clippy,absence,record}.log. CI adds object-policy oracle. Broader schema gates
remain open;660/682 unchanged.

### 2026-10-14: immutable8403 packaged runtime and HTTP/MCP checkpoint

Exact committed source 8403cae5b708b48ee86b865ab8046bbbb5534eae plus ignored CLI lock SHA256
59f7db815712b329afa16484d18c79c127862170af830cb2cd3abc572a36afa7 was extracted without Git metadata; uncommitted
asyncOTP/Checkout/URI drafts are excluded. Archive contains2711 regular
files/4754002 bytes, SHA256 dfc9a7699bc441f6c098d7015f8a7b7660433ab373c93580aa4c6547a024ae94.
Retained artifact: /Users/rohinbhargava/forklaunch-js-rust-port/cli/target/parity-artifacts/forklaunch-8403cae5b-dfc9a769.crate.
All2711 extracted regular-file bytes match the retained archive before AND
after runtime; retained checksum is unchanged. Three intentional public/test PEM
fixtures match immutable416d bytes; two baseline absolute TS private/public PEM
symlinks were preserved without following or reading their targets. No node_modules,
target or generated output is packaged.

Git-free package, locked/offline debug CLI install22.77s and repackaged dotfile list
pass. Relocated app generates/checks services, four worker families and modules,
provider/infra features, recursive/shared CLI/native contract/OpenAPI SDKs, strict
TypeScript/WebSocket consumers and Rust consumers. Live generated accounts passes
five HTTP/MCP listener configurations: startup, initialization/discovery/health,
Origin denial, enabled/disabled/custom endpoints, shared/separate/explicit/ephemeral/
fallback ports, atomic startup rejection and graceful shutdown. All five capability
callbacks pass actual TS signatures, freshness/forgery/replay and redelivery checks.
Packaged IAM147 PASS/3 deliberately ignored PG tests87.69s includes library
auth HTTP/MCP tests; standalone restricted PG receipts remain separate. No newly
rerun generated actual-client MFA29 claim. Generated Cargo.lock SHA256
f344050c72a9f0b34f7c223630dbb4d33034a060994247aa042b9796f9f7d188.

This checkpoint includes encryptedOTP/backup generation/plain-hashed policy,
ordered record/absence/object policies, AsyncAPI correlations, scoped PayPal reads/
VOIDED repair and direct Portal creation. Their specialized provider fixtures are
separate source receipts; archive startup does not create live provider objects.
Logs /private/tmp/fl-oct14-parity-{driver,package,install,repackaged-files,runtime}.log;
metadata /private/tmp/fl-oct14-parity-checkpoint.json. After receipt commit d6bb0c973, process/open-file and retained-checksum guards
passed. Owned roots were removed: source32,775,326 logicalbytes and
install/generated174,919,218 logicalbytes. Archive/logs stay retained; cleanup
metadata is /private/tmp/fl-oct14-parity-checkpoint.json and log
/private/tmp/fl-oct14-parity-cleanup.log.
Full parity remains660/682,22open.

### 2026-10-14: direct bound Checkout creation (working source)

Explicit finite hosted subscription/card profile verifies account/mode, registered
Customer, completed bound Plan/Product provenance and exact authenticated Price.
Stripe Price retrieval accepts Plan ID; no synthesized mapping or arbitrary raw
options/delegation are used. Four durable Customer/Plan body+binding claims precede
one POST and finite exact line-items readback. Separate Checkout journal/publisher
(migrations14/15) atomically commits local UUID, encrypted URI/provider fields/
receipt, binding and audits; only its own claims release. Runtime, Subscription
and Portal completion cannot release Checkout claims. Fresh exact dependencies
and context are checked after locks and before COMMIT. All uncertain effects
remain inspectable without automatic POST retry; historical results return IDs,
not bearer URLs. Actual provider expires_at replaces legacy local5-minute TTL.
Open/unpaid exact totals are admitted (including zero-total unpaid); zero-total
no_payment_required/completed states remain unresolved. Session totals are neither
later invoice/payment nor entitlement evidence. Remote reads do not lock remote
state. Generic/generated route wiring and managed transport remain separate.

Frozen14 hashes: /private/tmp/fl-bound-checkout-source-final.json. Stripe library14
tests, restrictedPG matrix1/6.52s, existing PortalPG1 +Subscription3 barrier/race
regressions, SDK22.6.2 wire, actual configured creation186 and legacy170 operations,
strict base+Stripe all-feature/all-target Clippy pass. Matrix covers0POST invalid
preflight, malformed post-effect/no-local-write, foreignactor/tenant/runtime
publication denial, OLD-tenant/crossjournal/concurrentpending claims, audit
rollback,5.1s deadline and true dropped reserve/finalCOMMIT acknowledgements.
Initial testTRUNCATE hitFK denial beforetrigger; CASCADE variant now proves
immutable trigger. Production changes after initial source were lint-only.
Cleanup zero owned roles/databases. Logs /private/tmp/fl-bound-checkout-{unit,
pg-final,portal-regression,subscription-regression,sdk,configured-uuid,legacy170,
clippy,cleanup}.log. CI tracks SDK+restrictedPG. No live provider/generated/
immutable Checkout claim; overall660/682 unchanged.

### 2026-10-14: completed-checkpoint cache cleanup

After successful immutable8403 runtime, a read-only dependency-file audit found
783 rebuildable cache files tied exclusively to three already-removed committed
checkpoint roots. Rechecked absent source roots, exact dependency prefixes/
file sizes and no-open-file guards before removing1,441,799,112 logicalbytes.
Current caches/binaries, retained archives, source/log receipts, unrelated SDK
watcher and PostgreSQL fixture remain. Evidence:
/private/tmp/fl-old-checkpoint-artifact-audit.json. Physical APFS reclaim is not
equated to logical file size.

### 2026-10-14: trusted asynchronous OTP field codecs (working source)

Owned Send hash/encrypt/decrypt futures receive bounded zeroizing1024-byte secret
wrappers without Debug/Serde/session/token context; empty/Unicode supported, colon
encoded outputs excluded explicitly. HTTP proof128-byte bound stays. Cooperative
Tokio timeout1ms..30s plus durable private revision/phase/lease prevents stale
publication; no DBtransaction spans callback, detached task or forced interruption
is claimed. Encoding failure restores an older proof only under exact owner CAS;
verification consumes availability before callback, including failure/cancellation.
Resend/consumed replacement cannot be overwritten or resurrected from absence.
Final session/trust/counter/state writes commit together with fresh authority/
lockout/clock checks. Initial clocks sample after writer acquisition, guard every
reservation write/COMMIT and again before callback; final guard follows awaited
writes immediately before COMMIT. Actual COMMIT timing itself is not guaranteed.
Default HTTP builtin modes retain exactly one clock sample/exact expiry; custom
HTTP uses fresh configured clock. Domain supplied-now methods advance monotonic
elapsed time; explicit clock overloads are additive. Default/no-codec instances
recognize custom states and fail closed. Bounded cleanup256/cursor and lazy expired
encoding recovery are explicit, without a background scheduling promise. Deploy
envelope-aware readers before enabling; native internal envelopes are not upstream
DB interchange. Native rejects stale-authority races that actual upstream permits.

Frozen20 final hashes: /private/tmp/fl-otp-async-freeze.json (manifest SHA256
0f71d6496dd973c15d6a7eb11c1a56ba1c79b7950d38da6385bbd636700aab16).
Actual10 async observations+fullfixture pass. Final domain focused14/0.03s,
restrictedPG1/48.28s, allfeature/alltargetClippy8.76s andminimal2.52s pass.
Initial/final lock/write/COMMIT delay tests prove zero callback entry after expiry;
wrong/success rollback, oneuse, cancellation/restart, resend+consumeABA, reentry,
currentlockout/signout/disable/nohook and cooperative timeout are covered. Earlier
affected45/1ignored56.52s predates final clock guards; final focused/PG prove those
corrections. Subsequent HTTP-only clock correction passes four-profile1/0.03s plus
strict allfeatureClippy8.96s/minimal; domain/storage unchanged. Cleanup0/0/0.
Logs /private/tmp/fl-otp-async-{native,regression,postgres,clippy,minimal,actual,
full-actual,cleanup}.log and fl-otp-async-http-clock{,-clippy,-minimal}.log.
Only existing Tokio time feature added; no lock/migration change. CI tracks helper.
No new generated/immutable async claim; backup field codecs remain open,660/682
unchanged.


### Pinned WHATWG URI validation (2026-10-14)

Optional zod-uri uses exact ada-url3.2.4 with embedded C++/header hashes matching
Node24.2.0. No normalization: returned strings retain original whitespace and
encoding. Default dependency graph excludes Ada/C++; feature builds need C++20.
205676 exact admission comparisons,162 ordered/message and54 composition checks
pass;69 allfeature and67 minimal validation tests pass. Live255HTTP/120MCP,
3safe500 and6 calls per SDK pass, as do strict allfeature/minimal Clippy and81036
UUID/email regressions. Frozen11 manifest /private/tmp/fl-zod-uri-source.json;
logs /private/tmp/fl-zod-uri-{parity,live,clippy,minimal-clippy,formats-regression}.log.
CI explicitly pins Node24.2 for these checks then restores its existing Node22.
This fixes the97 observed parser admission discrepancies; it does not claim
exhaustive URL equivalence, cross-platform/MSRV proof or corrected RFC URI schema
emission. Broader schema gate stays open; checklist remains660/682 (96.8%),22open.


### Declared billing request date profile (2026-10-14)

Opt-in BILLING_REQUEST_DATE_PROFILE=utc-iso-and-epoch projects request-only
string|number admission and normalizes declared startDate/endDate/expiresAt before
typed deserialization through shared HTTP/MCP dispatch. UTC years0000..9999,
fraction truncation, explicit unsupported400 for locale/offsetless/outside-profile
inputs; null/invalid reject. Missing/default/required semantics and opaque payloads
remain normal route behavior. Responses unchanged; defaultlegacy unchanged.
320 actual configured16field comparisons pass:112normalized,96actual-invalid,
96explicitunsupported,16omissions. Three Axum unit tests, live restrictedPG
base+Stripe HTTP/MCP with6 generated TypeScriptSDK calls,66route/28schema default
regression and strict allfeature/alltarget AxumClippy pass. Invalid/unsupported
requests cause zero provider calls. Cleanup0ownedroles/DBs.
Frozen9 /private/tmp/fl-billing-request-dates-source-final.json; logs
/private/tmp/fl-billing-request-dates-{check,oracle,unit,live,contracts,clippy,cleanup}.log.
CI adds oracle and live ignored integration execution using jobTSC/PG environment.
Lock adds only3 billing-Axum dependency edges (validation and two dev SDK crates).
No full JSDate/TimeClip-extreme or configured billing identity-wiring claim.
Checklist remains660/682 (96.8%),22open.


### Billing date generated artifact follow-through (2026-10-14)

Generated billing library export now explicitly projects package-owned routes with
the same public date projection and environment parser used by runtime startup.
contract_registry() remains legacy; contract_registry_with_request_dates is explicit.
Custom application routes are preserved. Actual rendered base/Stripe library
compilation/export passes legacy and opt-in contract+OpenAPI metadata, unchanged
responses and invalid-profile zero-output checks. Temporary source tree removed.
Proof /private/tmp/fl-billing-date-artifacts.log; permanent fixture
framework-rs/parity/rust_billing_date_artifacts.mjs added to CI. No new live SDK claim beyond
the preceding runtime-registry receipt; checklist remains660/682.


### Built-in backup field storage (2026-10-14)

BackupCodeStorage LegacyArray/PlainJson/Encrypted and independent opaque versioned
keys preserve direct service compatibility. First AuthHttp MFA opt-in defaults
encrypted, matching configured plugin; subsequent builders preserve explicit
selection. Unmarked arrays read in all profiles, migrate only on successful
consume/regenerate. Marked v1 strings decode under current policy; unknown marker
fails closed. No read-time rewrite. JSON77101/hex154282/prefix48 bounds are
separate from unchanged OTP32/192. Four deterministic Node/Rust vectors cover
Unicode/empty/duplicates/maximum escapes; retained-key reencoding and malformed,
wrong-policy/key/tag failures preserve original factor with transactional attempts,
sessions/trust, concurrent one-use and commit rollback checks. PlainJson still
requires tenant-record encryption. Native marker/envelope is not upstream DB
interchange. No async custom backup claim.
Frozen17 /private/tmp/fl-backup-storage-freeze.json SHA256
c5dbc5bbd91c4267ef06d4fd95ce58afcd80dc2216268ef572fbea9bde7e7d5d.
Finalfocused5/0.08s, affected54/58.23s(1PGignored), restrictedPG1/48.83s,
allfeature/alltargetClippy9.20s/minimal5.38s and actual9groups pass; cleanup0/0/0.
Logs /private/tmp/fl-backup-storage-{final,regression,postgres,clippy,minimal,actual,cleanup}.log.
No manifests/locks/migrations changed; CI tracks actual fixture. No new immutable
or generated MFA receipt. Broader plugin gate remains open;660/682 unchanged.


### Rustls advisory refresh and patch (2026-10-14)

A fresh RustSec scan (1329 advisories, commit
edb58f5974b83eb8302332821cb6d389777aa351) found newly published
RUSTSEC-2026-0346/0347/0348/0349 against rustls0.23.45 in all three lockfiles.
Targeted update to0.23.46 removes all four without adding scanner exceptions;
tracked lock deltas contain only Rustls version/checksum changes. The ignored
CLI lock was likewise updated (SHA256
d66f3120b6fab6af185dfa4e22c614b4bf5324370aede8667c9807e3e5bc5110).
Fresh all-lockfile gate passes with only existing reviewed findings visible;
report /private/tmp/fl-oct14-patched-security.json and corresponding.log.
CLI1121 tests pass; allfeature IAM library167 pass/3environment-gatedPGignored
in88.02s. Generated billing date base/Stripe template compilation/artifact checks
also pass against the patched graph. Separate restrictedPG receipts remain
applicable, not relabeled as this run. No hosted CI/publication claim.


### Provenance-aware direct Checkout read (2026-10-14)

New get_bound_created_checkout_session preserves local Customer UUIDs and verifies
the provider relation through completed actor-owned creation provenance. Mandatory
journal evidence, exact current Customer ciphertext+xmin/binding generation and
scope/API, Checkout record/binding and immutable receipt are observed together,
then rechecked after provider GET with final context deadline after awaited DB work.
Provenance selection does not depend on mutable customerId and never falls back.
Direct-only new API requires journal SELECT/audit permissions; existing bound
getter is unchanged including legacy direct/managed restricted-grant behavior.
No financial/mutation authority or generated default route wiring added.
Frozen6 /private/tmp/fl-checkout-provenance-read-final.json SHA256
53475c1c8d8f8530e57eea5b7aa6bfc0f618bd3931ae836f6031592a469c717b.
RestrictedPG11.67s passes create→read, wrong provider customer, actor/tenant/API,
mutable-ID downgrade, concurrent Customer xmin and5.1s final audit-lock freshness.
Legacy direct/managedPG5.42s and strict base/Stripe allfeature/alltargetClippy pass.
Logs /private/tmp/fl-checkout-provenance-{read-pg-final,legacy-pg,read-clippy}.log;
cleanup0ownedroles/DBs. Checklist remains660/682,22open.


### New example directory package coverage (2026-10-14)

Source-package guard identified missing embedded examples directories for billing
Axum requestdate and contract URI schema examples. Added relative directory
symlinks using the existing template convention; all1270 current embedded paths
pass cli/tests/rust_package_sources.sh. Proof
/private/tmp/fl-oct14-current-package-sources.log. This is working-source coverage,
not a new immutable archive/runtime receipt. Refetched origin/main remains
fully merged (zero missing commits through e36cfd29c).


### WHATWG URI schema and consumer consistency (2026-10-14)

Private factory provenance marks only the single-url-check repository factory.
It exports reserved forklaunch-whatwg-url without old RFC regex. Raw Zod URL,
TypeBox, multiple-check profiles and explicit user format/pattern overrides keep
existing behavior; schema extraction never invokes callbacks. UniversalSdk initial,
injected and isolated response AJV engines and generated TS sockets register new
URL admission. Native contract central compiler uses optional pinned Ada directly
(no validation dependency cycle), forwarded through HTTP/MCP/WS/Axum/SDK crates.
Feature-off rejects the reserved format only in compiled schema locations,
including references, not literal const/default data. Ordinary+multipart and RFC
retention are tested. No output normalization or exhaustive URL claim.
CLI socket generator supports explicit FORKLAUNCH_SDK_ZOD_URI unset/0/1;1 enables
compiler feature, invalid values fail. Generated Rust socket consumer opts into
its zod-uri feature. Default compiler returns explicit error and no artifact output.
Frozen21 /private/tmp/fl-uri-semantics-source.json. Schema203261 native/AJV plus90
shape/control and injected-engine checks pass; strict generatedTS socket and Rust
schema/codec6 cases pass. Native Axum143/contract27/HTTP73/WS18/MCP26 pass;
final contract ordinary/multipart2+2 and8crate strictClippy+finalminimal pass.
Canonical366 passes with0OpenAPI diffs; CLI websocket7 pass; configured12roots/
614exports strict types pass (actual compilerTS7.0.2, not claimed5.9).
Logs /private/tmp/fl-uri-semantics-{parity,sdk,regression,final-tests,clippy,
final-clippy,canonical,cli,types}.log. Runtime formatter churn removed; final SDK
proof rerun. Only framework lock1line contract→Ada; rustls0.23.46 preserved.
CI runs schema/SDK oracles under pinnedNode24.2 and explicitfeatureoff rejection.
Broad schema gate remains open;660/682 (96.8%),22open unchanged.


### Immutable9e374 package checkpoint prepared (2026-10-14)

Explicit source9e3749cae8b9d9cde907bd65742f8fd6fed23d71 excludes all async-backup
and bound-Checkout route drafts. Git-free package/offline locked debug installation,
repackaged dotfile list,2745 regular extracted-byte comparisons and intentional
PEM fixture checks pass. Archive4818854bytes SHA256
d5ce67809545afc4f88ce16dd37f6a319643e34d470444c71d1a44acbfd2c2da
retained at cli/target/parity-artifacts/forklaunch-9e3749cae-d5ce6780.crate.
CLI lock SHA256d66f3120b6fab6af185dfa4e22c614b4bf5324370aede8667c9807e3e5bc5110.
Metadata /private/tmp/fl-oct14-uri-checkpoint.json; driver/package/install/
repackaged-files logs use /private/tmp/fl-oct14-uri- prefix. Portable runtime
HTTP/MCP/SDK verification awaits native queue; no new runtime claim yet. Owned
source/install roots retained only until that verification and cleanup.


### Conjunctive numeric and array bound emission (2026-10-14)

TypeScript exporter previously overwrote stronger numeric min/max with later
weaker checks, and exact array length with weaker retained min/max. Finite bound
reduction now retains strongest minimum/maximum and exclusive ties, preserving
contradictions and OAS3.0 boolean versus3.1 numeric exclusive syntax. Array exact,
retained min/max conjunction applies to nonnegative safe-integer lengths. Explicit
OpenAPI overrides remain final; ordered source runtime diagnostics unchanged.
Nonfinite registrations, invalid lengths, multipleOf/string rules unchanged.
Frozen2 /private/tmp/fl-zod-bounds-source.json;153972 actual runtime/AJV comparisons
across9108 exports, six shapes, both dialects, repeated/tied/contradictory bounds,
overrides and zero schema-export callbacks pass. Strict12roots/614exports types
pass (actualTS7.0.2). Logs /private/tmp/fl-zod-bounds-{openapi,types}.log.
CI adds permanent fixture with explicit draft04 dependency fallback. No Rust,
Cargo/lock/dependency changes or new canonical-native receipt. Remaining tuple,
UTF16length and record-key export gaps keep broad schema gate open;660/682.


### Additional completed-checkpoint cache cleanup (2026-10-14)

Removed two unused cached test executables tied to previously removed committed
checkpoint source roots, reclaiming183215264 logical bytes. Source roots and their
old dependency files remain absent; exact file stats and no open-file handles were
checked before deletion. Current compilation caches, source worktrees and retained
proof archives preserved. Receipt
/private/tmp/fl-oct14-old-checkpoint-executable-cleanup.json. No physical APFS-space
claim.


### Trusted asynchronous backup field codecs (2026-10-14)

Owned Send encrypt/decrypt futures receive only zeroized bounded backup text
(JSON77101/ciphertext154330), never session/user/token context. Total whole-ms
lease1..30000 covers both callbacks and persistence; cooperative timeout cannot
interrupt blocking code/panic/COMMIT. Durable encrypted Verification v1 per-user
phase/owner/snapshot lease and backupCodesRevision reject concurrent/ABA final
writes without holding transactions across callbacks. Format2 custom fields fail
closed without codec; deploy upgraded readers/writers before enabling. Builtin
format1 and unmarked-array compatibility remain. Builtin selection clears callback.
Enroll/regen recheck password/authority/factor and cannot overwrite newer consumes.
Decode failure preserves factor/attempt; invalid proof records normal failure;
reencode failure preserves codes and consumes only exact unchanged pending challenge.
Final writes session/trust/counters/code removal atomically under current authority,
lockout and deadline sampled after waits and immediately before COMMIT. No absolute
COMMIT-completion deadline guarantee. Native rejects upstream stale signout/expiry
and unsafe regeneration races deliberately; not exact concurrency scheduling parity.
Cancellation/restart uses bounded256-row cleanup and lazy recovery; expired
Reencoding consumes only exact unchanged challenge. Root-found lazy-recovery
rollback fixed by committing recovery before stale result; both generic OAuth
expiry cleanup paths handle recognized leases correctly. Changed challenges and
missing/new owner never resurrect or authorize callbacks. Default HTTP builtin
clock behavior preserved; custom paths use fresh trusted clock.
Frozen21 /private/tmp/fl-backup-async-freeze.json SHA256
5ace0e877822ef11c51a8f7ac19acc425651e4797f588c88fb43949afa4addfb.
Focused11/0.08s, fullIAM178/3ignored89.14s, restrictedPG1/49.50s, strict
allfeature/alltarget+minimalClippy and actual12 observations pass; cleanup0/0/0.
Logs /private/tmp/fl-backup-async-{native,full,pg,clippy,minimal-clippy,actual}.log.
No IAM manifests/locks/migrations changed. Tested against main2813f0e28 plus frozen
IAM and coherent optional Checkout graph; no new immutable/generated claim.
Server-only viewBackupCodes remains separate;660/682 (96.8%),22open unchanged.


### Overlapping union and finite enum emission (2026-10-14)

Ordinary ZodUnion now exports anyOf, avoiding oneOf rejection when alternatives
overlap. Homogeneous finite scalar literal enums deduplicate; mixed finite literals
no longer infer the final member type after a mismatch. Empty union emits not:{}
rather than invalidoneOf:[]. Explicit composition annotation merge behavior and
discriminated unions retain existing behavior. Runtime diagnostics/callbacks unchanged.
Frozen2 /private/tmp/fl-zod-union-export-source.json;10674 actual Zod/AJV pairs over
592 OAS3.0/3.1 exports,12root/614export strict types(TS7.0.2) and153972/9108 bounds
regressions pass. Logs /private/tmp/fl-zod-union-{openapi,openapi-types,bounds-regression}.log.
SDK model/schema traversal supports both composition keywords; no new native/SDK
runtime receipt or precise generated never-model claim. Nonfinite enum/OAS3.0
nullable-union/Never-leaf emitter gaps explicitly remain. No Rust/Cargo changes;
broad schema gate stays open;660/682 (96.8%),22open.


### Explicit bound Checkout HTTP/MCP and both SDKs (2026-10-14)

BoundCheckoutV1 adds three operation-named contracts: finite UUID Customer/Plan/
quantity create under caller-visible operationUUID, provenance-aware publicUUID
get and actor-owned IDs-only inspect. Configuration fixes server scope/redirects/
completed Plan provenance; explicit separate trusted publisher is re-derived for
authenticated service actor without changing pool/role boundary. Startup denies
base/managed/backup fallback and partial/unknown settings. HMAC authenticates the
calling service, not individual end-user Party authorization; application owns it.
Unknown/ambiguous outcomes return typed202 without dispatch retry; completed
historical reads/inspection return IDs, never revive Checkout URLs. Runtime,
artifacts and both SDKs share contracts; Date projection APIs preserved.
New routes declare sanitized400/404/409/500/502/503, fixing live-discovered
undeclared400→framework500 only within these new contracts. Default66routes/
28schemas unchanged, no automatic alias provision or other-family parity claim.
Frozen17 /private/tmp/fl-bound-checkout-routes-final.json SHA256
ea4e581a0ce7063b38d49d07d0abf9cdd6934e600ac08b0e3baa6aa795d72402.
Live restrictedPG/HTTP/MCP plus strict actualTS and compiledRust SDK3calls each
pass (4.79s fixture); actor isolation/concurrent pending/idempotency/claims covered.
Unit4, rendered library14 env/artifact cases, actual base/Stripe binary Cargo/main
template compilation, Date artifact regression, default66/28 and strict billing
base/Stripe/Axum allfeature/alltarget+Axumminimal Clippy pass. Cleanup0roles/DBs.
Logs /private/tmp/fl-bound-checkout-{routes-live,routes-unit,artifacts,
date-artifacts-regression,default-contracts,routes-clippy,default-clippy}.log.
Only blueprintlockedge billingAxum→sdkRust(dev), no external upgrade; rustls0.23.46
retained. CI uses no-run compilation then direct test executable to release Cargo
build lock before nested Rust SDK compilation. Wrapper syntax checked; standalone
wrapper run queued after immutable runtime. No fresh CLI-generated bound-profile
runtime receipt yet; broad billing gate stays open,660/682 (96.8%),22open.


### Never-leaf and required property emission (2026-10-14)

ZodNever now emits not:{} instead of readOnly:true. Direct Never remains required
in objects; optional/default detection unchanged. Optional Never allows omission
and rejects all supplied JSON values. Explicit not overrides and discriminator
behavior preserved. Frozen3 /private/tmp/fl-zod-never-source.json;224pairs/16exports
across both OASversions, direct/optional/objects/arrays/unions/annotations and zero
root callbacks pass. Prior union10674/592 andbounds153972/9108 regressions plus
strict12root/614export types(TS7.0.2) pass. Logs
/private/tmp/fl-zod-never-{openapi,union,bounds,types}.log. No native/Cargo changes;
exact generated never/uninhabited types, broader object isOptional callback
execution and OAS3.0 nullable unions remain distinct boundaries.660/682 unchanged.


### Immutable9e374 portable runtime verification (2026-10-14)

Prepared9e374 source/archive checkpoint now passes portable relocated services,
all four worker families/modules, provider/infrafeature checks, recursive/shared
CLI/native contract/OpenAPI SDK generation and strict TS/WebSocket/Rust consumers.
Generated accounts HTTP/MCP listener matrix and allfive capability callbacks pass
startup/discovery/authentication/invocation/forgery/staleness/replay/redelivery/
shutdown checks. Packaged IAM167 pass/3 deliberately ignoredPG in82.76s; separate
PG receipts remain separate. No new generated actual-client MFA29 rerun claim.
All2745 regular extracted files match retained archive both before and after
runtime; SHA256d5ce67809545afc4f88ce16dd37f6a319643e34d470444c71d1a44acbfd2c2da
unchanged. Source9e3749cae8b9d9cde907bd65742f8fd6fed23d71 includes URI/date/builtin
backup/Checkoutread, excludes later asyncbackup/boundCheckout routes/emitter fixes.
Runtime log /private/tmp/fl-oct14-uri-runtime.log; metadata
/private/tmp/fl-oct14-uri-checkpoint.json. Cleanup of owned source/install roots
pending guarded removal; retained archive and logs remain.660/682 unchanged.


### Finite record key emission and safe schema merging (2026-10-14)

Structural finite string literal/enum/union key sets (optional/nullable wrappers)
export OAS3.1 propertyNames enum/false and OAS3.0 optional named properties with
additionalProperties:false. Empty set permits onlyempty record. Bounded pure key
classification executes no callbacks. Other coercion/trim/regexflags/UTF16 key
semantics remain unchanged unsupported boundaries, not generic key equivalence.
Existing ts-deepmerge dropped reserved names even inside schema properties. A
per-call reversible key bijection at actual schema positions protects property
maps before merge and restores own keys with defineProperty into fresh maps.
Original inputs/$refs and literal default/const/enum handling are preserved;
100k schema-position/64depth/cycle guards do not claim arbitrary literal-data cycle
safety. No prototype mutation. UniversalSdk default-owned AJV enables ownProperties;
injected engines retain caller options (must configure own-property semantics).
Frozen5 /private/tmp/fl-record-export-source.json.1744 exact runtime/AJV pairs over
160exports and16 explicitly asserted OAS3.0 AJV __proto__ filtering disagreements
pass; no claim0mismatches across that boundary. Default/injected SDK controls and
strict12roots/614exports types(TS7.0.2) pass. URI203261, bounds153972, union10674,
Never224 regression logs /private/tmp/fl-record-{openapi,types,uri,bounds,union,never}.log.
No native/Cargo/lock changes; broader schema gate remains open;660/682 unchanged.

### Checkpoint cleanup and CI runner follow-through (2026-10-14)

After committed7b6482390 runtime receipt, removed immutable9e374 source/install
roots (33146504+176104340=209250844 logical bytes), preserving archive/logs; process,
open-file and retainedSHA guards passed. Metadata/cleanup log fl-oct14-uri- prefix.
487 nonexecutable rebuildable cache files in80 groups tied exclusively to those
removed roots also removed (835932431 logicalbytes), preserving current cache and
executables; source/hash/open-file/stat guards passed, receipt
/private/tmp/fl-oct14-uri-cache-cleanup.json. No physical APFS-space claim.
New bound Checkout CI wrapper passes actual HTTP/MCP and6SDK calls in3.63s,
log /private/tmp/fl-bound-checkout-ci-wrapper.log, against coherent current-main
Aggregate initial compile. Wrapper releases Cargo buildlock before nested SDK
compile. This is not a new archive-installed bound-profile runtime claim.


### Checked dense JSON tuples (2026-10-14)

Additive Tuple{items,rest} preaborts short arrays before child/default evaluation;
optional/default slots still count toward minimum. Surplus/no-rest marksDirty then
evaluates only declared positions; rest validates every trailing item. Child status,
ordered paths/messages, original own-key order, sharedschema/work/output bytebudget
and preallocation/default-clone charging match finite checked composition rules.
Dense JSON only; sparse/undefined elements/arbitrary callbacks remain outsideprofile.
Frozen7 /private/tmp/fl-zod-tuple-source.json.73allfeature/71minimal validationtests,
19050 exact actual/native pairs each default/arbitrary_precision,291liveHTTP/
138FastMCP/nativeMCP,3safe500 modes and6calls each generatedTS/RustSDK pass;
strict validation allfeature/minimal and AxumexampleClippy pass. Logs
/private/tmp/fl-tuple-{validation,minimal,parity,live,clippy,minimal-clippy,live-clippy}.log.
No failure required production changes; scratch source installed coherently.
CI adds oracle and expanded existinglive fixture. SDK checks prove transport/value
behavior, not new precise tuple models. Positional OAS3.1 emission requires explicit
2020dialect, consumer selection/positional coercion and Rustmodel follow-through;
current emitter remains separate opengap.660/682 (96.8%),22open unchanged.


### Trusted server-only backup-code viewing (2026-10-14)

Adds view_backup_codes_for_user and trusted clock variant, returning opaque
ViewedBackupCodes with zeroizing owned strings and borrowed iteration, no Debug
or serialization. Caller authorizes the requested user; fixed store tenant remains
the isolation boundary. Actual upstream serverOnly behavior has no HTTP endpoint;
no native HTTP/MCP/client endpoint is introduced. No User/session/password record
is required, and viewing does not consume codes, migrate storage or mutate state.
Built-in policies and trusted async codecs support the bounded array profile
(100codes,128UTF8bytes each); truthy nonarray upstream data remains outside typed
profile. Falsy decoded values/malformed JSON return INVALID_BACKUP_CODE while
generic verification behavior is preserved. Any preexisting lease, even expired,
rejects viewing without cleanup. Factor digest/revision and lease absence are
rechecked for every profile; custom futures release the initial transaction and
recheck fresh wall/monotonic deadline after final reads/commit. Legacy arrays skip
callbacks/deadlines. Final reads cannot revoke returned plaintext or detect every
completed no-change lease; cooperating PostgreSQL writers use existing tenant
advisory locks. No new store isolation or post-return revocation claim.
Frozen9 /private/tmp/fl-backup-view-freeze.json SHA256
4945106c7b0381f8efe7c17bae68f19a62e99804b1166e9a36bda3371561fdb7.
26backup tests including6new pass18.67s; restrictedPG1pass49.11s with unchanged
audit and foreign-tenant denial;14actual-source observations and strict full/minimal
Clippy pass. Owned roles/records/audit cleaned to0. Logs
/private/tmp/fl-backup-view-{regression,pg,clippy,minimal-clippy,actual}.log.
No lock/template/dependency changes; no immutable archive runtime claim. Broad
MFA/library and parity gates remain open;660/682 (96.8%),22open unchanged.


### Aggregate bound billing and Portal operation routes (2026-10-14)

Explicit bound-billing-v1 selector admits shared trusted scope plus independently
selected checkout/portal configuration; at least one known family is required.
Legacy and bound-checkout-v1 remain compatible; mixed/partial/null/unknown config
rejects. Selected families share artifact/runtime contracts and one explicit
publisher database URL, with independent grants and role/database validation.
No provider startup I/O, no credentials/private configuration in metadata.
Startup rejects base/managed/backup/partial authority before listening.
Portal POST /bound-portal/operations/:operationId accepts only localCustomerId;
GET on the same operation path inspects IDs/state only. Fresh created200 may
return sealed DTO/URL, previous completion suppresses URL, uncertain202 never
automatically redispatches. Calling service derives audit/publisher actor;
application owns individual user/Party authorization. No Portal URL reread is
implemented in this checkpoint; original unextended5min local cache policy and
provenance-aware current customer/binding checks are a separate follow-through,
not evidence of remote URL validity. Other billing families remain separate.
Frozen13 /private/tmp/fl-bound-billing-portal-final.json SHA256
0dfbee680475505b70b562cf94fe112a70a6510fef8b9b0fb3476279572b6489.
RestrictedPG/HTTP/MCP and generated TS/Rust SDK2calls each (4total) pass4.03s;
6units,32base/Stripe config/artifact cases and actual binary-template compilation,
Date composition, default66routes28schemas, strict3billing allfeature/alltarget
and featureoff Axum Clippy pass. Separate Checkout regression6SDKcalls passes.
Root direct-executable Portal CI wrapper independently passes3.49s, avoiding
nested Cargo build-lock deadlock; logs /private/tmp/fl-bound-billing-portal-ci-wrapper.log
and /private/tmp/fl-bound-billing-{portal-live,aggregate-unit,aggregate-artifacts,
checkout-regression,date-regression,contracts-regression,aggregate-clippy,default-clippy}.log.
Owned roles/databases cleaned0. No manifests/locks/migrations/framework changes;
no latest immutable archive-installed profile claim.660/682 (96.8%),22open unchanged.


### SDK AJV own-property compilation (2026-10-14)

Private bounded compilation-copy adapter keeps original public documents, own
properties and reference strings. Exact-end collision-free patternProperties
references original __proto__ property schemas, preserving existing patterns and
$id-bearing property resources without duplicate inline resource IDs. Default
SDK engines now enforce previously filtered OAS3.0 finite-record __proto__ schemas;
raw AJV16exceptions in earlier Record receipt remain historical raw-engine facts.
Schema-position traversal follows effective dialect, OpenAPI roots and explicit
local targets with10k/64depth budgets; injected engine options/custom keywords
are retained. Caller-selected ownProperties:false remains caller policy.
Review reproduced an injected useDefaults:true dual-use bug: repairing a schema
under default/const/enum changed literal data/equality. Both SDK/socket consumers
now explicitly reject such targets when repair is required, before compilation;
harmless literal refs remain supported. No implicit literal rewriting or resource
rebasing. Socket draft04 resource pointers reset on id (other drafts use$id).
Frozen5 /private/tmp/fl-ajv-own-source.json verified.128SDK/six-dialect socket
cases plus dual-use/dialect-inactive/literal/resource/budget/injected controls pass;
root independent oracle rerun also passes (/private/tmp/fl-ajv-own-root-review.log).
18reference realHTTP,13body regressions, URI203261+90/injected and strict12roots/
614exports TS7.0.2 pass. Final emitted socket runtime strictTSC passes. Logs
/private/tmp/fl-ajv-own{,-reference,-body,-uri,-types,-socket-types}.log.
No Cargo/lock/native/generated-install rerun claim. Broader schema constraint
gate remains open;660/682 (96.8%),22open unchanged.


### Application-owned OTP background dispatcher (2026-10-14)

Trusted OtpBackgroundDispatcher accepts one opaque owned lazy Send/static Future;
service/AuthHttp builders preserve other options and default awaiting without
dispatcher. Proof commits before fallible delivery preparation. Preparation failure
remains500, nohandoff; dispatcher ResultErr and async delivery errors are reduced
to static diagnostics and retain proof/success200. No auth cookies/JWT minted.
Scheduling, bounded queue capacity, rejection, shutdown drain/drop belong to app;
no implicit runtime/tokio spawn. Rust task construction does not poll unlike eager
JS Promise. Dropping cannot revoke external effects already started; accepted then
Err task remains dispatcher-owned. Blocking/panics outside Result contract.
Frozen12 /private/tmp/fl-otp-background-freeze.json SHA256
6efd86c0ec164f7b9f821f70c717d10f2859aae38e9a91e482e8561b6bcdc95f.
Focused3pass3.11s, affectedOTP36pass/1environmentignored46.86s; actual5pending/
preparethrow/handlerthrow/deliveryreject controls pass; strictfullalltargets and
minimal Clippy pass. Logs /private/tmp/fl-otp-background-{native,regression,actual,
clippy,minimal-clippy}.log. HTTPhashedstorage immediate200 beforedrain and
builderpreservation are tested. No storage/schema/manifests/locks changes, no PG
rerun or immutablearchive claim. This is OTP-only backgroundhelper coverage;
verification/reset/invitations/cleanup remain separate.660/682 (96.8%),22open.


### Explicit response dialect and positional tuple coercion (2026-10-14)

Additive ResponseSchemaContext/ResponseAjvFactory passes effective dialect to
caller factories. Owned SDK selects draft7 by default and Ajv2020 only for explicit
schema $schema/document jsonSchemaDialect2020. Unmarked OAS3.1 keeps legacy items[]
behavior. Injected engines/options/keywords remain caller-owned; unsupportedengine
or conflicting document/resource dialects fail before input conversion. No automatic
translation, fetching or global schema dialect reinterpretation. Per-document/
dialect caches isolate factory instances and registered resources. Bounded resolver
adds legacy items[]/additionalItems and2020prefixItems/items positional conversion
for date/binary/nullable/ref slots/rest; old twoargument converter and non-tuple
inline path remain unchanged. Validation controls false/absent tail admission.
Frozen8 /private/tmp/fl-tuple-consumer-source.json.17realHTTP conversions plus
legacy TypeBox/homogeneous/unmarked3.1 controls, factory context/cache counters,
mismatch/mixedresource rejection and budgets pass (/private/tmp/fl-sdk-tuple.log).
Reference18/body13/own128/URI203261 regressions pass, logs
/private/tmp/fl-tuple-consumer-{reference,body,own,uri,types}.log. Parent refresh
public source inventory usingTS5.9.3 now619entries/1694declarations/0unresolved
across44packages; inventorytests pass. Refreshed strict12roots/619exports fixture
(actualTS7.0.2) passes, log /private/tmp/fl-tuple-consumer-refreshed-types.log;
initial parent invocation lacked required fixtureenv and was corrected.
No native/Cargo/lock/emitter changes; precise Rust tuple models and explicit
positional emitter follow-through remain open.660/682 (96.8%),22open unchanged.


### Immutable37b54 package and portable runtime (2026-10-14)

Explicit source37b54efb4119f623793ab43c9648ed75e27c0a31 git-free Cargo source
archive installs offline/locked/debug and retains dotfiles/repackage listings.
2775regular files match archive bytes before and after runtime;4871008byte archive
SHA25625019de2b2243f714712a05efd683fa7d0ac9b45522587a783f2eee89f03e556
retained at cli/target/parity-artifacts/forklaunch-37b54efb4-25019de2.crate.
CLI ignoredlock SHA256d66f3120b6fab6af185dfa4e22c614b4bf5324370aede8667c9807e3e5bc5110.
Same3intentional public/test PEM fixtures match earlier retained416d;2absolute
TSbaseline PEM symlinks preserved without dereferencing. No draft source leakage.
Relocated application with all4worker families, IAM/CAC/base+provider messaging/
billing/ecommerce modules and infrastructure/capabilities compiles;12OpenAPI exports,
contract/native/CLI recursive SDKs, strict generatedTS/Rust/WebSocket consumers and
HTTP/MCP lifecycle/discovery/health/origin/auth/invocation/shutdown checks pass.
PackagedIAM184pass/3deliberatelyignoredPG in83.13s; separate restrictedPG receipts
remain separate. All5signed capability callbacks pass. No new generated actual
MFA29client rerun or archive-generated bound-profile live claim: distinct
working-source Portal/Checkout HTTP/MCP+SDK fixtures cover those opt-in routes.
Source includes asyncbackup, View, AggregatePortal/Checkout, Tuple runtime and
own-property socket repair; excludes later OTPdispatcher, explicit response tuple
consumers, storedPortal/reset/emitter drafts. No latest-source equivalence claim.
Metadata /private/tmp/fl-oct14-billing-sdk-checkpoint.json; package/install/runtime
logs /private/tmp/fl-oct14-billing-sdk-{driver,package,install,repackaged-files,runtime}.log.
Owned source/install roots pending guarded cleanup; archive/logs retained.
660/682 (96.8%),22open unchanged.


### Immutable37b54 owned-workspace cleanup (2026-10-14)

After committed70395cf5a runtime/byte receipt, exact owned source and extracted
install/runtime roots were removed with prefix/no-symlink/process/open-file and
retained archiveSHA guards.33480662+177077613=210558275logicalbytes removed.
182nonexecutable rebuildable native cache files in80groups tied exclusively to
removed roots were pruned,414381438logicalbytes, excluding current source and
open files with stat/inode/hash guards. No executable/current-cache deletion or
physical APFS-space claim. Retained4.87MB archive and logs remain. Metadata
/private/tmp/fl-oct14-billing-sdk-checkpoint.json, cleanup log
/private/tmp/fl-oct14-billing-sdk-cleanup.log, cache receipt
/private/tmp/fl-oct14-billing-sdk-cache-cleanup.json.660/682 unchanged.


### Explicit JSON Schema2020 tuple emission (2026-10-14)

Additive Zod openapi2020/static capability and optional shared capability export
input schemas with explicit root$schema2020. Fourth generateSchema options argument
threads immutable recursive generation context,10kvisits/64depth budgets; root
$schema emitted once. Explicit OAS3.0 or conflicting metadata dialect rejects.
Default general exporter/TypeBox unchanged. Tuples emit prefixItems whennonempty,
minItems=all declaredslots (including optional/default), items=false+maxItems for
closed tuples or exact rest schema. Nullable tuple merging preserves array type.
Adapter reuses root-input projection, so root default/effect/preprocess callbacks
are not evaluated; existing nested callback/default semantics remain an explicit
gap, not a callback-free recursive exporter claim. Explicit metadata remainslast.
Frozen6 /private/tmp/fl-tuple-emitter-source.json.1620actualZod/AJV2020 comparisons
across108exports and20realHTTP SDK cases including newly emitted date/binary/rest/
nested/empty schemas pass; reference18/body13 and stricttypes pass. Logs
/private/tmp/fl-tuple-final-{emitter,types,rust_sdk_tuple_coercion,
rust_sdk_reference_coercion,rust_sdk_body_parity}.log. Existing bounds153972,
union10674,Never224,record1744+16rawAJVexceptions,URI203261+90,own128 regressions
pass (/private/tmp/fl-tuple-emitter-rust_*.log). Parent sourceinventory refreshed
TS5.9.3:619entries/1696declarations/0unresolved; tests and strict12roots/619exports
(actualTS7.0.2) pass. Logs fl-tuple-emitter-{inventory,refreshed-types}.log.
No native/Cargo/lock changes or precise generatedRusttuple claim. OAS3.0 tuples,
precise Rust tuple models and remaining emitter/constraint gates stay open;
660/682 (96.8%),22open unchanged.


### Provenance-aware stored Portal UUID rereads (2026-10-14)

New direct-only get_stored_bound_portal_session returns original sealed DTO under
current trusted profile, without providerHTTP or URL renewal. Actor-owned completed
creation receipt, immutable intent/scope/API/config/redirect, exact current Portal
record, durable alias and current Customer data/xmin/binding must match. Atomic
joined observation includes alias/Customerbinding xmin as well as data/generation;
repeat after sensitiveaudit awaits must equal initial snapshot, then commit.
Original createdAt<=clock<expiresAt and exact5min lifetime checked beforedecode
and on fresh completionclock afterCOMMIT; reversal/future/expiry reject withoutURL.
Data linearizes at final matching SELECT; observedAt denotes completionclockcheck,
not data unchanged throughCOMMIT. No historical never-rebound or remoteURLvalidity
claim. Duplicatecreate/inspection remain IDs-only, no cache/provenance downgrade.
AggregatePortal adds getStoredBoundPortalSession GET /bound-portal/sessions/:publicId
with stored DTO/observedAt200 and explicit expiry410; HTTP/MCP andSDK metadataagree.
Frozen12 /private/tmp/fl-stored-portal-final.json SHA256
3fd8b6aac5d69c85968b78d99a19c529be56864f069ce5d9102f260572bcfdd3.
RestrictedPG provenance/tenant/actor/config/API/mutation/auditexpiry+zero provider
trapcalls pass2.94s; TTL nanosecond/future/backward unit and6routeunits pass.
HTTP/MCPfresh+expired410/noURL andbothSDK3calls each6total pass3.21s;32artifact/env
cases plusactualbase/Stripe binarytemplatecompile, strictbilling allfeature/alltarget
andAxumfeatureoffClippy pass. Logs /private/tmp/fl-stored-portal-{domain-pg,
lifetime-unit,route-unit,live-route,artifacts,clippy,default-clippy}.log.
Ownedroles/databases cleaned0. Only fixture UUIDSerde/semicolon fixes during
validation, productiondomain unchanged after firstPG. No lock/dependency/template
changes andno new immutablearchive-generatedprofileclaim.660/682 (96.8%),22open.


### Trusted password-reset delivery and shared scheduler (2026-10-14)

Optional PasswordResetDelivery hook enables existing public reset-request route;
without hook RESET_PASSWORD_DISABLED stays. Email normalization/redirect trust and
bounds precede store access. Missing user gets same200message/no delivery/noinsert,
with discardedrandom24 and indexed dummylookup timing mitigation, notconstanttime.
Existing user inclsocialonly gets unbiased24ASCIIbase62 token with fixed1h expiry,
existing reset-password:<token>/userID verification format; configuredPG encrypts,
customstore owns persistence/security. Existing POSTreset consumption/expiry equality/
replay unchanged; GETcallback checks/redirects without consuming. No session issuance
or token in requestresponse/logs; prior sessionretention policy unchanged.
Opaque owned deliverycontext exposes boundedscalar publicuser projection, zeroizing
URL/token and noDebug/Serde/rawrequest. Borrowedfield validation precedes cloning.
Proof commits before synchronousprepare; prepareErr sanitizesempty500, asyncdelivery/
dispatchErr remains200 with staticdiagnostics and committedproof. No transaction
spans callbackfuture. Cancellation split atcommit explicit; no durablemailqueue claim.
Shared background::AuthBackgroundTask/Dispatcher/DeliveryFuture preserves oldOTP
publicnames asaliases; globalAuthHttp scheduler inherited byOTP unless explicitOTP
override, eitherbuilderorder. Lazy polling/capacity/drain/drop app-owned, noautoexecutor.
Frozen15 /private/tmp/fl-reset-delivery-freeze.json SHA256
8b1eb8e0a23307c794865fa22d9848c51a68c7e1f72a30c8ba96087e53616694.
Actual9, focusedreset4 (3.15s), scheduler4 (12.39s), resetfilter5 and restrictedPG1
(52.14s), strictfullalltargets/minimalClippy pass. Resetfilter5 includes new4 plus
elapsed-session test; unchanged oldresethelper is exercised directly by newdomain/
PG lifecycle, not falsely counted as independently rerun. Missingdummy uniqueAPI
initialfailure corrected to indexedfind_page limit1; nestedprojection andcancellation/
commitfault checks pass. Owned roles/records/audits0. Logs
/private/tmp/fl-reset-delivery-{actual,native,scheduler,regression,pg,clippy,minimal-clippy}.log.
No manifests/locks/schema/template changes; no immutablearchive/generated delivery
profile claim. Configurable tokenTTL/arbitraryuserprojection/rawJScontext and email/
invitation/globalbackgroundcoverage remain separate.660/682 (96.8%),22open unchanged.


### Precise fixed-length Rust tuple models (2026-10-14)

Shared CLI/native model compiler recognizes exact closed dense positional shapes
using effective min/max/tail constraints, including truncated prefixes and exact
empty arrays. Named item0/item1 sequence structs use custom SerializeSeq/SeqAccess
with strict missing/extra rejection, retaining null separately from missing. Empty
sequence serializes[] notunit/null; numeric sequence_len metadata avoids expression
depth and serde tuple arity16 limits. Existing field-based recursion/default analysis
follows required positionalBox edges, suppressing recursiveDefault. Allfour existing
RustModelTemplate construction sites audited for additive sequence metadata.
Root review found implicitdraft7 prefixItems incorrectly active during graph model
normalization; scoped ignore-prefix fix for emptydialect preserves explicit2020
inheritance and legacyitems[], with runtimePayloadCodec/model ['x']/[1] controls.
Other implicit dialectkeyword policies are not globally reinterpreted this slice.
Frozen5 /private/tmp/fl-fixed-tuple-source.json.79generatedRust acceptance/roundtrip
cases across14schemas:72actualZod cases+7schema/dialect controls; generatedTS valid
assignments/types compile. Arity0/1/2/16/17/32/128, nested/null/ref/legacy/truncation/
requiredrecursion tested (/private/tmp/fl-fixed-tuple-models.log). Nativecodegen
allfeature46, minimal46/nativeSDKRust5 and strictalltargetClippy pass; fullCLI1125
pass and1294 embeddedsource/template entries pass. Logs
/private/tmp/fl-fixed-tuple-{native,consumers,clippy,cli-full,package}.log.
Initial CLI unit assertions corrected for sharedmodule path/namedalias target,
no production defect; finalfullsuite green. Package coverage is currentworking
source including drafts, not immutable archive. No dependency/lock edits.
Variable prefix/rest still use existing broad representation and need separate
precise sequence/presence handling; this is not fulltuple or schemagate closure.
660/682 (96.8%),22open unchanged.


### Provenance-aware directly created Plan UUID reads (2026-10-14)

Explicit get_bound_created_plan requires actor-owned completed creation journal,
exact decrypted local DTO==sealedreceipt, current alias/scope/API/generation,
canonicalrequest/hash and normalized trustedProducts equal current policy. No
administrativealias/pendingjournal/mutableexternalId fallback or publication key.
Opaque snapshot holds full record/binding/journal xmin+proof, repeated after read
audit waits initially and after provider I/O; final matchingSELECT linearizes data.
Fresh direct Account/Balance and expanded Plan/Product checks require exactprovider
id/kind/mode/notdeleted/perunit terms from sealedrequest. Productpresentation alone
refreshes providerFields, preserving all localUUID/name/price fields; matches actual
configuredTS enrichment shape. This finite immutablecreation profile rejects changed
terms/policy rather than claiming all general updated Plan semantics. Finalprovider
context freshness after auditCOMMIT rejects expiry during lastDBwait. Later local/
remote changes remain possible. No providerPOST/localmutation/payment/entitlement
assertion, autoalias or historicalnever-rebound claim.
Frozen10 /private/tmp/fl-bound-plan-read-final.json SHA256
bb889a7864f36aa09a8d1a5fd85bdec42c18a5210e04ebb6ac1b5fa348b612a4.
65actualTS cases pass (/private/tmp/fl-bound-plan-read-ts.log), including Product-only
enrichment preserving localfields. RestrictedPG new read/provider/tenant/actor/API/
policy/provenance/mutation/auditwait deadline matrix passes5.54s; existing Plan
creation PGregression passes12.85s; strictbase+Stripe allfeature/alltargetClippy pass.
Logs /private/tmp/fl-bound-plan-read-{pg-final,creation-regression,clippy-final}.log.
Initial rawalias test copied incompleteDTO and was fixed to validDTO; two lintstyle
fixes followed, no functional production correction. Completed-journal UPDATE
immutability trigger prevents the proposed no-op journal mutation test; snapshot
still records xmin. Ownedfl_bound_plan roles cleaned0.
No manifest/lock/migration or default route/API change; aggregate route/config
integration remains separate.660/682 (96.8%),22open unchanged.


### Trusted explicit verification-email delivery (2026-10-14)

Optional EmailVerificationDelivery hook enables existing send-verification-email;
defaultdisabled unchanged. Explicit endpoint alwaysawaits and propagates sanitized
prepare/future error as empty500, deliberately ignoring globalbackgrounddispatcher
as actualsource does. Anonymous missing/alreadyverified gets same200 withoutdelivery,
with signedtoken dummywork and500ms floor includingcallbackerrors; sessionlookup
precedes floor, no constanttime claim. Valid authenticatedemailmismatch/alreadyverified
reject400 and skipfloor; invalid/expiredcookies takeanonymouspath. Trustedcallback
guard and8192byte callbackbound precede store/tokenwork. No new sessions/cookies/JWT
authentication, apart from existing ordinary session refresh behavior.
Opaque context carries bounded borrowed-before-clone publicprojection, zeroizing
URL/token, noDebug/Serde/rawrequest. HS256 lowercaseemail/iat/exp+1h proof uses
existingsecret and GETverifier, no updateTo/requestType/change-email authority.
It proves current tenant-local emailaddress ownership, not immutableuserid: address
reassignment can apply unexpiredproof to currentowner, and alreadyverified replay
succeeds. No singleuse/store/purpose/autoSignIn claim or silent extra authority.
Frozen9 /private/tmp/fl-email-delivery-freeze.json SHA256
0775c1d44ca62f86c3e7a9118f7a7f2aa4ea8bfee8a274c572484796fc587979.
Actual11/focused3(3.62s), restrictedPG1(52.91s), existinglive signedauthHTTP/MCP/
JWKS/refresh/CSRF/shutdown1(24.83s) pass; strictfullalltargetsClippy10.48s/minimal3.60s
pass. Logs /private/tmp/fl-email-delivery-{actual,native,pg,consumer,clippy,
minimal-clippy}.log; owned roles/records/audits0. Final lint also corrects prior
resetmodule docmarkdown and gates HTTPimplementation out ofminimallib.
No manifests/locks/migrations/templates, no immutablearchive deliveryprofile claim.
Signup/signin/change-email callbacks, verified before/afterhooks andexpiryoptions
remain separate;660/682 (96.8%),22open unchanged.


### Precise variable-prefix and rest Rust tuple models (2026-10-14)

Shared sequence models now preserve optional positions and typedrest: required
prefixfields T, laterfields Option<T>, nullableoptional Option<Option<T>> keeps
None absent versusSome(None) null. Fixedexistingfields/types unchanged. Serializer
validates contiguity, completeprefix before nonemptyrest, checkedlength/min/max
before firstwriter call. Decoder stopsoptionalpolling afterNone, validatesrequired
positions/totalbounds and checksmax before retainingrest. Open/true tail uses
Vec<Value>, typedtail usesVec<T>; unreachableprefix positions truncate atmax.
Requiredrest suppressesDefault; optionalprefix/Vec terminate recursive default
edges. Homogeneous/nonpositional/unsupportedbound schemas retain prior broad
behavior; runtimecodecs own other schema constraints beyond positional shapes.
ActualgeneratedTS proof caught existingrecursive rest Array<T> compiler rejection;
scoped rest rendering `(T)[]` resolves recursivealias and nullable/union cases,
homogeneous arrays unchanged. No generalized staticconstraint equivalence claim.
Frozen3 /private/tmp/fl-variable-tuple-source.json.892generatedRust acceptance/
exactJSON roundtrips across51schemas (19actualZod+873AJV7/2020), strictgenerated
TS assignments/recursion and constructedgap/rest/max pre-write denial pass.
GeneratedRust publicmodules compile via cargo rustc --lib -- -Dwarnings before
tests; fixed79 regression rerunpasses. Logs /private/tmp/fl-variable-tuple-{models,
fixed}.log. Codegen46allfeature/46minimal+nativeSDKRust5, strictallfeature/alltarget
Clippy, finalfullCLI1125 pass (/private/tmp/fl-variable-tuple-{native,minimal,
clippy,cli-full}.log). Lastsource changes were lint extraction/methodpointer
cleanup followedfinalminimal/SDK/fullCLI; finalfixture-only warningproof passed.
No lock/dependency/newexample edits. Broader callback/emitter/constraint gate
remainsopen;660/682 (96.8%),22open unchanged.


### Explicit aggregate Plan-read capability and routes (2026-10-14)

BoundBillingV1 adds exact optional planRead:{trustedProducts} within sharedscope.
requires_publisher() is checkout||portal only: Plan-read-only selection rejects
stray publisherURL and constructs no publisher connection; mixed/create selections
still require separately validated publisher. Public trusted constructor remains
sourcecompatible. Startup validates localdirect transport and required schema/table
SELECT/auditINSERT grants beforelisten, with zero providerI/O; deniesbase/managed/
backup/missinggrants. Ordinary legacyCRUD privileges neither inferred nor forbidden.
GET /bound-plans/:publicId uses authenticatedserviceactor and createdPlan provenance
getter, Product-only enrichment/no POST. Shared runtime/artifact contracts and both
SDKs expose exact selectedcapabilities; Dateprojection andlegacyselectors preserved.
Existing66legacyroutes remain; Plan-read-only describes the newselection, not an
entire read-only service or inability to use legacyAPI credentials.
Frozen11 /private/tmp/fl-plan-read-route-final.json SHA256
e7c88fc5012e21188a0680c72f3515e47d39d657c83199a1dd1c91a7a7d6f29d.
RestrictedPG/HTTP/MCP+bothSDK onecall each2total pass2.49s with Planrecord/binding/
journal mutation grants revoked; startupdenials beforeproviderI/O, actor/provenance/
wrongproviderID cases pass.46artifact/config cases plusactualbase/Stripe maincompile,
7units, strict3crateallfeature/alltarget andfeatureoffClippy pass. Rootdirectexe CI
wrapper independentlypasses2.46s (/private/tmp/fl-plan-read-root-wrapper.log),
releasingCargo buildlock before nestedSDKcompile. Logs /private/tmp/fl-plan-read-
{route-live,artifacts,route-units,route-clippy-final,route-default-clippy}.log.
Onlyfixturesemicolon correction; productionpassed unchanged. Ownedfl_bound_plan
roles0. No manifests/locks/newmigration; no newimmutablearchive selectedprofile
claim. Subscription/otherfamilies/remainingmapper semantics open;660/682 (96.8%).


### Opt-in pure input schema export (2026-10-14)

`openapi2020(schema, {pureInput:true})` and the explicit 2020 generator option
traverse supported ordinary constructed Zod schemas without evaluating default
factories, effects, predicates or built-in coercions. Unsupported nodes reject;
computed defaults are omitted, explicit OpenAPI defaults retained, omittedness is
structural, and nullable false schemas preserve null through an anyOf branch.
All union children traverse guards. Shared 10,000-work/64-depth limits and cycle
checks bound traversal. NativeEnum remains rejected until normalized projection.
This is not a sandbox for getters, Proxies or shape thunks; Date retains declared
wire semantics. Structural Default omittedness does not prove factory output valid.
Default exports remain compatible, including date/binary SDK behavior.
Frozen five-file manifest /private/tmp/fl-pure-input-source.json verified by root.
577 actual/AJV comparisons and 138 controls pass; tuple1620, SDK20, union10674,
Never224 and strict 12-root/619-export TS7.0.2 checks pass. Root regenerated public
inventory: 620 entries/1698 declarations across44 packages; syntactic counts are
not semantic parity. CI includes fixture. Broader22 gates remain open,660/682(96.8%).


### Trusted signup/signin verification policy (2026-10-14)

Optional EmailVerificationPolicy controls require/sendOnSignup/sendOnSignin;
manual delivery hook alone does not enable automatic sending. Required signup
returns token:null and no session, with generic submitted-profile duplicate
responses; required signin gates correct-password unverified users before MFA,
trust and session creation. Require without hook is supported. Default atomic
signup behavior remains unchanged. SignupOutcome is additive; legacy sign_up
fails closed before effects when required policy cannot return an AuthGrant.
Delivery preparation/scheduling runs outside DB transactions. Callback receives a
prospective user before persistence, unlike upstream transaction visibility;
mail can precede failed commit and concurrent duplicates can send before a winner.
Writer rechecks and atomic persistence prevent partial account/session state.
No claim of complete callback equivalence, autoSignIn, social or all hooks parity.
Actual13, focused3 including8HTTP configs, restrictedPG rollback/reentry/races,
fullIAM198pass/3ignored and strict full/minimal Clippy pass. Frozen11 manifest
/private/tmp/fl-email-policy-freeze.json verified. Logs /private/tmp/fl-email-policy-
{actual,native,pg,full,clippy,minimal-clippy}.log. Existing metadata already admits
nullable signup token and403; no new Cookie/JWT on denied paths. Broad660/682
(96.8%),22open unchanged.


### Bound completed Subscription read (2026-10-14)

Library get_bound_created_subscription uses actor-owned sealed completed receipt,
exact local DTO/alias generation, Customer and Plan provenance and current trusted
profile. Plan creator may be catalog actor: subscription authority is explicit
trusted mapping, not same-actor equality. Nonlocking joined observations repeat
after provider/audit waits; original request deadline is checked after commit.
Fresh pinned account/mode Customer, expandedPlan and Subscription reads validate
original item/start/customer/Plan/quantity/description. Only active/incomplete and
incomplete-to-active transitions are supported; local status/active remain intact,
current provider fields replace only stripeFields, matching actual TS getter.
Direct and authenticated-event-recovered completed receipts work; pending/admin
fallback, trial/discount/delegated transitions and lifecycle synchronization do not.
CustomerParty is explicit admin assertion, not external IAM ownership proof.
ActualTS66, new restrictedPG matrix and existing creation/recovery tests2pass6.21s,
strict base/Stripe allfeature/alltarget Clippy pass. Frozen11 manifest
/private/tmp/fl-subscription-read-final.json verified. Logs /private/tmp/fl-subscription-
read-final-pg.log and /private/tmp/fl-subscription-read-clippy-final.log. No new
routes yet; aggregate selection remains separate. Broad660/682(96.8%),22open.


### Generated Rust tuple internal names (2026-10-14)

Generated visitor/generic identifiers now use reserved __Forklaunch prefixes,
which public ASCII-alphanumeric model-name normalization cannot produce. Public
model names and field layout remain unchanged. Recursive models SequenceVisitor,
A, D and S prove earlier internal-name shadowing fixed. 47 codegen tests,920
variable cases/55 schemas,79 fixed cases, strict generated rustc -Dwarnings and
real TS assignments/roundtrips, strict Clippy and1126 CLI tests pass. Frozen4
manifest /private/tmp/fl-tuple-collision-source.json verified; isolated cli/target
logs /private/tmp/fl-tuple-collision-{units,variable,fixed,clippy,cli}.log. No lock or
dependency changes. Immutable4ff checkpoint predates this fix and does not prove
these model names. Broader schema/codegen gates remain open,660/682(96.8%).


### Finite NativeEnum schema projection (2026-10-14)

Numeric reverse maps, mixed string/number enums, duplicate values and empty enums
now follow actual pinned Zod getValidEnumValues semantics, including own private
key behavior. Homogeneous enums declare type, mixed enums use enum without type,
empty enums emit false schema. Explicit annotation merging remains compatible;
enum_ factory union(Object.values) semantics are intentionally distinct. Pure
input now supports normalized finite NativeEnum, superseding its earlier exclusion;
non-JSON effective values reject, while raw nonfinite baseline behavior is unchanged.
Frozen4 /private/tmp/fl-native-enum-source.json hashes verified.16080 actual/AJV
comparisons across240exports and41controls pass; pure577+132controls,union10674,
tuple1620,SDK20,Record1744+16rawAJV exceptions,Never224 and strictTS7.0.2
12roots/620exports pass. No public export/dependency/Cargo changes. CI includes
oracle. Logs /private/tmp/fl-native-enum-{parity,types}.log and named regression
logs. This finite correction does not close the broad schema gate;660/682(96.8%).


### Repeated finite string bounds (2026-10-14)

For strings without trim/case transforms, safe nonnegative integer min/max/length
registrations now export the conjunction's strongest lower/upper bound rather
than its last declaration. Contradictions stay rejecting; explicit annotation
merge remains unchanged. Unsupported registrations/ordered transforms retain
baseline behavior.196800 BMP/JSON actualZod/AJV comparisons across12300exports
and10controls pass. Six nonBMP UTF16-versus-code-point discrepancies remain
explicitly asserted; this does not close string-length semantic equivalence.
Pure577+132,NativeEnum16080+41,numeric/array153972,union10674,Record1744+16,
tuple1620,SDK20,URI203261TS-only and strictTS7.0.2 12roots/620exports pass.
Frozen2 /private/tmp/fl-string-bounds-source.json verified; logs
/private/tmp/fl-string-bounds-{parity,types}.log plus named regression logs.
Root regenerated inventory source locations and passed inventory checks; no
new exports or dependencies. CI includes oracle.660/682(96.8%),22open unchanged.


### Immutable tuple/billing checkpoint portable runtime (2026-10-14)

Exact committed source4ff9e3958a5103c179dbb948cb7e8340437de5e0 packaged and
installed offline/locked from git-free extracted source. Archive2799regularfiles,
4914201bytes, SHA256db6cb114da0e7faabb74e01ceb1ae0da081020194764fbabbbc177171ca0033c,
retained cli/target/parity-artifacts/forklaunch-4ff9e3958-db6cb114.crate.
All regular source bytes verified before and after complete runtime. CLI ignored
lock SHA256d66f3120b6fab6af185dfa4e22c614b4bf5324370aede8667c9807e3e5bc5110.
Repackage/dotfile/symlink guards and intentional PEM fixtures preserved.
Portable relocation, all4worker families, modules/12OpenAPI exports, strict native
and TypeScript WebSocket/HTTP SDK consumers, contract/OpenAPI imports and native
recursive generation pass. Actual generated service HTTP/MCP starts, shared and
separate listeners, explicit/ephemeral ports, TypeScript+2000 fallback, atomic
startup rejection and graceful shutdown pass. Five generated capability callbacks
pass forgery/staleness/replay/event tests. PackagedIAM195pass/3PGignored111.77s;
separate restrictedPG receipts remain required, no new MFA29 generated rerun claim.
Metadata /private/tmp/fl-oct14-tuples-billing-checkpoint.json; runtime log
/private/tmp/fl-oct14-tuples-billing-runtime.log. Runtime exited0/finalPASS marker;
archive/hash/source verified independently by root verification script.
This source predates pure-input/verification-required/Subscription reads/tuple
collision/NativeEnum/stringbounds/auto-signin/CORS changes; their working-source
receipts are separate. Known tuple public-name collision fixed later is not
covered by this archive. Cleanup follows only after committing this receipt.
Checklist660/682(96.8%),22broad gates remain open; startup evidence is local,
not an AWS deployment or published-crate receipt.


### Pure input built-in string transform guard (2026-10-14)

Opt-in pure input now rejects trim/toLowerCase/toUpperCase registrations as
unsupported projections, including nested/optional/union/default wrappers.
These transforms do not execute user callbacks but can change admission through
ordered constraints or Unicode expansion (lowercase İ/uppercase ß); silently
exporting raw length constraints was unsound. Default exporter remains unchanged.
577comparisons+246controls (including108 transform/nesting/default guards and
three actual admission reproducers), strictTS7.0.2 12roots/620exports,NativeEnum
16080+41,tuple1620 andSDK20 pass. Frozen2 manifest
/private/tmp/fl-pure-transforms-source.json verified; logs /private/tmp/fl-pure-
transforms-{parity,types,zod_native_enum_openapi,zod_tuple_openapi,sdk_tuple_coercion}.log.
Root regenerated inventory source locations and checks pass. This supported
profile rejects transformed strings rather than claiming exact transformed input
projection; callback/Unicode/general schema gates remain open.660/682(96.8%).


### Completed immutable4ff checkpoint cleanup (2026-10-14)

After committing runtime receipt71828f01c, root removed only the two exact owned
checkpoint/archive-install workspaces:211507581logicalbytes across9103regular
files. Prefix/resolve/symlink/process/open-file guards and retained archive hash
checks passed. Then182nonexecutable cache files across80groups exclusively
referencing those removed roots were removed:416205458logicalbytes. Current
framework/blueprint/CLI sources, executables, symlinks, open files and changed
inode/mtime/size were excluded. Retained crate SHA256 unchanged; logs and receipts
retained. Total627713039logicalbytes removed; physical APFS free space is not
claimed. Otherjobs' worktrees/caches/PG/watcher were preserved. Metadata and logs:
/private/tmp/fl-oct14-tuples-billing-checkpoint.json,
/private/tmp/fl-oct14-tuples-billing-cleanup.log,
/private/tmp/fl-oct14-tuples-billing-cache-cleanup.json. Broad660/682(96.8%).


### Independent signup auto-signin option (2026-10-14)

Trusted with_signup_auto_sign_in(false) on AuthHttp/CredentialService suppresses
signup sessions independently of verification requirements; defaulttrue preserves
existing behavior. New SignupOutcome::Unauthenticated returns token:null/no cookie,
with generic duplicate submitted-profile privacy; later signin remains enabled.
Verification sending fallback still depends on require, not auto-signin. Synthetic
configured public output fills absent image/phoneNumber:null. Legacy sign_up fails
before effects when no AuthGrant can be issued. New enum variant requires exhaustive
consumer matches to adapt; existing variants/default methods remain available.
Actual5,focused3(43.92s),affectedpolicy5(53.27s),restrictedPG1(83.36s) and strict
allfeature/alltarget+minimal Clippy pass. Concurrent duplicates yield one account,
zero sessions; owned roles/records/audits0. Frozen9 manifest
/private/tmp/fl-signup-auto-signin-freeze.json verified (manifest SHA256
f08f68d890ab363fd0e813b4c01f24575baa55e62870e45736171d278e08253d).
Logs /private/tmp/fl-signup-auto-signin-{actual,native,regression,pg,clippy,
minimal-clippy}.log. Existing nullable token metadata adequate; prospective-user
callback boundary and broaderhooks remain as previously documented. No fullIAM
rerun claimed for this slice (affected5 and focused3 used).660/682(96.8%),22open.


### Aggregate completed Subscription read selection (2026-10-14)

BoundBillingV1 now accepts optional subscriptionRead:{plans:[{localPlanId,
creationOperationId}],collectionPolicy:"automatic_default_incomplete_v1"} under
shared pinned deployment/account/mode. Exact bounded nonempty Plan mappings and
finite policy required. Sole read selection needs no publisher; mixed creation
still requires separately trusted publisher. GET /bound-subscriptions/:publicId
uses authenticated service actor and the completed-provenance library getter;
HTTP/MCP and generated artifacts/SDKs share registry metadata. Local direct
transport/schema/SELECT/auditINSERT grants validate before listening, zero
provider startup I/O; unsupported/backup/missing grants reject. Existing66legacy
routes remain: selector does not make the whole service read-only. No provider
POST or lifecycle/entitlement synchronization added.
Frozen10 manifest /private/tmp/fl-subscription-read-route-final.json verified,
SHA256e8c2cbb901836d536685eb1b3a3e0762fb56d6beffd60c01f4a686f155127181.
RestrictedPG/liveHTTP/MCP+oneTS/oneRust SDK read3.11s,8units,62rendered config/
artifact cases+actualbase/Stripe maincompile,strict3crateallfeature/alltarget and
featureoff Clippy pass. OwnedfixturePGroles/databases0. Logs /private/tmp/fl-
subscription-route-{live,units,artifacts-final,clippy-final,default-clippy}.log.
Root CI direct-executable wrapper releases Cargo lock before nested SDK builds;
syntax checked, same executable validated by agent. Independent wrapper rerun
queued behind focused CORS check. Corrected copied getter before native compile,
boxed test DTO/path classifier/format lints; no manifest/migration/template edits.
Broader mapper/mutation/recovery gates remain open;660/682(96.8%),22open.


### Static CORS configured final statuses and Subscription wrapper (2026-10-14)

TypeScript CORS optionsSuccessStatus now accepts explicitly configured final
HTTP200–599, including redirect/error preflight responses with empty body, rather
than incorrectly requiring2xx. Informational statuses reject at construction.
Default204 and downstream handlers/disabled/preflightContinue behavior preserved.
Focused6native tests,810actualTypeScript/native HTTP comparisons and strict
allfeature/alltarget Axum Clippy pass.650pairs exact;160explicitly assert Express
zero Content-Length versus Hyper omission on204/304. The304 transport difference
was discovered by expanded tests, recorded precisely, and remains unclosed.
Dynamic origin delegates/arbitraryJS coercions remain outside static profile.
Logs /private/tmp/fl-cors-final-status-{native,parity,clippy}.log.
Independent new Subscription CI direct-executable wrapper passes restrictedPG/
HTTP/MCP/oneTS+oneRust SDK3.01s (/private/tmp/fl-subscription-route-root-wrapper.log),
confirming Cargo lock is released before SDK subprocess builds. This supersedes
the queued wrapper status in preceding receipt. No deps/locks.660/682(96.8%).


### Literal string schema constraints across AJV and Rust (2026-10-14)

Non-transforming startsWith/endsWith/includes declarations now emit escaped
conjunctive pattern constraints rather than disappearing. endsWith uses true-end
lookahead, avoiding ECMAScript $'s final-newline admission. Supports well-formed
Unicode literal registrations, includes position undefined/numericzero (including
negativezero), and empty includes at any position as a no-op. Nonzero nonempty
positions/lone-surrogate registration literals retain baseline general export and
reject in pure mode. Arbitrary RegExp flags/engines and transformed strings remain
outside this slice. Existing format/pattern and annotation merges preserved.
92880actualZod/AJV comparisons across1080exports+20controls and30960shipped
Rust jsonschema/fancy-regex comparisons across360schemas pass, including newline,
metacharacter/control/NUL/astral/contradictory/nested cases. StrictTS7.0.2
12roots/620exports passed, then root refreshed inventory to621entries/1699records
and independently passed12roots/621exports. Pure577+246,stringbounds196800+6
explicit Unicode differences,NativeEnum16080+41,SDK20,URI203261TS-only pass.
Frozen4 /private/tmp/fl-string-literals-source.json verified; logs /private/tmp/fl-
string-literals-{native,types}.log plus named regression logs; root types log
/private/tmp/fl-string-literals-root-types.log. CI enables native proof with existing
contract schema-validation example; no Rust source/deps/locks changed. Broader
schema/Unicode/callback gates remain open,660/682(96.8%),22open unchanged.


### Optional post-reset session revocation (2026-10-14)

Defaultfalse with_revoke_sessions_on_password_reset is available on trusted
CredentialService/AuthHttp. Enabled reset first commits password update and
consumed proof, then deletes all current user's sessions (including expired)
through tenant-bound indexed256-row cursor pages in a separate writer transaction.
Foreign users remain untouched;513-session test proves pagination. No cookie
clearing is invented. A second-delete fault rolls back earlier session deletions
while the changed password/consumed proof remain committed, matching upstream's
observable failure boundary. Native revocation batch is transactional; no claim
about arbitrary source adapter partial deletes or onPasswordReset hooks.
Actual4,focused3(18.68s),affectedreset8(18.70s),restrictedPG1(99.67s),strictfull
allfeature/alltarget18.08s+minimal3.08s pass. Ownedroles/records/audits0.
Frozen10 /private/tmp/fl-reset-revocation-freeze.json verified (manifest SHA256
3f2bdb242ddaa60b77ec07e6fcce790f3204940ce400a38dc46b3b97ed32af5b).
Logs /private/tmp/fl-reset-revocation-{actual,native,regression,pg,clippy,
minimal-clippy}.log. No schema/dependency changes. Further trusted hooks and
provider lifecycle remain open;660/682(96.8%),22broad gates unchanged.


### Factory email and URI scalar composition provenance (2026-10-14)

Factory-only email/WHATWG URI grammar now survives added nontransform length or
supported literal checks and repeated identical format checks. Private marker and
original format must remain; strict check whitelist prevents inferring provenance
from public metadata. Previously checks.length===1 silently downgraded email.min
or uri.min to different RFC validators. Mixed formats, arbitrary regex/transform,
unsupported literal positions, rawZod/TypeBox and explicit user overrides preserve
prior boundaries; no general format conjunction or rawUUID equivalence claimed.
6600actualZod/AJV comparisons across300exports+13controls and2200native comparisons
across100schemas pass, including explicit featureoff rejection and pinnedAda3.2.4.
StrictTS7.0.2 12roots/621exports; email1799,URI203261+90/injectedTS-only,pure577+246,
literal92880+20,SDK20 regressions pass. Frozen5 manifest
/private/tmp/fl-factory-string-source.json verified; logs /private/tmp/fl-factory-
string-composition.log,/private/tmp/fl-factory-string-types.log and named regression
logs. Root refreshed source locations/checked inventory621/1699; no exports/deps/
locks/native source changes. CI native fixture runs within pinnedNode24.2 URI group.
Broad schema/mapper/plugin/deployment gates remain open;660/682(96.8%),22open.


### Actor-owned completed PaymentLink library read (2026-10-14)

get_bound_created_payment_link adds a direct completed-creation getter with opaque
joined actor/tenant/scope/API/canonicalhash/receipt/localbody/alias-generation proof.
Original normalized trustedPrices must match, managed sealed payload rejects before
provider I/O. Fresh pinned Account/Balance and exact PaymentLink id/kind/mode GET
replace only providerFields; public UUID/local amount/status remain unchanged,
including active=false provider responses. No line-item/price/payment/lifecycle or
entitlement inference, POST/claims/publisher/cache mutation or administrative-only
alias fallback. Existing administrative scoped getter remains compatible.
Read evidence is bounded before additional cloning (32depth/8192nodes/256KiB worst
case), repeated after provider and audit waits, with final postcommit deadline.
Data linearizes at last matching SELECT, not guaranteed unchanged afterward.
Older reserve_payment_link_creation still canonicalizes before size check; this
separate hardening gap is not silently changed by the getter.
Frozen9 /private/tmp/fl-created-payment-link-read-final.json verified, SHA256
e81c76c4b56407fbcf245955b08374b89b67a2ddc7a76d6df19bed7f188e184a.
New restrictedPG matrix5.53s,legacydirect/managed2tests10.67s,originalcreation/
uncertainCOMMIT17.80s,strictbase+Stripeallfeature/alltargetClippy12.49s pass.
ActualTS24cache-read cases/fullconfigured oracle pass. DeniedauditINSERT yields
zero provider calls; actor/tenant/profile/admin-only/managed/oversize/GETmutation/
auditexpiry/localbody cases pass. Immutable journal no-op mutation remains denied.
Ownedroles0. Logs /private/tmp/fl-created-payment-link-read-{final,legacy,creation,
clippy-final}.log; actual /private/tmp/fl-payment-link-read-audit-ts.log and
/private/tmp/fl-payment-link-provenance-audit-configured.log. No new route/migration/
manifest or reservation behavior; aggregate selection remains separate.660/682(96.8%).


### Trusted post-password-reset observer (2026-10-14)

with_password_reset_observer implements the bounded onPasswordReset subset:
prepare and owned future are always awaited after committed password/proof state,
before optional session revocation, outside DB transactions. Background dispatcher
is bypassed. Sync/future/projection failures return sanitized empty500 and preserve
sessions. Reentrant callback can use the new password; its newly created session
is subsequently revoked when enabled. Native cancellation may leave revocation
unperformed with committed password/consumed proof; no JavaScript cancellation
semantics claim. Blocking callbacks/panics are not intercepted.
Opaque context has only bounded zeroized public user_json, no Debug/Serde/password/
token/session/raw Request. Projection validates before cloning/serialization;
context snapshot captured before password update and projection error delayed
until after commit. No-hook path skips projection. Arbitrary user fields and
upstream optional Request are outside this supported subset.
Actual6,focused3(34.43s),restrictedPG1(135.28s),affectedreset11(34.57s),strict
allfeature/alltarget17.62s+minimal3.84s pass. Ownedroles/records/audits0.
Frozen11 /private/tmp/fl-reset-observer-freeze.json verified, SHA256
ee2b9da2af7c5db832dcaf2e02752d5afca1317f8d7a6278e49ed85b9fb79e97.
Logs /private/tmp/fl-reset-hook-actual.log and /private/tmp/fl-reset-observer-
{native,pg,regression,clippy,minimal-clippy}.log. Final source-only rustfmt reexport
ordering followed green checks; no deps/locks. Broader auth hooks/provider/plugin
gates remain open;660/682(96.8%),22open unchanged.

### 2026-10-14 — Current integration checkpoint

Committed reserved Self model names (2f93d3915), aggregate PaymentLink read
HTTP/MCP/two SDKs (6f77f4eb6), awaited email-verification hooks (624cadc97),
and pre-clone PaymentLink reservation admission (23ec69119). Detailed behavior,
limitations and validation receipts are in the implementation checklist and
security/auth reviews. Current public source inventory remains621entries/1699
records across44packages with0unresolved; strict TypeScript7.0.2 validates all
12roots/621exports (/private/tmp/fl-current-configured-types.log).

Immutable source6f77f4eb616d125b4706fda8690b5e302a9d7b6b packages2832regular files,
4959058bytes, SHA256e4a2d118a72c9c028971b37c0d05403e9082ea038255fb11d04520ecfeafb9a5.
Offline locked installation, git-free repackage listing, PEM fixture allowlist and
extracted bytes passed. Retained cli/target/parity-artifacts/
forklaunch-6f77f4eb6-e4a2d118.crate. Portable runtime validation is IN PROGRESS;
this receipt does not claim its completion. This source excludes later verification
hooks/admission and uncommitted builtin-name/auto-signin/list work.
Checklist660/682(96.8%),22broad gates remain open.

### 2026-10-14 — Rust model builtin-name compatibility

Standalone model generation now qualifies standard-library and Serde references,
so public names such as String, Vec, Option, Box, BTreeMap, Result, Default,
Serialize/Deserialize and Some/None/Ok/Err no longer shadow generated dependencies.
Model names and TypeScript outputs stay unchanged; Rust type-expression metadata
now uses absolute paths resolving to the same types. Shared HTTP/socket consumers
use qualified Serde derives, and textual generated-source assertions are updated.
Existing transport reserved-name rejection remains unchanged.
Strict generated models609cases/126models,5 prior TS byte-identical controls,
real HTTP/socket consumers and optional-body example pass. Codegen49/SDK Rust5,
fixed tuples79/variable920/Self144, strict all-feature/all-target Clippy and
full CLI1128 pass. Fourteen-path manifest /private/tmp/fl-builtin-source.json
verified; logs /private/tmp/fl-builtin-{models,units,fixed,variable,self,clippy,cli}.log.
No dependency/lock changes. Broader schema gates remain open;660/682(96.8%),22open.

### 2026-10-14 — Immutable6f77 packaged runtime PASS

Exact source6f77f4eb616d125b4706fda8690b5e302a9d7b6b passed offline locked package/
installation, git-free repackage listing, relocated generated services/modules/
four worker variants, OpenAPI/contract/AsyncAPI exports, strict generated Rust/
TypeScript SDK consumers and native recursive generators. Real HTTP/MCP startup,
initialization/discovery/health, custom/disabled endpoints, shared/separate
listeners, explicit/ephemeral ports and +2000 fallback, atomic startup rejection
and graceful shutdown pass. All five capability callbacks pass signed
TypeScript interoperability and rejection/replay checks. Packaged IAM207pass/
3PostgreSQLignored (144.44s); PostgreSQL has separate working-source receipts.

All2832regular source files match retained archive before and after execution.
Archive4959058bytes SHA256e4a2d118a72c9c028971b37c0d05403e9082ea038255fb11d04520ecfeafb9a5,
cli/target/parity-artifacts/forklaunch-6f77f4eb6-e4a2d118.crate. Metadata and full
runtime evidence: /private/tmp/fl-oct14-payment-link-checkpoint.json and
/private/tmp/fl-oct14-payment-link-runtime.log; independent byte check via
/private/tmp/fl-oct14-payment-link-verify.py passed. Excludes later verification
hooks, admission, builtin-name qualification and ongoing drafts by exact source
commit. Local receipt only; no public publication or hosted/cloud validation claim.
Checklist660/682(96.8%),22broad gates remain open.

### 2026-10-15 — Completed immutable workspace cleanup

After committed runtime receipt9b84f9a33, removed the exact owned6f77 checkpoint
and generated archive roots (212646681 logical bytes), then 182 exclusive
nonexecutable cache files in 80 groups (417385757 logical bytes).
Total 630032438 logical bytes; no APFS physical-space claim. Guards verified
exact owned roots, no symlink/open-file/active-process use, current-source cache
exclusion and retained archive SHA256 before/after. Executables, live workspaces
and active agent caches preserved. Evidence /private/tmp/fl-oct14-payment-link-
cleanup.log and /private/tmp/fl-oct14-payment-link-cache-cleanup.{json,log}.


### 2026-10-15 — explicit Zod length and numeric-check profile

`openapi2020(schema, { zodChecks: true })` emits the versioned reserved
`x-forklaunch-zod-checks` capability. UTF-16 length and ordered Zod numeric
remainder checks now retain their actual source semantics in native validators,
UniversalSdk and generated socket clients. Ordinary explicit length/multipleOf
annotations override the corresponding synthesized assertions; the reserved
keyword cannot be supplied through annotations. Default schema export is unchanged.
Native consumers opt into the `zod-checks` Cargo feature; socket CLI compilation
uses `FORKLAUNCH_SDK_ZOD_CHECKS=1`. Feature-off compilation rejects the capability
in schema positions, including referenced targets, while preserving literal data.

The source fixture passed 2,304 baseline observations, 2,220 exact opted-in
Zod/AJV comparisons and 3,332 native observations. The moved shared numeric kernel
passed 192,816 actual-Zod comparisons. Generated consumer checks cover five real
HTTP responses and twenty TS/Rust socket encode/decode outcomes, strict generated
compilation and feature-off rejection without output. Native tests, strict Clippy,
1,129 CLI tests and both workspace lock checks passed. Only the shared common
crate dependency edge to the existing pinned `ryu-js` changed in tracked locks.

This profile does not close the schema gate: regex and mixed-format conjunctions
remain open. Native strings cover representable Unicode, excluding lone UTF-16
surrogates. Numeric comparisons use finite IEEE-754 values. Rules are capped at
10,000; length bounds must be nonnegative safe integers. Unprojectable combinations
of string transformations and synthesized length checks fail explicitly. AJV
registration preserves supplied engine options; a shared engine is reusable within
one helper module instance, while separately bundled helpers require separate
engines. CI runs both permanent fixtures, `rust_zod_checks.mjs` and
`rust_zod_checks_consumers.mjs`. The checklist remains 660/682 (96.8%).


Canonical revalidation after the profile integration passes all 366 live outcomes
and zero normalized OpenAPI differences. Three stale nullable-union snapshots
were refreshed to the already-committed emitter's `anyOf` output. Normalization
flattens only unique unconstrained string/number/boolean/null alternatives;
overlapping number/integer, constrained branches and sibling assertions remain
intact. Negative controls verify those exclusions. Independent integration checks
also passed the 623-entry/1,703-record inventory, all 12 configured TypeScript roots
and 623 public exports, workflow YAML parsing and whitespace validation.


### 2026-10-15 — shared authentication session policy

Additive `SessionPolicy` builders propagate configured expiry, refresh age,
freshness and refresh disabling across credential, Google, MFA, verification,
organization and direct-session flows. Existing constructors retain the one-day
ForkLaunch preset; whole-second positive expiry is explicit. Persistent cookie
Max-Age follows policy, while transient credential and pending-factor grants keep
the upstream fixed one-day stored TTL. Rotation/enrollment/verification creation
uses configured expiry even with transient cookies. Existing stored expiry is
preserved until refresh, with equality boundaries and policy changes tested.

Actual Better Auth comparisons pass 22 clock/creation cases; three focused native
tests cover the policy and HTTP creation families. The full IAM suite passes
214 tests (three PostgreSQL tests ignored in that run). The separately executed
restricted PostgreSQL lifecycle passes in 164.26 seconds, including the new domain
and HTTP policy matrices. Its first run exposed a shared-fixture email collision;
a unique seed fixed the fixture, without production changes. Full and minimal
strict Clippy pass. CI runs the permanent actual-source policy fixture and existing
full/native PostgreSQL suites.

Direct services retain caller-supplied `now`; AuthHttp currently samples the
request clock before asynchronous credential work. Frozen-clock comparisons prove
arithmetic and option selection, not post-await wall-clock equivalence. Resampling
at upstream creation points remains work alongside deferred refresh, cookie cache,
secondary storage, broader credential/organization options and plugin coverage.
No broad checklist gate is closed by this feature group.


### 2026-10-15 — ordered email, UUID and URI assertions

The explicit Zod-check profile now preserves conjunctions of email, UUID and URI
checks instead of losing earlier checks to the schema's single format field.
Native email/UUID predicates share the established validation kernels; URI demand
requires the existing `zod-uri` capability and pinned Ada behavior. Synthesized
assertions respect ordinary explicit metadata overrides. Arbitrary regex patterns
remain present and are not claimed equivalent by this feature. Transformations
combined with synthesized format assertions fail explicitly.

The final matrix passes 30,564 actual Zod/AJV comparisons and 30,564 native
comparisons in ordinary and pure-input opt-in modes. Default-export byte-equivalence
controls remain green. Nine actual HTTP responses and 36 generated TS/Rust socket
codec outcomes pass, including impossible conjunctions. Native common/contract/
validation tests (10/30/71), 81,036 existing format-kernel comparisons, prior
length/remainder matrices, strict generated compilation, strict Clippy and all
12 configured TypeScript roots pass. Canonical validation again passes 366 live
outcomes with zero normalized OpenAPI differences. No dependencies or lock versions
changed. CI pins Node 24.2.0/Ada 3.2.4 for the format and consumer fixtures and checks
URI rejection when `zod-checks` is enabled without `zod-uri`.

Regex and other format families remain open; this closes the named conjunction
behavior, not the broad schema gate. The source inventory remains 623 entries and
1,703 declaration records across 44 packages. Session-policy PostgreSQL cleanup
also confirms zero owned fixture roles, records and audit rows after its passing run.


### 2026-10-15 — PaymentLink creation HTTP, MCP and SDK surfaces

An explicit `paymentLink` selector exposes direct, actor-bound creation through
POST `/bound-payment-links` and historical IDs-only inspection through GET
`/bound-payment-link-operations/:operationId`. The existing immutable journal binds
operation UUID, actor, tenant, trusted profile and canonical request. Trusted prices
and bounded typed line items are required; callers cannot supply raw provider
options or publication state. Declared amount is not charged-total proof.

Completed initial/repeated operations return 200 with IDs only. Uncertain outcomes
return 202 and may already have provider/local effects; pending repeats do not
redispatch. Inspection neither revives URLs nor asserts current provider ownership.
Creation authority is separate from read selection. Startup checks each required
local privilege individually with no provider I/O; creation needs no record/binding
SELECT or sensitive-read audit INSERT. Base, backup and managed transports reject
this direct profile. Existing 66 default routes remain unchanged.

The final restricted PostgreSQL test passes real HTTP/MCP and two calls from each
generated TS/Rust SDK in 2.74 seconds. It covers individual missing grants,
invalid inputs before provider calls, actor/payload conflicts, integral JSON-float
normalization, completed/pending repeats and IDs-only inspection. Ten units, 104
rendered artifact/startup cases including compiled base/Stripe binaries, strict
all-feature billing Clippy and feature-off Axum Clippy pass. Owned test roles return
to zero. CI uses a direct-executable wrapper to release Cargo's build lock before
nested Rust SDK compilation. Plan/Subscription creation surfaces, managed creation
and broader mutation/reconciliation semantics remain open.


### 2026-10-15 — identifier and Base64 Zod assertions

The explicit Zod-check profile now preserves CUID, CUID2, ULID, Nano ID, Base64
and Base64url checks across native schema validation, UniversalSdk and generated
socket clients. Pure native predicates follow the actual Zod grammar: CUID counts
UTF-16 units and ECMAScript whitespace, CUID2 has no invented generator length,
ULID does not add a timestamp-overflow restriction, and Base64 syntax does not
require canonical pad bits. URL-safe Base64 retains optional padding. Ordered
conjunctions and explicit format overrides follow the existing profile. Default
schema exports remain byte-equivalent; string transformations with synthesized
checks are rejected explicitly.

The permanent `rust_zod_identifier_checks.mjs` fixture passes 66,624 actual Zod/AJV
comparisons and 66,624 native observations, including Unicode/whitespace, padding,
length boundaries, nested/optional/union schemas, combined formats and inputs up
to one MiB. Lone surrogate controls are JS-only because Rust strings cannot
represent them. Generated consumers pass 15 actual HTTP responses and 60 TS/Rust
socket encode/decode outcomes, strict compilation and feature-off rejection.
The prior ordered-format matrix still passes 30,564 comparisons on both sides.
Strict all-target/all-feature Clippy for common/contract/validation/both SDK crates,
all 12 configured TypeScript roots and 623 exports, and the regenerated source
inventory pass. No dependency or lock changes were needed.

Raw date/time, duration, IP/CIDR, JWT and emoji checks and arbitrary regex semantics
remain separate work. The broad schema gate and 660/682 checklist count are unchanged.


### 2026-10-15 — immutable d7bd packaged-runtime checkpoint

Exact commit `d7bd2a7d85e8657d9475d9b8deb18d1d0bd4fea4` passes offline locked CLI
packaging, installation and repackage-list checks. The retained crate contains
2,858 regular files and is 5,005,037 bytes, SHA-256
`eb809c04bb1c101af5750966c63b7a4ae08373cb6cf0cb0ede48ee2d81d1cf71`.
All regular source bytes were verified against the retained archive before and
after runtime testing; the existing intentional PEM allowlist/bytes also pass.
The CLI lock hash remains
`d66f3120b6fab6af185dfa4e22c614b4bf5324370aede8667c9807e3e5bc5110`.

The installed CLI generates and relocates the application, service, all four
worker families and selected blueprint modules. Workspace compilation, all component
OpenAPI/contract exports, AsyncAPI/socket imports, strict TS/Rust SDK generation
and regeneration, and bundled native generators pass. Actual HTTP/MCP checks pass
initialization/discovery/health, enabled/disabled/custom endpoints, shared/separate
listeners, explicit/ephemeral ports, the +2000 fallback, atomic startup rejection
and graceful shutdown. All five generated capability callbacks pass actual-TS
signature and forgery/staleness/replay/redelivery checks. Bundled IAM passes
214 tests with three PostgreSQL-only tests intentionally ignored; restricted PG
proof is recorded separately above. The generated app resolves its own dependency
graph; the locked offline claim applies to CLI package/install operations.

Retained archive: `cli/target/parity-artifacts/forklaunch-d7bd2a7d8-eb809c04.crate`.
Local evidence: `/private/tmp/fl-oct15-policy-format-checkpoint.json` and
`/private/tmp/fl-oct15-policy-format-runtime.log`. Credential/TTL and Plan route
drafts, regex prototype and the later six identifier/Base64 assertions are excluded
from this immutable checkpoint. It does not prove cloud deployment or public
registry publication. Full parity remains 660/682 (96.8%).


After committing that checkpoint, guarded cleanup removed its two exact owned
workspace roots (213,691,674 logical bytes) and 195 exclusive non-executable cache
files in 86 groups (421,193,548 logical bytes). Total: 634,885,222 logical bytes;
no physical APFS reclamation claim. Guards require completed source verification,
no symlink/active process/open file, unchanged cache inode/size/mtime and retained
archive hash before/after. Current source caches, executables, unrelated worktrees
and the shared PostgreSQL fixture are preserved. Logs:
`/private/tmp/fl-oct15-policy-format-cleanup.log` and
`/private/tmp/fl-oct15-policy-format-cache-cleanup.json`.


### 2026-10-15 — Plan creation HTTP, MCP and SDK surfaces

A separate trusted `plan` selector exposes POST `/bound-plans` and historical
GET `/bound-plan-operations/:operationId`, without granting creation through
`planRead`. Typed inputs admit an allowlisted product and finite per-unit licensed
Plan configuration. Integral minor-unit amounts are bounded by the JavaScript safe
integer maximum, including normalized integral JSON floats. Authenticated product
read and exact Plan readback precede durable publication. No supplied public ID,
raw Stripe options, nested product creation or entitlement claim is accepted.

Actor/tenant/profile/request-bound journal semantics preserve completed duplicates,
202 uncertainty and pending repeats without another provider POST. Creation and
inspection return IDs only; inspection asserts neither current provider ownership
nor retry authority. Startup checks individual ordinary-role journal/record/binding
and audit privileges without provider I/O. Base/backup/managed transports reject the
profile. No separate publisher is invented; existing 66 default routes remain.

Restricted PostgreSQL, real HTTP/MCP and two calls from each generated SDK pass
in 2.87 seconds. Eleven unit tests, 120 rendered artifact/startup cases including
base/Stripe binary compilation, strict all-feature billing Clippy and feature-off
Axum Clippy pass. Owned fixture roles return to zero. The initial compile found a
metadata integer-literal type error, corrected before execution. Runtime wrappers
now retain Cargo's rendered JSON diagnostics on failure; all seven wrapper syntax
checks pass. The final ten-source manifest is
`/private/tmp/fl-plan-creation-route-final.json`; logs use
`/private/tmp/fl-plan-creation-route-{live-final,unit,artifacts,clippy,minimal}.log`.
Subscription creation, managed creation and broader mutations remain open.


### 2026-10-15 — Trusted credential policy and token lifetimes

Added bounded native credential policy for disabling signup and UTF-16 password
lengths, with upstream zero fallback, minimum-first new-password checks and
maximum-only existing-password proofs. Effective limits are 1..4096 code units.
Trusted whole-second reset and verification TTLs reach existing issuance paths;
reset zero falls back to 3600 seconds while verification zero is preserved.
HTTP request validation still precedes handler policy. No request can select policy.

Actual Better Auth fixture: 17 observations passed. Focused native tests: 3 passed;
full IAM: 217 passed, 3 PostgreSQL tests separately ignored; restricted PostgreSQL
proof passed in 176.66 seconds and owned roles, records and audit rows returned to
zero. Strict full and minimal Clippy passed. Frozen 20-source manifest:
`/private/tmp/fl-credential-policy-freeze.json`; evidence logs:
`/private/tmp/fl-credential-policy-{actual,native,full,pg,clippy,minimal-clippy}.log`.
Change-email delivery and delete-user remain disabled. Fresh clock sampling after
awaited work is a separate pending slice; unrestricted callbacks, broader plugins
and full Better Auth parity remain open. Checklist remains 660/682 (96.8%).


### 2026-10-15 — Raw Zod date assertion

The existing explicit `zodChecks` profile now preserves `.date()` through the
TypeScript emitter, owned and generated AJV helpers, and native contract validation.
The shared Rust predicate implements exact ASCII four-digit Gregorian syntax,
including year zero and century leap-year boundaries. Existing metadata override,
ordered conjunction, transformation rejection and feature-off behavior remain.
Default export is unchanged; this adds no Date coercion or datetime/time support.

The permanent `rust_zod_date_checks.mjs` fixture passes 70,000 actual Zod/AJV and
70,000 native observations, covering February in all 10,000 years, invalid month/day
boundaries, Unicode/control suffixes, nesting and metadata overrides. Live owned
UniversalSdk responses pass 16 checks; generated TypeScript/Rust socket consumers
pass 64 checks with strict generated-source compilation and feature-off rejection.
Strict Clippy passes all targets/features for common, contract, validation and both
SDK crates. Configured TypeScript passes all 12 roots and 623 exports; refreshed
public API inventory passes 623 entries/1703 declarations. Logs:
`/private/tmp/fl-zod-date-{native,consumers,clippy,types,inventory}.log`.
Regex and further format/configuration parity remain open; 660/682 is unchanged.


### 2026-10-15 — Subscription creation HTTP, MCP and SDK surfaces

The independent `subscription` selector now exposes POST `/bound-subscriptions`
and historical GET `/bound-subscription-operations/:operationId`. It explicitly
selects `internal_service_tenant_v1` authority: customer association is a trusted
service assertion, not proof of end-user ownership. Typed requests admit existing
local Customer/Plan IDs, safe-integer positive quantity and bounded description.
Plan creation provenance and provider evidence remain required. A separately
constructed `TrustedSubscriptionPublisher` is mandatory; the existing constructor
remains compatible and cannot silently acquire this authority. Generated binaries
wire the explicit publisher pool. Both roles' individual startup privileges are
checked without provider I/O. Read-only selectors gain no creation capability.

Restricted PostgreSQL, live HTTP/MCP and four generated SDK calls pass in 3.32s.
Completed operations leave zero claims; uncertain outcomes retain four claims and
return 202 without a second provider POST. Actor-bound inspection returns historical
IDs only, with no recovery/retry/current-ownership assertion. Twelve unit tests,
136 rendered artifact/startup cases including base/Stripe binary compilation,
strict full/all-target billing Clippy and minimal Axum Clippy pass. Owned fixture
roles/databases return to zero. Test closure ownership and a parser lint were fixed
before final live/artifact reruns. Final 12-source manifest:
`/private/tmp/fl-subscription-creation-route-final.json`; logs:
`/private/tmp/fl-subscription-creation-route-{live-final2,unit,artifacts-final,clippy-final,minimal}.log`.
Default 66 routes remain unchanged. Managed creation, broader mutations and the
full billing gate remain open; checklist stays 660/682 (96.8%).


### 2026-10-15 — Raw Zod duration assertion

Explicit `zodChecks` now preserves raw `.duration()` across ordinary/pure exports,
owned/generated AJV and native validation. Native parsing performs a linear byte
scan with no numeric conversion or backtracking: optional overall/component signs,
ordered date/time units, integer components and terminal dot/comma fractions match
the pinned actual Zod grammar. Metadata overrides and transform rejection remain;
default export is unchanged. This is lexical validation, not duration arithmetic.

Permanent `rust_zod_duration_checks.mjs` passes 34,626 actual Zod/AJV comparisons
and 34,626 native observations, including component permutations, signs, fractions,
control/Unicode inputs, deterministic mutations, nested schemas, overrides and
million-digit inputs. Live owned HTTP responses pass17, generated TS/Rust socket
checks pass68 with strict source compilation and feature-off rejection. Strict
all-target/all-feature Clippy for five affected crates, configured TypeScript all
12 roots/623 exports, and public API inventory623/1703 pass. Logs:
`/private/tmp/fl-zod-duration-{js,native,consumers,types,clippy}.log`.
Time/datetime options, regex and other format gaps remain; 660/682 unchanged.


### 2026-10-15 — Raw Zod time precision assertions

The explicit versioned check profile now carries ordered `timePrecisions` rules
through the emitter, owned/generated AJV and native validation. Null precision
allows optional seconds and fractions; zero allows optional integer seconds;
positive integers through 1000 require exact fractional digits. Native linear
parsing deliberately preserves actual Zod's repeated seconds-group acceptance for
positive precision. Negative, fractional, nonfinite and larger configurations fail
admission. Per-list and aggregate 10,000-rule limits cover the new field. Explicit
format overrides and transform rejection remain; default export is unchanged.

Permanent `rust_zod_time_checks.mjs` passes 108,336 actual Zod/AJV and 108,336
native observations, including all hour/minute boundaries, precision 0/1/2/3/9/1000,
optional/repeated seconds, Unicode/control suffixes, million-digit fractions,
conjunctions, nested schemas and invalid-profile rejection. Live HTTP checks pass
19; generated TypeScript/Rust socket checks pass 76 with strict generated-source
compilation and feature-off rejection. Strict all-target/all-feature Clippy across
five affected crates, configured TypeScript all 12 roots/623 exports, and refreshed
public API inventory 623/1703 pass. Evidence logs:
`/private/tmp/fl-zod-time-{js,native,consumers,types,clippy}.log`.
Datetime options, regex and other constraints remain open; checklist 660/682 unchanged.


### 2026-10-15 — Raw Zod datetime options

Explicit `zodChecks` now carries ordered `dateTimes` rules with null/0..1000
fractional precision and boolean local/offset options. Native validation composes
exact Gregorian date/time predicates with Zod's lexical timezone grammar, preserving
optional offset colon and unrestricted two-digit offset fields such as +99:99.
Local timestamps and repeated seconds groups follow configured actual Zod behavior.
The explicit projection removes its superseded ordinary date-time assertion; default
export and explicit metadata overrides are unchanged. New rules count toward the
aggregate 10,000 limit; malformed options/profiles fail rather than being ignored.

Permanent `rust_zod_datetime_checks.mjs` passes 86,592 actual Zod/AJV comparisons
and 86,592 native observations across precision/local/offset combinations, Gregorian
boundaries, malformed/control/Unicode values, nested/conjunctive schemas, override
and admission checks. Live owned HTTP checks pass 20; generated TypeScript/Rust
socket checks pass 80 with strict source compilation and feature-off rejection.
Strict all-target/all-feature Clippy for five affected crates, configured TypeScript
12 roots/623 exports, and public API inventory 623/1703 pass. Logs:
`/private/tmp/fl-zod-datetime-{js,native,consumers,types,clippy}.log`.
Regex, remaining formats and broader constraints remain open; 660/682 unchanged.


### 2026-10-15 — Raw Zod IP and CIDR assertions

Explicit `zodChecks` now preserves `.ip()` and `.cidr()` with default, v4 and v6
selection through the emitter, owned/generated AJV and native contract validation.
Native predicates reproduce the pinned Zod grammar, including legacy mixed address
syntax, lowercase mapped-prefix rules, two-digit embedded octets with leading zero,
and permissive fe80 zone groups. CIDR checks reject leading-zero prefix widths and
preserve host bits. Version admission, metadata overrides, conjunctions and transform
rejection are explicit. Default export remains unchanged. Native parsing uses no
new dependency or unbounded collections and performs no output normalization.

Permanent `rust_zod_ip_checks.mjs` passes 244,444 actual Zod/AJV comparisons and
244,444 native observations, including IPv4 octet boundaries at every position,
compressed/mixed IPv6 forms, zones, every relevant CIDR width, Unicode/control and
million-character inputs, nesting and overrides. Live HTTP checks pass 22; generated
TypeScript/Rust socket checks pass 88 with strict generated-source compilation and
feature-off rejection. Strict all-target/all-feature Clippy for five affected crates,
configured TypeScript 12 roots/623 exports, and inventory 623/1703 pass. An initial
iterator compile error was corrected; Clippy then required extracting the unchanged
format-list parser, followed by a passing final native differential rerun. Logs:
`/private/tmp/fl-zod-ip-{js,native-final,consumers,types,clippy-final}.log`.
Regex, JWT/emoji formats and broader constraints remain open; 660/682 unchanged.


### 2026-10-15 — Subscription recovery HTTP, MCP and SDK surfaces

An independent `subscriptionRecovery` selector exposes POST
`/bound-subscription-operations/:operationId/recover` with exact `{eventId}` and
actor-owned historical IDs inspection. It requires explicit internal service tenant
authority and a checked separate publisher. Recovery-only configuration exposes no
create route and requires journal/claim SELECT, audit and dependency locking grants
without runtime journal/claim INSERT. Combined explicit selectors share the publisher
and deduplicate inspection. Exact authenticated creation-event request-key, API
version, account/mode, time window and current Customer/Plan/Subscription evidence
feed the existing atomic recovery library. Recovery never issues a provider POST.

Live restricted PostgreSQL, HTTP/MCP and four SDK calls pass in 3.55 seconds; SDK
calls exercise completed historical replay/inspection, not four new recoveries.
Malformed event evidence retains four claims; concurrent valid recovery publishes
once and leaves zero claims. Creation regression passes in 3.58 seconds. Thirteen
unit tests, 152 rendered artifact cases, strict billing all-feature/all-target and
minimal Clippy pass. Owned fixture roles/databases return to zero. An initial fixture
actor mismatch was corrected to the real HMAC principal; production was unchanged.
Frozen 12-source manifest: `/private/tmp/fl-subscription-recovery-route-final.json`.
Logs: `/private/tmp/fl-subscription-recovery-route-{live4,unit,artifacts,clippy,minimal}.log`
and `/private/tmp/fl-subscription-recovery-creation-regression.log`. Managed recovery
and broader billing mutation semantics remain open; 660/682 is unchanged.


### 2026-10-15 — Raw Zod emoji assertions with pinned Unicode data

Explicit `zodChecks` now preserves `.emoji()` using the Unicode16 union of
Extended_Pictographic and Emoji_Component. A checked generator derives 84 merged
ranges for Rust and both TypeScript helpers; runtime acceptance is independent of
the host Unicode version. The actual reference pins Node24.2/Unicode16. Individual
components, including digits and joiners, intentionally pass without claiming valid
grapheme sequences. Unicode source data, SHA provenance and License V3 are retained
in `forklaunch-common/data/unicode-16.0.0`; offline locked Cargo package listing
confirms license/data/generated Rust table inclusion. No dependencies changed.

Permanent `rust_zod_emoji_checks.mjs` passes 2,224,436 actual Zod/AJV comparisons
and 2,224,436 native observations, covering every Unicode scalar, components,
sequences, nested/conjunctive schemas and metadata overrides. Final generated-table
TypeScript rerun passes the same 2,224,436 cases. Live HTTP checks pass 23; generated
TypeScript/Rust socket checks pass 92 with strict compilation and feature-off
rejection. Strict all-target/all-feature Clippy for five affected crates, configured
TypeScript 12 roots/623 exports, and inventory 623/1703 pass. Initial generated
integer-literal style diagnostics were fixed in the generator and final Clippy passed.
Evidence: `/private/tmp/fl-zod-emoji-{native,js-final,consumers,types-final,clippy-final,package-list}.log`.
Regex, JWT format and broader schema constraints remain open; 660/682 unchanged.


### 2026-10-15 — Fresh HTTP clock sampling across auth grants

A private optional time source now propagates the configured HTTP clock through
credentials, sessions, Google, MFA and initial organization session checks. Direct
service APIs retain their explicitly supplied timestamp. Both clock/factor builder
orders bind the same clock. Session expiry/refresh checks sample after authoritative
reads; grants sample after awaited hashing/delivery/provider/store work. Password
and MFA grant rotation recheck authority before mint/commit, preserving the existing
challenge/session equality rules and independent async OTP/backup lease clocks.
Late writes cannot renew authority that crossed its old expiry during the wait.
This strengthens selected native race boundaries; it does not promise a deadline
after COMMIT or exact upstream race behavior.

Actual delayed-work oracle: 3 cases passed. Pre-review focused clock test passed;
full IAM 218 passed/3 ignored in 166.38s; restricted PostgreSQL passed in 215.96s
with owned roles/records/audit rows zero. Root review then found an old supplied-time
load in externally supplied change-email proof application. Corrected final source
passes its dedicated delayed-writer test and all 8 affected email regressions, plus
strict full/all-target and minimal Clippy. The earlier full/PG runs are retained as
pre-correction evidence, not represented as reruns of the final source.

Final 34-source manifest: `/private/tmp/fl-clock-final-freeze.json`. Original logs:
`/private/tmp/fl-clock-{actual,native,full,pg,clippy,minimal-clippy}.log`; final review
logs: `/private/tmp/fl-clock-review-{native,email,clippy,minimal-clippy}.log`.
Secondary organization transaction rechecks remain a concrete follow-up, as do
broader auth options/plugins. No broad gate closes; checklist remains 660/682.


### 2026-10-15 — Raw Zod JWT structure and header policy

Explicit `zodChecks` now carries ordered `jwtAlgorithms` rules through ordinary/pure
exports, owned/generated AJV and native contract validation. Native header decoding
preserves JavaScript atob Latin1 semantics and permissive unused base64 pad bits; an
iterative JSON reader handles duplicate keys, escaped lone UTF16 units, overflowing
and underflowing numeric truthiness, and deeply nested containers without recursive
call-stack growth. Optional typ must equal JWT, alg must be truthy, and a nonempty
configured algorithm must match exactly. Null/empty algorithm permits any truthy
alg, as upstream does. Payload syntax and optional empty signature are preserved;
no cryptographic signature, payload decoding or claim verification is introduced.

Permanent `rust_zod_jwt_checks.mjs` passes 83,494 actual Zod/AJV comparisons and
83,494 native observations, including 20,000-deep JSON, byte mutations, surrogate
escapes, numeric extremes, metadata overrides and per-list/aggregate rule admission.
Live owned HTTP checks pass 25; generated TypeScript/Rust socket checks pass 100
with strict source compilation and feature-off rejection. Strict all-target/all-
feature Clippy across five affected crates, configured TypeScript 12 roots/623
exports, and public API inventory 623/1703 pass. No dependencies changed. Logs:
`/private/tmp/fl-zod-jwt-{native-final,consumers,types,clippy}.log`.
Arbitrary regex, broader transforms/configuration and schema constraints remain open;
this does not close the validator gate. Checklist remains 660/682.


### 2026-10-15 — Organization transaction clock rechecks

All 23 secondary organization session loads now use the configured HTTP clock
and session policy after awaited authority reads. Direct explicit-time APIs retain
their supplied timestamp. This covers existing organization, member, invitation,
team, role and active-selection rechecks. Exact expiry equality remains valid;
this is an authorization snapshot, with no post-commit expiry guarantee.

The focused delay matrix passes 24 cases, including unchanged graph/selection
state on rejection, explicit-time behavior and real HTTP dispatch. Full IAM passes
220 tests (3 ignored); the separate restricted PostgreSQL test passes in 218.44s.
Strict all-feature/all-target and minimal Clippy pass. Owned PostgreSQL test roles,
records and audit rows were cleaned. The full suite also covers the preceding
email clock correction. No dependencies or locks changed.

Frozen 26-source receipt: `/private/tmp/fl-org-clock-freeze.json`, SHA256
`43479e6f5d3280240173edcacd413c7acb265ce03ab75f4ffc56ce5ec01a14ed`.
Logs: `/private/tmp/fl-org-clock-{native,full,pg,clippy,minimal-clippy}.log`.
Existing IAM native/PostgreSQL CI runs these tests. Broader organization options
and auth parity remain open; checklist remains 660/682 (96.8%).


### 2026-10-15 — Bound Portal local expiration

An explicit `portalLocalExpiration` selector adds actor/profile-bound expiration
and historical status over HTTP/MCP and both generated SDKs. It grants no creation
or provider authority. First expiration verifies the completed creation receipt,
unchanged Customer/binding/record evidence and original five-minute lifetime.
Migration16 appends an immutable local tombstone with audit in one transaction;
all stored bound reads consult it, so restoring a mutable row cannot restore access.
No provider call is made; an already issued Stripe URL may remain usable remotely.

Confirmed commit/repeat returns `localExpired`; lost acknowledgement returns202
`localExpirationUnknown`. Historical status/repeat remains actor/profile-bound;
`notRecorded` is not rollback proof for an in-flight uncertain transaction. Existing
Portal read deployments require the new table SELECT grant at startup, independently
of expiration INSERT permission. The migration owner remains trusted.

Restricted PostgreSQL domain matrix passes3.22s, including migration15-to16/rerun,
immutable/RLS history, concurrency, audit rollback, stale evidence, blocked-audit
expiry/Customer locks and actual dropped COMMIT acknowledgement. Stored-read
regression passes2.88s. Final live HTTP/MCP wrapper passes3.82s with ten SDK calls
and old-selector missing-SELECT startup rejection. Units14, artifacts166, strict
full20.04s/minimal7.50s Clippy pass. Owned test databases/roles are clean. CI now
runs the new domain matrix alongside the expanded existing Portal runtime wrapper.

Frozen14-source manifest `/private/tmp/fl-portal-local-expiration-final.json`, SHA256
`cb8d27b3559ad8c4f6a8ac8fd0b924851af48500afdab81654c2cfc26b4060d2`.
Logs `/private/tmp/fl-portal-local-expiration-{pg2,read-regression,live-final,unit-final,artifacts-final,clippy4,minimal}.log`.
Broader billing mutations/reconciliation remain open; checklist stays660/682(96.8%).


### 2026-10-15 — Packaged Unicode provenance correction

An expanded immutable CLI check found that common-crate Unicode data/license files
were present in the standalone crate but absent from the CLI template directory.
The template now includes that data directory. The permanent installed-package
check verifies exact Unicode data/license SHA256s and the new JWT/emoji and
credential/clock sources. The check correctly failed against the older69cee
archive. Corrected immutable snapshot `f2b3062695841eee5ce066ddea4bd05be07422f5`
passes offline locked CLI packaging/installation/repackage listing and fresh portable
project generation with the new provenance assertions. All2,897 extracted regular
files match their archive bytes after generation. Retained archive SHA256:
`60cc68eb4f1c7e53f296d203b4b699e886f07e37bb885ae02fa2396c8658072e`.
This is package/generation evidence, not a full runtime pass for this snapshot.
The separate69cee runtime suite was still running when recorded.

Receipt: `/private/tmp/fl-oct15-licensed-checkpoint.json`; generation log:
`/private/tmp/fl-oct15-licensed-generation.log`. Exact owned extraction/install
workspaces were removed after process/open-file and retained-archive hash checks,
reclaiming185,515,249 logical file bytes (no physical APFS claim). The archive and
logs remain. This does not change runtime format semantics or close a broad gate.


### 2026-10-15 — Expanded immutable runtime checkpoint

Exact committed snapshot `69cee669b4a2904e4a6f626d0c0732ab7b9485dd` passed
CLI packaging/installation from an archive, portable services/all four workers,
OpenAPI/AsyncAPI export, contract regeneration, strict generated TypeScript/Rust
consumers and native SDK generators. Live HTTP/MCP initialization, discovery,
health invocation and Origin rejection pass with shared/separate/disabled/custom
listeners, explicit/ephemeral ports, TypeScript +2000 fallback, atomic startup
rejection and graceful shutdown. All five generated capability callbacks pass
actual TypeScript signature checks, forgery/staleness/replay rejection and event
redelivery. Packaged IAM passes220 tests/3 ignored in168.41s; PostgreSQL proof is
separate and is not claimed by this packaged suite.

The snapshot includes the committed raw date/duration/time/datetime/IP/emoji/JWT
formats, credential policy, fresh organization clocks and Subscription surfaces.
It predates Portal local expiration, current organization scalar policy, and regex
integration. Its Unicode data/license template omission was discovered by expanded
checks and fixed separately in `f2b306269`; the preceding receipt proves corrected
package/install/generation provenance. This runtime pass does not erase that
known older-archive packaging defect or imply a full runtime rerun of the fix.

Archive2,889 regular files, SHA256
`59d5ef01b7141a8cb008bfce6a57c07c72ef197fb5c573500df190dc8b35b4de`;
all extracted source bytes verified before/after runtime. Retained under
`cli/target/parity-artifacts/forklaunch-69cee669b-59d5ef01.crate`.
Receipt `/private/tmp/fl-oct15-expanded-checkpoint.json`, runtime log
`/private/tmp/fl-oct15-expanded-runtime.log`. CLI package/install is offline/locked;
generated runtime dependency resolution is a separate fresh graph, not an
all-offline/locked claim. Exact owned workspaces were cleaned after active-process,
open-file and archive-integrity checks:215,137,127 logical file bytes removed,
without a physical APFS space claim. No public registry or cloud deployment ran.
No broad gate closes; checklist remains660/682(96.8%).


### 2026-10-15 — Bounded ECMAScript regex foundation

Default-off common feature `zod-regex` now exposes only checked compile/match
operations over a reviewed fork of regress0.11.1, pinned to Node24.2/Unicode16.
Pattern/input UTF16 APIs preserve lone surrogates; Rust str retains its narrower
representation domain. Invalid flags/syntax, resource exhaustion and ordinary
no-match remain distinct. Bounded pre-admission scanning, actual requested-storage
and logical-work accounting, recursive depth guards and shared lookaround work
limits prevent budget exhaustion from becoming false. These counters are not RSS,
wall-clock or process-OOM guarantees; valid patterns may exceed a selected quota.

Production vendor upstream540 tests/1 ignored, intended std+utf16-only34 library
and10 adversarial tests, common14 tests, strict all-feature/all-target Clippy and
feature-off check pass. Permanent reference fixture passes178,182 semantic and
1,717 constructor comparisons using actual ForkLaunch/Node and native behavior.
Measured16MiB compile requested-storage/10million work admits the semantic corpus.
Original MIT/Apache licenses, Unicode license/inputs/generator, original archive,
patches and hashes are retained. Offline vendor packaging passes74 entries and
required-member checks after correcting Cargo's omission of a nested generator
package by retaining its manifest as `Cargo.toml.generator` with instructions.
CLI templates include all provenance through explicit source links.

Frozen90-path receipt `/private/tmp/fl-regex-foundation-source.json`, validation
commands `/private/tmp/fl-regex-foundation-receipt.md`; logs
`/private/tmp/fl-regex-{vendor-production-tests,vendor-bounded-tests,common-production-clippy,kernel-production-parity,vendor-package}.log`.
CI runs both vendor feature profiles, common tests and the pinned reference fixture.
Release metadata discovery passes50 packages in dependency order; optional-feature
sparse-registry installation is still pending. No schema keyword or HTTP/MCP/WS
consumer is enabled here: checked aggregate budgets and operational-error propagation
remain active integration work. This foundation does not close the broad validator
gate; checklist stays660/682(96.8%).


### 2026-10-15 — Trusted scalar organization policy

Typed `OrganizationPolicy` and matching HTTP/service builders now configure
allow-create, organization count and membership limits. Defaults remain true,
unlimited and100. Organization limit0 denies creation; membership limit0 falls
back to100. Creation preserves allow/count/duplicate-slug error order. The pinned
default adapter counts its first100 matching membership rows (including non-owner,
duplicate/orphan rows); limits above100 are therefore not an unbounded quota.
Membership checks cover server add-member and invitation acceptance. Full graph
reads apply the configured limit to user hydration, preserving the upstream
failure on a pre-existing larger graph rather than silently truncating members.
Trusted policy cannot be overridden by HTTP request fields. Native transactions
serialize count/insert; bounded storage scans still fail explicitly at their cap.
Async policy callbacks and server-user impersonation remain separate work.

Actual Better Auth24 cases, focused native matrix0.10s, full IAM221/3ignored165.81s,
restricted PostgreSQL218.25s and strict full10.50s/minimal4.91s Clippy pass. Tests
cover defaults/order/scope,100-row window, quotas and concurrent admissions,
invitation status, hydration and HTTP policy spoofing. Owned PostgreSQL roles,
records and audit rows are clean. Final Clippy changed only two test assertions
from equivalent match to let-else after runtime tests; production stayed unchanged.

Frozen13-source receipt `/private/tmp/fl-org-policy-freeze.json`, SHA256
`b909f782cf64d75bd87776360254ab7be2f5782a76c54eb628b17b65e49a06e1`.
Logs `/private/tmp/fl-org-policy-{actual,native,full,pg,clippy,minimal-clippy}.log`.
CI includes the actual policy fixture and existing native/PG suites. Broader Better
Auth options and invitation policy/delivery remain open;660/682(96.8%) unchanged.


### 2026-10-15 — Fifty-package registry installation with bounded regex

Immutable committed snapshot `c045e5814d1a0dd154c386dd8da215be56ce6125` packages
all50 public crates plus the required literal-key serde_json root patch. Source
preparation ran offline. A private loopback sparse registry then installed all50
crates by exact version; Cargo metadata verified registry origin for every owned
crate and rejected unexpected local dependencies. The consumer enables common
`zod-regex` when the release includes its dependency. Before serving, it verifies
true/false Unicode regex matching, typed Work exhaustion and invalid flag rejection.
The registry-built HTTP/MCP service passes discovery/health/Origin, shared/separate/
disabled/custom listeners, explicit/ephemeral/+2000 ports, atomic startup rejection
and graceful shutdown. Compilation took2m18s. Registry consumer resolution is a
fresh graph, not an offline installation claim.

Verification now records boundedRegex=true and release preflight requires that
proof for reports containing the regex crate. The release-script regression suite
passes10 tests, including rejection of absent/false/nonboolean optional-feature
proof. No public registry publication, credentials or cloud deployment was used.

Release report, verification, all50 original crate archives, parser patch and
consumer manifests/lock are retained at
`cli/target/parity-artifacts/registry-c045e5814`. All53 report/archive artifacts were
hash-checked before and after cleanup. Receipt:
`/private/tmp/fl-regex-registry-checkpoint.json`; logs
`/private/tmp/fl-regex-registry-{prepare,verify}.log`. Exact clean detached worktree
and temporary release root were removed after process/open-file checks, removing
49,651,071 logical bytes without a physical APFS claim. Public installation and
checked schema/HTTP/MCP/WS regex integration remain open;660/682(96.8%) unchanged.


### 2026-10-15 — PaymentLink metadata expiration library

Explicit BoundPaymentLinkExpirationProfile and a separate trusted publisher now
support the existing TypeScript metadata.status=EXPIRED operation against a sealed
provider alias. This does not set active=false or revoke a Stripe URL. Local
payment status/amount are unchanged; completion hides the bound record from GET
and list. The actual TypeScript fixture records its existing UUID dispatch gap
and cache/optional database deletion semantics, not an arbitrary-target guarantee.

Migration17 adds immutable encrypted intent/completion evidence, separate record
and alias claims, and private current-row counters. Only a newly confirmed
reservation yields an opaque single-dispatch ticket. Exact repeats inspect stored
IDs/status; competing operations on the same public ID cannot issue another POST.
Uncertain commit/response retains claims indefinitely and permits inspection only.
The separate publisher atomically records completion, audit and claim release.
Startup for existing created-record reads now requires SELECT on the expiration
journal. No HTTP/MCP/SDK selection is included in this library checkpoint.

Validation: actual extracted TS6 cases and official Stripe22.6.2 wire pass. Final
restricted PostgreSQL tests2 pass6.52s, including migration16-to17 backfill under
an ordinary migration owner, role denial, races, dropped acknowledgments, audit
rollback and immutable history. Existing list/read regressions pass6.39s/5.47s.
Strict all-feature/all-target Clippy passes1.87s and minimal8.16s. Owned fixture
databases/roles remaining0. CI runs both oracle fixtures and the PostgreSQL suite.

Frozen16-source reviewed manifest /private/tmp/fl-payment-link-expiration-reviewed.json,
SHA2563110bdd8ab98a128887f2ade269714b46df7034284266076701c1c7fb928b72c.
Only a transaction-ownership documentation correction followed the agent's final
native checks; executable code matches its tested freeze. Logs:
/private/tmp/fl-payment-link-expiration-{source,sdk,final-pg,clippy-final2,minimal}.log.
Surface wiring and event-linked recovery remain open;660/682(96.8%) unchanged.


### 2026-10-15 — Trusted scalar invitation policy

Additive InvitationPolicy configures whole-second TTL, pending count and recipient
verification through trusted service/HTTP builders. Defaults remain48h/100/true;
TTL0 uses48h while count0 denies new invitations but permits resend. Counting takes
the first100 matching pending records before filtering expiration. Resend order
is preserved. Checked timestamp overflow returns a storage error. The verification
option affects get/accept/reject by ID; list-user invitations independently require
verified email. Request JSON cannot override this configuration.

Actual Better Auth fixture19 passes, including three cancellation observations
that remain reference-only for the next slice. Native focused1 and affected
organization regressions4 pass0.15s/0.27s. Restricted PostgreSQL passes222.95s,
including concurrency and recipient/expiry boundaries; owned roles/records/audits0.
Strict all-feature/all-target Clippy20.04s and minimal3.04s pass. Only trailing fixture whitespace was trimmed after native validation. No full unrelated IAM suite rerun is
claimed; the immediately preceding organization policy checkpoint ran221 tests.

Frozen13-source manifest /private/tmp/fl-invitation-policy-reviewed.json, SHA256
c102db80403426b2810609a266e0c07d500f00c637651f64f0e1182432ae42e3.
Logs /private/tmp/fl-invitation-policy-{actual,native,organizations,pg,clippy,minimal-clippy}.log.
Root independently reran the19-case oracle with explicit compiler/dependency paths.
That check exposed an implicit compiler lookup in12 Better Auth CI fixture steps;
all now select CI's installed TypeScript5.9.3. The updated workflow parses and all
12 compiler settings were checked. Cancellation-on-reinvite, delivery callbacks,
custom generators and broader Better Auth parity remain open;660/682 unchanged.


### 2026-10-15 — Reproducible workspace formatting checks

CI now derives every workspace member from locked offline Cargo metadata and
passes those packages explicitly to rustfmt. The old --all option also traversed
excluded local vendor dependencies, including preserved upstream/generated regex
sources. All19 framework and30 blueprint members are checked; excluded archives
and generated Unicode source bytes remain unchanged. Three existing first-party
formatting discrepancies were corrected without behavior changes. Both complete
member checks pass; workflow YAML parses. Logs:
/private/tmp/fl-oct15-{framework,blueprint}-member-format.log.
This is CI reliability work, not a closed feature gate;660/682 unchanged.


### 2026-10-15 — PaymentLink expiration HTTP/MCP and both SDKs

Explicit paymentLinkExpiration selection requires trusted prices, the existing
internal_service_tenant_v1 authority declaration and a separate expiration
publisher. POST /bound-payment-links/:publicId/expire accepts only operationId;
GET /bound-payment-links/:publicId/expiration-operations/:operationId exposes
historical IDs/status. Creation/read selectors never infer mutation capability.
The effect remains metadata.status=EXPIRED, not active=false or URL revocation.
POST reports completed200 or pending/stale-evidence202; inspection returns200 for
a recorded operation and404 when absent. Missing evidence does not prove rollback.

Selected startup checks every required runtime/publisher read, lock and insert
privilege, publication function execution, role separation and database identity.
Base/managed/backup or incomplete configurations fail before listening without
provider I/O. Existing constructors remain compatible; the new explicit publisher
constructor and generated billing binary select this capability independently.

Final direct-executable runtime wrapper passes3.73s against restricted PostgreSQL,
including HTTP, both MCP operations, four actual SDK calls (two TS/two Rust),
concurrent one-POST dispatch, historical no-provider reads, pending202/no retry,
and runtime/publisher privilege matrices. Units15, generated artifact cases166
(including actual base/Stripe main templates), full strict Clippy17.94s and minimal
8.44s pass. Owned PostgreSQL databases/roles0/0. Source formatting/diff checks pass.

Frozen12-source manifest /private/tmp/fl-payment-link-expiration-route-final.json,
SHA256b7ca19e08e5f67ce60bbbdca1fed7ed936a1b9deb8009092b7d3b5f60857e1e4.
Root-owned runtime wrapper was exercised by the final proof and added to CI.
Logs /private/tmp/fl-payment-link-expiration-route-{wrapper-final,artifacts,units,clippy,minimal}.log.
Event-linked recovery and broader billing remain open;660/682(96.8%) unchanged.


### 2026-10-15 — Checked regex consumer integration

Isolated committed source477b2e3458329759e8b0655d4421064bf37d369f adds explicit
zodRegex export and version1 sourceUnits/flags rules, with default export unchanged.
Explicit pattern annotations retain precedence; transforms without an override
are rejected in this profile. Native zod-regex stays default-off. Checked contract
compilation preserves Invalid versus Operational; legacy compiler rejects active
regex demand even with the feature enabled. Literal-data occurrences are inert.

A private synchronous RAII evaluation frame accumulates work and preserves any
evaluated resource failure across not/if/anyOf/oneOf/allOf/contains. Nested calls,
unwind and threads are isolated. HTTP checked version bodies/fields fail with safe
correlated500 in every validation mode; MCP admission returns an internal error
without changing input; WebSocket encode/decode exposes Operational. Composition
CheckedString preserves scalar ordering and atomic output with typed RegexResource.

The separate explicit JavaScript host-native policy propagates through owned
UniversalSdk response engines and generated socket codecs. Injected engines cannot
implicitly enable the reserved keyword, including strict:false. Host RegExp is
synchronous/uninterruptible and does not share native work/storage guarantees.
Native JSON strings exclude lone surrogates; explicit UTF16 kernel inputs do not.
Native quotas charge configured engine allowances and adapter vectors before work;
allocator overhead/RSS, OOM and wall-clock guarantees are outside those quotas.
CLI FORKLAUNCH_SDK_ZOD_REGEX accepts only unset/0/1 and forwards generated features.

Validation: native376 total (Axum145,contract37,HTTP74,MCP27,validation74,WS19),
final focused7 feature-on/2 feature-off, strict nine-crate all-feature/all-target
Clippy, actual Zod/host-AJV2720 plus policy controls/threeHTTP calls, six live SDK
responses and24 generated TS/Rust socket checks all pass. Existing2304 baseline/
2220 opted-in checks and30564 format observations pass. CLI socket subset9 passes
with its three existing warnings. Canonical366 live outcomes and both OpenAPI
endpoints pass with zero normalized document differences. Configured TypeScript
checks12 roots/625 exports; inventory625 entries/1708 declarations/44 packages has
zero unresolved entries and passes its tests. Framework/blueprint locks unchanged.
Root's complete19-member framework formatting check also passes.

Source39 manifest /private/tmp/fl-regex-integration-source.json SHA256
77505ea2a228a397bb6a38021a427830f2da586edc05ee4c33da9cb1b883d0c2.
Full receipt /private/tmp/fl-regex-integration-receipt.md and logs
/private/tmp/fl-regex-isolated-{full,final-focused,clippy,profile,consumers,canonical}.log.
CI includes profile/generated-consumer and explicit feature-off tests under pinned
Node24.2. The framework guide explains APIs and host/native limits. Vendor/kernel
bytes remain unchanged. Broader schema parity stays open;660/682(96.8%) unchanged.

Integrated as f7b0f2351:38 paths match the frozen source byte-for-byte; the contract
compiler retains one preexisting formatting-only line wrap, checked exactly against
the isolated version. Integrated39 manifest /private/tmp/fl-regex-integrated-source.json
SHA2560ebd2dbc1281bffa03b7c50b82d69cde95b5db5a55d0ea9e3c5661b09fdf0572.
The integrated19-member framework formatting check passes. No behavior change was
introduced by this merge; billing recovery validates against this combined source.


### 2026-10-15 — Cancellation-on-reinvite ordering and concurrency

Trusted default-false cancellation builders on OrganizationService/AuthHttp now
match cancelPendingInvitationsOnReInvite. Explicit resend retains precedence.
Role/recipient checks precede a separately committed cancellation; quota/team or
replacement failure leaves the old invitation canceled, preserving upstream's
observable boundary. A second tenant writer rechecks current session/roles and
the exact canceled predecessor before replacement. No user callbacks run inside
transactions and no automatic cancellation retry occurs.

Stable ID/value snapshots of already-returned bounded matching rows permit
unchanged existing duplicates, including100/101-window shifts, without changing
selection/quota first100 windows. Newly observed/changed pending rows deny through
the existing duplicate error; missing/changed predecessor yields static409.
These are explicit stronger concurrency guards than independent upstream calls.
Both cancellation and replacement COMMIT acknowledgement failures are tested:
persisted state can outlive the error response and callers must inspect it.

Actual Better Auth10, focused native1(0.12s), organization5(0.32s), restricted
PostgreSQL225.37s and strict full17.86s/minimal3.99s pass. After only explicit-import
and equivalent let-chain lint fixes, exact final organization5 reran successfully
in0.28s. The PG receipt predates those style-only fixes; no repeat is claimed.
Owned fixture roles/records/audits0. Validation used the pre-integration framework;
regex was integrated after the lease was released, leaving these IAM bytes intact.

Frozen12 manifest /private/tmp/fl-invitation-cancel-freeze.json SHA256
b8a33d9426563b38596444be4e25872e5204e7a967b68646b5c83ec1ce3adcc3.
Logs /private/tmp/fl-invitation-cancel-{actual,native,organizations,pg,clippy,minimal-clippy,final-organizations}.log.
CI includes the actual fixture with explicit compiler configuration. Invitation
delivery and broader Better Auth options remain open;660/682(96.8%) unchanged.


### 2026-10-15 — Regex integration workspace cleanup

After integration/source-hash verification, clean HEAD and zero open-file checks,
removed the owned fl-regex-integration worktree and fl-regex-native-target cache.
This removed 6,042,631,686 logical bytes; no physical APFS savings claim is made.
The original source commit remains on codex/rust-regex-integration-proof, with
all manifests/logs/receipts outside the removed roots. The exact owned draft stash
was dropped only after its content had been integrated and verified. Cleanup
receipt /private/tmp/fl-regex-integration-cleanup.json. Subsequent format work uses
a fresh isolated source tree and the existing CLI target to avoid another duplicate
cache. Broader checklist remains660/682(96.8%).


### 2026-10-15 — Event-linked PaymentLink expiration recovery library

A pending expiration can now restore opaque publication-only proof after fresh
account/mode checks, authenticated fixed Event and PaymentLink GETs, and exact
request-key, rendering-version, resource-ID/type/mode and EXPIRED-metadata checks.
The event must be payment_link.updated, have a non-null matching request key,
no unsupported context, and fall within reservation−5s..+5min without future time.
This is an explicit admission policy, not a causality, event-availability or
request-origin guarantee. It does not deactivate a URL or prove payment state.

Recovery retains actor/tenant/public-ID/profile and canonical creation provenance,
locks unchanged dependencies and requires exactly the two pending claims. Its
opaque proof exposes neither a dispatch key nor a reservation constructor. The
separate trusted publisher atomically records bounded encrypted event/current
body evidence and hash, completion and claim release through existing migration17.
No provider POST, automatic retry, migration or shared framework change is added.
Concurrent winners and uncertain final COMMIT acknowledgements inspect historical
receipts; unavailable inspection leaves an IDs-only unresolved outcome.

Malformed event IDs and missing claims SELECT fail before provider reads. Recovery
works after reservation INSERT grants are revoked, and does not change the
mutation-only startup grant requirements. Expired provider context, mismatched
metadata/resources, oversized/deep evidence and audit failure retain claims.
Automatic events without request keys and unavailable retained events stay pending.

Official Stripe22.6.2 SDK wire passes exact authenticated Event/PaymentLink GETs.
Final restricted PostgreSQL matrix passes6.77s, including a5.1s delayed read,
concurrency, encrypted evidence, audit rollback and actual lost COMMIT ACK.
Existing expiration2 regression passes6.42s. Strict all-feature/all-target
base/Stripe Clippy9.28s, minimal4.27s and scoped formatting/diff checks pass.
Owned fixture databases/roles0/0. These native checks used integrated regex source.

Frozen11 manifest /private/tmp/fl-payment-link-expiration-recovery-final.json,
SHA25619cc444136379950d4ca9774f0a1c186c5f21cf907335e2c99be184977d6c65a.
Logs /private/tmp/fl-payment-link-expiration-recovery-{sdk,pg-final,regression,clippy-final,minimal}.log.
CI includes the SDK oracle and recovery PostgreSQL matrix. HTTP/MCP/SDK recovery
selection remains separate; broad checklist660/682(96.8%) unchanged.

### 2026-10-15: Ordered runtime string formats

Commit `d6605594d` exposes the sixteen shared Zod format kernels through ordered
runtime string checks, retaining the optional Ada URI capability. Checked time
precision and IP versions reject unsupported registration separately from input
errors; borrowed JWT algorithm checks charge schema and repeated evaluation bytes.
JWT checks validate syntax/header constraints, without authenticating signatures.
Ordered trim, custom diagnostics and atomic whole-input preservation remain intact.

The frozen nine-source manifest is `/private/tmp/fl-runtime-formats-source.json`;
proof is `/private/tmp/fl-runtime-formats-receipt.md`. Actual/native comparisons
pass 65,703 cases plus four unsupported registrations. Validation tests pass 76
with all features and 71 without defaults; strict validation/Axum Clippy passes.
Live coverage passes 267 HTTP comparisons, 126 MCP calls, three operational-error
controls and six calls through each generated SDK. Logs use the
`/private/tmp/fl-runtime-formats-` prefix. CI runs the oracle with pinned Node
24.2.0 and retains the extended composition fixture. Native inputs remain valid
Unicode JSON; time precision is bounded to 0..1000. Intersections, discriminated
unions, allocating case transforms, recursion and arbitrary callbacks/non-JSON
outputs remain separate schema work. Checklist remains 660/682.

### 2026-10-15: Trusted invitation delivery after persistence

Optional invitation delivery captures bounded configured organization, invitation,
member and public-user projections in the authoritative transaction. Preparation
runs only after confirmed persistence; its errors yield a sanitized empty500,
while future/dispatcher failures retain successful admission. Default delivery
is awaited; an explicit shared dispatcher supports background delivery. New
invitations and resends deliver; cancellation-only failures and uncertain commit
acknowledgements do not. No hook leaves existing projection behavior unchanged.
No raw request, arbitrary custom projection, panic or blocking-code parity is claimed.

Frozen13-source manifest `/private/tmp/fl-invitation-delivery-freeze.json` has
SHA256 `1316afe803b110af768cfbbdc0b2d1cf940783beb19c21761670c229bfce8d31`.
Actual Better Auth9 checks pass; final organization6 pass0.28s. Restricted PostgreSQL
lifecycle passes227.45s, with owned roles/records/audits cleaned. PG precedes only
layout changes (boxing callback payload and extracting HTTP service construction);
final exact-source organization tests and strict all-feature/all-target Clippy
12.53s/minimal4.81s pass. Logs use `/private/tmp/fl-invitation-delivery-`.
This receipt does not claim live TCP/browser delivery. Broader auth remains open.

After native jobs released the blueprint target,551 generated executable artifacts
older than24h were removed after checking no candidate was open. This removed
9,189,656,400 logical bytes; source and retained archives remain. Candidate hashes
are not asserted: exact paths/sizes/mtimes and completion are recorded in
`/private/tmp/fl-oct15-old-blueprint-binaries{,-cleanup}.json`.

### 2026-10-15: Unconstrained factory schema annotations

Removed factory-owned `type: object` annotations from Zod `any` and `unknown`;
`type<T>()` delegates to `any`. These validators admit every JSON value at runtime,
so exported schemas now preserve that acceptance. Explicit application OpenAPI
object overrides remain authoritative. A regression first failed on null against
the old factory schema, then passed11,106 actual Zod/AJV comparisons over616
OAS3.0/3.1 exports, including direct/optional/object/array wrappers and existing
union/enum controls. Configured TypeScript7.0.2 passes12 roots and625 exports.
Logs: `/private/tmp/fl-unconstrained-factory-{before,after,types}.log`. Native and
generated SDK consumer revalidation for this annotation change remains queued behind the
immutable packaged-runtime run, which intentionally predates this source change.

The factory regression also passes54 live HTTP responses through the actual
Universal SDK across any/unknown/type factories and all18 JSON examples; source
OpenAPI documents remain unchanged. Log: `/private/tmp/fl-unconstrained-factory-sdk.log`.

### 2026-10-15: TypeBox unconstrained factory consistency

The same factory-only correction now applies to TypeBox Any/Unknown and its
`type<T>()` alias. Kind-driven parsing already admitted all JSON values; the
factory-owned object annotation incorrectly restricted exported-schema consumers.
The permanent fixture first reproduced that disagreement, then passed162 actual
TypeBox/AJV checks across direct/object/array shapes. Combined Zod/TypeBox factories
pass108 live actual Universal SDK response checks, preserving explicit application
object overrides. Configured TypeScript7.0.2 passes12 roots/625 exports. Logs:
`/private/tmp/fl-unconstrained-typebox-{before,after,types}.log`. Native acceptance
validation is prepared and queued after the immutable package target releases.

### 2026-10-15: PaymentLink expiration recovery HTTP/MCP/SDK surfaces

Independent `paymentLinkExpirationRecovery` selects trusted prices and explicit
`internal_service_tenant_v1` authority with a separate expiration publisher. It
exposes event-linked recovery and historical inspection without enabling provider
mutation. Recovery-only startup needs read/lock/audit/publication authority but
no reservation INSERT privileges. Combined expiration/recovery selectors require
the same normalized trusted-price set and expose one inspection operation.
Recovery accepts only an eventId; authenticated actor/tenant/profile checks and
provider event/current-object evidence remain mandatory. It performs no provider
POST, reports unresolved outcomes as202, and returns IDs-only historical receipts.

Frozen12-source manifest `/private/tmp/fl-payment-link-expiration-recovery-route-final.json`
has SHA256 `5fe8f1a20783d91dabbea931f05663e0a5f726b1502466b78b2090e15af28d1a`.
Final restricted PostgreSQL HTTP/MCP and four actual generated SDK calls pass3.40s;
existing expiration regression passes3.20s. Sixteen units and186 rendered artifact
cases pass, including compiled base/Stripe mains. Artifact totals now count
executed cases dynamically instead of the old stale printed166. Strict
base/Stripe/Axum all-feature/all-target Clippy passes18.58s; minimal Axum8.43s.
Scoped formatting/diff checks and owned database/role cleanup pass. Logs use
`/private/tmp/fl-payment-link-expiration-recovery-route-` with suffixes
wrapper-final,regression,units-final,clippy-final,minimal,artifacts-final,cleanup.
CI runs the direct-executable wrapper so SDK compilation does not hold Cargo's
outer test lock. Broader billing/reconciliation gates remain open;660/682 unchanged.

The committed runtime-format worktree was removed after confirming its exact
HEAD, clean state, nine integrated source hashes and zero active/open paths.
Proof branch and external logs/manifests remain;4,176 regular files totaling
44,720,196 logical bytes were removed. Receipt: `/private/tmp/fl-runtime-formats-cleanup.json`.

### 2026-10-15: Trusted scalar team policy

`TeamPolicy` configures default-team creation, maximum teams, maximum members per
team and last-team removal. The service and HTTP builder propagate the same
trusted options across organization creation, direct team membership, server-side
member addition, invitation admission/acceptance and deletion. Requests cannot
override policy. Zero team maximum is unlimited; zero member maximum denies new
pairs but preserves existing memberships. Default-team creation bypasses limits,
matching the reference. Team counts and invitation member hydration retain their
first100 windows; duplicate reuse and empty teamId behavior are covered.

Frozen18-source manifest `/private/tmp/fl-team-policy-freeze.json` has SHA256
`1940dbf87ed1b4e0480e357678a5d10b7db97ee4c904129f83ebd510c1201eee`.
Actual Better Auth27 cases pass; focused native1 passes0.06s and final exact-source
organization7 pass0.28s. Restricted PostgreSQL lifecycle passes225.49s, including
concurrent final seats/team quotas/last-team deletion and tenant isolation; owned
roles/data/audits are cleaned. Strict full/all-target Clippy passes10.96s and
minimal33.33s (including build-lock wait). Logs use `/private/tmp/fl-team-policy-`.
Native rejected operations roll back bookkeeping atomically; upstream can persist
an undercount correction before rejecting a seat. That failure-side difference
and arbitrary async policies remain explicit, rather than claimed equivalent.
No live TCP/browser receipt is asserted here. Broader auth remains open.

After the native lease released,1,095 project rlibs older than24h were removed
after checking no native target process or candidate open file remained.
8,486,436,936 logical bytes were removed, retaining third-party dependencies,
source and proof archives. Exact paths/sizes/mtimes and completion are recorded in
`/private/tmp/fl-oct15-old-blueprint-project-rlibs{,-cleanup}.json`.

### 2026-10-15: Native unconstrained factory conformance

The factory schema fixture now additionally passes594 native contract comparisons
across Zod/TypeBox direct and wrapped schemas. The same final run passes11,106
Zod/AJV comparisons,162 TypeBox/AJV comparisons and108 live actual Universal SDK
HTTP responses. CI enables this native branch with the existing schema-validation
example; its required feature is explicit. Log:
`/private/tmp/fl-unconstrained-factories-native-final.log`. The first invocation
identified a missing test-command feature flag, corrected before the final pass.
This closes the native validation follow-up for factory annotations; generated
Rust/TypeScript SDK-specific consumer expansion remains separate.

### 2026-10-15: Configured Subscription update preflight

Generated Stripe services now select configured update preflight before provider
GET/POST, preserving generic/custom constructors' prior behavior. Retained owner
identity, supplied owner equality and configured party/provider enums are checked;
blank checks use exact ECMAScript whitespace, including FEFF while excluding0085.
Non-UUID IDs remain admitted because the actual configured update schema uses a
plain string. Missing records/provider IDs preserve preflight ordering. The generated
backend propagates selection through its real HTTP dispatch; base templates do
not enable Stripe configuration. No extra financial capability/journal was added.

Frozen9-source manifest `/private/tmp/fl-subscription-configured-owner-final.json`
has SHA256 `ccaa63bd0791210499f05ea041bcd163daa00d4360010f67919e73c5e6854f31`.
Actual TypeScript84 cases, native2 tests, real HMAC HTTP/restricted PostgreSQL
0.53s, legacy170 comparisons,186 generated artifact cases and compiled actual
mains pass. Strict all-feature/all-target Clippy21.56s and minimal1.44s pass;
owned database/role cleanup is0/0. Logs use `/private/tmp/fl-subscription-configured-owner-`
with suffixes source,native-final,http-final,legacy170,artifacts,clippy-final,minimal,cleanup.
Native HTTP400/status/sanitization is tested, but actual TS ordinary-Error HTTP
envelopes were not compared. Explicit JS undefined reaches provider and local
persistence before response-mapper failure; JSON cannot express that state.

[RUST-BILLING-REMAINING-BEHAVIOR-AUDIT.md](RUST-BILLING-REMAINING-BEHAVIOR-AUDIT.md)
separates confirmed configured gaps, already-matching baseline operations, explicit
reconciliation requirements and extra authority hardening. Raw-ID dispatch shared
by both languages is not itself a missing Rust operation. Broader billing and
transport error gates remain open;660/682 unchanged.

### 2026-10-15: Immutable packaged CLI and generated runtime with checked regex

Exact commit `e0af2cc0d08680d2fd9887b1abb95db73728c9c9` was archived into a
git-free source tree, packaged/installed/repackaged offline with the retained CLI
lock, and used to generate a relocated application. The archive contains3,005
regular files and6,874,324 bytes; SHA256
`888fdb3a62aaa1743c523971ab2a8f06bff282fd713cb95c7ba0a21acfd0abed`.
Retained artifact: `cli/target/parity-artifacts/forklaunch-e0af2cc0d-888fdb3a.crate`.
The allowlisted certificate fixtures and vendored regex/Unicode source/license
coverage are verified. All archived source bytes matched again after runtime.

The installed CLI generated portable modules, four worker kinds and infrastructure
capabilities; native services/SDKs compiled, contract/OpenAPI/AsyncAPI generation
and regeneration checks passed, and generated Rust/TypeScript consumers passed
strict checks. Optional `FORKLAUNCH_SDK_ZOD_REGEX=1` forwarding and packaged
contract regex tests passed. Packaged IAM223 tests passed with3 explicitly ignored
PostgreSQL tests (167.78s); this snapshot's separate restricted PostgreSQL evidence
is recorded in the individual feature receipts.

Generated HTTP/MCP startup passed shared/separate/disabled/custom listeners,
explicit/ephemeral ports and the TypeScript+2000 fallback, initialization/discovery/
health calls, Origin rejection, atomic startup failure and graceful shutdown.
All five generated native capability callbacks passed actual TypeScript signatures,
forgery/staleness/replay rejection, event redelivery and unknown-feature behavior.
AWS S3 and all generated blueprint implementation checks passed locally; this
does not assert deployed AWS tasks or public registry publication.

Driver9733 and runtime25277 both terminated0. Metadata/log hashes and excluded
later changes are in `/private/tmp/fl-oct15-regex-package-checkpoint.json`; runtime
log is `/private/tmp/fl-oct15-regex-package-runtime.log`. This immutable snapshot
predates runtime-format expansion, invitation delivery/team policies, factory
annotation corrections and recovery-route/configured-owner changes. Those retain
their own later source receipts. After verifying archive integrity and zero active/
open paths, both owned source/install/generated roots were removed:265,596,389
logical bytes, with the retained archive hash unchanged. Broader gates remain open.

The unconstrained-factory follow-up is now validated in generated consumers too:
`rust_zod_checks_consumers.mjs` includes factory-produced any/unknown/type fields
and ten scalar/aggregate payload variants. The final run passes35 live actual
Universal SDK responses and140 generated TypeScript/Rust socket codec checks,
strict generated source checks and feature-off rejection before output. Log:
`/private/tmp/fl-unconstrained-factories-generated-consumers.log`. This closes the
queued factory-specific generated-consumer follow-up; broader schema gates remain.

### 2026-10-15: Bounded runtime JSON intersections

Commit `59df4f436` adds `ZodJsonSchema::Intersection { left, right }`. Both branches
evaluate in source order; dirty transformed values remain private and merge only
when neither branch aborted. Merge conflicts follow branch diagnostics, while
public invalid outcomes preserve the original whole input. Equality-only private
origin tokens preserve same-reference input/default container behavior, including
nested values; distinct constructed object merges retain actual prototype-key
semantics. Tokens are never dereferenced. Origin tracking activates only for
schemas containing intersections; existing schemas avoid its extra allocation.
Admission, evaluation, provenance and merging share bounded node/byte budgets.
Deferred missing-order failures still allow a later valid union branch to win.

Frozen11-source manifest `/private/tmp/fl-intersection-source.json` SHA256
`39285c6323d8385a09c8b10e632ddc6451fe2f068d45744b811e3d1a5e643c77` was verified
against all eleven integrated files. After helper refactors, validation83 full/78
minimal tests and strict validation/Axum full/all-target plus minimal validation
Clippy pass. Each default/arbitrary-precision configuration passes the same12,096
actual-source/native cases. Live checks pass291 HTTP comparisons,138 MCP calls,
three operational500 controls and six calls through each generated SDK. Logs use
`/private/tmp/fl-intersection-`; commands/scope are in `fl-intersection-receipt.md`.
CI runs the new differential fixture and extended existing live fixture.

This does not admit sparse arrays, Date/non-JSON outputs or arbitrary callbacks;
nonfinite outputs required for merging remain explicit unsupported values.
Discriminated dispatch, allocating case transforms and remaining schema/emitter
constraints stay open. No dependency, manifest, lock or TypeScript API changes.

The committed intersection worktree was removed after exact-HEAD/clean-state,
eleven integrated source hashes and no active/open-path checks. The proof branch
and external artifacts remain. Receipt `/private/tmp/fl-intersection-cleanup.json`
records4,185 regular files and44,828,865 logical bytes removed.


### 2026-10-15: Organization deletion and dynamic-role controls

Trusted `OrganizationControls` now propagates through native service and HTTP
builders. Deletion disabling preserves body validation and rejects before session
lookup or refresh. Optional dynamic-role quotas count all organization-role rows
under the writer lock, including configured-name records; zero denies creation.
Existing authorization and predefined-name checks precede quota checks, which
precede permission-resource and duplicate-name checks. Defaults preserve behavior.

Actual Better Auth16 cases, final native organization8 tests, restricted PostgreSQL
(224.12s) and strict all-feature/all-target Clippy pass. Minimal-library strict
Clippy also passes. PostgreSQL completed before the final test-only malformed-input
assertions; production source did not change afterward. Owned PostgreSQL roles,
records and audit cleanup counts are zero. Frozen11 manifest
`/private/tmp/fl-org-controls-freeze.json` has SHA256
`aa1e1e84557da24c07bfb13f2241dff6ce0a135964dcd678494bb2bc5fa1fa26`;
logs use `/private/tmp/fl-org-controls-`. CI runs the actual-source fixture.

An additional minimal-all-target experiment exposed ten existing HTTP-feature
references in keyring configuration/rotation tests; it failed and is retained in
`fl-org-controls-minimal-clippy-final.log`. This does not replace the passing
minimal-library check and remains a separate feature-gating follow-up. Creator-role
lifecycle and broader auth coverage remain open; checklist660/682 is unchanged.


### 2026-10-15: Configured Subscription HTTP error parity

Selected configured Subscription ownership failures now return the actual TS
sanitized500 text envelope with supplied or generated TypeScript correlation IDs.
A dedicated mapper error code keeps malformed input admission at400; generic
backends preserve native correlation behavior and existing owner-update semantics.
The adapter reuses the existing HTTP formatter through one internal dependency
and lockfile edge; no dependency versions changed.

Actual configured mapper/createContext/guard/final Express error-handler source
passes12 real HTTP cases. The fixture owns its route, entropy primitive and
telemetry; this is not a full TypeScript application-bootstrap claim. Native
signed HTTP with restricted PostgreSQL passes in0.58s, service2 tests pass,
legacy170 comparisons and186 rendered artifact cases (including compiled actual
binary templates) pass. Strict all-feature/all-target Clippy21.67s and minimal
Clippy1.68s pass; PostgreSQL cleanup counts are0/0. Frozen8 manifest
`/private/tmp/fl-subscription-owner-transport-final.json` SHA256
`5f584310917d4705efae0827f90ab2caba7f7738c4a2ffd2a65a89e10beeb6d3`
is verified. Logs use `/private/tmp/fl-subscription-owner-`; CI runs the new actual
HTTP oracle alongside the existing native fixture. Broader billing and general
schema-error parity remain open; checklist660/682 is unchanged.


### 2026-10-15: Minimal auth test feature gating repaired

Four HTTP-dependent keyring tests and their response helper now require the
existing `http` feature. This fixes the ten missing Axum/AuthHttp references
reported during organization-controls validation while retaining non-HTTP keyring
coverage in minimal builds. Production code and dependencies are unchanged.
Strict minimal-all-target Clippy passes12.72s and the minimal keyring suite passes
38 tests in55.45s (terminal session54797, exit0). Logs:
`/private/tmp/fl-creator-role-minimal-clippy.log` and
`/private/tmp/fl-creator-role-minimal-keyring.log`. Exact three-file hashes are in
`/private/tmp/fl-keyring-http-test-gating-source.json`. Both commands are now in CI.
The earlier minimal-all-target failure is resolved; broader auth remains open.


### 2026-10-15: Trusted organization creator-role lifecycle

`CreatorRole` now propagates through service and HTTP builders, organization
creation, invitations, member removal/updates/leaving and scoped last-creator
counts. Empty configuration uses `owner`; otherwise the exact trusted string is
preserved, bounded to4096 UTF-8 bytes. Actor trimming follows each actual route;
comma-containing configuration is not silently split. Creator permission bypass
applies only to member-role updates. Configuration does not register a static role
or grant general organization permissions. Request bodies cannot override it.

Actual Better Auth14 cases, native organization9 tests (0.28s), restricted
PostgreSQL (224.70s), strict full/all-target Clippy18.71s and minimal/all-target
Clippy12.72s pass. Concurrent last-creator departures/demotions, first100/101
counting, foreign organization exclusion and HTTP propagation are covered. Owned
PostgreSQL role/record/audit cleanup counts are zero. Minimal keyring38 tests also
pass with the separately committed HTTP test-gating repair. Frozen16 manifest
`/private/tmp/fl-creator-role-freeze.json` SHA256
`33d3bab007b98931c808debcbf2a422e720692fb71b66e4f9179515898c9f932`
was checked against integrated source; logs use `/private/tmp/fl-creator-role-`.
CI runs the permanent actual-source fixture. No dependencies or locks changed.

Configurable access-control roles/statements, seven async policy variants,
custom default-team creation, topology toggles, schema mappings and lifecycle
hooks remain open. This closes the creator-role implementation slice, not the
broader Better Auth gate; checklist660/682 is unchanged.


### 2026-10-15: Bounded enums and discriminated JSON unions

Commit `85e337314` adds raw Enum/NativeEnum leaves and DiscriminatedUnion to the
finite JSON evaluator. The published enum factory retains its ordinary-union
lowering. Native enum filtering follows actual Zod while dispatch uses raw own
values; reverse names can select a branch whose enum leaf then rejects them.
Registration preserves numeric own-key order, SameValueZero duplicates, absence,
nullable and static-default semantics. Only the selected branch evaluates.
Registration, comparisons, diagnostics and retained values debit shared budgets.

Final validation90 full/85 minimal tests, strict validation/Axum all-feature
all-target and minimal validation Clippy pass. Each numeric configuration passes
5,838 actual/native discriminator comparisons (including2,358 registration
rejections) and12,096 intersection regressions. Live checks pass327 exact HTTP
comparisons,156 MCP comparisons with scoped error envelopes,3 safe operational500
checks and6 calls through each generated SDK. TS-only75 HTTP/6 MCP checks pass.
Live proof precedes behavior-preserving private helper extraction; final suites,
Clippy and both differential oracles ran afterward. Commands and source order:
`/private/tmp/fl-discriminated-receipt.md`; logs use `fl-discriminated-`.

All9 integrated files match `/private/tmp/fl-discriminated-source.json` SHA256
`a707eda1e6632df217eb2239eb2782e56d07b241b5f1cba2fda428a26c2df661`.
Six explicit prototype-field boundaries retain existing rejection behavior;
non-JSON values, arbitrary callbacks/lazy graphs and remaining transforms remain
open. No dependency, lock or public TypeScript API changed. CI runs the new oracle
and the expanded existing live fixture. Broader schema gates remain open.

The clean committed worktree was removed after exact HEAD, all integrated hashes
and zero open-path checks. `/private/tmp/fl-discriminated-cleanup.json` records
4,198 regular files and44,987,559 logical bytes removed; its proof branch and
external receipts remain. Native build caches are retained for active agents.


### 2026-10-15: Configured Checkout and PaymentLink cache-first GET

Generated Stripe services now select configured cache-first reads. Public IDs and
local DTO fields remain local; retained provider references select the provider
GET, and fresh identity/kind/mode/customer evidence must agree before return.
Read paths perform no cache writes. Generic services retain historical behavior.
Malformed or missing cached DTOs fail before provider I/O; native request-ID
admission remains separate. A narrow configured client path retains provider
status in structured details. Selected HTTP dispatch preserves sanitized TS
404/429/500 text and correlation behavior, including storage and connection500.
The existing TypeScript response-status policy prevents undocumented429 from
being converted to500. Generic response formatting remains unchanged.

Actual configured source369 cases and actual-source HTTP48 cases pass. The HTTP
oracle executes real configured services/mappers, context and final Express error
handler with real Stripe SDK error classes at an owned provider callback; it does
not prove full SDK retry/redirect/wire or application-bootstrap behavior. Native
Stripe2 tests and signed restricted-PostgreSQL HTTP1 pass; the final HTTP run took
1.13s. Legacy170 comparisons and186 rendered-artifact cases, including compiled
actual binary templates and selected-profile assertions, pass. Strict full/all-
target Clippy7.33s and minimal Clippy37.61s pass; owned PG cleanup counts are0/0.
Logs use `/private/tmp/fl-configured-cached-read-`. Frozen11 manifest
`/private/tmp/fl-configured-cached-read-final.json` SHA256
`864c8c1522ef534b57a86bb1c3a427b093aabe6149467ee029f4d271e4b06335`
was verified. CI includes the new actual HTTP oracle and native read fixtures.

Configured cache date coercion beyond typed native persisted dates, cache-selected
PaymentLink lists, broader wire failure/retry behavior and remaining billing gates
stay open. No framework, dependency or lock changes. Checklist660/682 unchanged.


### 2026-10-15: Ordered Unicode16 lowercase and uppercase transforms

Commit `54e44c773` makes checked strings available in default builds and adds
allocating Lowercase/Uppercase operations; Regex remains feature-gated. Existing
borrowed string APIs retain their signatures. Pinned Unicode16 full mappings and
Final_Sigma context rules compose with trim, ordered assertions and regex checks.
Public failures remain atomic; private dirty values retain intersection semantics.
Exact output capacity and scan work are charged before allocation; sigma context
scans are linear. This is requested-work/storage accounting, not an RSS bound.

Exact native comparison passes2,224,128 scalar mappings and13,855 context/order/
long-run cases. Full95/minimal89 tests, strict validation/Axum full and validation
minimal Clippy pass. Live checks pass351 HTTP comparisons,168 MCP comparisons
with scoped envelopes,3 safe Operational500 cases and6 calls through each SDK.
The discriminator regression passes5,838 cases per numeric mode. Production
source was unchanged across those proofs; only an example hexadecimal literal
was restyled before final scalar/discriminator reruns. Commands and source order:
`/private/tmp/fl-case-receipt.md`; logs use `/private/tmp/fl-case-`.

All17 integrated paths match `/private/tmp/fl-case-source.json` SHA256
`901dc6b4fe3adc9ca05440b3e3f616213887d84b02177934722846c0ac4f5ae4`,
including the template data symlink text. Licensed SpecialCasing data retains its
original SHA and final blank line through a file-specific Git whitespace rule.
The reproducible generator checks source hashes and reuses retained UnicodeData/
DerivedCoreProperties. Offline package listing confirms source/data/license/
provenance inclusion; permanent CLI archive guards were added. This is package
inclusion evidence, not a new immutable package-install/runtime checkpoint.

Remaining numeric length registrations, lone UTF16/non-JSON domains, arbitrary
callbacks/lazy graphs and pure-input schema emission stay open. Every pinned
ZodString check family now has an admitted native operation, but this does not
close the broader schema gate. Checklist660/682 is unchanged.

The clean committed case worktree was removed after exact HEAD,17 integrated
hashes and zero open paths. `/private/tmp/fl-case-cleanup.json` records4,214 files
and45,191,355 logical bytes removed; proof branch/receipts and active caches remain.


### 2026-10-15: Configured cached billing date coercion

Configured Checkout and PaymentLink reads now normalize their five declared date
fields before PostgreSQL typed DTO validation, through an additive BillingStore
hook. Epoch milliseconds and admitted ISO strings preserve chrono extended years;
optional omission and opaque provider data remain unchanged. Reads do not rewrite
cached or encrypted bytes. Generic typed reads retain their existing validation.
Locale-dependent/offsetless strings and JavaScript dates outside chrono's range
remain explicit unsupported boundaries; non-JSON Date/NaN observations are not
native parity claims. One existing workspace validation dependency edge was added,
without version changes.

Actual TypeScript160, native135 shared JSON comparisons (3 tests), restricted
PostgreSQL HTTP1 with35 date cases, configured369, legacy170 and artifact186 checks
pass. Full strict Clippy29.09s and minimal3.57s pass; owned database cleanup is0.
Logs use `/private/tmp/fl-billing-cached-dates-`. All10 frozen paths match
`/private/tmp/fl-billing-cached-dates-final.json` SHA256
`715f1416cf521da7625526f9f555c97b00662f27f5afd96299e5ed7bb084d818`.
CI includes the actual date oracle and existing native read suites. Cache-selected
lists and broader billing gates remain open. Checklist660/682 is unchanged.


### 2026-10-15: Configurable organization access control

Trusted immutable OrganizationAccessControl adds bounded resource catalogs and
explicit static role maps across organization, member, invitation, team and
dynamic-role permission paths. An explicit map replaces default roles. Exact
names, role OR/short-circuit order, same-role dynamic merges and creator-role
bypass boundaries follow the actual source. Finite Object.prototype-name cases,
Zod __proto__ stripping, inherited lookup faults and JavaScript own-key order are
covered without arbitrary JavaScript execution. Existing scoped-service mapping
remains separate. Configurations are bounded before I/O.

Actual-source59 observations pass:57 declarative cases and2 explicitly residual
custom-authorizer cases. Final organization11 tests pass0.30s; strict full and
minimal all-target Clippy pass12.13s/24.03s. The final restricted PostgreSQL
integration passes226.37s after all prototype changes; owned roles/records/audit
cleanup is0/0/0. All27 frozen paths match
`/private/tmp/fl-org-access-freeze.json` SHA256
`3fc4661f44b26693c916c0f6c384b944fc8c276d7e878ecebce4179980d0d225`.
Logs use `/private/tmp/fl-org-access-`; CI includes the new actual fixture.
Custom Role.authorize callbacks, remaining asynchronous options, custom default
teams, topology/schema extensions and hooks remain open. No dependencies changed.
Checklist660/682 remains unchanged.


### 2026-10-15: General numeric string length bounds

Commit6a7a6d9bc adds MinFloat/MaxFloat/LengthFloat with every IEEE registration,
JavaScript diagnostic number formatting and NaN-inert exact length comparisons.
Existing u32/borrowed APIs remain supported. Ordered custom messages, UTF16 length,
trim/case composition and atomic outputs are covered. No dependencies or schema
emitter changed; lone UTF16, non-JSON domains and broader schema gates remain open.

Actual/native10,200 comparisons, full96/minimal90 tests, strict validation/Axum
checks, Unicode regression2,224,128 scalar plus13,855 context cases and live369HTTP/
177MCP/3safe500/6TypeScriptSDK/6RustSDK calls pass. Full/minimal tests preceded an
equivalent Clippy partial_cmp adjustment; focused test, oracle, strict checks and
live consumers followed it. Exact commands/source order are in
`/private/tmp/fl-string-float-length-receipt.md`; logs share that prefix. All6
integrated hashes match `/private/tmp/fl-string-float-length-source.json` SHA256
`c6dcb70c3d06f7187bd299ad9f3412166ce690c5a4b8f861401baf33638bfe80`.
CI includes the new oracle. Checklist660/682 remains unchanged.


The committed string-length worktree was removed after exact HEAD, six integrated
hashes and zero open process/file references were checked. Receipt
`/private/tmp/fl-string-float-length-cleanup.json` records4,219 regular files and
45,247,323 logical bytes removed; the proof branch and native caches are retained.


### 2026-10-15: Shared restricted PostgreSQL auth test harness

A shared test-only lifecycle now gives focused organization tests the same
restricted role, tenant separation, encryption and audit setup as the retained
full auth integration. Both exercise the common organization suite, including
baseline lifecycle, delayed HTTP clocks, foreign-tenant quota seed and eight
policy families. Proof execution is spawned so proof panics still reach cleanup.
An explicit committed-write/panic regression verifies cleanup of records, audit,
unique/lookup claims and the temporary role. Production behavior is unchanged.

Focused organization PostgreSQL passes15.98s, deliberate-panic cleanup0.27s and
retained full auth PostgreSQL231.39s. Strict full/minimal all-target Clippy passes
12.90s/13.44s. All5 paths match `/private/tmp/fl-org-pg-harness-freeze.json` SHA256
`c2e4e2a37fa1f817f88f475645181bc1efb7f2e94f10fd09c0ce7e18f6ed5eec`.
Logs use `/private/tmp/fl-org-pg-harness-`. CI retains the full suite and adds the
focused and panic-cleanup tests. This reduces the cost of validating subsequent
organization changes without reducing full integration coverage. Checklist660/682
is unchanged.


### 2026-10-15: Configured cache-selected PaymentLink lists

Selected configured lists now enumerate local cache keys for absent/empty IDs,
preserve explicit order and duplicates, reject missing/malformed rows, validate
the complete local selection before provider reads, retrieve retained references
sequentially and filter explicit boolean active values. The trusted default limit
is100 with a hard maximum1000. Concrete Memory/PostgreSQL stores bound key selection
before cloning/decryption/audit and never silently truncate. The compatible custom
store fallback must use its existing list API to enumerate; it does not claim the
concrete stores' bounded enumeration cost. No provider account-wide scan, cache
rewrite, alias provisioning or new monetary authority is introduced.

The signed PostgreSQL HTTP/MCP proof found and fixed an actual integration gap:
no-parameter GET/list dispatch must use the existing request context and selected
TypeScript error formatter. Request admission remains unchanged. Actual source373
and Express-error72 cases, native4 tests, PostgreSQL HTTP/MCP1 (1.42s), legacy170,
artifact186 and full/minimal strict Clippy5.88s/3.54s pass. Existing CI fixtures
cover the expanded cases. Owned database/role cleanup is0. No dependency changes.

All11 paths match `/private/tmp/fl-billing-configured-list-final.json` SHA256
`180b7f8a887aed31910bcccedb6d42101479e035daa8e717cde5e087b59a01bd`.
Logs use `/private/tmp/fl-billing-configured-list-`. Initial failing rolling logs
were overwritten during repair; `initial-failures.txt` explicitly records excerpts
and root cause, not an original full log. Remaining configured mutation preflight,
provider reconciliation and broader billing gates stay open. Checklist660/682 is
unchanged.


### 2026-10-15: Fresh immutable packaged CLI, services and HTTP/MCP proof

Exact source commitd3b63b39dea72b5b4f34567d3f8ab5b24c94a993 was archived without
Git state, packaged and installed offline/locked using the recorded CLI lock.
The retained archive contains3,052 regular files (6,982,095 compressed bytes), SHA256
`25ba44f70c047c58ddf58e01beeee1b5d64a12bf7681d0bc8f75f60a84142244`, at
`cli/target/parity-artifacts/forklaunch-d3b63b39d-25ba44f7.crate`.
All extracted regular source bytes were checked against the archive after runtime
validation. Dotfiles, portable source/data/license guards, Unicode casing data,
regex feature forwarding and the three intentional PEM fixtures passed checks.

The installed CLI generated and relocated the application, portable services and
all four worker variants; workspace/infra feature checks, OpenAPI/AsyncAPI export,
contract/OpenAPI/socket/imported-AsyncAPI SDK generation/check, strict TypeScript
consumers, Rust SDKs and direct native generators pass. Packaged IAM tests pass229
with3 database tests explicitly ignored, in166.30s; this is not a database proof.
AsyncAPI22, WebSocket and MCP integration checks pass. Fresh generated service
HTTP/MCP startup passes five configurations, including shared/separate listeners,
disabled/custom endpoints, explicit/ephemeral/fallback ports, initialization,
discovery, health, Origin rejection, atomic startup rejection and graceful shutdown.
All five capability callbacks pass actual TypeScript signature and invalid/replay/
redelivery checks. This remains local validation, not AWS or public publication.

Driver43282 and runtime65918 exited0. Receipt and logs use
`/private/tmp/fl-oct15-case-package-`; the metadata records exact source/archive/
lock/log hashes. This includes Unicode and string numeric bounds, declarative auth
access control and cached billing dates; later auth-harness/list/array/authorizer
changes are excluded from this immutable checkpoint. Owned extraction/install/
generated roots were removed after no-open-file/process checks and retained archive
hash verification:4,226+5,579 files,267,976,298 logical bytes. Native caches and the
archive remain. Checklist660/682 is unchanged.


### 2026-10-15: Checked general array bounds and messages

Commit70157fe93 adds ZodArrayRule/Check, zod_array_checks and CheckedArray while
retaining the existing Array/u32 API. General IEEE bounds preserve last registration
per kind, exact/min/max diagnostic order, NaN-inert checks and custom messages.
Dirty container checks still evaluate elements; union recovery, intersection
behavior and atomic public outputs remain intact. Rule/message resources are
charged before allocating output. Sparse/non-JSON domains, arbitrary callbacks and
numeric cardinality schema emission remain separate open work.

Actual/native169,920 comparisons, full98/minimal92 tests and strict validation/Axum
checks pass. Final focused2 tests, legacy u32array117,963 and shared string10,200
comparisons pass after the mechanical check-count helper/lifetime lint edits.
Live387HTTP/186MCP/3safe500/6TypeScriptSDK/6RustSDK cases pass. No semantic differential
failure occurred; the full/minimal suites preceded those mechanical edits, while
final strict, focused, differential and live checks followed them. Commands and
source order: `/private/tmp/fl-array-float-length-receipt.md`; logs share the prefix.
All10 integrated paths match `/private/tmp/fl-array-float-length-source.json` SHA256
`e45d482641f069a7b2b8bf803e009797db2d197f9ea25e9929b66b00ca001f97`.
No manifests/dependencies changed. CI includes the new oracle. Checklist660/682 is
unchanged.


The clean committed array worktree was removed after exact HEAD,10 integrated
hashes and no active/open path checks. `/private/tmp/fl-array-float-length-cleanup.json`
records4,228 regular files and45,349,618 logical bytes removed; proof branch and
native caches remain. Main was also fetched again: origin/main e36cfd29c has zero
commits missing from this branch.


### 2026-10-15: Configured PaymentLink update preflight

Generated Stripe services now explicitly select a separate configured PaymentLink
update profile. It reads/normalizes cached DTOs and validates admitted patches
before provider mutation, matching configured TypeScript effect ordering. Generic
services and the read-only selection retain their existing provider-first behavior.
The public mutation target is unchanged; this adds no alias remapping or monetary
authority. Selected errors use the TypeScript status/text/correlation path.

Native1 and restricted PostgreSQL/HMAC HTTP1 (0.65s) pass, along with actual mapper
preflight30, configured373, legacy170, artifact186 and full/minimal strict Clippy
19.50s/3.58s. Actual fixture43 consists of39 owned-route/final-error-handler cases
and4 actual parse-middleware/controller successes. The latter prove null/number
coercion remains a separate request gap: amount:null becomes0 in TypeScript, while
the native typed request currently rejects it. The owned error route bypasses
controller admission; its39 cases do not prove full request-schema parity. Cache
and returned successful amount follow the provider line sum, not the requested
amount. Owned database/role cleanup is0. CI includes the new source/native tests.

All9 paths match `/private/tmp/fl-payment-link-update-final.json` SHA256
`39b0087e9a2bfd3ddf9c91787d5e2628472aabf6ab75b0304bb983f94e3b7344`.
Logs use `/private/tmp/fl-payment-link-update-`. No dependency changes. Remaining
request coercion and broader billing gates stay open. Checklist660/682 unchanged.


### 2026-10-15: Trusted synchronous organization role authorizers

OrganizationRoleAuthorizers binds exact own-role callbacks to an immutable access
control snapshot. Installation replaces that snapshot; a later with_access_control
replaces roles and clears callbacks. Typed synchronous callbacks receive borrowed
role/statements/admitted permissions, return Result<bool>, preserve role order and
short-circuiting, and suppress declarative fallback on false. Stored dynamic roles
replace matching callbacks. Creator bypass and malformed stored-grant ordering
remain unchanged. Callback errors become sanitized empty500. The callback request
removes __proto__ as actual Zod admission does; an entries helper exposes JS own-key
order. Declarative config retains its Eq/Debug API. Arbitrary JS receivers, invalid
return shapes, asynchronous scheduling and panic isolation are not claimed.

Actual fixture16 includes15 admitted cases and1 nondeclared undefined-return
observation. Final coherent-source organization13 (1 ignored) and focused restricted
PostgreSQL1 pass0.29s/16.09s, including prior declarative and callback lifecycles.
Full/minimal all-target strict Clippy passes19.01s/12.99s. The initial organization
run was preliminary because array integration and a callback admission correction
followed it; final tests include both. Only a redundant must_use attribute was
removed after final tests, with strict checks on final source. All owned database,
audit/index and role cleanup counts are0.

All18 paths match `/private/tmp/fl-org-authorizers-freeze.json` SHA256
`d4aec2b0e1ad8b6ff5763519e1cc30e1ec744f8ec22d0fdb80827b8a52f32c00`.
Logs use `/private/tmp/fl-org-authorizers-`; CI adds the actual oracle and existing
focused PG exercises the new cases. No dependencies changed. Remaining async
organization policies, topology/schema/hooks and broader Better Auth gates stay
open. Checklist660/682 unchanged.


### 2026-10-15 — Exact opt-in array cardinality export

Integrated a2a50e304 preserves fractional and nonfinite Zod array bounds through
explicit zodChecks metadata, native schema validation and both SDK consumers.
Baseline export bytes and explicit own-key overrides remain unchanged. A shared
IEEE cardinality kernel avoids silently capping bounds. No dependencies or vendor
sources changed; the contract manifest adds only a gated example.

Both JSON modes pass 7,699 instance comparisons across 966 schemas, 128 baseline
controls and 44 raw probes. The arbitrary-precision mode explicitly retains six
known draft04 raw-schema admission exceptions; those are outside the new profile.
Common15/contract39/validation98, feature-off contract31, strict Clippy and all12
TypeScript roots/625 exports pass. Consumer coverage is38 live HTTP responses and
152 generated SDK codec/type-admission checks. Prior checks3332 native, array169920,
string10200, baseline bounds153972 and pure-export577+246 regressions pass.

All13 integrated source hashes match `/private/tmp/fl-cardinality-source.json`
SHA256 `168a6285f4004291d75f0271fe69ad269a7ccc920588e90f553a84beeb6bf033`.
Detailed evidence and corrections: `/private/tmp/fl-cardinality-receipt.md` and
`/private/tmp/fl-cardinality-*.log`. CI runs the new oracle in both number modes.
This closes the retained-array cardinality export slice; broader schema/emitter
and validation gates remain open. Checklist660/682 (96.8%) is unchanged.


### 2026-10-15 — Awaited organization creation callbacks

Trusted allow-create and organization-limit callbacks now preserve actual awaited
ordering, captured public-user context, list-read failure ordering and token-based
active-session updates. A true limit result denies creation. Hooks run outside
writer transactions; pending cancellation performs no graph writes. Callback
preparation/future errors produce sanitized empty500 responses. HTTP timestamps
sample the clock after awaited admission. Scalar defaults remain unchanged.

The actual-source oracle passes31 observations, including state/token races and
late timestamps. Native organization14 (1 ignored), focused restricted PostgreSQL1
(17.31s), full/minimal all-target strict Clippy pass. Restricted tests cover reentry,
races, rollback, lost acknowledgements and cancellation; all five cleanup counts
are0. Native atomic graph writes remain an explicit difference from upstream
partial persistence on failures. No post-await authorization conditions were added
to the callback path. No dependencies changed.

All11 source paths match `/private/tmp/fl-org-creation-freeze.json`, SHA256
`a49bc845b00a7ebc95f9371d24ad7ea7a1b155fce9622810740963b0166b5edc`.
Logs: `/private/tmp/fl-org-creation-{actual,native,pg,clippy,minimal-clippy,cleanup}.log`.
The initial compile missed one existing helper import; final coherent proof follows
its restoration and includes a2a50e304 shared cardinality integration. CI adds the
actual oracle; the existing focused PG job includes the new cases. Broader auth
options/hooks remain open. Checklist660/682 is unchanged.


Creation-callback follow-up: removed one trailing space in the committed upstream
fixture after diff-check reported it. This whitespace-only change does not alter
the ca673b4e1 frozen-source proof; no native source changed. The cardinality export
worktree was also removed after exact fd02a44e4 HEAD, clean status, all13 source
hashes and zero owned processes were verified. Cleanup receipt:
`/private/tmp/fl-cardinality-cleanup.json` (54,516 KiB logical; no physical disk
reclamation claim). Branch, source manifest and logs are retained.


### 2026-10-15 — Configured PaymentLink request Number coercion

The selected configured update now applies JavaScript Number coercion before auth,
validates remaining fields and preserves original raw HMAC bytes. Null, strings,
booleans and arrays follow actual admission; NaN conversions fail before provider
I/O. Typed service inputs admit infinities and reject NaN. Nonfinite results are
internally omitted only for this mapper, whose requested amount is never forwarded
and whose output/cache amount is replaced by the provider line-item total. This is
not a generic JSON Infinity representation.

Actual102 observations include42 live Express requests through actual parse/auth/
controller/service/mapper with owned registry/cache seams. Native restricted PG
HTTP passes (1.57s), including6 MCP and7 generated TypeScript SDK calls with strict
input compilation. Typed/domain1, metadata projection1, configured373, legacy170,
artifacts186 and full/minimal strict Clippy (20.96s/7.23s) pass. Owned database/role
cleanup counts are0. The new SDK fixture runs inside the existing HTTP test; CI
provides TSC_BIN explicitly. No dependencies changed.

Important remaining differences: actual TS metadata exposes inner number while
native selected request metadata describes wider admitted JSON inputs; invalid
HMAC returns native401 versus actual403, queued separately. Raw numeric overflow
and lone-surrogate parser limits remain. No new generated Rust SDK proof claimed.
Initial metadata registration, auth-status and missing required SDK fixture id
failures are preserved in separate logs; final tests follow their corrections.

All9 source hashes match `/private/tmp/fl-payment-link-coercion-final.json`, SHA256
`79fe81394684a8c715235e9f1bea03455e3136328b6da7aeb1f6368dab9d7d00`.
Evidence uses `/private/tmp/fl-payment-link-coercion-*.log`. Broader configured
billing/auth gates remain open. Checklist660/682 is unchanged.


### 2026-10-15 — Pure-input record key schema conformance

Integrated5025496c1 retains admitted record key constraints as propertyNames in
explicit pureInput2020 output. String-domain specialization avoids coercing AJV
attempts to mutate immutable property-name variables. Baseline OAS/general2020
exports stay unchanged; explicit final metadata keeps existing override semantics.
Unresolved key-schema references reject explicitly. Key transformations/coercion/
effects and native lone-surrogate boundaries remain separate work.

The initial350-case audit exposed88 mismatches. Final coverage passes2400 exact
actual/AJV/native/injected-consumer comparisons across96 schemas,41 live SDK HTTP
responses and164 generated TS/Rust codec/type-admission checks. Legacy1744 cases
across160 exports retain16 explicitly asserted raw OAS3.0 AJV prototype exceptions;
pure577+246 regressions and strict12-root/625-export types pass. No Rust sources,
dependencies or locks changed. Final fixture-only controls follow the native run;
production changes afterward were formatting only.

All3 integrated hashes match `/private/tmp/fl-record-pure-source.json` SHA256
`a1024ee0ca02dfa3b198af4be75d8d85bbc7bad4bb06ef208cd138636481a8eb`.
Receipt and logs: `/private/tmp/fl-record-pure-receipt.md` and matching prefix.
CI runs the new oracle with native checks. Broad schema/emitter/validation gates
remain open; checklist660/682 unchanged.


### 2026-10-15 — Validated CORS delegate integration checkpoint

Awaited CORS origin delegates are frozen on isolated commit
`fb1aa08a05f2ebc40fde3cf19cbb8cae535aa691`, pending integration after the shared
billing-auth router edits are committed. This is not a claim that main contains
the feature. The typed callback receives joined optional Origin bytes and returns
one awaited static policy or sanitized error; missing/empty headers, disabled
versus denied allowlists, callback-once and cancellation are explicit.

Eight native tests,64 actual/native HTTP comparisons, strict full/minimal
all-target checks and810 static CORS regression pairs pass. The static regression
retains160 explicitly asserted204/304 Content-Length transport differences;650
pairs are exact. Callback errors preserve the actual early-middleware body
correlation fallback even when the outer response has a supplied correlation
header. Regex/multiple-callback/JS coercion behavior remains outside this typed
interface. No dependencies changed.

Source manifest `/private/tmp/fl-cors-delegate-source.json` SHA256
`e901506493e92e1b7ac97a8633b6236352d05f433f3cc09a2ad6bb44e082fac8` and
`/private/tmp/fl-cors-delegate-receipt.md` retain proof and integration instructions.
Both native chains completed; CLI cache handed back to billing.

The previously integrated record worktree was removed after exact625c58808 HEAD,
clean status, all3 source hashes and zero owned processes were verified. Receipt
`/private/tmp/fl-record-pure-cleanup.json` records54,696 KiB logical; branch and
proof retained. Checklist660/682 remains unchanged.


### 2026-10-15 — Awaited membership quota callbacks

Trusted add-member and invitation acceptance now await configured membership
quota callbacks outside writer transactions. Captured count and public user/org
context, full IEEE numeric comparison, current pending-invitation CAS and actual
team/session race behavior are preserved. Scalar defaults stay unchanged; full
organization hydration uses the upstream callback-present fallback100. Native
atomic graph rollback remains an explicit boundary from upstream fault behavior.

Actual81 comparisons, native organization15 (1 ignored), focused restricted PG1
(19.73s), and strict full/minimal all-target Clippy (12.94s/12.41s) pass. All five
owned cleanup counts are0. Exact18 source hashes match
`/private/tmp/fl-membership-callback-freeze.json`, SHA256
`9bc24b0b7606dc9f791c52adee3246fe7d884db1e3653a4ae9b5ae77e4ea5bff`.
Logs use `/private/tmp/fl-membership-callback-`. Initial shared route-key compile
failure was fixed before final tests; later strict fixes changed imports/helper
module layout only. Final proof includes coherent billing auth/router drafts,
excludes later CORS/raw-path integration. CI adds the actual oracle and existing
focused PG includes new cases. No dependencies changed. Broader auth gates remain
open; checklist660/682 unchanged.


### 2026-10-15 — CORS delegates integrated and workspace cleaned

The previously frozen CORS implementation is now integrated as4c71cb5d5. All7
committed source hashes match its source manifest; the shared router file was
staged selectively so concurrent billing auth/raw-path drafts remain unstaged.
Existing isolated native8/live64/strictfull22.68s/minimal27.67s/static810 evidence
is retained as described above. Billing's final coherent graph validation will
also run the8 CORS tests; this receipt does not claim that pending run completed.
CI now executes the live delegate comparison as well as static CORS regression.

Removed only the completed CORS worktree after exactfb1aa08a0 HEAD, clean status,
all7 source hashes and zero owned processes were verified. Cleanup receipt
`/private/tmp/fl-cors-delegate-cleanup.json`:4247 regular files,45,577,931 logical
bytes. The branch, manifest and proof logs remain. Broader CORS/route/error gates
remain open; checklist660/682 unchanged.


### 2026-10-15 — Pure discriminator missing-tag dispatch

Integratedc39ec43b4 makes explicit pureInput2020 branches require their discriminator
unless Zod's existing undefined option selects that exact branch. Default callbacks
are never invoked. Canonical JSON prototype-named discriminators require an own
key; null/custom-prototype direct JavaScript objects remain outside that JSON
representation. Explicit branch required annotations and final union annotations
retain existing caller-owned semantics. Baseline/general exports stay unchanged.

The initial140-case audit found12 mismatches. Final900 actual/native/injected
comparisons across96 schemas pass; raw AJV has894 exact plus6 explicitly repaired
prototype exceptions. Owned-engine648 admission controls preserve existing coercion
policy without claiming blanket strict-Zod equivalence. Live SDK44 HTTP and176
TS/Rust codec/type-admission checks pass, along with strict generated source,
12-root/625-export TypeScript, pure577+246 and union11106/616exports plus162TypeBox
and108liveSDK regressions. No native source/dependencies/locks changed.

All3 hashes match `/private/tmp/fl-discriminator-input-source.json`, SHA256
`8fb276edd04ce7fe92c8ae7b6b15df0994c607a937ca419de62cad4b5a3dafaf`.
Receipt/log prefix `/private/tmp/fl-discriminator-input-`; CI adds the new native
oracle. Broader schema/validation gates remain open; checklist660/682 unchanged.


### 2026-10-15 — Configured billing HMAC wire behavior

Generated Stripe billing now selects actual HMAC semantics for its four internal
PaymentLink/Checkout/Subscription/Plan families. Portal JWT, bound capabilities
and generic builders retain historical behavior. Selected verification matches
part ordering/error classes, old/future/repeated signatures, raw bodies and local
mounted paths; route-specific keys remain private. Original URI path provenance
is additive. Per-route error formatting affects rendering only; billing explicitly
declares selected401/403 text schemas, preserving absent generic declarations.

Actual91 HTTP observations, native3 tests, restricted PG HTTP1 (1.56s) with36 auth
cases/11 generated TypeScript SDK calls/6 MCP calls/Portal JSON controls pass.
Artifacts186, generic170, shared auth5 and integrated CORS8 regressions pass.
Strict billing full8.94s/minimal6.30s and shared Axum+HTTP8.05s pass. Source corrections
included Hash key choice, lint-only refactoring and fixture metadata assumptions;
original failures are retained. No dependencies/locks changed.

Remaining measured differences: actual trailing-slash routes accept200 but native
strict matching returns404 with zero provider effects; legacy and offsetless Date
strings accepted upstream remain unsupported. ISO/full TimeClip is covered. SDKs
forward caller-supplied Authorization, so callers choose the mount-local signed
path; no automatic signing rewrite is claimed. Follow-up routing/date work remains.

All11 frozen paths match `/private/tmp/fl-billing-hmac-final.json`, SHA256
`e865ceb95942f44b34a55367855104ff20043fb764a4a00ff6586d2cb57211c9`.
Detailed proof: `/private/tmp/fl-billing-hmac-receipt.md` and matching log prefix.
CI adds the actual HMAC oracle; existing backend/artifact jobs exercise selected
behavior. Broader auth/billing/route gates remain open; checklist660/682 unchanged.


### 2026-10-15 — Full numeric CORS maxAge

Integrated3f5ed284f adds explicit max_age_number for JavaScript Number values,
preserving fractions, negatives, signed zero, NaN and infinities as header text.
It takes precedence over the unchanged legacy safe-integer field. Only preflight
responses emit the header. A feature-gated common formatter reuses existing ryu-js;
Axum adapter enables the feature, while common remains dependency-free by default.
No dependency version or lock changed; two feature declarations changed.

Actual direct32 middleware observations, common16 tests, CORS9 tests and1350 live
TypeScript/native static comparisons pass. The latter has950 exact pairs and400
explicitly asserted204/304 Content-Length transport differences. Full common/Axum
all-target Clippy7.19s, minimal adapter Clippy14.38s and feature-off common8 tests
pass. One documentation backtick lint was fixed after semantic tests; final strict
checks use corrected source. Logs: `/private/tmp/fl-cors-numeric-*.log`.

All8 integrated hashes match `/private/tmp/fl-cors-numeric-source.json` SHA256
`db47427ef9dafe65aab77410d29ef47e8f026530844e5788c0e9e8b6f2fc1ed2`.
Existing CORS CI runs the expanded matrix. Exact committed worktree was removed
after clean status,8 source hashes and zero owned processes were verified;
`/private/tmp/fl-cors-numeric-cleanup.json` records4255 regular files/45663893
logical bytes. Branch/proof remain. Earlier discriminator cleanup likewise retains
`/private/tmp/fl-discriminator-input-cleanup.json` (54,704 KiB logical). Broader
route/error gates remain open; checklist660/682 unchanged.


### 2026-10-15 — Exact opt-in numeric bound exports

The zodChecks exporter now retains all finite inclusive/exclusive numeric bounds,
treats NaN comparisons as inert and represents impossible finite-number domains
without serializing nonfinite schema values as null. Impossible numeric assertions
remain inert for non-number nullable branches. Baseline180 export hashes and own
bound annotation overrides remain unchanged. No native sources/dependencies/locks
changed.

Final12060 comparisons across670 schemas pass in both native number modes;
48 live SDK HTTP responses and192 generated TS/Rust codec/type-admission checks,
strict12-root/625-export types,153972 prior bounds and2220 prior checks pass.
An expanded wrapper audit caught an initial unconditional-false nullable bug;
the retained failing proof precedes corrected native/consumer reruns. Final source
was unchanged after validation. CI runs the oracle in both number modes.

All3 hashes match `/private/tmp/fl-number-bound-export-source.json`, SHA256
`9c8133dda255857a90617deb47e80b5b079bc9a7cb62865318c56066e95f1854`.
Detailed receipt/log prefix: `/private/tmp/fl-number-bound-export-`. Broader
schema/emitter/validation gates remain open; checklist660/682 unchanged.


### 2026-10-15 — Awaited role quotas and shared permission snapshots

Trusted maximumRolesPerOrganization callbacks now preserve full IEEE number
comparisons and upstream ordering: initial authorization, awaited callback, then
role count. Permission checks after the callback use the latest shared cached
grants and authorizer configuration, including updates from nested or concurrent
checks. Memory clones and PostgreSQL stores sharing the same pool and tenant
share a cache automatically; other stores can supply an explicit trusted scope.
Scopes remain tenant/application isolated. Active cache slots are bounded and
released after requests; callbacks run outside cache locks. Atomic native writes
remain an explicit boundary against arbitrary upstream adapter interleavings.

The pinned upstream oracle passes 29 cases; native organization/cache tests pass
18 with one PostgreSQL test separately exercised. Restricted PostgreSQL passes
in 21.48s; strict full/minimal all-target Clippy passes in 19.48s/15.04s.
All five owned database cleanup counts are zero. Earlier failing oracle assertions
and the corrected test-only Clippy lint remain in the retained evidence. Final
proof includes the frozen billing alias draft; it is not a committed-only graph
claim. CI now runs the permanent upstream callback oracle.

All 18 source hashes match `/private/tmp/fl-role-limit-freeze.json`, SHA256
`f1b0658a1b89f269fbdf6d109ca2b709b57fd8d28f814321f0b5f7cb04a5dbd4`.
Logs: `/private/tmp/fl-role-limit-{actual,native,pg,clippy,minimal-clippy,cleanup}.log`.
Broader auth/ORM hooks gates remain open; checklist remains 660/682 (96.8%).

The completed numeric-bound worktree was removed only after exact commit,
three source hashes, clean status and zero owned processes were verified.
`/private/tmp/fl-number-bound-export-cleanup.json` retains the receipt for
54,804 KiB logical data; branch, source commit and proofs remain available.


### 2026-10-15 — Reviewed public symbol evidence map

Added a machine-readable map for 16 common naming/hash/path declarations to
native public functions and actual-source/native consumer fixtures, with explicit
Unicode string and runtime-coercion boundaries. A CI pointer check rejects stale
source declarations, missing native functions, duplicate entries, absent fixture
outputs and paths outside the repository. All 16 references pass. This is an
index into existing conformance evidence, not a new runtime proof; no broad
package/symbol gate is closed. Unlisted declarations remain under audit.
Checklist remains 660/682 (96.8%).


### 2026-10-15 — Selected billing trailing-slash routes

Configured billing HMAC routes now accept exactly one optional final slash via
an explicit whole-path router selection. Original URI bytes remain available for
signing; canonical registry/contracts and per-method authentication stay intact.
Nesting rebases selected paths. Root has no alternate spelling; wildcard/unknown
selections fail at startup. Generic, bound and Portal paths retain their defaults.

Actual Express oracle100, native HMAC4, nested/mixed-policy integration1,
restricted PostgreSQL HTTP1 with12 generated TS SDK calls, and186 rendered
base/Stripe artifact cases pass. Strict full/minimal billing Clippy passes in
8.81s/7.22s; strict Axum passes in10.94s. A test-only function split corrected
an initial length lint. Actual automatic OPTIONS200 versus native405,
case-insensitive matching and legacy/offsetless date inputs remain measured gaps.

Seven source hashes match `/private/tmp/fl-billing-trailing-slash-final.json`,
SHA256 `e5c4ce0517510793b38550cafbb38a2a11def17271d167c5efb5ccb4f679a738`.
Detailed receipt and logs: `/private/tmp/fl-billing-trailing-slash-receipt.md`.
Existing CI exercises the expanded upstream/PG/artifact fixtures. Broader route,
auth and configured billing gates remain open; checklist660/682 unchanged.

Guarded cleanup removed197 generated cache files (565,967,508 logical bytes)
whose dependency files referenced previously removed, owned package roots.
Retained archive hashes, inactive target, no open files and unchanged inode,
mtime and size were verified. Current sources and proof archives are preserved;
`/private/tmp/fl-oct15-retired-package-cache-cleanup.json` records the guards.

The full source-inventory check caught stale metadata from previously committed
array and unconstrained-validator changes. Regeneration with TypeScript5.9.3
now records1,709 declaration/re-export records (625 entry points unchanged);
source inventory and all16 symbol references pass after refresh.


### 2026-10-15 — Extended UTF16 bounds and immutable package validation

Integrated f7dff8dfd extends the opt-in zodChecks profile with directional
UTF16 bound arrays for finite numbers and exact NaN/Infinity/-Infinity tags.
Existing safe-integer scalar admission and artifact bytes remain unchanged.
Own bound overrides retain precedence; assertions affect strings only and share
the existing aggregate10,000-rule budget, checked before rule-vector allocation.
No dependency or lock changed. Lone JavaScript surrogate inputs remain outside
native UTF8 strings; no arbitrary transform inference is claimed.

Both native number modes pass5,020 actual-Zod/AJV/native comparisons/502schemas,
48 stable profiles and24 override/admission controls each. Contract41/full and
32/feature-off tests, strict full/minimal Clippy, prior arrays7,699 and strings
10,200,51 live HTTP responses and204 generated TS/Rust socket codec/type checks
pass. Strict types12roots/625exports and prior opted-in2220 checks pass.
Nine integrated hashes match `/private/tmp/fl-utf16-bound-domain-source.json`,
SHA256 `c91e36b03ab20ad25204329e8a13d620d614d428d8bc266516aeedfb63425268`.
Full receipt: `/private/tmp/fl-utf16-bound-domain-receipt.md`; CI runs both modes.

Separately, immutable source6fcbf887cb9b48ae804ce57a02dddd1fb2dc1004 passes
fresh offline locked CLI packaging/install and relocated runtime validation:
portable services/four workers, infra/module checks, OpenAPI/AsyncAPI, strict
Rust/TS SDK consumers and native generators,233 IAM tests (five PG tests ignored),
five HTTP/MCP listener modes and five signed capability callbacks. Startup,
discovery, health, Origin rejection, startup-failure handling and graceful shutdown
pass. Every one of3,084 archive files was byte-verified after runtime. This source
excludes later role-quota/cache, trailing-slash and extendedUTF16 changes; those
have their separate focused proofs above. No public registry or AWS claim.

Retained archive `cli/target/parity-artifacts/forklaunch-6fcbf887c-94404aa3.crate`
has SHA256 `94404aa3b007116b3a1a56c50b22e2ca3a076735771b8ff8a7c143de2ff66825`.
Runtime log `/private/tmp/fl-oct15-auth-package-runtime.log` SHA256
`2e0f74f7019f3c1746611fbfd914692b9dbad1673dee61b454e4a27f1accc060`.
Metadata `/private/tmp/fl-oct15-auth-package-checkpoint.json` records exact guarded
cleanup of two owned roots/269,373,483 logical bytes after terminal success.
An additional351 retired owned CLI cache files/1,039,414,478 logical bytes were
removed after archive/open-file/inode checks; unrelated watcher46080 was preserved.
Receipt: `/private/tmp/fl-oct15-retired-package-cli-cache-cleanup.json`.
Broad gates remain open; checklist660/682 (96.8%) unchanged.

The integrated UTF16 worktree was removed after exact commit, nine source hashes,
clean status and no active/open owned files were verified. Branch and proofs
remain; `/private/tmp/fl-utf16-bound-domain-cleanup.json` records54,876KiB logical.


### 2026-10-15 — Nested LocalRouter prefix correction

A follow-up alias audit reproduced404 instead of201 when a nested LocalRouter
received the optional final slash: canonical suffix removal failed, leaving the
mount prefix in the local path. The same bridge also mishandled local root routes.
Prefix derivation now handles the root explicitly and removes exactly one final
transport slash before matching the canonical suffix. Original raw path/signature
bytes and canonical contract/version keys remain unchanged.

The retained failing run is `/private/tmp/fl-nested-local-alias-red.log`.
Eight direct/local, root/nonroot, canonical/slash cases now pass, asserting raw
path, selected version, exactly-once preprocessing and transformed responses.
All19 versioning and148 Axum tests pass, with strict full/all-target and minimal
Clippy20.50s/6.27s. Proofs use integrated UTF16 f7dff8dfd. Two source hashes match
`/private/tmp/fl-nested-local-alias-final.json`, SHA256
`88509eb2df6fbc91c41770370c68c995e13f3eddedde1a66998214bd638d94c4`.
Receipt `/private/tmp/fl-nested-local-alias-receipt.md` names every log. Existing
Axum CI exercises the expanded regression. OPTIONS and case-insensitive routing
remain separate measured gaps. Checklist remains660/682 (96.8%).


### 2026-10-15 — Awaited invitation quota callbacks

Trusted invitationLimit callbacks now receive owned configured user/organization/
member projections and preserve full IEEE numeric comparisons. Existing duplicate
rejection and resend bypass the callback. A prior invitation cancellation commits
before the callback; preparation/future errors, quota failure and dropped futures
retain that cancellation. Current pending-count window and team checks follow the
await without repeating identity/duplicate admission. Expiry uses a fresh clock.
Default scalar/resend/delivery behavior remains covered. Trusted native callbacks
can capture services/store handles; arbitrary mutable JavaScript AuthContext and
additional-field projection remain explicit boundaries. Final writes are atomic
natively; arbitrary upstream adapter interleavings are not claimed equivalent.

Actual upstream31, native organization19 (one separately exercised PG test),
restricted PostgreSQL15.18s, strict full/minimal Clippy12.61s/12.39s all pass.
All five database cleanup counts are zero. Initial visibility/doc/test-helper
lint failures were corrected; final native/PG semantic source is unchanged by
subsequent documentation/test-helper cleanup. Proof graph includes UTF16 f7dff8dfd
and the nested-router correction. CI runs the permanent upstream callback oracle.
Twelve source hashes match `/private/tmp/fl-invitation-limit-freeze.json`, SHA256
`8f981d4fbb6c7e36b86865bfbfdde861a52d54140caac223858e7e1cfea00387`.
Logs: `/private/tmp/fl-invitation-limit-{actual,native,pg,clippy,minimal-clippy,cleanup}.log`.
Broader auth/ORM hooks coverage remains open; checklist660/682 (96.8%) unchanged.


### 2026-10-15 — Reconciliation documentation audit

Reviewed the existing scoped Stripe direct/connected and PayPal VOIDED operator
implementations against their guides. Corrected stale introductory text that
still described these integrations as future/unsupported, and added the existing
PayPal migration12 receipt flow to the open reconciliation checklist description.
The provider-specific evidence requirements and retained-reference behavior remain
unchanged. This is documentation reconciliation, not new runtime validation or a
closed gate; pending/ambiguous/charged and omitted-history cases remain open.


### 2026-10-15 — Combined IAM regression after quota integration

The full all-feature IAM library suite passes237 tests with zero failures in
164.17s; five database-specific tests are ignored in this run and retain their
separate focused PostgreSQL receipts. This covers the integrated role quota/cache
and invitation callbacks, beyond the earlier immutable package checkpoint.
Command: `cargo test --locked --manifest-path blueprint-rs/Cargo.toml -p forklaunch-iam-better-auth --all-features --lib`.
Existing blueprint target/jobs1/incremental0; session44982 terminal0. The production
graph was held fixed during the run, including billing's coherent automatic-OPTIONS
draft; no immutable/committed-only source claim is made for that draft.
Log `/private/tmp/fl-iam-role-invitation-full.log` SHA256 `3270f6e78e1aecf48afaef5b40c58c82ba93870922942cdb1ce1d27bef5a3b5d`.
Broad auth/ORM gates and checklist660/682 remain unchanged.


### 2026-10-15 — Selected billing automatic OPTIONS

Selected configured billing paths now answer automatic OPTIONS with the actual
TypeScript sorted method list, implicit HEAD, identical Allow/body, text/plain,
nosniff and byte Content-Length. The explicit path-wide selector rebases during
nesting and shares optional slash aliases. An explicit OPTIONS contract retains
its handler/authentication. CORS stays outermost; preflightContinue retains its
existing behavior. Default generic/Portal/bound paths and SDK/MCP registries stay
unchanged. Automatic responses perform no provider operations.

Actual Express116, focused native2, restricted PostgreSQLHTTP1 with11 new OPTIONS
cases and12 existing generatedTS SDK calls, all150 Axum tests, strict full Axum
7.51s/billing18.14s/minimal Axum, and186 rendered generated artifact cases pass.
Test-only import/configuration ordering corrections preceded green results;
production source remained fixed during the final proof graph and full IAM237.
Six hashes match `/private/tmp/fl-billing-options-final.json`, SHA256
`a181313db00ad0838b2fd44d1386b158f94a211eed064d03fe5dd94189daa3b5`.
Logs/receipt: `/private/tmp/fl-billing-options-receipt.md`; existing CI exercises
the expanded tests. Selection counts exact canonical-pattern methods; Express
union across overlapping patterns remains outside this profile. Case-insensitive
routing and legacy date handling remain measured gaps. Broader route/auth gates
and checklist660/682 remain open.


### 2026-10-15 — Positioned UTF16 literal predicates

Integrated0bd149fa4 extends the explicit zodChecks assertion profile to startsWith,
endsWith and includes predicates outside the portable-pattern subset, including
full IEEE numeric positions and exact UTF16 needle units. Native matching scans
UTF16 without copying the input using a bounded linear matcher. Existing portable
artifact bytes and explicit annotation merge/override behavior are preserved;
pureInput admits the supported predicates only with zodChecks. There are no new
dependencies, manifests or locks and no inferred callback transformations.

Both native modes pass6,562 admitted semantic comparisons/386schemas,52 stable
portable artifacts,5 annotation controls and16 aligned admission controls.
One JS-admitted262,144-unit schema exceeds the existing native whole-schema node
budget; it is separately asserted as a resource rejection, not counted equivalent.
The original failure remains in `fl-string-predicate-domain-native-before.log`.
Direct compiler tests independently prove the keyword unit budget. Whole-schema
limits can reject earlier; neither guard was weakened.

Common18/contract44 all-feature tests, feature-off33, focused3 and strict full/
minimal Clippy pass. Earlier literals92,880JS/30,960native, UTF16bounds5,020native,
pureInput577+246 controls,54 live HTTP responses,216 generatedTS/Rust socket
checks and strict12-root/625-export types pass. One checked-conversion spelling
was corrected for Clippy; final focused/default-native checks were rerun afterward.
Eleven hashes match `/private/tmp/fl-string-predicate-domain-source.json`, SHA256
`5c3cc57bc1f675f77431460f53d838ffbf824ac180cd86d1050a44e119c3a5c1`.
Detailed receipt: `/private/tmp/fl-string-predicate-domain-receipt.md`; CI runs both
number modes and existing expanded consumers. Broad schema/error gates remain
open; checklist660/682 (96.8%) unchanged.

Guarded cleanup additionally removed199 generated CLI outputs/734,819,579 logical
bytes referring exclusively to the completed, removed auth-package roots. Retained
archive hash, no active compiler/open candidates, unchanged inode/mtime/size and
current-source exclusions were verified. Unrelated watcher46080 remains running.
Receipt: `/private/tmp/fl-oct15-auth-package-cache-cleanup.json`.

The completed predicate worktree was removed after exact commit, all 11 source
hashes, clean status and no active/open owned files were verified. Branch and
proofs remain; `/private/tmp/fl-string-predicate-domain-cleanup.json` records
54,992 KiB logical data. Current inventory metadata now has 1,710 declaration/
re-export records; the 625 entry points are unchanged.

### 2026-10-15 — Organization callback metadata audit

Independent installed Better Auth observations (`/private/tmp/fl-organization-metadata-audit.mjs`, direct exit 0; corresponding `.log`) distinguish normal creation from adapter-seeded rows. Organization creation serializes metadata to a JSON string for storage, returns an object to the client, and supplies the stored string to invitation quota and delivery callbacks. That ordinary path agrees with the native string projection. Adapter-seeded object, null and numeric metadata also produce HTTP 200 upstream and reach both callbacks unchanged. The native shared organization projection currently rejects object and numeric metadata; bounded shape-preserving metadata projection remains part of the open ORM/options gate. This audit logged only synthetic metadata and status, and is evidence of a remaining difference rather than native conformance.

### 2026-10-15 — Awaited team count and membership quota callbacks

Integrated 25-file source manifest `/private/tmp/fl-team-limit-freeze.json`, SHA256 `e0e77e86f2b910ef3a157ad279dd03ca290f0fee5f4d3ff5840ac2b071b215ac`, independently verified before staging. Typed policies preserve captured team counts, caller-specific IEEE comparisons, duplicate seat behavior, server-only null-session creation and default-team callback bypass. Invitation acceptance supplies a borrowed current-transaction adapter, so callback mutations participate in rollback. Other callers expose autocommit operations; server member creation retains compensating cleanup. Contexts contain actual session tokens and intentionally omit Debug/Serde implementations. Independent captured stores are not enlisted.

Final actual memory-reference fixture: 81 observations, direct exit 0. Final native organization regression: 20 passed, one PostgreSQL test deliberately ignored; focused PostgreSQL passed separately in 20.25s. Strict full/minimal Clippy passed in 20.54s/12.15s. Final command chain 72097 exited 0; owned cleanup returned five zero counts. Logs: `/private/tmp/fl-team-limit-{actual,native,pg,clippy,minimal-clippy,cleanup}.log`. CI runs the permanent actual fixture and watches its path. The earlier boolean user-projection failure was fixed using the existing account projection and its failing log retained.

These proofs establish the memory-reference numeric profile and native PostgreSQL implementation. Actual configured MikroORM integer predicates for fractional/nonfinite/out-of-range limits remain under audit; native PostgreSQL success alone does not establish adapter equivalence. Metadata shape and broader ORM/hook options also remain open. Checklist remains 660/682 (96.8%), 22 open.

### 2026-10-15 — Shape-preserving organization callback metadata

The earlier metadata audit gap is now implemented in the shared organization projection used by invitation delivery, invitation quotas and membership quotas. Stored objects, arrays, numbers, booleans and null retain their JSON shape; JSON strings remain strings and missing fields remain absent. Ordinary creation's serialized storage behavior is unchanged. Before cloning, native resource checks admit at most 64 nesting levels, 4,096 nodes and 4,096 aggregate key/string UTF-8 bytes. These limits are explicit native boundaries, not upstream rejection equivalence; other identity fields retain their prior scalar validation.

Seven-file source manifest `/private/tmp/fl-org-metadata-freeze.json`, SHA256 `ed8116037462f31366abc7fe1a90599df75bcd78c55a14d6dfa2a2c09b172388`. Permanent actual fixture passes six invitation/delivery cases and five direct membership cases, including creation storage/response distinctions. Initial direct membership probe used the wrong callback argument shape and was corrected to upstream `(user, organization)` before the passing run. Native organization suite passes 21 tests (one PostgreSQL test ignored), with delivery/quota and both membership callers exercising stored shapes plus exact resource edges. Focused PostgreSQL passes separately in 22.79s; strict full/minimal Clippy passes in 23.21s/14.14s. Set-e chain19766 exited 0. Logs `/private/tmp/fl-org-metadata-{actual,native,pg,clippy,minimal}.log`; YAML parse and diff checks pass. CI runs the permanent reference fixture.

This proof used the coherent in-progress ordered-routing graph as a dependency; it does not claim final routing conformance. Broader ORM/options gates remain open. Checklist remains 660/682 (96.8%), 22 open.

Metadata reference follow-up: five additional actual invitation-acceptance cases pass, bringing the permanent oracle to six invitation/delivery, five direct add-member and five acceptance cases. Fixture SHA256 `5a1fdd097c05a784135fe3fd33d15b1ce3039bcd3ba41a20b61f0d9286c864be`; direct exit 0, `/private/tmp/fl-org-metadata-actual-accept.log`. No production changes; the prior native and PostgreSQL suites already exercised both membership callers for these shapes. The seven-file manifest above remains the immutable pre-expansion source receipt.

### 2026-10-15 — Legacy/local date grammar reference audit

The remaining HMAC date gap extends beyond the two previously identified strings. Permanent source-only oracle `framework-rs/parity/fixtures/rust_v8_date_grammar_reference.mjs` observes 400 date strings across UTC, America/Los_Angeles, America/New_York and Asia/Kathmandu: 1,600 observations using Node24.2.0/V8 13.6.233.10-node.17/ICU77.1. Valid counts are 340/339/339/340; 136 inputs vary by timezone. Explicit Los Angeles DST controls assert gap advancement and the earlier instant during a fold. This is reference evidence only; the existing timezone-independent native ISO profile is unchanged.

Pinned upstream parser and date-conversion sources were downloaded from [Node v24.2.0's V8 source](https://github.com/nodejs/node/tree/v24.2.0/deps/v8/src/date) with source URLs/hashes in `/private/tmp/fl-v8-date-parser-source/manifest.json` and BSD license retained alongside them. ISO parsing can continue into a stateful legacy parser, and local civil times require timezone conversion before TimeClip. An implementation must preserve those distinctions, named-month/timezone forms, partial-parser state, and DST behavior; adding only slash dates or assuming UTC would not close this gap. Actual oracle/log/output: `/private/tmp/fl-v8-date-grammar-audit.{mjs,log,json}`, direct exit0. No native date conformance or checklist completion is claimed.

### 2026-10-15 — Selected ECMAScript case matching and ordered routing

Fourteen-file manifest `/private/tmp/fl-billing-case-final.json`, SHA256 `cfae8090dac438f5359ac6fdd20dd20fb6856228ecd47813a0a8b92029160de5`, independently verified before staging. Optional `typescript-routing` uses the existing pinned non-Unicode ECMAScript regex engine; generated Stripe enables it. Static-segment matching preserves original URI, query, parameter and HMAC-signature bytes. Registry operation order governs selected overlapping routes; earlier GET governs HEAD handler/auth/version while the observable method stays HEAD and the body is suppressed. Automatic OPTIONS unions matching methods. Unselected exact-case routes, wildcards and native/TypeScript fallbacks retain their behavior. Canonical registry/artifacts contain no synthetic dispatch paths. The expected lock delta adds the existing local regex/hashbrown dependency closure, with no upgrades.

Actual oracle passes 125 HMAC cases, 12 installed Express order controls and 10 direct Unicode matcher controls (direct matcher and encoded HTTP observations distinguished). Native focused6 covers HEAD/auth/version, URI/escaped parameters/query, mixed route policies, wildcards/fallbacks and eight nested/root/slash callback-once cases. Full Axum156/HTTP74 pass. Restricted PostgreSQL/live billing HTTP passes in1.64s with uppercase signing, rejected-auth zero-provider checks, existing12 generated TypeScript SDK calls and11 OPTIONS controls. Missing-feature startup rejection passes. Strict Axum full2.03s/minimal10.99s and billing20.20s pass. Rendered artifact186 cases and both binary templates compile; the harness now renders each family's conditional Cargo template before checking. Original failure log retained. Final chain12309 terminal0. Exact logs and command boundaries are in `/private/tmp/fl-billing-case-receipt.md`; no generated workspace or additional target cache retained.

This closes measured case/order gaps in the selected canonical static/parameter/catch-all profile. Arbitrary Express route grammar and middleware next-route behavior are not certified; HMAC legacy/local dates remain open. Checklist stays660/682 (96.8%),22 open.

### 2026-10-15 — Configured ORM team-seat predicate profile

Fourteen-file manifest `/private/tmp/fl-team-orm-freeze.json`, SHA256 `e4f5d6ac93afd6aeb2c9f9e6521fa883a5af5fbf4e1c464e7fbb522dc8ede355`, verified before staging. `TeamSeatQueryProfile::MikroOrmPostgres` reproduces configured adapter nonfinite predicate failures at seat reservation, after duplicate-pair bypass. Callback values remain full IEEE; invitation comparisons remain their JavaScript behavior. Missing-team, direct team addition, member creation compensation and invitation-acceptance rollback preserve caller ordering. Generated Better Auth presets select this explicit profile; generic store/service defaults retain memory-reference comparisons.

Actual configured MikroORM/PostgreSQL54 cases pass, including exact empty500, finite fractional/large values, nonfinite operands, existing pairs, deleted teams and per-caller persisted state. Native organization21 pass (one PG test ignored); restricted PG passes31.92s; strict full/alltargets22.81s and minimal/alltargets14.16s pass. Sessions43450 and75222 exited0. Cleanup verifies roles/records/audit/unique/lookup/schema counts all zero; the actual fixture now explicitly drops its owned schema after table teardown. Original fixture-only wrong HTTP prefix failure retained. Logs `/private/tmp/fl-team-orm-{native,pg,clippy,minimal-clippy,cleanup}.log`, actual `/private/tmp/fl-team-limit-orm-permanent.log`. Proof includes the coherent ordered-routing graph. Generated source selection is reviewed; a new complete packaged runtime checkpoint is pending. No dependencies/locks changed. Broader ORM options and the22 broad gates remain open.

### 2026-10-15 — Trusted whole-request version diagnostics

Integrated source commit2f37c74b4 as ca42c1d05, preserving ordered-routing changes in HTTP/lib. Ten-path isolated manifest `/private/tmp/fl-version-request-parser-source.json`, SHA256 `8f0d79cb672e3cd058e693a95afa962683f19dee1f61985a0a8b92029160de5`, independently verified. Trusted per-version request parsers own complete params/headers/query/body projection and public ordered diagnostics, once per candidate; last successful projection wins atomically. All versions require registration and old field/body hooks conflict. Raw credentials/maps/bytes and wire parser/verifier stages remain intact. Operational failures remain safe fatal500 in every validation mode. Retained diagnostics are bounded; generic JSON Schema failures are not relabeled as Zod failures. ParseError formatting moved to HTTP with compatible validation reexports.

Actual498 JSON/text/form requests yield662 exact native/local response pairs plus334 protected status/callback comparisons. The text fixture now explicitly registers its trusted literal parser: all528 pairs match, resolving its72 former differences;48 MCP calls pass. Isolated full suites Axum150/HTTP77/validation98 and strict full/minimal Clippy pass. Existing form1062 comparisons pass. Existing generic JSON fixture retains24 diagnostic differences while216 exact pairs/24 MCP calls pass; automatic provenance is a separate active task. Logs and exact scopes: `/private/tmp/fl-version-request-parser-receipt.md`. Final isolated chain53621 exited0. Combined ordered-routing graph verification follows integration. CI now runs the native-enabled request-parser fixture and the configured ORM team fixture. Broader versioning/validation gates remain open.

### 2026-10-15 — Combined routing/diagnostic graph and owned cleanup

Committed graph1209c948e passes full all-feature library suites Axum156, HTTP77 and validation98; native-enabled whole-request oracle again passes498 actual cases,662 exact response pairs and334 protected status/callback pairs. Strict all-feature/all-target Clippy across all three crates passes12.87s. Root chain58465 exited0; logs `/private/tmp/fl-oct15-combined-{routing-diagnostics,request-parser,clippy}.log`. This verifies the merged HTTP/lib changes, not merely the isolated agent branches.

Removed the obsolete committed diagnostics worktree `/private/tmp/fl-version-request-diagnostics` only after exact2f37c74b4 HEAD, clean status, all10 source hashes, and zero active-process/open-file references were verified. Branch retained. Cleanup receipt `/private/tmp/fl-version-request-parser-cleanup.json`:4,275 regular files,45,962,724 logical bytes; no physical APFS-space claim. The new provenance worktree and unrelated workspaces remain intact.

Immutable1209c948e package/install and source-byte checks pass with3,111 archive files and SHA256 `fe17afc6d429b0b378d2c7b86253001de24a5ba4eed38482bb7e694ef2a6c96e`. Runtime session18469 is still active; generated service HTTP/MCP startup results are pending and are not yet counted as passed. Retained archive `cli/target/parity-artifacts/forklaunch-1209c948e-fe17afc6.crate`; checkpoint metadata `/private/tmp/fl-oct15-ordered-package-checkpoint.json`.

### 2026-10-15 — Multipart charset and stream-order audit

New actual-source oracle `framework-rs/parity/fixtures/rust_multipart_text_reference.mjs` passes49 observations through the installed Busboy and repository content-parser middleware. Default/UTF-8 fields replace malformed byte sequences; Latin1, ASCII and the parser's Windows-1252 aliases preserve byte values as U+0000–U+00FF. UTF-16LE ignores an unmatched final byte, and base64 yields encoded text. In this pinned implementation, nonempty fields using other decoder names produce an own property with undefined value; empty fields produce an empty string. Native typed and versioned multipart parsers currently require strict UTF-8 and do not reproduce these charset semantics. This is an open measured request-media gap, not native conformance.

Mixed file/text collisions are also stream-schedule dependent upstream: a file followed by text returns the file when the entire body is one chunk, but the text when bytes arrive on separate event-loop turns. Text followed by file returns the file in both controls. Therefore a buffered last-part rule cannot truthfully claim complete collision parity. Oracle preserves key presence separately from undefined values and tests both schedules. Scratch observations `/private/tmp/fl-multipart-text-audit.{mjs,json,log}`; direct exit0. Next implementation is an explicit shared text-decoding profile for typed and versioned extraction, preserving the native default and recording JSON/undefined and lone-surrogate boundaries. Multipart stream scheduling remains separately open.

Multipart decoder prototype follow-up: standalone Rust source `/private/tmp/fl-multipart-text-profile.rs` matches45 field-decoder observations and458,592 admitted direct Busboy1.6.0 decoder comparisons (all one/two-byte sequences plus boundary controls over seven decoder names). Another2,050 UTF-16 cases explicitly report the native unpaired-surrogate representation boundary; they are not counted as equivalent upstream rejection. Prototype uses strict unsafe-code prohibition and the existing CLI artifact directory, with no third target cache. Direct comparison process36099 exited0. Source/runner/log hashes are retained in `/private/tmp/fl-multipart-text-profile-checkpoint.json`. Parser wiring, version-plan/cache identity integration and typed/native HTTP coverage remain pending; no production charset behavior changed in this checkpoint.

### 2026-10-15 — immutable ordered-routing/diagnostics package runtime completion

The archive at source `1209c948e6476914e187a2338eac5dbe0b822909` passed installed CLI generation of portable services, four worker families, modules and infra, OpenAPI/AsyncAPI and both SDK checks, strict TypeScript/native checks, all five HTTP/MCP startup/listener/shutdown modes and five signed capability callbacks. Bundled IAM: **239 passed, zero failed, five PostgreSQL tests ignored** (separate focused database receipts apply), 163.59s. All **3,111 regular archive files / 7,105,514 bytes** were verified again after runtime. Archive SHA-256: `fe17afc6d429b0b378d2c7b86253001de24a5ba4eed38482bb7e694ef2a6c96e`; retained under `cli/target/parity-artifacts/forklaunch-1209c948e-fe17afc6.crate`.

Local receipt: `/private/tmp/fl-oct15-ordered-package-checkpoint.json`; runtime log: `/private/tmp/fl-oct15-ordered-package-runtime.log`. The exact two owned temporary source/install roots were removed after retained-archive hash, source integrity, process and open-file guards: **270,829,374 logical bytes / 9,963 regular files**; no physical APFS-space claim. Organization topology, multipart decoding, date grammar and automatic diagnostic provenance work outside the archived revision are not certified by this run. This is local packaged-runtime evidence, not AWS deployment or public-registry publication. Broad checklist remains **660/682 (96.8%), 22 open**.

### 2026-10-15 — trusted organization feature topology

`OrganizationFeatures` selects teams and dynamic access control independently; configured defaults retain both. Disabled route families disappear from HTTP/OpenAPI (37/32/28/23 organization endpoints), including GET/POST/HEAD behavior. Disabled teams omit implicit default-team creation, full-organization team data, invitation teamId and session/JWT activeTeamId projections while preserving stored rows for reenabling. Submitted invitation teamId is discarded; server addMember with a team fails before user/duplicate lookups. Disabled dynamic roles ignore stored grants and use static admission. Trusted Rust domain methods remain statically present and reject disabled operations before side effects; JavaScript's absent method identity is not claimed.

Actual source: four configurations, 168 route/method probes plus schema, JWT, callback, invitation and preserved-state assertions pass. Native organization regression **22 passed / one PG ignored**, 1.58s; restricted PG passes32.02s and cleanup reports zero residual records/roles/audit/unique/lookup state. Strict full/alltargets12.03s and minimal/alltargets14.03s pass. The final test-only boxed-future harness allocation change passes strict compilation; its runtime rerun was intentionally interrupted to release the shared target and remains pending. Earlier runtime results precede that allocation-only test change. Original assertion and lint failures are retained in `/private/tmp/fl-org-features-*.log`; no lint suppression or domain workaround was added.

Frozen41-source manifest `/private/tmp/fl-org-features-freeze.json`, SHA256 `f4f3673763b156d036cbfd5296d5a504001ad6dd37188831c3acc6b30c614404`, verified before integration. No dependency/lock changes. Eighteen exact obsolete IAM test executables were removed after open-file checks; source/hash/byte manifest `/private/tmp/fl-org-features-stale-test-binary-cleanup.json` retains the cleanup evidence. Broad auth/ORM gates and checklist **660/682, 22 open** remain unchanged.

### 2026-10-15 — opt-in multipart text decoding integrated

`BusboyTextOptions` now wires the previously tested decoder into both bounded typed Axum extraction and versioned buffered parsing. Strict UTF-8 remains the default; trusted opt-in supports malformed UTF-8 replacement, BOM preservation, Latin-1 aliases, UTF-16LE and base64 text, including configured default charsets. Byte limits precede decoding; file bytes remain unchanged. Unsupported decoders normalize upstream own-undefined to absent JSON fields; isolated UTF-16 surrogates return an explicit safe representation error. Mixed file/text collisions remain upstream chunk-scheduling-dependent and are not certified by this profile. Registry registration binds the complete request/options/auth snapshot and resets prepared-selection identity when options change.

Permanent runner `framework-rs/parity/rust_multipart_text_parity.mjs`: **45 actual parser cases pass in each native parser**, including typed binary preservation and schema rejection; four focused native tests also cover defaults, bounds, surrogate boundaries, registry extension/staleness/duplicates and version cache identity. Strict HTTP/Axum all-features/all-targets passes11.37s; final session36901 exits0. The existing prototype's458,592 direct decoder equivalences and2,050 explicit unpaired-surrogate boundaries remain separate evidence. Three initial new-test selection API/return-shape errors were corrected without changing parser semantics; logs are retained alongside final `/private/tmp/fl-multipart-native-{parity,clippy}.log`. Source/log hash manifest `/private/tmp/fl-multipart-native-freeze.json`.

CI runs the permanent multipart runner and organization feature reference fixture. Workflow YAML, public inventory (44/625/1710 using pinned TS5.9.3) and16 symbol evidence pointers pass. An initial inventory invocation used the local incompatible TypeScript API; the pinned rerun passed without source changes. Generated HTTP/MCP package proof remains the earlier immutable1209 archive, which excludes these new multipart options. Broader content-type/streaming and versioning gates remain open; checklist stays **660/682 (96.8%),22 open**.

### 2026-10-15 — multipart limit semantics audited against actual middleware

Permanent `framework-rs/parity/fixtures/rust_multipart_limits_reference.mjs` passes **100 actual-source observations**: whole-buffer versus byte-per-event-loop-turn input; fields/files/parts limits including zero, negative, fractional count, NaN and infinities; integer field/file byte truncation including split UTF-8; and default1MiB field truncation versus explicit infinity. Upstream field/file count limits use strict equality; part admission uses greater-than-or-equal, so fractional and negative count settings differ by limit family. Integer byte limits truncate rather than return the native resource-limit413. The actual middleware ignores Busboy limit notifications. Native safety limits and the integrated text decoder do not yet implement these parser options; the content-type gate remains open.

An initial isolated fractional **byte**-limit audit failed with uncaught upstream `ERR_OUT_OF_RANGE` from `Buffer.concat` (length1.5) under fragmented input; log `/private/tmp/fl-multipart-limits-audit.log` is retained. This is distinct from admitted fractional count limits and is not silently modeled as truncation. The passing permanent fixture covers the stated integer-byte/count scope, is in CI, and does not claim native conformance. Final log `/private/tmp/fl-multipart-limits-reference.log`; no production behavior changed in this audit.

### 2026-10-15 — automatic raw-Zod version diagnostics

Integrated isolated `3245ba9b1564cbd87ec32cd36350a059801db3d7` as `a64097eb4`; all18 source paths were verified against the commit before the merge, and existing multipart fields/exports remain present. `openapi2020(...,{zodDiagnostics:true,diagnosticDefaults})` retains versioned ordered source metadata and binds it to its exact exported projection. The validation-owned compiler registers whole-request parsers atomically using the existing native Zod evaluator. Application fixtures no longer handcraft their error evaluator. Default closures are never executed; declared defaults are identity-bound. Validation-overriding annotations and unbound effects/error callbacks reject this opt-in while baseline export remains unchanged.

Built-in JSON without a reviver exposes trusted source through a selection-local typed cache. It decodes once into the ordered JSON document across version candidates; request extensions cannot populate the cache, source/decoded sizes are checked before production, cached failures remain operational and incompatible types fail explicitly. Raw object key order is retained through whole-request composition. Callback-created order and unknown effects remain open, rather than reparsing stale raw bytes after mutation.

Final isolated proofs: **342 actual observations,342 exact native pairs in each of default/arbitrary-precision modes;120 TS requests→240 exact native/local comparisons, zero of the prior24 diagnostic differences in the opted-in profile, plus24 real MCP calls**, with verifier/handler counts preserved. Baseline mode still has216 exact pairs and24 generic-schema differences. HTTP79 (isolated base77)/validation103 (base98) unit tests pass; strict full/alltargets4.83s, minimal5.13s and changed Axum example12.17s pass. Pure exporter577 comparisons+246 controls pass. Initial draft compile/assertion/style failures were corrected and retained in the receipt. Main combined native regression remains pending the date build lease.

Agent receipt `/private/tmp/fl-zod-diagnostic-provenance-receipt.json`; frozen manifest `/private/tmp/fl-zod-diagnostic-provenance-source.json`, SHA256 `c7f09632fc37c7824e15f2772221d3333ec915b7633566811d415a277352668b`. Root also verified source bytes in `/private/tmp/fl-zod-provenance-root-source.json`. On merged main, the actual342 exporter observations and strict TypeScript7.0.2 **12 roots/626 exports** pass; current inventory is **44 packages/626 entry points/1,713 declaration/re-export records**. CI includes the native metadata differential and opted-in JSON HTTP/MCP fixture. Current inventory and16 symbol pointers pass.

This closes the measured raw-Zod opt-in fixture differences, not the generic diagnostics gate: authenticated factory preprocessing, actual module/generator adoption, runtime-bound callbacks/default identity and broader JS domains remain. No dependency/lock changes in this slice. Source/decode admission remains16MiB, provenance10k nodes/depth64 and existing evaluator limits. JavaScript-only surrogates and negative-zero declared-default serialization remain explicit boundaries. Broad checklist **660/682 (96.8%),22 open** is unchanged.

### 2026-10-15 — full V8 string-date parser for selected HMAC

Integrated the optional common `javascript-date` module and selected billing HMAC path. It ports V8 ISO-to-legacy scanner/composer behavior, UTF-16 input, full TimeClip range and overflow rules; private IANA2025b data supplies exact named-zone resolution and DST gap/fold behavior. Unix host discovery follows the audited ICU policy and uses a bounded single-entry cache keyed by the trusted process TZ value; parsing happens after releasing the guard, and explicit-offset/date-only input bypasses discovery. Runtime starts no Node process and downloads no timezone data. Common without features remains dependency-free.

Final current-source proofs: **15,527 actual Node comparisons**, five common units, four selected HMAC units, **228 added signed HTTP/correlation probes across four zones**, existing HTTP/MCP and12 generated TypeScript SDK calls per zone, and rendered artifact186 checks pass. Actual HMAC fixture passes152 cases in each zone. Strict common full/alltargets and minimal pass; billing full/alltargets18.11s and minimal11.98s pass. Security audit reports `unexpected=[]` against the configured advisory database and existing baseline; this is not a claim that all baseline advisories disappeared. Sole new dependency is pinned `tz-rs=0.7.3` with no transitive dependencies; both lock diffs contain only that package and intended common/billing edges. All seven vendored data/license files retain their recorded hashes and are visible through the actual template source symlink; no fresh packaged CLI installation is claimed.

Frozen30-source manifest `/private/tmp/fl-v8-date-final-manifest.json`, SHA256 `c44fd94751a8c5dd8c757e3f10ac29431477b9b441fdf7d42e369437d54b9996`, verified before integration; detailed logs `/private/tmp/fl-v8-date-final-receipt.json`. Initial host sentinel mismatch and Clippy style failures are retained; generated upstream offset literals have a narrow style exemption preserving original bytes. CI pins Node24.2 for the date oracle and live HMAC wrapper. Host runtime proof is macOS; separate Linux discovery verification is pending. Windows discovery, non-UTF8 TZ and validation/request/cache date-coercion integration remain explicit follow-ups. These checks do not close broader billing, validation or release gates. Checklist remains **660/682 (96.8%),22 open**.
