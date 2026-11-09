# Rust Framework And Blueprint Implementation Plan

Current verified progress (2026-10-15): **660/682 checklist items (96.8%), 22 open**.
The unchecked boxes below are the remaining gates; detailed validation receipts
are in [the parity inventory](RUST-TYPESCRIPT-PARITY-INVENTORY.md).

## Purpose

Build a Rust-native ForkLaunch stack with two new workspaces:

- `framework-rs`: the Rust framework runtime, contracts, adapters, SDK generation, infrastructure traits, and testing utilities.
- `blueprint-rs`: Rust blueprint libraries that mirror the style of the existing TypeScript `blueprint` workspace: shared core primitives, interface crates, and implementation crates for IAM, billing, workers, monitoring, and generated service scaffolds.

The Rust stack should interoperate with the TypeScript stack through shared contract artifacts. A Rust service should be able to generate TypeScript SDKs for frontend consumers and Rust SDKs for service-to-service consumers.

## Active Task Queue (2026-10-15)

Current parallel work (these are slices of the existing gates, not extra checklist items):

- Active: remaining Better Auth organization/plugin options, remaining validation date coercion/platform discovery, automatic factory diagnostic provenance for generic schema validation, and remaining multipart streaming/collision semantics.
- Latest integrated slices: full V8 string-date parsing for selected HMAC with live four-zone proof, explicit raw-Zod diagnostic provenance with automatic native parser registration, opt-in multipart text decoding with typed/versioned differential proof, trusted organization team/dynamic-role feature topology, trusted whole-request version diagnostics and configured ORM team-seat predicates, selected Unicode case matching and ordered HEAD/OPTIONS routing, shape-preserving organization callback metadata, team quota callbacks with borrowed acceptance-transaction access (memory-reference numeric profile), role and invitation quota callbacks, shared permission caching, optional trailing-slash routing, nested LocalRouter root/alias correction, selected automatic OPTIONS, extended UTF-16 bounds and positioned literal predicates. Their dated receipts below identify exact scopes and remaining differences.
- Latest combined IAM regression: 237 passed, five PostgreSQL tests deliberately ignored in that run and covered by separate focused receipts.
- Latest immutable package checkpoint: `1209c948e`, 3,111 archive files verified before/after runtime, 239 bundled IAM tests (five PostgreSQL tests covered separately), all five HTTP/MCP listener modes and five signed capability callbacks passed. This includes ordered routing, team ORM predicates and trusted whole-request diagnostics; later organization topology, multipart, date and automatic diagnostic provenance drafts are excluded.
- Current source inventory: 44 packages, 626 entry points and 1,713 declaration/re-export records; 16 reviewed common symbol mappings have checked implementation/fixture references. These are audit counts, not completed feature counts.

Earlier milestone snapshots (historical counts and boundaries apply to their named source revisions):

- Completed: immutable95f1b203b packaged runtime, generated five-family billing HTTP/MCP lifecycle, versioned text-wire profile and real shared/generated SDK/auth composition. Dated source/hash/test receipts are in the inventory.
- Completed: PaymentLink update result identity/totals guards and single configured local mapping; UUID/provider mutation ownership remains open.
- Completed: optional MCP omission, explicit optional-body contracts and safe root documentation, referenced shared-SDK special-value conversion, required Portal URLs, consumed Plan/Subscription references, and trusted resolved signing keys.
- Completed: immutable5a8eba9a8 packaged runtime with optional-body/configured-diagnostics changes, and opt-in direct PaymentLink UUID creation with encrypted intent/binding receipts, restricted PostgreSQL and lost-COMMIT-ack proofs (a53679391). These narrow profiles leave managed creation and general recovery open.
- Completed: fixed opt-in Better Auth TOTP lifecycle, restricted PostgreSQL, actual client/native bridge and generated realTCP HTTP/MCP proof (277605211 plus generated fixture receipt); Zod multipleOf, fractional/nonfinite bounds and literal string rules now pass actual/native differential tests. Broader plugin/schema gates remain open.
- Completed: opt-in managed UUID PaymentLink creation/publication with signed platform context and durable claim barriers; direct Plan creation with verified Product/Plan evidence. Restricted PostgreSQL, lost-COMMIT acknowledgements and official SDK checks pass; broader mutation/recovery remains open.
- Completed: immutable416d archive installation and portable runtime HTTP/MCP checks, including generated Better Auth29 actual-client calls and112 frontend actions; UUID/email runtime grammars81036 comparisons and default no-delivery OTP routes also pass.
- Completed: verified Customer association registration and immutable Subscription reservation/dependency barriers; trusted fixed OTP delivery plus bounded OTP digit/validity/attempt options, UUID/email/binary runtime grammars, TS email schema correction and fixed separate-channel AsyncAPI replies. Each has actual-source/native and scoped runtime/database receipts.
- Completed: trusted direct Subscription creation/publication, bounded reusable JSON composition and live HTTP/MCP/both-SDK bridge, TOTP digit/period and trusted lifecycle/passwordless options. Each has scoped native/actual-source receipts.
- Completed: GET-only interrupted Subscription recovery with exact Event request-key/version/time linkage, current dependency proofs and separate-role atomic publication; isolated PG, native provider, official SDK and strict Clippy pass.
- Completed: ordered reusable unions with raw/path-aware diagnostics,13,398 actual/native comparisons and expanded99 HTTP/45 MCP/both-SDK checks; challenge/device lifetime options pass native22 and restricted PG.
- Completed: direct scoped captured-payment adoption with atomic receipt/rollback/lost-COMMIT proof; configurable MFA account lockout with disabled zero-write success; strict JSON literals/ordered enum lowering with118,132 actual/native and117 HTTP/54 MCP/both-SDK checks.
- Completed: explicit Connected captured-payment adoption; immutable dda838 archive installs offline/locked with2666 byte-verified files and passes portable services/workers/modules/SDKs, generated HTTP/MCP listener/capability checks and packaged IAM137 tests.
- Completed: backup generation policy, plain/hashed OTP storage, ordered strict objects/records, scoped PayPal ORDER reads and VOIDED failure repair. Actual-source/native, HTTP/MCP and restricted PostgreSQL receipts cover their respective boundaries.
- Completed: built-in encrypted OTP field storage and bounded versioned key policy, with5 Node/Rust vectors, native35 and restricted PG.
- Completed: checked AsyncAPI correlation expressions and metadata import/export, including referenced trait precedence; native38, minimal22, official parser and strict Clippy pass.
- Completed: JSON-domain Null/Undefined/Void/Never and ordered nullish lowering, with4212 exact pairs per feature mode and195 HTTP/90 MCP/both-SDK checks.
- Completed: direct bound BillingPortal creation with immutable intent, Customer claims and separate-role atomic UUID publication; restricted PG, official SDK and legacy170/configured186 checks pass.
- Completed: object passthrough/catchall policies with8910 exact comparisons per feature mode and219 HTTP/102 MCP/both-SDK checks.
- Completed: immutable8403 archive installs offline/locked with2711 before/after byte-verified files and passes portable services/workers/modules/SDKs, generated HTTP/MCP listeners/capability callbacks and packaged IAM147 tests (3 PG tests deliberately ignored; separate restricted PG receipts apply).
- Completed: direct bound Checkout creation with four durable Customer/Plan claims, exact Price/line-item evidence and separate-role local UUID publication; restricted PG, Portal/Subscription regressions, SDK and configured186/legacy170 checks pass. Generated default HTTP wiring remains a separate gate.
- Completed: trusted async OTP field codecs with durable revision/lease CAS, no transaction across callbacks, final14/PG/actual10 and default HTTP clock-preservation checks. Native race safety is deliberately stronger than upstream.
- Completed: optional pinned Node24.2/Ada3.2.4 URI validation,205676 exact admission comparisons plus ordered/composition and255 HTTP/120 MCP/both-SDK checks. Factory-only WHATWG schema emission and native/AJV/socket consumer validation now also pass203261 comparisons; arbitrary RFC uri and user overrides are preserved.
- Completed: opt-in declared billing UTC ISO/epoch requestdate normalization with320 actual-source classifications, live base/Stripe HTTP/MCP and6 generated SDK calls. Default legacy and response metadata remain unchanged.
- Completed: built-in backup field storage (legacy/plain/encrypted), independent versioned keys and authorized legacy migration; actual9, final5/affected54, restricted PG and strict Clippy pass. First HTTP MFA opt-in defaults encrypted; direct services retain legacy compatibility.
- Completed: direct bound Checkout creation→read through explicit provenance-aware getter, preserving local Customer UUID with exact receipt/current Customer generation checks. New/legacy restricted PG and strict Clippy pass; default routes unchanged.
- Completed: factory-only WHATWG URI schema format with explicit native capability, AJV/injected-engine/generated socket validation, canonical366 and strict native/CLI/type checks.
- Completed: conjunctive finite numeric/array OpenAPI bounds with153972 actual runtime/AJV comparisons across OAS3.0/3.1 and strict types. Tuple/string/record emitter gaps remain open.
- Completed: trusted async backup codecs with durable phases/revision leases, lazy/generic expiry recovery and exact final authority/CAS; actual12/focused11/fullIAM178/restrictedPG and strict Clippy pass.
- Completed: ordinary union anyOf and finite literal enum dedup/mixed/empty emission,10674 comparisons and153972 bounds regressions; explicit annotations/discriminated unions unchanged.
- Completed: explicit BoundCheckoutV1 HTTP/MCP create/get/inspect and both SDKs, separate actor-scoped publisher, typed202 uncertainty and localUUID preservation; live restrictedPG/unit/artifact/generated-main/default regressions and strict Clippy pass. Other billing families/default identity wiring stay open.
- Completed: Never-leaf false-schema and required-object emission with224 exact cases, union/bounds regressions and strict types. Exact generated never types remain separate.
- Completed: finite record-key export and safe schema-property merge preservation,1744 exact pairs plus16 explicitly asserted OAS3.0AJV proto exceptions; defaultSDK own-property validation and prior emitter regressions pass.
- Completed: immutable9e374 portable services/workers/HTTP/MCP/SDK verification with2745 before/after byte checks; owned roots/cache cleaned.
- Completed: checked dense JSON Tuple items/rest with19050 pairs per serde mode, sharedbudget checks and291HTTP/138MCP/bothSDK transport comparisons. Positional schema/typedtuple models remain separate.
- Completed: trusted server-only backup viewing with opaque zeroizing result, all-profile generation/lease checks,26backup/14actual/restrictedPG and strictClippy proofs; no public route.
- Completed: aggregate bound billing selector and Portal create/IDs-only inspect routes,32configuration/artifact cases, actual generated-main compilation, HTTP/MCP/four SDK calls and strictClippy.
- Completed: private SDK own-property compilation,128cases/controls and strict types; dual-use literal schema repairs reject explicitly to preserve defaults/equality.
- Completed: optional application-owned OTP background delivery,3focused/36affected/5actual checks and strictClippy; defaultawait retained.
- Completed: stored Portal UUID rereads with sealed/current provenance, original5minTTL, no providerI/O and explicit expired410; restrictedPG/races plusHTTP/MCP/sixSDKcalls and strictClippy pass. Explicit tuple consumer/emitter slices also pass; precise Rust tuple models remain separate.
- Completed: trusted password-reset delivery/shared app-owned scheduler, actual9/domainHTTP/restrictedPG/strictClippy proofs, existing OTP aliases and explicit override precedence.
- Completed: precise fixed Rust tuple sequence models through shared CLI/native generation,79modelcases/native46/fullCLI1125 and strictClippy; implicitdraft7 prefix annotation control fixed.
- Completed: provenance-aware directly created Plan UUID library reads with Product-only enrichment,65actualTS/restrictedPG/provider/race/deadline and strictClippy checks. Aggregate read-route configuration remains separate.
- Completed: trusted explicit verification-email delivery, compatible1h email-address tokens, anonymousfloor/privacy and callbackerrors, actual11/PG/liveHTTPMCP/strictClippy proofs.
- Completed: variable-prefix/rest Rust sequence models with absent/null preservation and pre-write contiguity/bounds checks;892modelcases/strictgeneratedRust+TS/native46/fullCLI1125 pass.
- Completed: explicit aggregate Plan-read selection/GET without inferred publisher,46config/artifact cases, startup grants,HTTP/MCP/twoSDKcalls and strictClippy. Existing66legacyroutes remain.
- Completed: opt-in pure input export without named Zod callback execution;577 actual/AJV comparisons,138 controls and strict12-root/620-export types pass. Finite NativeEnum now has separate normalized projection evidence.
- Completed: signup/signin verification-required policy with no-session signup, duplicate privacy and correct-password gate; actual13/restrictedPG/fullIAM198 and strictClippy pass. Prospective-user callback visibility differs from upstream transactions.
- Completed: completed Subscription UUID library reads with sealed Customer/Plan provenance and fresh finite provider evidence; actualTS66/restrictedPG and strictClippy pass. Local lifecycle fields remain unchanged.
- Completed: reserved Rust tuple visitor/generic names, recursive SequenceVisitor/A/D/S collision proofs;920variable/79fixed/47codegen/1126CLI and strictgeneratedRust pass.
- Completed: aggregate PaymentLink read HTTP/MCP/SDK selection; restricted PostgreSQL and both live SDKs pass, including independent CI wrapper. Reserved Self model names preserve TypeScript graph names; 144 generated cases and 1,128 CLI tests pass.
- Completed: trusted before/after email-verification hooks and pre-clone PaymentLink reservation limits; actual-source, restricted PostgreSQL and strict native checks pass.
- Completed: verification auto-signin, Rust builtin-name compatibility, bounded PaymentLink list library/HTTP/MCP/SDKs, and immutable6f77 packaged runtime/startup validation.
- Completed: opt-in exact Zod UTF-16 length and numeric remainder assertions across native validators, HTTP SDK and generated socket clients; 2,220 Zod/AJV and 3,332 native observations pass. Regex/format conjunctions remain open.
- Completed: shared authentication session policy across credential, Google, MFA, verification and session flows; 22 actual-source cases, 214 full-suite tests and restricted PostgreSQL lifecycle pass. Post-await clock sampling and broader auth options remain open.
- Completed: explicit ordered email/UUID/URI conjunctions pass 30,564 actual-source and native comparisons, plus generated HTTP/socket consumers. Regex and other format families remain open.
- Completed: explicit direct PaymentLink creation and IDs-only inspection pass restricted PostgreSQL, live HTTP/MCP, both SDKs and 104 rendered startup/artifact cases. Plan/Subscription creation and broader reconciliation remain open.
- Completed: explicit CUID/CUID2/ULID/Nano ID/Base64/Base64url assertions pass 66,624 actual-source/native comparisons and live generated HTTP/socket consumers. Other raw formats and regex remain open.
- Completed: immutable d7bd packaged CLI/runtime checkpoint, with 2,858 verified archive files, generated services/workers/modules, actual HTTP/MCP startup and all five capability callbacks, strict SDKs and 214 bundled IAM tests. Later drafts are excluded; cloud/publication gates remain open.
- Completed: direct Plan creation and IDs-only inspection across HTTP/MCP/both SDKs, restricted PostgreSQL and 120 rendered artifact/startup cases. Subscription creation and broader mutations remain open.
- Historical active scope: broader credential/options parity, explicit Zod format/regex compatibility, and bound billing creation HTTP/MCP/SDK wiring. Signup auto-signin, Subscription read selection, password-reset revocation/observer, and bounded string-schema fixes have landed. Broader plugin/mapper/schema/deployment gates remain open.
- Validation: immutable4ff9e3958 package/install passed2799 files with SHA256db6cb114da0e7faabb74e01ceb1ae0da081020194764fbabbbc177171ca0033c. Portable runtime/HTTP/MCP/SDK checks now pass with195IAM tests and3PGignored; all2799bytes verified after. Current policy/pure/Subscription/collision fixes are excluded by exact source commit.
- Validation: immutable37b54 archive package/install/portable services/workers/SDK/HTTP/MCP runtime passes with2775 before/after byte-verified files and packagedIAM184 (3PGignored). Later OTPdispatcher/tupleconsumer/new drafts are excluded by source commit; owned roots and414MB exclusive nonexecutable cache cleaned with guards.
- Validation: immutable d835 archive installed with2622 byte-verified files and passes full portable runtime/HTTP/MCP checks; expanded generated MFA29 also passes, with zero remaining fixture roles/records/audits.
- Integration: private companion source-pair CI is committed with pinned source/hash verification; local locked platform61 tests/types pass. Hosted CI has not been run.
- Integration: origin/main through e36cfd29c is already merged; refetched2026-10-14 with no missing main commits. Owned completed checkpoints and redundant generated dependencies are cleaned as verification permits.

Historical completed queue and remaining broad gates:

- [x] Merge `origin/main` through `c5a4248d1` into `rohin/rust-framework` (`ac1f18e2a`), preserving the configured auth adapter and main's scoped authorization/encryption policy; all 1,120 combined CLI tests pass.
- [x] Replace generated services' TCP stub with Axum and graceful shutdown.
- [x] Remove build-machine Cargo paths by bundling framework source in generated applications.
- [x] Fix SDK mode's missing path argument and sync discovery's optional-argument panic.
- [x] Revalidate the combined workspaces: latest CLI 955 tests pass; latest all-feature framework 114 and blueprint 63 tests pass. Formatting and all-feature Clippy checks pass in both Rust workspaces. Real Redis/PostgreSQL integration and six TypeScript encryption interoperability fixtures pass separately.
- [x] Complete the post-merge generation rerun: init, change, sync, delete, OpenAPI/SDK, and relocated-service HTTP/TypeScript SDK suites pass.
- [x] Validate a real optimized generated Rust service release build through the shared exporter, including framework contracts and OpenAPI artifacts.
- [x] Validate Rust/mixed release build order, shared export dispatch, SDK package builds, failure stops, mixed crate locations, and prebuilt isolation using the actual CLI with owned platform/command fixtures.
- [x] Validate real Cargo/PNPM mixed release builds with a generated Rust service/TypeScript SDK and a minimal tsx service export hook; reuse those artifacts with intentionally unbuildable source, preserving prebuilt isolation and bounded mixed crate paths.
- [x] Complete relocated generated-service container build, health/OpenAPI, and SIGTERM shutdown checks. The Docker capacity blocker is resolved; `rust_container_runtime.sh` passes locally and runs in CI. Builder/runtime both use Debian bookworm.
- [x] Add broker-backed Kafka and generated Kafka workers, real transaction/process-crash/group-fanout tests, CI broker coverage, all three durable-worker source-archive installation, and complete embedded source packaging (currently 145 files including OTLP/CAC/request-HMAC sources).
- [x] Wire generated Redis/PostgreSQL distributed controls with separate-process command/HTTP fanout, scoped isolation, graceful disconnect failure, explicit restart semantics, and packaged CLI generation checks. HTTP commands apply through the ordered subscription to preserve command ordering.
- [ ] Validate generated ECS defaults, health checks, and service startup separately from the local container checks. The companion platform branch `rohin/rust-runtime-parity` now implements per-component native argv startup, runtime-aware Docker/CodeBuild/image-gate planning, separate mixed image contexts, HOST/PORT and worker health defaults. CLI releases carry runtime and executable metadata. Local Linux HTTP/OpenAPI/MCP and SIGTERM checks pass; actual AWS tasks/load balancers, explicit native migrations, database URL/role/tenant provisioning and cloud scaling remain unverified. Generated durable workers now support explicit control-service versus consumer roles; local Redis fanout and storage-independent control startup are validated below. See the native platform deployment receipt in the parity inventory.
- [x] Keep generated Rust SDKs for empty worker HTTP contracts free of unused Serde imports, path helpers and client-field warnings; compile packaged PostgreSQL/Redis worker SDKs with warnings denied.
- [x] Verify all 752 current embedded Rust source/template files are present in the CLI Cargo package, including documentation examples; `.template` manifests and source symlinks preserve complete generated workspaces.
- [x] Generate one shared HTTP SDK from native contracts and TypeScript OpenAPI, preserve imported clients through lifecycle refresh, and watch both languages/custom paths. All 1,023 CLI tests, strict generated Rust/TypeScript consumers, live clients against both servers, relocation, loss/recovery and native HTTP/MCP startup pass.
- [x] Dispatch per-project sync/delete by project language; preserve mixed manifest fields through TypeScript init/change/delete and async router removal. Actual Rust/TypeScript service, worker and library sync/rename/delete, preservation on unsupported router edits, all 1,025 CLI tests and native sync/delete regressions pass.
- [x] Attach native services/workers/libraries/modules directly to existing Node/Bun applications. Preserve TypeScript source/tooling and Cargo settings, generate a separate native Dockerfile, and validate relocation, both runtime variants, HTTP/MCP startup and the mixed native container.
- [x] Complete application-wide mixed sync, custom native member/dependency cleanup and SDK target selection (2026-10-12 runtime and CLI validation).
- [x] Add TypeScript workspaces to Rust-first applications with staged initialization, both package trees and native source preservation.
- [x] Compile and exercise mixed compliance clients with both IAM variants, typed native adapters and explicit coverage for legacy services.
- [x] Include existing TypeScript modules and generated HTTP SDKs in root PNPM/Bun workspaces; validate combined builds and both languages’ HTTP/MCP startup.
- [x] Finish unified mixed development orchestration (phases 13–15). Node/Bun mixed and TypeScript-only fixtures pass source reloads, contracts, SDK refresh, HTTP/MCP and owned watcher shutdown; Bun also covers socket lifecycle and watched image reuse.
- [ ] Finish AsyncAPI export and cross-language contract conformance checks (phases 13–15).
- [x] Install the CLI from its Cargo source archive outside the checkout, generate and relocate service/PostgreSQL/Redis workers, compile them, and export real framework contracts/OpenAPI plus both SDK targets; run the regression in the CLI build CI job.
- [ ] Validate registry publication and installation of published crate versions (phases 13–15). Source-archive and local sparse-registry installation of all 50 public crates are checked, including bounded regex behavior and registry-built HTTP/MCP startup. Public crates.io publication and installation remain unverified; no crate has been published during this port.

PostgreSQL IAM is wired into generated modules. General service storage, auth/provider lifecycle and billing remain explicit tasks in phase 12. Kafka has a broker-backed implementation and generated runtime coverage; additional TypeScript module families are tracked below.

## Full TypeScript Parity Scope

The requested target is full runtime, infrastructure, blueprint, CLI, SDK, release, and deployment parity. Generation checks alone are insufficient. Every remaining phase below stays in scope; a check is complete only when behavior is implemented and validated.

Current implementation work:

- [x] Enforce route authentication and RBAC in Axum, including implicit HEAD and routes with schema validation disabled; expose verified typed sessions and local-handler claims.
- [x] Implement direct JWK and cached remote JWKS verification with explicit algorithm/claim policy, rotation, response bounds, expiry, and concurrent-fetch tests.
- [x] Implement Redis-backed TTL records, binary batches, literal prefix scanning, and FIFO operations; validate against an isolated Redis container.
- [x] Add an OpenTelemetry collector and HTTP instruments/spans with SDK export assertions, correlation, and cancellation handling.
- [x] Add framework/blueprint CI covering all features, real Redis/PostgreSQL integration, and TypeScript encryption interoperability.
- [x] Add PostgreSQL IAM CRUD, role/permission resolution, persistent sessions, salted Argon2id credentials, and tenant RLS.
- [x] Port field encryption with byte-compatible deterministic envelopes/blind indexes and bidirectional legacy/rotation checks against the TypeScript implementation.
- [x] Complete streaming-body telemetry with measured frame bytes, producer context propagation, normal completion, body errors, cancellation, HEAD handling, and balanced active-request counts.
- [x] Generate portable PostgreSQL IAM modules with explicit migrations, safe runtime-role checks, typed schemas, and async opaque bearer authentication.
- [x] Add an async PostgreSQL worker queue with leases/retries and validate 50 deliveries across ten concurrent consumers, tenant isolation, and stale-token fencing.
- [x] Add generated OTLP provider/exporter bootstrap. Generated services/IAM/all workers own JSON logging, optional HTTP/protobuf or HTTP/JSON trace/metric/log providers, W3C propagation and blocking-pool shutdown flushing; actual receiver and generated HTTP SIGTERM tests pass. Offline exports run before initialization.
- [ ] Finish production persistence and encryption, IAM/provider/session lifecycle, billing/Stripe, and acknowledged database/Redis/Kafka workers (phases 9 and 12).
- [ ] Finish streaming/multipart/WebSocket runtime and SDK behavior, mixed-app orchestration, AsyncAPI export, and cross-language conformance (phases 3–8 and 13–14).
- [ ] Finish container/ECS, published-source installation, documentation, benchmarks, and release validation (phase 15).

## Backend Generation Completion Gate

Generation is tracked separately from production framework and blueprint parity below.

- [x] Bundle framework source with generated applications and use relative Cargo dependencies.
- [x] Serve generated service handlers over Axum with body limits, request IDs, CORS, and documentation UI.
- [x] Shut down generated HTTP services gracefully on Ctrl-C and SIGTERM.
- [x] Exercise init, change, sync, delete, contract/OpenAPI export, and TypeScript/Rust SDK generation in a dedicated CI matrix group.
- [x] Add a relocated-app HTTP test calling the generated Rust service through its generated TypeScript SDK.
- [x] Generate durable PostgreSQL workers and verify renewal, retries, drain, restart recovery, source preservation, backend conversion, rename, and delete against a real database.
- [x] Export accurate durable-worker contracts without unmounted example HTTP routes or internal management endpoints; validate empty application HTTP SDKs against both generated backends. Empty contracts and durable-worker manifest fallbacks also stay empty during SDK generation instead of inventing health methods.
- [x] Document usage and distinguish generated examples from production storage and queue integrations.

## Core Principles

- Contracts are the source of truth.
- Rust types define request and response shapes.
- Runtime validation still happens at wire boundaries.
- OpenAPI, AsyncAPI, MCP tools, TypeScript SDKs, and Rust SDKs are generated from the same contract registry.
- `axum` is the first HTTP adapter, not the identity of the framework.
- `blueprint-rs` mirrors the TypeScript blueprint package style, but uses idiomatic Rust crates, traits, derive macros, and generated SDK crates.

## Proposed Repository Layout

```text
framework-rs/
  Cargo.toml
  crates/
    forklaunch-common/
    forklaunch-core/
    forklaunch-contract/
    forklaunch-contract-macros/
    forklaunch-http/
    forklaunch-axum/
    forklaunch-openapi/
    forklaunch-asyncapi/
    forklaunch-mcp/
    forklaunch-sdk-codegen/
    forklaunch-sdk-rust/
    forklaunch-sdk-typescript/
    forklaunch-ws/
    forklaunch-infrastructure-redis/
    forklaunch-infrastructure-s3/
    forklaunch-testing/

blueprint-rs/
  Cargo.toml
  crates/
    forklaunch-blueprint-core/
    interfaces/
      forklaunch-iam-interface/
      forklaunch-billing-interface/
      forklaunch-worker-interface/
      forklaunch-monitoring-interface/
    implementations/
      iam/
        forklaunch-iam-base/
      billing/
        forklaunch-billing-base/
        forklaunch-billing-stripe/
      worker/
        forklaunch-worker-database/
        forklaunch-worker-redis/
        forklaunch-worker-kafka/
    client-sdk/
    sample-app/
    sample-service/
    sample-worker/
```

## Library Choices

- HTTP runtime: `tokio`, `axum`, `tower`, `tower-http`.
- HTTP extras: `axum-extra`.
- Serialization: `serde`, `serde_json`.
- JSON Schema: `schemars`.
- Validation: `garde`.
- OpenAPI: custom ForkLaunch registry and native emitter; optional adapters must preserve the canonical registry.
- Telemetry: `tracing`, `tracing-subscriber`, `opentelemetry`, `opentelemetry-otlp`, `tracing-opentelemetry`.
- Auth: `jsonwebtoken`, `reqwest`, `moka`, `hmac`, `sha2`, `base64`, `subtle`.
- Secrets and encryption: `secrecy`, `zeroize`, `aes-gcm` or `chacha20poly1305`.
- MCP: `rmcp`.
- WebSockets: Axum WebSocket extractor plus `tokio-tungstenite`.
- Redis: `redis`.
- S3/object storage: `aws-sdk-s3`, with optional `object_store` adapter later.
- Persistence: optional `sqlx` integrations behind domain store traits; SeaORM adapters are application-owned.
- Config: `config`, `dotenvy`.
- Testing: `testcontainers`, `rstest`, `wiremock`, `insta`, `proptest`.
- Proc macros: `syn`, `quote`, `proc-macro2`, `darling`.

## Rust Auth Framework Findings

There does not appear to be a direct Rust equivalent to Better Auth that should be adopted wholesale without review. The Rust ecosystem has useful auth building blocks and some higher-level frameworks, but ForkLaunch should own the `blueprint-rs` IAM contract and expose adapters where useful.

### Candidates To Evaluate

- `loco-rs`: Rails-style Rust framework with built-in user auth, password reset, verification, JWT/API auth, and SaaS templates. Useful as a reference, but likely too opinionated to embed in ForkLaunch.
- `rs-auth`: composable auth facade with core, Postgres, Axum, OAuth, hooks, email, events, and rate-limit modules. Promising, but young and needs code review before adoption.
- `auth-framework`: broad OAuth2 client/server framework. Potentially relevant for OAuth provider/server use cases, but needs security and maintenance review.
- `tower-sessions`: strong Tower/Axum session middleware with pluggable stores. Good candidate for session storage, not a full identity framework.
- `axum-login`, `axum_session_auth`: Axum session/auth helpers. Useful references or optional adapters, not a complete Better Auth replacement.
- `openidconnect`: strong candidate for OIDC/OAuth relying-party and provider primitives.
- `oauth2` / `oauth-axum`: useful for OAuth provider flows, especially if we want narrower control than a full framework.
- `webgates` / `authkestra`: newer auth orchestration options worth evaluating, but not default choices until audited.
- `webauthn-rs`: candidate for passkeys/WebAuthn later.

### Recommendation

- [x] Implement `forklaunch-iam-base` as ForkLaunch-owned auth domain logic. The crate owns credentials, directory services and optional PostgreSQL opaque sessions; phase-12 IAM and phase-16 directory fixtures cover the implementation. This reconciles the architectural recommendation, not remaining provider/client parity.
- [x] Use `argon2` and `password-hash` for salted Argon2id password hashing; reject malformed hashes and disabled users.
- [x] Use `tower-sessions` for server-side sessions when cookie/session auth is enabled. Optional `forklaunch-axum/cookie-session` supplies secure cookies, subject resolution, session/CSRF rotation, Origin checks and logout; live HTTP/MCP authentication and fail-closed feature-disabled tests pass. Better Auth retains its protocol-specific adapter.
- [x] Use `openidconnect` and `oauth2` for OAuth/OIDC provider integrations. Optional `forklaunch-auth/oidc` provides bounded discovery, authorization code + S256 PKCE, browser-bound single-use state, public/HTTP Basic clients and RS256 ID-token/nonce/access-token-hash verification. Local provider and generated-feature HTTP/MCP checks pass; refresh grants validate original identity and token rotation. Fresh keys are fetched from the pinned JWKS endpoint for each ID-token verification, including refresh grants, with actual key-rotation/revocation tests. Account/session provisioning remains in the broader provider lifecycle gate.
- [x] Use `jsonwebtoken` for JWT validation/issuance where JWTs are enough. (`forklaunch-auth` verifies HS256 and explicit JWK/JWKS policies and issues typed HS256 claims; actual JOSE verifies native-issued tokens and both canonical servers accept them. Better Auth provider-specific formats remain in their adapter.)
- [x] Keep Better Auth parity at the behavior/API level, not as a crate dependency. `forklaunch-iam-better-auth` owns the native implementation, while actual TypeScript/Better Auth fixtures define comparison behavior. Remaining ORM/client/options/provider gaps stay explicitly open in phase 16.
- [x] Add the optional adapters selected by the security review: `forklaunch-axum/cookie-session` over tower-sessions and `forklaunch-auth/oidc` over openidconnect/oauth2. Locked dependency review, encrypted PostgreSQL/browser state, adversarial protocol checks and generated HTTP/MCP startup pass. The evaluation deliberately defers competing whole-framework adoption; later passkey/custom-provider work remains in the broader IAM options gate.

### Auth Evaluation Checklist

Completed candidate-selection review: [Rust auth evaluation](RUST-AUTH-FRAMEWORK-EVALUATION.md). These are evaluation results, not implementation claims; cookie/OIDC/adapters remain separate open tasks.

- [x] Maintenance activity and release cadence.
- [x] Security posture and dependency risk.
- [x] Password hashing defaults and upgrade strategy.
- [x] Session storage model and CSRF behavior.
- [x] OAuth/OIDC provider coverage.
- [x] Multi-tenant organization support.
- [x] Role, permission, scope, and feature entitlement integration.
- [x] WebAuthn/passkey support path.
- [x] Framework independence from Axum.
- [x] Fit with generated OpenAPI, SDKs, and MCP auth boundaries.

## Phase 0 - Contract Shape And Compatibility

- [x] Define the Rust contract IR for HTTP routes.
- [x] Define the Rust contract IR for WebSocket/event channels. (`forklaunch-ws`: `WebSocketContract`/`EventContract`)
- [x] Map TypeScript `ContractDetails` fields to Rust equivalents.
- [x] Map TypeScript typed body variants to Rust equivalents: JSON, text, file, binary, multipart, URL-encoded, SSE, unknown.
- [x] Map response status code schemas and response headers.
- [x] Map route options: request validation, response validation, OpenAPI inclusion, MCP inclusion.
- [x] Map auth and access levels: public, authenticated, protected, internal.
- [x] Define contract artifact format emitted by Rust services.
- [ ] Confirm TypeScript and Rust contract artifacts can normalize to the same OpenAPI and AsyncAPI outputs.
- [x] Add snapshot fixtures from representative TypeScript routes for parity testing. (`rust_billing_contracts_parity.mjs` records 66 actual configured base/Stripe route contracts and 28 actual validator/OpenAPI DTO schemas; existing Rust comparisons run before the snapshot check.)

## Phase 1 - `framework-rs` Workspace Scaffold

- [x] Create `framework-rs/Cargo.toml` workspace.
- [x] Add shared lint, format, clippy, test, and docs commands.
- [x] Add crate publishing metadata conventions.
- [x] Add CI commands for `cargo fmt`, `cargo clippy`, `cargo test`, and doc generation.
- [x] Add crate feature policy: keep optional integrations behind feature flags.
- [x] Add README explaining Rust framework scope and relationship to TypeScript framework.

## Phase 2 - Core Crates

### `forklaunch-common`

- [x] Port path helpers: slash cleanup, base path joins, OpenAPI path conversion.
- [x] Port string helpers needed by SDK and route names: camel case, pretty camel case, identifier sanitization.
- [x] Add safe JSON helpers and deterministic JSON sorting for snapshots.
- [x] Add common error and result helpers.

### `forklaunch-core`

- [x] Add service metadata, runtime environment, and runtime config validation primitives.

### `forklaunch-contract`

- [x] Define `Contract`, `RouteContract`, `VersionedContract`, `RequestContract`, `ResponseContract`.
- [x] Define `BodySchema`, `HeaderSchema`, `ParamSchema`, `QuerySchema`, `ResponseSchema`.
- [x] Define `AccessLevel`, `AuthConfig`, `JwtAuth`, `BasicAuth`, `HmacAuth`, `RbacConfig`.
- [x] Define `ContractRegistry`.
- [x] Define a language-neutral serializable contract artifact.
- [x] Add conversion to OpenAPI-friendly structures.

### `forklaunch-contract-macros`

- [x] Add derive macro for request and response schemas.
- [x] Add route macro or builder helpers for low-boilerplate route registration.
- [x] Add compile-time checks for route params matching `/:param` path entries.
- [x] Add operation name normalization for SDK generation.

## Phase 3 - Validation And Schema Integration

- [x] Use `serde::Deserialize` for parsing wire input into Rust types.
- [x] Use `garde::Validate` for runtime validation.
- [x] Use `schemars::JsonSchema` for schema metadata.
- [x] Add a framework trait that combines `DeserializeOwned + Serialize + JsonSchema + Validate`. (`forklaunch-validation::FrameworkSchema`)
- [x] Standardize parse errors into ForkLaunch validation errors.
- [x] Support coercion where appropriate and explicit: path/query string to numeric, bool, uuid, date/time. (`forklaunch-validation::coerce`)
- [x] Decide which TypeScript coercions should not be copied because Rust should be stricter. (documented in crate root)
- [x] Add test fixtures for validation errors and coercion behavior.

## Phase 4 - HTTP Core And Axum Adapter

### `forklaunch-http`

- [x] Define request context: correlation ID, idempotency key, trace context.
- [x] Define framework request and response wrappers.
- [x] Define typed handler traits.
- [x] Define middleware traits.
- [x] Define local route execution API equivalent to TypeScript local `fetch`.
- [x] Define error response model matching TypeScript behavior.
- [x] Validate required params, query, headers, body presence, JSON content-type, and documented response statuses in local route execution.

### `forklaunch-axum`

- [x] Add initial Axum adapter facade with route summaries and response conversion helpers.
- [x] Add Axum request-parts conversion into `FrameworkRequest`.
- [x] Add Axum preflight route matching with path-param extraction and shared request validation.
- [x] Implement `ForkLaunchRouter` over `axum::Router`.
- [x] Implement nested routers and base paths.
- [x] Implement typed route registration for GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE.
- [x] Implement request validation for params, query, headers, and body.
- [x] Preserve declared binary/file inputs as bytes even for ASCII, Unicode and zero-length sources in both Axum adapters; compare actual and declared media types consistently when charset/boundary parameters are present. Empty declared text and URL-encoded inputs also retain their body kind; missing required media headers still fail.
- [x] Implement response validation by status code.
- [ ] Implement content-type handling for JSON, text, binary, multipart, URL-encoded, SSE. (request content-type validated; typed multipart extraction and SSE emission implemented; remaining request-stream/media conformance stays in Phase 3)
- [x] Implement CORS, request body limits, request IDs, and tracing layers through `tower-http`.
- [x] Implement docs and OpenAPI endpoints.
- [x] Implement local route execution for tests and service-internal calls. (via `tower::ServiceExt::oneshot`)
- [ ] Add parity tests against TypeScript route behavior.

## Phase 5 - OpenAPI, Docs, And MCP

### `forklaunch-openapi`

- [x] Generate OpenAPI 3.1 from `ContractRegistry`.
- [ ] Support versioned APIs. Ordered JSON/text/file/form/multipart selection, shared-parser checks and limits, per-version response validation, MCP tools, named/full-metadata OpenAPI exports and both typed SDKs are implemented. Configurable parser limits/charsets/compression and native per-version body preprocessing/transformation are implemented. Native parser verify/reviver callbacks are also implemented; optional body preprocessing/defaults and absent transformed outputs are implemented. Native parameter/query/header preprocessing, defaults and transformations are implemented and validated over generated HTTP/MCP. An explicit finite version-aware response callback now admits ordered schemas/statuses from every matched request version while preserving last-selected request/auth behavior. An explicit default Express simple-query profile now preserves repeated query arrays during version selection, with actual HTTP projection and admission-limit comparisons. An opt-in pre-authentication selection stage now matches TS ordering and reuses request-bound outcomes without rerunning callbacks; six auth-order status differences are resolved while diagnostic-body differences remain explicit. The opt-in first JSON wire parser now preserves framing/media skips and unmatched parsed values across Error/Warning/None, with240 live JSON comparisons and cached callback-once projection. The matching opt-in text profile now passes528 native/local comparisons (456 exact,72 known schema-diagnostic differences),48 supplied-body MCP calls and48 protected auth cases. Sparse values, mutable holders and remaining codec/invalid-string boundaries stay open; see the 2026-10-13 versioning receipts.
- [x] Support servers, descriptions, tags, summaries, operation IDs, auth schemes, response codes.
- [x] Serve raw OpenAPI JSON.
- [x] Support file output mode for build and CI.
- [x] Add snapshot tests for OpenAPI output.

### Documentation UI

- [x] Serve Scalar UI from generated OpenAPI.
- [x] Optionally serve Swagger UI. (`RouterConfig::swagger_enabled`)
- [x] Allow docs disabling.
- [x] Match TypeScript service/runtime environment variables for generated Rust docs path, host, and port.
- [x] Match OpenAPI server URL and description env configuration.

### `forklaunch-mcp`

- [x] Generate MCP tools from eligible HTTP contracts. (`mcp_tools`)
- [x] Use `rmcp` for a Streamable HTTP server behind `rmcp-server`, with bounded HTTP-router dispatch, per-request transport authentication, Host/Origin checks and generated-service wiring.
- [x] Support per-route MCP opt-in and opt-out. (`route.options.mcp`)
- [x] Support transport authentication callback. (`McpAuthCallback`)
- [x] Support additional manually registered tools. (`McpToolRegistry::with_tool`)
- [x] Map input/output schemas from `schemars` metadata. (`inputSchema`/`outputSchema` from contract `SchemaRef`)

## Phase 6 - SDK Generation

### Shared SDK Codegen

- [x] Create `forklaunch-sdk-codegen` around the contract artifact and OpenAPI.
- [x] Resolve operation names consistently across TypeScript and Rust.
- [x] Keep generated service fallback operation names stable across export; isolate lifecycle fallback per service and refresh existing Rust as well as TypeScript SDKs on deletion. Preserve unaffected custom contracts and reject malformed artifacts explicitly.
- [x] Generate response models from JSON Schema for TypeScript and Rust SDKs.
- [x] Preserve nullable schemas across OpenAPI 3.0 and 3.1 forms, typed dictionary values, unconstrained arrays, and additional object fields in CLI SDKs; validate null/populated payloads through both artifact sources and live language fixtures.
- [x] Generate typed untagged Rust enums for oneOf/anyOf and multiple-type schemas, including nested arrays/objects and nullable roots; validate each object/scalar branch through both artifact sources and live language servers. Closed structured objects reject unknown fields; full schema-constraint validation remains server-side.
- [x] Generate TypeScript request field/body types from contract request schemas.
- [x] Preserve status-code-specific response typing.
- [x] Carry params, query, headers, body content type, response headers, and response bodies in `SdkGenerationPlan`.
- [x] Support params, query, headers, and JSON body transport in deterministic TypeScript and Rust SDK generators. Path substitution matches whole segments and percent-encodes UTF-8/reserved bytes consistently across both SDKs; conformance fixtures cover prefix collisions, recursive-looking values, and repeated parameters.
- [x] Decode text and binary download responses in CLI-generated Rust and browser TypeScript SDKs, including UTF-8 and arbitrary bytes; validate both artifact sources against live Rust and compiled TypeScript servers.
- [x] Transport text and raw binary request bodies through CLI-generated TypeScript and Rust clients using the contract media type; validate UTF-8, arbitrary bytes, nonzero binary view offsets, and JSON regressions against both artifact sources and live language servers.
- [x] Transport URL-encoded requests through both CLI SDKs and artifact sources: TypeScript form records retain Unicode and encode composite values as one JSON string per field; explicit URLSearchParams can carry repeated values. Rust sends caller-encoded strings with the form MIME. Live signed Twilio callbacks validate both clients. OpenAPI named primitive schemas now retain string/boolean/integer/number types instead of defaulting to objects.
- [x] Preserve request body kinds/media in framework SDK plans; generate raw text, byte/file and JSON requests in native Rust builders and TypeScript fetch clients. TypeScript also sends FormData with actual browser boundaries and URLSearchParams; validate bytes and MIME against a live HTTP server in CI, and compile/run Rust builders with warnings denied.
- [x] Support multipart/file-form uploads in generated CLI TypeScript/Rust SDKs through contract and OpenAPI artifacts, with standard parsers, repeated/binary/empty files and encoder-owned boundaries.
- [x] Finish multipart response decoding and automatic typed runtime multipart extraction beyond explicit Axum extractors. Optional `Multipart<T>` validates Serde/garde models with byte/part bounds; both SDK generators decode bounded form-data responses from contract/OpenAPI sources, preserve repeated/binary/empty files and reject malformed/oversized payloads. Actual TypeScript parser comparisons and both SDK languages against Rust/TypeScript services pass, with HTTP/MCP startup/shutdown.
- [x] Generate bounded, pull-driven SSE response clients from both contract and OpenAPI artifacts, with typed Rust envelopes, iterator/abort/drop cancellation, auth before work, standard framing, terminal errors and live HTTP/MCP startup/shutdown verification.
- [x] Generate framework-native Rust HTTP clients with typed status decoding, guarded recursive models, Serde presence rules and bounded SSE through the shared CLI compiler/template; validate live presence, recursion and SSE with HTTP/MCP startup/shutdown.
- [x] Generate framework-native TypeScript typed HTTP/status/business-error responses, request schemas, omission/null models and bounded SSE using the shared CLI compiler/runtime; validate actual media/status handling and live presence/recursive/SSE clients with HTTP/MCP startup/shutdown.
- [x] Complete remaining SSE emitter/helper/schema-validation conformance against TypeScript. Explicit first-event compatibility and stricter default validation/framing are documented below.
- [x] Support empty success bodies, including 204/205 and HEAD, with TypeScript `undefined` and Rust `()`; preserve explicit JSON null and validate both artifact sources against live Rust and compiled TypeScript servers.
- [x] Preserve every explicit 2xx response in TypeScript discriminated unions and Rust status enums, including distinct typed JSON models and 204/205 variants. Validate both SDKs and artifact sources against live Rust and compiled TypeScript servers; reject duplicate status/model names before generation, and avoid inventing success responses for error-only contracts.
- [x] Preserve declared business-error payloads and media types in CLI SDK imports and clients, overriding default error status types with contract-specific JSON/text/binary shapes. Schema-free JSON uses unknown/serde_json::Value.
- [x] Support guarded recursive schemas in CLI SDK imports/clients through contract and OpenAPI artifacts, preserving bounded reference processing and explicit unsupported-reference errors.
- [x] Share the bounded local schema graph/model compiler between CLI and both framework-native HTTP generators.
- [x] Bring the framework-native TypeScript generator to guarded recursive model parity through the shared bounded compiler and strict/live recursive fixtures.
- [ ] Complete remaining schema-constraint/emitter conformance. Schema-bearing applicator traversal and dialect-inactive annotation handling are validated across native/generated SDKs; see the 2026-10-13 applicator receipt. Ordered Zod number constraints now include inclusive/exclusive exact safe-integer bounds with 47,090 actual/native comparisons; remaining emitter/constraint boundaries stay open.
- [x] Resolve schema resource IDs, anchors and dynamic references in SDK imports. Bundled HTTP resources, static/dynamic anchors, 2019-09 recursive roots, legacy fragment-only and URI/path-fragment IDs, socket unions, raw AsyncAPI relocation and external AsyncAPI resource imports are implemented. Mixed supported runtime drafts and message unions translate to 2020-12 with original resource identities preserved; see the dated resource receipts. Broader constraint/emitter conformance remains separate.
- [x] Preserve Rust model omission versus explicit null and reject missing required nullable fields, with 95 actual TypeScript validator cases through both SDK artifact formats and live Rust/TypeScript HTTP and MCP.
- [x] Add contract snapshot tests to ensure generated SDKs stay stable. (`client-sdk::tests::sdk_generation_is_deterministic`)

### TypeScript SDKs For Frontend

- [x] Add initial deterministic TypeScript fetch-client source generator from `SdkGenerationPlan`.
- [x] Generate browser-safe TypeScript fetch clients for Rust services.
- [x] Preserve the same consumer syntax as pure live-typed TypeScript SDKs.
- [x] Make generated TypeScript SDKs assignable to the same TypeScript shape as the live `MapToSdk` output.
- [x] Ensure callers can switch between TypeScript live mode and generated SDK mode without changing application call sites.
- [x] Generate SDK methods from route operation names using the same casing and grouping rules as live TypeScript SDKs.
- [x] Preserve the same request object shape: `params`, `query`, `body`, `headers`, and version fields where applicable.
- [x] Preserve status-code-specific response typing and discriminated response handling.
- [x] Emit ESM output with no Node-only dependencies by default.
- [x] Support custom `fetch`, base URL, default headers, auth hooks, and request interceptors.
- [x] Generate request and response TypeScript types.
- [x] Generate discriminated response unions by status code.
- [x] Support binary downloads as Uint8Array in browser TypeScript clients.
- [x] Support browser file uploads through FormData, Blob and ArrayBuffer, preserving byte-view bounds and encoder-owned multipart boundaries.
- [x] Add optional TanStack Query helpers as a later feature. (`forkLaunchQueryOptions`/`forkLaunchMutationOptions`; actual TanStack Query Core/React Query 5.104.2 conformance and generated HTTP/MCP runtime checks.)
- [x] Add TypeScript SDK tests against a relocated generated Rust HTTP server (`rust_http_runtime.sh`).

### TypeScript SDK Compatibility Contract

Rust-generated TypeScript SDKs should feel like the existing pure live-typed TypeScript SDKs. The implementation mechanism changes, but the frontend syntax should not.

```typescript
import { usersService } from '@/sdk';

const response = await usersService.users.createUser({
  body: {
    email: 'test@example.com',
    name: 'Test User'
  },
  headers: {
    authorization: token
  }
});
```

Requirements:

- [x] The generated SDK exports the same service/module shape that live TypeScript SDK mode would expose.
- [x] The generated SDK uses network calls under the hood for Rust services.
- [x] Support direct live imports for TypeScript services in mixed-app SDK live mode. Source edits are visible immediately; generated mode restores declaration resolution. Rust SDK artifacts, Cargo configuration and shared editor/build files remain intact. Pure Rust apps reject live mode. Node/Bun regression and mixed HTTP/MCP/SDK lifecycle fixtures pass (2026-10-13).
- [x] Mixed apps import one generated HTTP SDK surface for TypeScript and Rust services, validated by strict consumers and live calls in both client languages.
- [x] A Rust contract change requires regenerating the TypeScript SDK.
- [x] Regeneration should be deterministic so changed contracts produce reviewable diffs.
- [x] Generated TypeScript SDKs must fail TypeScript compilation when downstream code uses removed routes, renamed fields, changed request bodies, or changed response shapes.
- [x] Generated TypeScript SDK tests should compile representative frontend call sites against the generated SDK.

### Rust SDKs For Backend And Rust Clients

- [x] Add initial deterministic Rust request-builder source generator from `SdkGenerationPlan`.
- [x] Generate a Rust SDK crate per service.
- [x] Use `reqwest`, `serde`, and generated typed models.
- [x] Generate typed async methods from operation IDs.
- [x] Generate request builder structs for each operation.
- [x] Generate typed response model structs from contract JSON Schema, preserving exact JSON property names through Serde rename attributes (including camelCase, keywords, punctuation, and escaped names). Allocate distinct deterministic field and nested-model names after normalization; validate collision payloads through both SDKs, artifact sources, and live language fixtures.
- [x] Generate response enums by status code.
- [x] Support auth hooks for JWT, HMAC, Basic, and custom headers.
- [x] Support Rust-native error enums.
- [x] Run generated Rust SDKs against a relocated generated Rust HTTP server, including typed models, concurrent requests, HTTP errors, and transport failures (`rust_http_runtime.sh`).
- [x] Run generated Rust SDKs against a compiled TypeScript HTTP wire fixture, checking typed payloads, query/header transport, concurrent calls, HTTP errors, and transport failures. Full TypeScript framework conformance remains in phase 14.

### CLI Integration

- [x] Add commands for `forklaunch sdk generate ts`.
- [x] Add commands for `forklaunch sdk generate rust`.
- [x] Add commands for `forklaunch sdk generate all`.
- [x] Add manifest entries for SDK output directories and target languages.
- [x] Add support for Rust service contract discovery.

## Phase 7 - Auth, Errors, And Telemetry

### Auth

- [x] Implement JWT signature key validation. (`forklaunch-auth::verify_jwt_hs256`)
- [x] Implement direct JWK validation. (`jwt::verify_jwk`, algorithm/key-usage/claim policy tests)
- [x] Implement JWKS URL fetching and caching. (`remote-jwks` feature; rotation, expiry, bounds, and concurrent fetch tests)
- [x] Implement Basic auth callback. (`forklaunch-auth::parse_basic` + app-provided verifier)
- [x] Implement HMAC auth and HMAC header generation. (`forklaunch-auth::hmac_sign`/`hmac_verify`)
- [x] Implement constant-time signature comparison. (`subtle::ConstantTimeEq`)
- [x] Implement session enrichment equivalent to `decodeResource`. (`Principal::from_claims`)
- [x] Implement roles, permissions, scopes, scope hierarchy, feature checks, and subscription checks. (`forklaunch-auth::authorize`)
- [x] Add typed session extraction for handlers. (`Session<T>`, Axum extensions, local-handler verified claims)

### Errors

- [x] Implement standard errors: validation, unauthorized, forbidden, not found, rate limited, internal. (`forklaunch-http::ErrorKind`)
- [x] Include correlation IDs in safe error responses.
- [x] Hide internal error details by default. (`ErrorResponse::redacted`)
- [x] Add structured internal error logging; redact internal handler/status errors from HTTP responses.
- [x] Add tests for error response parity.

### Telemetry

- [x] Implement `OpenTelemetryCollector` around `tracing` and OpenTelemetry. (application-owned providers; generated OTLP bootstrap remains separate)
- [x] Support trace, debug, info, warn, error, fatal logging methods.
- [x] Support counters, gauges, histograms, and up-down counters.
- [x] Add automatic HTTP request spans. (SDK export assertions and parent-context tests)
- [x] Add request/response metrics. (full response lifetime/count/status/active/measured body bytes, including streaming failures and cancellation)
- [x] Add auth failure telemetry.
- [x] Add error correlation telemetry. (correlation in spans/logs, excluded from metric dimensions)

## Phase 8 - WebSockets And AsyncAPI

### `forklaunch-ws`

- [x] Implement typed WebSocket server wrapper. (`TypedWebSocketServer`, authenticated upgrade callback, application-owned listener; optional `transport` feature)
- [x] Implement typed WebSocket client wrapper. (`transport::connect`, caller headers/subprotocols and TLS roots)
- [x] Compose typed Axum WebSocket upgrades with HTTP/MCP on one listener; preserve subprotocol and failed-upgrade configuration, validate frame/message bounds, and verify application-owned shutdown.
- [x] Validate incoming and outgoing messages. (`WebSocketContract::validate_incoming`/`validate_outgoing`)
- [x] Support client messages, server messages, ping, pong, error messages, and close reason schemas. (`WsMessageKind` + error/close schemas)
- [x] Support channel and operation metadata. (`SocketContract`/`SocketSchemaEntry` preserve TypeScript singular/plural precedence, channel fan-out, error/control/context/user-data metadata and derived framework schemas)
- [x] Compile bounded raw JSON Schema codecs for generated-client validation before serde field removal, with recursive/scoped references, explicit AJV email/UUID formats, exact decimals and disabled external retrieval.
- [x] Support automatic JSON encoding and decoding. (`WebSocketEnvelope::to_json`/`from_json`)
- [x] Add integration tests for server/client validation. (real native peers, bidirectional actual TypeScript peers, 168 Zod payload comparisons and 4,104 raw JSON Schema codec comparisons, frame/control byte limits, stalled handshake/write deadlines, cancellation and close handshakes)

Native typed runtime uses raw UTF-8 string / JSON payloads matching the TypeScript wrappers; the legacy tagged `WebSocketEnvelope` contract remains a separate protocol. `PayloadCodec::framework()` optionally applies serde/garde constraints. Invalid incoming native payloads close with 1008 before delivery; outgoing failures remain local. Control replies preserve exact bytes and validate the outgoing pong schema. Actual TypeScript tests also fixed the wrapper's automatic pong/close paths, which previously revalidated encoded Buffers as objects. Schema coercion follows Rust's documented stricter policy. Optional Axum `websocket::upgrade` now shares the native typed engine on existing HTTP/MCP listeners, with pre-upgrade authorization, subprotocol negotiation, bounded invalid/oversized input, and explicitly propagated connection shutdown tested together. Partial-write cancellation releases the socket. Raw socket schema metadata and checked AsyncAPI export are now implemented; generated Rust clients now share these codecs; generated TypeScript clients now use the checked raw-schema profile too; CLI `--from socket-contract` now generates both clients and checked AsyncAPI from versioned multi-socket artifacts, with service selection, drift checks, bounded native compilation and custom-path watch regeneration. Service-registry socket exports now run through Rust contract export, with checked names, custom paths and ownership-aware move/removal cleanup. External AsyncAPI imports remain open.

### `forklaunch-asyncapi`

- [x] Generate AsyncAPI 3.0 from event contracts. General `AsyncApiDocument::to_json` now returns a checked result with a channel map, grouped messages, separate send/receive operations, actual supplied payload/header schemas, and conflict errors.
- [x] Add checked typed WebSocket AsyncAPI generation with actual envelope discriminators, supplied schema components, declared error/close controls, local reference checks, and official AsyncAPI parser conformance in CI. Legacy `generate_asyncapi_json` retains placeholder payload schemas.
- [x] Export raw typed socket AsyncAPI without event envelopes, with document options, schema relocation, literal-safe reference handling, bounded schema traversal and explicit conflict/scope rejection. Compare 34 actual TypeScript/native documents against the official JSON Schema; 63 parser cases also pass, with eleven documented parser limitations for percent/prototype map keys and literal `$ref` data. Fix actual TypeScript reference escaping, special map keys, invalid channel addresses and boolean-schema object form.
- [x] Support channel grouping.
- [x] Support client-to-server and server-to-client operations.
- [x] Add snapshot tests for AsyncAPI output.
- [x] Integrate versioned multi-socket artifacts with CLI SDK generation/check/watch, portable bundled generators, both workspace/package formats, checked AsyncAPI aggregation and all-service validation before writes. (relocated strict consumers/live peers, parser, custom paths, containment and generated HTTP/MCP startup)
- [x] Generate TypeScript WebSocket clients. (`generate_typescript_websocket_sdk_source`: Node ws/AJV raw JSON profile, strict ESM/CommonJS declarations and recursive/empty consumers, actual TS/native peers, bounded queues, write/close deadlines, cancellation and owned iterators)
- [x] Add declared Draft 6 support to generated TypeScript socket codecs, with an embedded official meta-schema, typed ESM/CommonJS consumers and native conformance comparisons.
- [x] Add declared Draft 4 support to generated TypeScript socket codecs, preserving older keyword/model semantics: ignore newer AJV adapter keywords and model literals, validate boolean exclusive bounds, reject unsupported legacy `id` scopes, and compile both module formats plus portable CLI/archive consumers.
- [x] Align safe integral decimal/exponent spellings with JavaScript JSON numbers in compiled socket codecs before validation/typed integer deserialization, preserving negative zero for floating-point/JSON-value targets and rejecting string/fraction coercion. Generic serde codecs retain their stricter policy.
- [x] Preserve declared non-Draft-7 JSON Schema dialects in both raw AsyncAPI exporters through Multi Format Schema Objects, including reference relocation inside the wrapper, official-schema/parser conformance and rejection of legacy Draft 4 resource scopes. Draft 6 codecs ignore newer conditional keywords.
- [x] Preserve ignored Draft 4/6 keyword annotations through native/TypeScript AsyncAPI normalization, schema relocation and SDK model graphs, including scalar annotations, untouched reference data, 34 metadata documents and 4,104 codec comparisons.
- [x] Prevent AJV resource discovery from treating ignored legacy annotation IDs as active resources. On a private compilation copy, retain explicitly referenced schemas in annotation storage, recursive/local targets, property names and literal IDs in active const data; strict ESM/CommonJS fixtures, all codec cases and portable generation/rename/HTTP/MCP startup pass.
- [x] Follow explicit local schema references into legacy annotation storage during relocation and checked AsyncAPI export, including nested/recursive edges and once-decoded percent-encoded pointers. Preserve unreferenced literals and embedded resource bases, bound traversal to 64 steps/100,000 locations, and independently validate exported payload constraints with AJV and the official schema/parser.
- [x] Decode URI fragments once in shared SDK model graphs and CLI OpenAPI reference objects; canonicalize equivalent recursive edges and URI-encode relocated pointers after JSON-pointer token escaping. Exercise Unicode/slash/tilde/percent definition names in both native generators and portable CLI socket artifacts.
- [x] Extract standalone schemas from containing documents with transitive/recursive local reference copies, original literal data, UTF-8/percent pointer handling, collision-checked definitions and shared copy/output limits. Reject external/missing targets, resource IDs/dynamic scope and dictionary aliases; this is the schema foundation for external AsyncAPI input, whose transport/direction integration remains open.
- [x] Bundle schema-dictionary aliases with separate interpretation contexts when the same value acts as both a schema and literal assertion data. Checked unscoped exports, document extraction and socket unions preserve literal assertions and recursive edges under shared copy budgets. Actual AJV comparisons, generated Rust/TypeScript consumers, native validation and strict Clippy pass. Resource-preserving relocation remains conservative; general resource resolution is a separate task.
- [x] Align checked schema reference profiles by rejecting encoded pointer separators before model/artifact generation. Preserve literal assertion/annotation data, percent-encoded names and general OpenAPI reference-object URI decoding. Actual/native/generated codec comparisons include rejected leading separators; imported SDK validation checks no-write behavior.
- [x] Align raw native JSON Schema codec resolution of encoded interior separators with actual AJV token decoding outside the checked SDK/export profile. An isolated, provenance-pinned resolver decodes each URI token once at lookup and registry-discovery boundaries, preserving internal pointers, literals, recursion and embedded resource IDs. Default upstream RFC behavior remains independently tested. Checked SDK/export/import profiles continue rejecting this ambiguous extension.
- [x] Match actual AJV validation of constraints beside `$ref` in Drafts 4–7 through a separate isolated opt-in validator feature, preserving dialect-specific keywords and original schema documents. All five supported dialects pass 12,960 actual helper/AJV codec comparisons, including interior separators, invalid escapes, annotation targets, recursive and scoped resources, sibling constraints and unchanged literal assertions.
- [x] Bundle explicitly referenced schemas stored inside const/enum/default/example data without mutating the original literal payload constraints. Share collision-checked copies and recursive edges with bounded common relocation, preserve embedded resource bases, retain idempotence, and validate full original output with independent AJV plus structural parser checks. Portable multi-socket fixtures compile both language models.
- [x] Carry explicit raw binary payload metadata through checked socket contracts and both generated clients. Canonical binary entries produce Rust `Vec<u8>`/TypeScript `Uint8Array` codecs and octet-stream AsyncAPI messages; live generated clients preserve arbitrary bytes and enforce message/control bounds. Reject mixed binary/JSON directions, contradictory schemas and binary close reasons. External AsyncAPI binary imports remain in the broader import task.
- [x] Exercise and harden actual TypeScript automatic control-reply failures and abnormal closes with required pong/close schemas. (one error and 1008 before typed delivery, five-second unresponsive-peer close bound, preserved manual validation and explicit autoPong=false; live CI fixture and 110 unit/integration tests)
- [x] Reject boolean `false` message schemas before typed delivery on actual TypeScript client and server sockets; preserve `true` decoding and absent-schema raw frames, with a reproduced live regression and outgoing validation checks.
- [x] Generate Rust WebSocket clients. (`SocketSdkPlan`, `generate_rust_websocket_sdk_source`: typed directions/controls, recursive unions, retained upgrade responses, actual TypeScript server conformance, invalid peer rejection and warning-free recursive/empty consumer compilation)

## Phase 9 - Infrastructure Crates

### Redis Cache

- [x] Define `TtlCache` trait.
- [x] Implement Redis-backed TTL records. (`redis-client` adapter; binary/expiry/batch/prefix/FIFO integration checks)
- [x] Implement batch put/read/delete/peek.
- [x] Implement list keys by prefix.
- [x] Implement queue enqueue/dequeue/peek operations. (`InMemoryRedisQueue`)
- [x] Implement batch queue operations.
- [x] Add compliance-aware encryption/decryption hooks. (`CacheCodec` + `put_encrypted`/`get_decrypted`)
- [x] Add Redis container integration tests. (isolated local container and Redis service in CI)

### S3 Object Store

- [x] Define `ObjectStore` trait.
- [x] Implement async S3-backed put/read/delete through the official AWS SDK, with namespace fences, byte/count/deadline limits, actual tags, ordered batches, and bounded paginated listings. Validate against an isolated MinIO container.
- [x] Implement batch operations.
- [x] Implement stream upload/download. (`StreamingUpload` + `download_chunks`)
- [x] Implement bucket initialization.
- [x] Add compliance-aware encryption/decryption hooks. (`ObjectCodec` + `put_encrypted`/`get_decrypted`)
- [x] Add MinIO/S3-compatible container integration tests and run them in framework CI; test permission-denied bucket initialization and deadlines separately.
- [x] Add SDK-signed S3 download links with bounded whole-second lifetimes, namespace/key fences, credential-resolution deadlines, and UTF-8 attachment filenames; verify actual download, expiration, and tampering rejection against MinIO.
- [x] Add native bounded S3 chunked downloads and exact-length byte-stream/file uploads without whole-object buffering; test actual file transfers, metadata, drop behavior, overflow, truncation, idle deadlines, and source errors. Legacy streaming helpers still buffer.
- [x] Add native S3 multipart transfers for unknown-length sources with bounded part buffering, CRC32 checksums, hidden-until-complete publication, source/byte/deadline bounds, and cancellation abort cleanup; validate real multiple-part, empty/exact-boundary, overflow, failure, timeout and cancellation behavior against MinIO.
- [x] Add presigned S3 POST upload policies, with exact key/type/byte bounds, bounded SDK credential resolution, temporary-token/rotation coverage, AWS signature vectors, actual TypeScript policy fixtures and live MinIO rejection tests.

### Persistence

- [x] Define persistence extension points for `sqlx`. (optional PostgreSQL feature; tenant-bound transactions)
- [x] Implement field encryption helpers. (TypeScript-compatible v1–v4 envelopes, tenant derivation, key rotation, blind indexes; actual TypeScript interoperability tests)
- [x] Implement tenant context helpers. (validated identity and transaction-local database context)
- [x] Implement RLS setup helpers for Postgres. (safe DDL identifiers, FORCE RLS, read/write policies)
- [x] Implement retention policy enforcement hooks. (Explicit tenant-scoped async store boundary, dry runs, bounded batches, metadata-driven anonymization and incomplete-pass reporting; concrete database adapters and general calendar cutoff parsing remain within production persistence work.)
- [x] Add compliance metadata derives where appropriate. (`ComplianceMetadata` on named structs; explicit classification/storage-name/nullability, generic and renamed-dependency support; no implicit encryption or database writes.)

## Phase 10 - `blueprint-rs` Workspace Scaffold

- [x] Create `blueprint-rs/Cargo.toml` workspace.
- [x] Add shared lint, format, clippy, test, and docs commands.
- [x] Add README explaining relationship to TypeScript `blueprint`.
- [x] Add crate naming conventions.
- [x] Add feature flag policy for optional integrations.
- [x] Add sample app/service/worker crates for smoke testing.

## Phase 11 - `blueprint-rs` Core And Interfaces

### `forklaunch-blueprint-core`

- [x] Re-export common framework traits used by blueprints. (`pub use forklaunch_http as http`, `contract`)
- [x] Define initial blueprint error/result, module metadata, audit context, and pagination primitives.
- [x] Define shared entity, DTO, pagination, and API result primitives. (`Entity`, `Identifier`, `Timestamps`, `ApiResult`)
- [x] Define common auth/session DTOs. (`SessionContext`)
- [x] Define common billing DTOs. (`MoneyAmount`)
- [x] Define common worker/job DTOs. (`JobDescriptor`)
- [x] Define blueprint testing helpers. (`forklaunch_blueprint_core::testing`)

### IAM Interface

- [x] Define initial user, session, credentials, permission models, and IAM service trait.
- [x] Define user, organization, tenant, role, permission, session, and token models. (tenant modeled as `Organization`)
- [x] Define IAM service traits.
- [x] Define IAM route contracts. (`forklaunch_iam_interface::contracts::registry`)
- [x] Define IAM SDK generation fixtures. (`client-sdk` tests)
- [x] Generate TypeScript and Rust SDKs from IAM contracts. (`generate::iam_typescript_sdk`/`iam_rust_sdk`)

### Billing Interface

- [x] Define initial customer, money, plan, subscription, checkout session models, and billing service trait.
- [x] Define customer, subscription, plan, price, invoice, entitlement, usage, and webhook models.
- [x] Define billing service traits.
- [x] Define billing route contracts. (`forklaunch_billing_interface::contracts::registry`)
- [x] Define billing SDK generation fixtures. (`client-sdk` tests)
- [x] Generate TypeScript and Rust SDKs from billing contracts. (`generate::billing_typescript_sdk`/`billing_rust_sdk`)

### Worker Interface

- [x] Define initial job, receipt, stats, status models, and worker queue trait.
- [x] Define job, queue, retry policy, schedule, worker result, and distributed signal models.
- [x] Define worker traits.
- [x] Define worker event contracts. (`forklaunch_worker_interface::events::channel`)
- [x] Generate AsyncAPI where applicable. (`forklaunch_worker_interface::events::asyncapi_json`)

### Monitoring Interface

- [x] Define initial health status, metric, and monitoring sink models.
- [x] Define health, readiness, metrics, audit event, and telemetry DTOs.
- [x] Define monitoring route contracts. (`forklaunch_monitoring_interface::contracts::registry`)
- [x] Add reusable health/readiness handlers. (`liveness`, `aggregate_readiness`)

## Phase 12 - `blueprint-rs` Implementations

### IAM Base

- [x] Implement initial in-memory IAM service for interface and SDK smoke tests.
- [x] Implement base IAM service with `sqlx`. (optional asynchronous `PostgresIamService`)
- [x] Implement user and organization CRUD. (tenant-bound PostgreSQL storage and keyset pagination)
- [x] Implement role and permission resolution. (`roles_for_user` + `permissions_for_user`)
- [x] Implement persistent opaque session issuance, hashed token storage, expiry, revocation, password-change invalidation, and disabled-user checks.
- [x] Port configured IAM directory CRUD and grant surfacing: typed single/batch interfaces, transactional memory/PostgreSQL stores, relationship hydration and cleanup, encrypted sensitive fields, retained-key uniqueness claims and atomic actor audits. Real PostgreSQL fixtures cover rollback, cross-tenant reads, runtime-role checks, concurrent uniqueness, key rotation and ciphertext swapping. Direct actual TypeScript conformance covers 44 service operations, 30 routes and 20 request/response schema layouts. Correct upstream permission batch awaiting/transaction propagation and user batch ID filters.
- [x] Generate the thirty configured IAM directory HTTP/MCP operations alongside opaque sessions. Validate live typed Rust and TypeScript SDK calls, nested models and status unions. Verify request HMAC and JWKS JWT policy, role/permission checks, storage-resolved organization scope, atomic mixed-organization rejection and SDK generation. Preserve the legacy `/users/:id` route as `getSessionUser` so directory `/user/:id` retains `getUser` in SDK/MCP contracts.
- [x] Add configured IAM base discovery/compliance routes and actual compliance-policy conformance. Public discovery returns 204; system-role JWT export/erase return 404 because all configured entity scalars (including inherited base properties) are classified `none`. Execute the actual TypeScript compliance service to verify empty results without entity reads. All 33 configured routes and 22 request/response schema layouts match the actual sources; generated HTTP/MCP, both SDKs and lifecycle checks pass.
- [x] Support application-specific directory compliance classifications and generic entity metadata. Explicit generic metadata selects export fields and whole-record erasure independently of encryption; trusted tenant stores, per-entity transactions, sanitized incomplete-operation errors and system-role HTTP/MCP routes pass actual TypeScript, encrypted PostgreSQL and generated lifecycle fixtures. Scalar subject links use the existing bounded directory listing; scalable indexed traversal remains separate.
- [ ] Complete generic directory mapper/filter and transaction extensions. Explicit identity associations and opt-in stored-session authorization now pass restricted PostgreSQL and generated HTTP/MCP plus both SDKs. Verified JWTs must match complete persisted sessions; current directory roles/permissions and organization replace token grants before authorization. Grant edits, organization moves, disable/unlink/revoke/delete and restart behavior are verified. Typed custom DTO/context mapper hooks and caller-owned atomic directory transactions now pass actual TS and restricted PostgreSQL checks, including rollback after caught errors, cancellation and mapper panic. Bounded typed filters now pass 33 actual MikroORM/PostgreSQL comparisons, preserving NULL logic, UUID identity, relationship membership and mandatory scope; arbitrary filters and broader custom persistence semantics remain open. Directory CRUD passwords remain ignored; email/profile/grants never implicitly provision or link credential identities.
- [x] Complete configured base IAM OAuth/OIDC authorization-code login: operator-selected issuer/client, S256 PKCE, verified identity, explicit tenant/issuer/subject binding and persistent opaque/RS256 local sessions. Generated browser start/callback uses encrypted single-use PostgreSQL pending state, fixed callback/origin, cookie/CSRF rotation, restart durability and local revocation. Native provider/database tests, real Chromium relay checks and full generated HTTP/MCP/SDK/lifecycle checks pass. Provider tokens are deliberately discarded; generic provider refresh remains a framework API. Directory credential integration stays in the preceding gate, broader Better Auth/linking extensions stay below, and public JWKS hosting remains application-owned.
- [x] Expose async IAM read/session/repository traits and a reusable Axum adapter; wire PostgreSQL IAM into `init module --language rust -m iam-base` and application `--modules iam-base`.
- [x] Validate generated IAM through its typed TypeScript SDK: restricted runtime role, tenant isolation, restart persistence, logout revocation, graceful shutdown, source preservation, rename, and delete.
- [x] Update generated SDK artifacts and crate names when a Rust service is renamed (including IAM); reject occupied project/artifact destinations before mutation.
- [x] Implement tenant-aware queries. (explicit predicates plus transaction-local RLS)
- [x] Add embedded migrations and isolated PostgreSQL container tests with a non-superuser role, including pooled-connection context cleanup.

### Billing Base

- [x] Implement initial in-memory billing service for interface and SDK smoke tests.
- [x] Implement the five asynchronous base billing service traits and typed DTOs with owned memory and tenant-RLS PostgreSQL storage. Plans/subscriptions are durable; checkout/payment-link/portal sessions use TTL cache primary storage with optional database backup. Actual TS base-class conformance covers 168 operations over explicit generic mapper/cache/ORM boundaries. Native fixtures cover concurrent disjoint updates, unique provider IDs, real cache expiry independent of provider expiry, encrypted provider data/session URIs, audit rollback, cross-tenant isolation, key rotation and reopening. Configured Stripe mappers/provider integration and generated modules remain separate gates.
- [x] Implement customer/subscription/plan persistence. Native typed customer CRUD now uses the same durable memory/PostgreSQL store as plans/subscriptions; migration 3 adds customers, encrypts email/name in field-bound envelopes and retains tenant RLS and atomic actor audits. Shared tests cover concurrent updates/creation, nullable-name clearing, filtering, audit rollback, retained-key rotation and ciphertext substitution. The configured TypeScript module has no separate customer service; its 168 five-service comparisons remain green.
- [x] Implement entitlement checks. (metered `has_entitlement`)
- [x] Implement usage recording. (`record_usage` + `recorded_usage`)
- [x] Add embedded billing migrations and isolated PostgreSQL container tests. `forklaunch-billing-base/migrations`, `tests/services.rs` and `cli/tests/rust_billing_runtime.sh` cover restricted-role RLS, encryption, audits, concurrency and reopening. The fixture uses owned Docker containers directly rather than the testcontainers crate; validated in the phase-16 billing runtime checkpoint.

### Billing Stripe

- [x] Implement initial Stripe config, webhook event, and checkout adapter shape.
- [x] Implement Stripe client integration. `forklaunch-billing-stripe/src/client.rs` supports direct and managed transports; the phase-16 fixtures cover 21 official SDK and 23 managed gateway comparisons.
- [x] Implement webhook verification. (`webhook::verify` — `t=...,v1=<hex>` HMAC-SHA256, constant-time)
- [x] Implement subscription sync. `forklaunch-billing-stripe/src/synchronization.rs` and the phase-16 webhook fixture cover subscription updates, missed-create recovery and party reassignment protection. Interrupted provider/local writes remain separately open.
- [x] Implement customer and checkout helpers. `StripeResource::Customer` supports create/retrieve/update/list/list-all/delete through the existing direct and instance-signed client alongside checkout lifecycle helpers. Official Stripe SDK and actual managed TypeScript transport fixtures pass 26 and 28 cases. Fresh mixed billing/Stripe generation, both SDKs, HTTP/MCP, persistence and lifecycle pass; provider/local-write reconciliation remains separately open.
- [x] Add controlled HTTP provider tests for Stripe interactions. `tests/transport.rs` uses real local Axum HTTP peers rather than wiremock; actual TypeScript/official Stripe SDK conformance covers transport and service effects in the phase-16 fixtures.

### Worker Implementations

- [x] Implement initial in-memory database, Redis, and Kafka queue adapters behind the worker trait.
- [x] Implement PostgreSQL-backed worker queue with tenant RLS, atomic batch enqueue, concurrent claims, renewable fenced leases, acknowledgments, fixed retry backoff, crash recovery, and failed-job retention.
- [x] Define an async leased-queue interface and validate the PostgreSQL adapter against a restricted-role database, including migration coexistence with IAM.
- [x] Wire PostgreSQL queues into new generated workers with polling, lease renewal, bounded operations, and graceful drain; validate actual process crashes, restart recovery, tenant isolation, and source-preserving lifecycle operations.
- [x] Convert existing example workers to PostgreSQL on explicit backend changes, preserving their handler and unrelated Cargo metadata.
- [x] Add bounded per-process concurrency settings to the shared runner and generated PostgreSQL/Redis workers; renew every active lease and drain all in-flight handlers on shutdown.
- [x] Add shared process-local pause/resume/drain controls with lease renewal, irreversible drain, retention coordination, and real Redis/PostgreSQL coverage.
- [x] Add atomic ordered batch claims for PostgreSQL and Redis with independent delivery capabilities, tenant fencing, concurrent-consumer coverage, and Redis selection-validation regression tests.
- [x] Add batch-handler settings and per-job batch failure results; preserve existing single handlers, renew all leases, cancel on lease loss, validate complete failure sets before acknowledgments, and exercise concurrency/drain/recovery against both durable backends.
- [x] Add optional worker HTTP health/readiness and bearer-authenticated pause/resume/drain endpoints; validate lifecycle, authorization, retention coordination, and graceful shutdown in generated Redis/PostgreSQL processes.
- [x] Wire durable Redis queues into generated worker binaries; preserve application handlers through sync, rename, and Redis/PostgreSQL conversion, and validate real Redis process lifecycle behavior.
- [x] Wire broker-backed transactional Kafka into generated worker binaries; validate explicit topic provisioning, non-mutating peek/dead-letter inspection, retries, source offsets, actual SIGKILL recovery, authenticated HTTP/Redis distributed controls, source preservation, rename, backend conversion and delete against an isolated broker.
- [x] Add tenant-bound dead-letter listing/requeue and explicit age-based, bounded completed/failed retention APIs; expose generated administration commands with payload/token-free summaries. Validate races, tenant isolation, retry fencing, and legacy Redis terminal-index upgrades.
- [x] Add opt-in automatic retention scheduling and runtime policy settings for both durable backends; bound per-status purges and storage waits, stop maintenance during shutdown, and drain handlers on maintenance failure.
- [x] Implement Redis-backed worker queue. (`redis-client` async leased adapter: atomic mixed-queue batches/claims, persisted retry budgets, fenced ack/renew/fail, bounded expiry recovery, retained terminal jobs; real Redis concurrent-consumer and shared runner integration coverage. Generated Redis workers use the shared runner.)
- [x] Implement a broker-backed Kafka producer/consumer with atomic batch publication, offset/retry/dead-letter transactions, namespace/tenant/queue/group scoping, bounded payloads and operation deadlines, explicit consumer-session/handler/drain deadlines, TLS/SASL configuration, stable replica fencing and fail-closed assignment handling. Validate group fanout, offset persistence, pause/drain abandonment recovery, tenant isolation and producer fencing against a pinned ARM64/AMD64 Redpanda broker; run in CI. Kafka uses log/partition semantics rather than the database lease API.
- [x] Validate a generated Kafka worker's Linux container build/startup against the isolated broker, including bundled librdkafka/OpenSSL. Relocated Linux image passes offline export, migration, enqueue/processing/inspection, health/controls and SIGTERM; builder includes the libcurl headers required by upstream native compilation.
- [x] Define worker signaling interfaces and an in-memory reference bus. (`SignalBus` + `InMemorySignalBus`)
- [x] Implement namespace/tenant/queue-bound Redis Pub/Sub worker signaling with ordered bounded subscriptions, sanitized publication deadlines, independent publishers and sticky drain forwarding. Verify actual child-process delivery, isolation, overflow/malformed-message errors and owned TCP disconnect behavior in the Redis CI suite.
- [x] Add subscriber-free administrative Redis signal publishers and `http::run_with_control` so distributed forwarding, HTTP readiness and durable consumption share one control; test paused jobs, resume completion, readiness changes and graceful drain against real Redis.
- [x] Wire opt-in distributed signaling into generated Redis/PostgreSQL worker commands and authenticated control endpoints. Validate separate-process fanout, namespace/tenant/queue isolation, queue-storage-independent administration, paused jobs, resume and drain against both backends. Subscription failure drains active handlers and exits with an error; explicit restart begins running without replay or automatic desired-state restoration.
- [x] Add AsyncAPI generation for worker events where applicable. (`events::asyncapi_json`)

## Phase 13 - CLI And Template Integration

The existing CLI is already written in Rust. The Rust framework work should extend the current CLI rather than introduce a second CLI.

Current CLI structure to build on:

- Commands live under `cli/src/init`, `cli/src/openapi`, `cli/src/sdk`, `cli/src/sync`, `cli/src/release`, and `cli/src/deploy`.
- Templates live under `cli/src/templates`.
- Manifest structs live under `cli/src/core/manifest`.
- SDK support currently exists as `forklaunch sdk mode`; generation should become a first-class SDK subcommand family.
- OpenAPI export already exists for TypeScript services and should become language-aware.

### CLI Product Shape

- [x] Wire Rust and mixed project generation into the existing `forklaunch init` command family.
- [x] Add `cli/tests` coverage for Rust and mixed project generation plus Rust init, change, sync, delete, OpenAPI export, and SDK generation command paths.
- [x] Add dedicated Rust CLI parity scripts mirroring TypeScript command coverage for init, change, sync, delete, OpenAPI export, SDK generation, dry-run, custom-path, and Cargo workspace checks.
- [x] Add `ProjectLanguage` to the manifest model: `typescript`, `rust`.
- [x] Add application-level language mode: `typescript`, `rust`, `mixed`.
- [x] Add project metadata fields: `language`, `crateName`, `cratePath`, `contractArtifactPath`, `openapiPath`, `asyncapiPath`, `sdkTargets`.
- [x] Preserve existing TypeScript defaults so existing manifests parse without migration.
- [x] Add Rust-aware project dependency topology to the manifest.
- [x] Add Cargo workspace metadata for Rust projects.
- [x] Add PNPM workspace metadata only when TypeScript packages or TypeScript SDK output are present.

### Init Commands

- [x] Add `forklaunch init app <name> --language rust`.
- [x] Add `forklaunch init app <name> --language mixed`.
- [x] Add `forklaunch init service <name> --language rust`.
- [x] Add `forklaunch init worker <name> --language rust`.
- [x] Add `forklaunch init library <name> --language rust`.
- [x] Add `forklaunch init router <name> --language rust` if Rust route module generation remains separate from service generation.
- [x] Add prompts for Rust package/crate name when the generated default is not enough. Interactive native init offers `my-<kind>` and accepts an explicit project/crate name; positional names remain available for automation, and noninteractive omission fails without writes. Real-terminal application/service/library/worker tests validate custom/default names and invalid/noninteractive rejection.
- [x] Add prompts for SDK targets: TypeScript frontend SDK, Rust backend SDK, or both.

### Rust Templates

- [x] Add `cli/src/templates/rust/application`.
- [x] Add `cli/src/templates/rust/project/service`.
- [x] Add `cli/src/templates/rust/project/worker`.
- [x] Add `cli/src/templates/rust/project/library`.
- [x] Add `cli/src/templates/rust/router`.
- [x] Add `cli/src/templates/rust/blueprint/iam-base` as a symlink to `blueprint-rs`.
- [x] Add `cli/src/templates/rust/blueprint/billing-base` as a symlink to `blueprint-rs`.
- [x] Add `cli/src/templates/rust/blueprint/billing-stripe` as a symlink to `blueprint-rs`.
- [x] Add generated `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/routes`, `src/domain`, `src/persistence`, `src/config`, and `tests` files.
- [x] Replace placeholder Rust crate output with runnable generated service, worker, library, router, and smoke-test implementations.
- [x] Add Rust Dockerfile templates.
- [x] Add Rust env example and config files.
- [x] Add Rust `.gitignore`, `rustfmt.toml`, and clippy config files.

### Template Rendering

- [x] Add Rust template namespaces under `cli/src/templates/rust` and SDK template namespaces under `cli/src/templates/sdk`.
- [x] Reuse the rendering primitive from `generate_with_template` for native scaffolds and SDKs. Shared Ramhorn parsing/rendering and embedded-template diagnostics replace duplicated renderers; legacy TypeScript replacements and each generator's file-ownership rules remain at their existing boundaries. All 1,075 CLI tests pass.
- [x] Add Rust-specific generation variables: crate name, module path, lib name, binary name, Rust edition, SDK targets.
- [x] Avoid TypeScript AST transforms for Rust templates.
- [x] Render Rust app, service, worker, library, router, TypeScript SDK, and Rust SDK scaffolds through Ramhorn templates instead of inline stubs.
- [x] Add Rust-aware file preservation rules for user-edited code.

### Workspace Updates

- [x] Generate and update root `Cargo.toml` workspaces for Rust apps.
- [x] Generate service crate `Cargo.toml` files.
- [x] Add local path dependencies between Rust crates.
- [x] Add generated Rust SDK crates to the workspace when selected.
- [x] Keep PNPM/Bun generation for TypeScript SDKs and TypeScript projects.
- [x] Support mixed apps with both `Cargo.toml` and `package.json`/`pnpm-workspace.yaml`.

### OpenAPI And Contract Export

- [x] Make `forklaunch openapi export` detect project language from manifest.
- [x] For TypeScript projects, keep the existing TSX/openapi export path.
- [x] For Rust projects, run the Rust service binary in contract export mode.
- [x] Add Rust manifest/OpenAPI artifact fallback export for generated Rust projects.
- [x] Standardize output paths under `.forklaunch/openapi/<service>/openapi.json`.
- [x] Add fallback `.forklaunch/contracts/<service>/contract.json` artifacts for generated Rust projects.
- [x] Replace fallback Rust contract artifacts with binary-exported `framework-rs` contract registry output for generated Rust service and worker crates.
- [x] Add `.forklaunch/asyncapi/<service>/asyncapi.json` for raw WebSocket contract artifacts through `sdk generate --from socket-contract`, with custom project output paths.
- [x] Export socket artifacts automatically from generated service registries, with checked module names, service title/version, custom contained output paths, unchanged-owned move/removal cleanup and preserved manual/edited artifacts. Relocated CLI and archive fixtures generate both SDKs from registered metadata and validate HTTP/MCP startup.
- [x] Import explicit AsyncAPI 3.0.0 JSON WebSocket server documents through native/CLI generation/check/watch, offline official schema validation, bounded local-reference extraction, server/client direction mapping, disjoint message checks, input ownership and SDK rename/pruning. Relocated strict consumers/live peers and HTTP/MCP startup validate the supported profile.
- [ ] Extend external AsyncAPI imports to event projects, additional payload formats/protocols, upgrade authentication/parameters, replies and richer message matching. Bounded message/operation trait merging is implemented, including local references and inherited wire checks. Explicit binary messages, the five supported JSON Schema dialects, offline component/payload resource imports, JSON string/null wire encoding and fixed same/separate-channel operation replies are implemented and validated below. Dynamic reply addresses and automatic request correlation remain open.
- [x] Prune unchanged obsolete socket modules during regeneration, with per-package hashes, read-only drift checks, all-service staging, preserved edits/untracked files/compiler options and symlink containment.
- [x] Retire owned socket SDK files/AsyncAPI after an exported last-socket removal or explicit service deletion, removing Cargo members and empty directories while preserving edited/untracked files and compiler options. Live watch requires prior registry provenance plus a fresh empty framework contract, survives transient missing artifacts and restores both packages when sockets return. Shared pnpm package globs remain available for retained user packages.
- [x] Rename socket artifacts and owned SDK packages with their service, preserving compiler options and retaining edited/untracked files at old SDK locations. Default/explicit default paths follow the new name, custom paths retain their location, occupied destinations reject before source mutation, strict consumers compile and renamed HTTP/MCP startup passes. Deletion also removes unchanged owned custom registry artifacts.
- [x] Ensure release/deploy manifest generation reads TypeScript and Rust outputs uniformly.

### SDK Commands

- [x] Add `forklaunch sdk generate --target typescript`.
- [x] Add `forklaunch sdk generate --target rust`.
- [x] Add `forklaunch sdk generate --target all`.
- [x] Add `forklaunch sdk generate --service <name>`.
- [x] Add `forklaunch sdk generate --from contract`.
- [x] Add `forklaunch sdk generate --from openapi` reading actual OpenAPI 3.0/3.1 JSON artifacts, operation IDs, parameters, JSON body models, and bounded document-local references; artifact edits invalidate check mode, and missing/unsupported artifacts fail before output writes.
- [x] Add `forklaunch sdk watch --target typescript` for regenerating TS SDKs when Rust contract artifacts change.
- [x] Add `forklaunch sync` integration that regenerates TypeScript SDKs after Rust route or model changes.
- [x] Add a CI check mode that runs SDK generation and fails when generated TypeScript/Rust SDKs are stale.
- [x] Generate TypeScript SDKs into a stable `sdks/typescript` package path.
- [x] Generate Rust SDK crates into the Cargo workspace.
- [x] Generate TypeScript and Rust SDK contents from Ramhorn templates.
- [x] Generate SDK operations from Rust `framework-rs` contract artifacts when `--from contract` is selected.
- [x] Generate SDK response models and status-code response maps from Rust `framework-rs` contract schemas.
- [x] Add `forklaunch sdk generate --check` for stale generated SDK detection.
- [x] Add SDK generation to app init when SDK targets are selected.
- [x] Keep `forklaunch sdk mode --type live` as the TypeScript-only local live import mode.
- [x] Treat Rust services as generated-SDK services.
- [x] Ensure mixed apps expose one stable generated SDK import surface.

### Sync, Change, Delete

- [x] Extend `sync service`, `sync worker`, and `sync library` for Rust projects.
- [x] Extend `sync all` for Rust Cargo workspace projects.
- [x] Extend `change application` for Rust application metadata.
- [x] Extend `change service`, `change worker`, and `change library` for Rust project metadata.
- [x] Extend `change router` for Rust route module renames.
- [x] Extend delete commands to remove Rust manifest entries, route modules, crate directories, and Cargo workspace members.
- [x] Remove generated SDK references during Rust deletes when SDK generation lands.
- [x] Extend dependency detection to include Rust SDK imports and Cargo path dependencies.
- [x] Keep TypeScript AST transforms isolated from Rust project updates.

### Local Development

- [x] Generate `cargo run -p <service>` scripts or `just`/`make` tasks for Rust services.
- [x] Add mixed dev orchestration for TypeScript and Rust services. Node/Bun fixtures validate HTTP/MCP, source reloads and supervised shutdown.
- [x] Integrate source-triggered contract export and SDK refresh into mixed development. Native/TypeScript/shared-library edits, combined HTTP/socket SDKs, socket removal/restore and standalone/owned watchers pass Node/Bun runtime fixtures.
- [x] Add Docker Compose service entries for Rust services.
- [x] Add health check defaults for Rust services.
- [x] Add Rust test commands to app-level scripts.

### Release And Deploy

- [x] Add Rust build artifacts to release packaging.
- [x] Add Rust Docker build path.
- [x] Add Rust service metadata to release manifest generation.
- [x] Add Rust OpenAPI artifacts to platform upload.
- [x] Add Rust SDK artifacts to release outputs when selected.
- [ ] Verify ECS/container defaults work for Rust services.

## Phase 14 - Cross-Language Conformance

- [x] Build a canonical contract fixture service in TypeScript. (`framework-rs/parity/fixtures/canonical-iam/service.ts` bundles actual repository Express/core/Zod sources; the live paired runner checks 18 HTTP outcomes and both OpenAPI endpoints, explicitly retaining three rejected-JWT status differences.)
- [x] Build the same canonical contract fixture service in Rust. (`forklaunch-blueprint-sample-service::iam_router` — runnable, contract-driven, serves OpenAPI)
- [x] Snapshot OpenAPI outputs and compare normalized output. (`rust_canonical_http_parity.mjs` compares both actual full documents; the selected default-error profile now passes 366 HTTP outcomes with zero normalized OpenAPI differences. Broader runtime error behavior remains in the error-parity gate.)
- [x] Snapshot AsyncAPI outputs and compare normalized output. (`rust_ws_metadata_parity.mjs` compares 34 actual TypeScript/native WebSocket documents, then checks committed key-normalized snapshots; official schema/parser and payload/reference checks also pass. Broader external AsyncAPI protocols remain separately tracked.)
- [x] Run generated TypeScript SDK against the Rust service. (`cli/tests/rust_http_runtime.sh`; relocated app, live HTTP, graceful shutdown verified)
- [x] Run generated Rust SDK against the compiled TypeScript HTTP wire fixture, including typed JSON, statuses, text/binary uploads and multipart files through both contract and OpenAPI generation. Full TypeScript framework service conformance remains separate.
- [x] Add CLI tests that generate Rust TypeScript/Rust SDKs and typecheck the generated TypeScript SDK entrypoint.
- [x] Add CLI tests that assert generated Rust and TypeScript SDK model shapes come from Rust contract schemas.
- [ ] Test validation parity for params, query, headers, body, multipart, binary, SSE, and WebSockets.
- [ ] Test auth parity for JWT, JWKS, Basic, HMAC, roles, permissions, and scopes. An opt-in verified-session TypeScript policy now matches 1,495 actual middleware cases for access, set presence, permission/role precedence, exact scopes, features and subscriptions. Generated signed-JWT HTTP/MCP cases pass. Opt-in Basic/JWT credential verification now matches 1,931 actual middleware cases, including Node decoding, colon behavior, trusted async resource decoders, falsy JWT results and sanitized callback failures. Synchronous login and resource verification pass generated HTTP/MCP plus both SDK checks. HMAC header/verifier dispatch additionally matches 1,320 middleware cases, with real generated signature/freshness checks. Ordered session validation/grant callbacks match another 2,335 actual middleware cases, including missing callbacks, short-circuiting and unchanged claims. Schema-validator equivalence, remaining callback/date boundaries and error-envelope differences remain open.
- [ ] Test error response parity. Opt-in `AuthErrorFormat::TypeScriptText` matches actual credential/policy rejection text and UTF-8 media types, with generated HTTP/MCP checks in JSON/text modes. Opt-in LocalRouter handler errors now match safe TypeScript text/correlation, and caller-provided request ParseErrors now match the final TypeScript 400 text envelope, with an opt-in typed parser/transport failure profile and application-supplied unversioned query validation plus bounded finite-response validation bringing the canonical comparison to 366 live HTTP outcomes, including malformed URI decoding, repeated/invalid queries and trusted header/body response diagnostics. Structured parse-error formatting additionally matches 918 actual TypeScript cases and multiline session errors pass generated HTTP/MCP in both wire modes. Auth error schemas, corrected empty-response emission, runtime-checked request-validator rejection metadata and bounded opt-in default error strings now leave zero normalized OpenAPI differences in the canonical IAM fixture. Default-string declarations are backed by handler and middleware tests, with explicit incompatible-profile checks. Opt-in router fallbacks additionally match 100 actual HTTP requests for escaped unknown-path/method 404s, HEAD headers and malformed matched parameters, preserving application/internal fallbacks. An independent opt-in correlation profile matches 108 actual requests, including generated UUIDs, repeated/Latin-1 values, HEAD and application overrides without inventing incoming headers. Native schema diagnostics, remaining extractor origins, CORS policy, remaining versioned400/default-error boundaries and broader artifact coverage stay open. Bounded versioned missing404/500 and guarded401/403 default-string declarations now pass actual-source/runtime and metadata checks while preserving original callback schemas.
- [x] Test telemetry presence where feasible. A relocated generated service passes HTTP/MCP startup and real OTLP trace/log/metric export on SIGTERM, W3C parent/correlation propagation, status/size/duration instruments, zero active requests and bounded metric dimensions. Native middleware covers auth failures, unmatched routes, streaming completion/cancellation/errors and HEAD; JSON/protobuf exporter fixtures cover headers/resources/opt-out/configuration errors.

## Phase 15 - Release Readiness

- [x] Add framework workspace guide, complete crate/feature catalog, routing/auth/media/infrastructure and operations guidance, linked CLI examples and strict all-feature API docs. All 19 crate docs build with warnings denied; CI enforces the build.
- [x] Add blueprint workspace guide, complete crate/feature catalog, module/provider/worker/compliance and operations guidance, linked CLI examples and strict all-feature API docs. All 30 distinct crate docs build with warnings denied; CI enforces the build.
- [x] Add examples for Rust services, Rust workers, and mixed TypeScript/Rust apps. ([Runnable generation guide](../docs/adding-projects/rust.md): service startup, database/Redis/BullMQ/Kafka workers, both mixed-app directions and HTTP/MCP probes. Existing generated/runtime fixtures validate these examples; remaining broad documentation coverage stays open.)
- [x] Add a migration guide for TypeScript users adopting native services/workers/libraries/modules, with workspace requirements, preserved settings, shared SDKs and separate Docker builds.
- [x] Add crate publishing pipeline. The [release workflow and guide](../docs/adding-projects/rust-releases.md) package 49 versioned public crates, exclude three private samples, stage registry installation and verify a real HTTP/MCP service. Explicit tagged publication checks clean source, artifact integrity and the matching validation receipt, preserves the required parser artifact on an existing release and uploads in dependency order. Nine release regression tests and local registry/startup validation pass; actual public publication remains a separate gate.
- [x] Add version compatibility matrix between TypeScript and Rust packages. ([44-package source baseline and artifact boundaries](../docs/adding-projects/rust-compatibility.md); no unpublished registry or full-runtime compatibility claims.)
- [x] Add performance benchmarks for HTTP, validation, SDK calls, Redis, S3, and WebSockets. (`rust_benchmarks.mjs` drives the feature-gated native example and a generated TypeScript SDK; all six live paths pass debug smoke and optimized runs with isolated Redis/MinIO, warmups, correctness checks and JSON reports. CI checks execution without timing thresholds.)
- [x] Add security review for auth, HMAC, JWT, encryption, secrets, and generated SDKs. ([Engineering review](RUST-SECURITY-REVIEW.md) records source/fixture evidence, fixed HS256 timing and dependency findings, application-owned controls and three exact reviewed dependency notices. CI scans all lockfiles and rejects new/changed/unreviewed findings; broader provider/deployment gates remain open.)

## Initial Milestone Definition

The first useful milestone is not full parity. It is:

- [x] A Rust HTTP service using `framework-rs`.
- [x] Typed request/response structs with validation-mode metadata and OpenAPI output.
- [x] A generated TypeScript SDK package derived from the Rust service contract.
- [x] A generated Rust SDK crate derived from the same Rust service contract.
- [x] A minimal `blueprint-rs` IAM interface crate proving the blueprint style.
- [x] CI snapshot tests for the generated OpenAPI and SDK output. (`forklaunch-sdk-typescript::snapshots` commits normalized OpenAPI and both typed HTTP SDKs for a shared catalog contract; the workspace CI test job runs them. Actual TypeScript-to-Rust output equivalence remains a separate gate.)

## Open Decisions

- [x] Choose the native ForkLaunch registry emitter as canonical; neither `aide` nor `utoipa` is required. Optional adapters must preserve the registry and pass conformance fixtures. ([Decision](../docs/adding-projects/rust-compatibility.md#implementation-choices))
- [x] Preserve explicit path/query coercion and safe integral socket JSON normalization; retain declared JSON body types and reject loose string/boolean coercions. This reconciles the implemented phase-2 policy. ([Decision](../docs/adding-projects/rust-compatibility.md#implementation-choices))
- [x] Generate Rust SDKs from both OpenAPI and the richer ForkLaunch contract artifact; verify equivalent live HTTP behavior from both formats.
- [x] Generate TypeScript SDKs through both the CLI and framework-native API using shared model compilation, templates and fetch/SSE runtimes.
- [x] Support configured Better Auth behavior directly in the isolated native `forklaunch-iam-better-auth` crate, with encrypted tenant storage and a raw HTTP namespace alongside generic IAM/session integration. Remaining behavior and CLI-generation gates are tracked below.
- [x] Standardize built-in PostgreSQL persistence on optional `sqlx` behind domain store traits; SeaORM remains an application adapter, not a required parity dependency. ([Decision](../docs/adding-projects/rust-compatibility.md#implementation-choices))

## Additional TypeScript Module Coverage Required For Full Parity

The initial phases cover only part of the repository's published TypeScript surface.
These packages are part of the full-parity target as well; their absence must not
be hidden by completed framework/blueprint scaffold checks. Audit concrete exports,
contracts and TypeScript behavior, then implement idiomatic Rust equivalents and
validate generated modules and SDKs. Runtime-specific JavaScript adapters do not
require literal Rust clones, but their public request/response behavior does.

- [ ] Inventory every published framework/blueprint package and map its public runtime, provider, service, DTO, contract and generated-module capabilities to Rust implementations and conformance fixtures. The 44-package/module inventory is recorded in [RUST-TYPESCRIPT-PARITY-INVENTORY.md](RUST-TYPESCRIPT-PARITY-INVENTORY.md); the checked source inventory now resolves 626 entry points and records 1,713 declaration/re-export records, public members, type aliases, external boundaries, module sources and CLI bins. Sixteen common naming/hash/path symbols have explicit implementations and actual-source conformance; remaining detailed semantic symbol/DTO/fixture mapping stays open.
- [x] Complete messaging interface/base persistence, generated SMS module and Twilio delivery/status-callback integration with provider fixtures and tenant isolation. (`blueprint/interfaces/messaging`, `blueprint/implementations/messaging/{base,twilio}`, generated messaging modules) Actual TypeScript base/Twilio fixtures cover DTOs, all 16 status cases, dispatch forms, callbacks and error clearing. Generated Rust/mixed modules pass both SDKs through contract and OpenAPI imports, JWT/HMAC failures, signed-form tampering, restricted-role RLS/encryption/auditing, restart persistence, compliance and init/rename/sync/delete. SMS storage uses native Rust migrations; shared MikroORM schema and generic compliance/retention wrappers remain separate tasks.
- [x] Port all five billing interface service families and base persistence/cache lifecycle. The 168-operation actual TS base-class comparison and native memory/PostgreSQL tests pass; provider ID/party lookups use indexed tenant-scoped queries, and metadata-only ID lookup avoids decrypting provider data. Native responses retain complete typed DTOs when a generic mapper would otherwise return partial portal data; configured Stripe mappers and Redis cache interoperability remain separate.
- [x] Port Stripe billing authenticated transport and five asynchronous provider services over tenant-bound base persistence. Twenty-one official Stripe 22.6.2 transport comparisons and 144 actual TS wrapper operations pass, including plan replacement, subscription item updates, nonresumable cancellation errors, session IDs/expiry, payment-link metadata, provider failures and cache/backup lifecycle. Native checks enforce timeout/response/form bounds, redaction and redirect refusal. Shared raw-body verification retains 20 SDK signature comparisons and adds stale billing callback rejection; the synchronous reference adapter now fails closed instead of fabricating provider results. Fixtures use owned generic mapper/storage boundaries, so configured application mappers remain a separate gate.
- [x] Define the thirty shared billing application contracts for both variants. Sixty-six actual TS route comparisons (including compliance and Stripe callbacks) and 28 actual configured DTO schema comparisons pass, including HMAC versus admin/JWKS portal auth, canonical paths, params/query/body, response statuses, required fields, UUID base IDs and provider-specific enums/`stripeFields`. Legacy customer/invoice reference contracts remain compatible under their existing module; generated billing uses the new module registry. Callback/platform contracts are covered by the webhook task below.
- [x] Generate billing-base and billing-stripe modules with 33 HTTP/MCP routes each (five service families, compliance and health), configured enums and top-level `stripeFields` translation. Live generated TypeScript/Rust SDKs, HMAC and admin/system JWT roles, query lists, tenant isolation, encrypted/audited PostgreSQL data, restart persistence, disabled/custom-path MCP, graceful shutdown and init/sync/rename/delete fixtures pass. Fresh archive installation compiles both variants and validates service HTTP/MCP startup and contract/SDK exports. Current TS billing entities classify all fields as non-PII; compliance preserves system-role authorization and empty 404 results.
- [ ] Complete configured billing mapper semantic review and provider reconciliation. Generated party lookups explicitly configure lowercase application enum values; generic services retain uppercase defaults matching the actual TS base fixture. Native typed responses preserve complete IDs/fields where TS configured cache mappers drop them. Managed Stripe transport is covered below; mixed deployment remains a separate gate. Portal webhook expiry now uses checked Unix seconds in both implementations; configured HTTP enums accept SUCCESS/CREATED/UPDATED webhook states. Cache-only checkout/portal reads preserve provider DTOs and restore declared dates without ORM access. Configured cache-only portal updates now use schema-validated DTO hooks without ORM access; provider replacement, identity guards and custom mapper fallback pass actual-source tests. Configured checkout URI and checkout/portal/plan/subscription create provider options now admit omission consistently across TS DTOs, Zod, TypeBox and native contracts; nullable URI storage has a generated migration guide. Configured checkout/payment-link reads resolve retained provider references while preserving public UUIDs, with schema/identity checks and custom mapper fallback. Configured payment-link lists now validate cache-key/public identities, read owned references sequentially and filter explicit provider active booleans; trusted record limits fail before reads instead of silently truncating Stripe pages. Opt-in native direct and managed payment-link reads now resolve durable public UUID aliases under fresh account/mode verification, with generation, expiry, tenant isolation and audited-read PostgreSQL checks passing. The companion dedicated signed endpoint also passes actual controller/protocol tests and cross-language HTTP/mock comparisons. PaymentLink create payloads now require an object with a line_items array of objects before provider calls; nested options remain intact. Configured billing-base cache-only updates use identity-checked DTO hooks without ORM access. Configured PaymentLink updates now validate cached identity and patch schemas before provider mutations; configured Plan updates validate before provider replacement and preserve omitted names. Configured Subscription updates validate before provider I/O and reject owner changes that would leave the provider customer unchanged. Pure configured creation preflight now runs before provider effects for all five billing families;52 new source cases,353 configured regressions,170 service comparisons and12-root strict types pass. Alias auto-provisioning, provider/local identity mapping, remaining Stripe mutation/update semantics and complete validator behavior stay open.
- [x] Implement Stripe billing webhook business synchronization for portal, checkout, payment-link, legacy plan, price, paginated product and subscription events. Forty-three actual TS webhook state/provider-effect comparisons pass, including partial missing-plan updates, subscription party reassignment protection and missed-created-event recovery. Direct callbacks verify exact Stripe-signed bytes; platform payment callbacks bind instance HMAC credentials, platform key ID, path/body/time and atomic PostgreSQL nonces. Tenant-RLS metadata-only success receipts persist across restarts, reject event type reuse and serialize duplicate delivery; failed events retry. Admission limits preserve pool capacity for callback business writes. Native PostgreSQL deduplication/concurrency/retry/isolation/nonce/admission tests pass. Generated base/Stripe runtime gates now cover 33/35 HTTP/MCP routes, signed direct callbacks, platform key-ID/time/nonce failures, subscription updates without party reassignment, duplicate callbacks across restart, disabled callback credentials, both SDKs and CLI lifecycle. The generated signed MCP callback fixture exposed a framework transport bug: text bodies with application/json content type were JSON-encoded. MCP now dispatches by contract body kind and preserves signed bytes, with a dedicated regression test.
- [x] Port managed outbound Stripe transport through a reusable instance gateway client. Twenty-three actual TS createStripeClient/gateway-fetch comparisons verify gateway URL prefixes, instance-key HMAC over full query filters and exact form bytes, fresh nonces, products/prices and existing resources, provider failures and native redirect refusal. Direct transport retains 21 official SDK comparisons and all 144 service operations. Generic JSON capability requests have native signature/path/body/nonce/configuration checks. Generated services select complete trusted managed settings before vendor keys, accept the TS STRIPE_API_KEY name plus the existing STRIPE_SECRET_KEY alias, and carry optional gateway/callback settings through Compose. Generated restricted-role PostgreSQL runtime validates keyless managed HTTP/MCP startup and signed plan CRUD/list, local keys ignored in managed mode, incomplete managed settings rejected, both SDKs, callbacks, restart and CLI lifecycle. The full fixture passes.
- [ ] Finish interrupted billing provider/local-write reconciliation and remaining configured mapper semantics. Durable webhook intent and audited explicit resolution pass restricted-role PostgreSQL, generated signed HTTP/platform delivery, restart, both SDKs and HTTP/MCP lifecycle validation. Optional authenticated checkout recovery now verifies the exact Stripe event/current session and atomically repairs inspected local state with audit/receipt/attempt completion; Direct and managed authenticated paused-effect acknowledgement now atomically completes delivery bookkeeping only when exact provider/binding/customer evidence and local inactive state agree, with stale snapshot/deadline checks. Managed reads use a dedicated signed, account/mode/generation-bound event/subscription endpoint. It proves no pause-epoch causality and performs no subscription/provider writes; other webhook effects and missing ownership remain unresolved. Plan replacement deletes the old provider plan before creating its successor, and provider calls precede local writes, matching TS; external effects are not transactionally atomic. Cancelled Stripe subscriptions cannot be resumed and provider errors leave local state inactive.
- [x] Port all ecommerce interface DTOs/traits and the six base services with PostgreSQL persistence. Actual TypeScript service conformance covers 208 operations and all 36 order-state pairs. Native memory/PostgreSQL fixtures cover stock/cart/order/payment concurrency, uniqueness, tenant RLS, encrypted/audited addresses, key rotation and audit rollback. Cache failures/timeouts fall back to storage; native integer/overflow checks preserve database limits.
- [x] Port ecommerce product-controller search and catalog reads: variant price/options/stock filters, explicit empty results, optional inventory, stable createdAt/id pages, SQL limit/offset and tenant isolation. Thirty-five actual TypeScript controller comparisons and memory/PostgreSQL fixtures pass. Native pagination uses integral limits/offsets; SQL/storage failures propagate instead of being treated as absent stock.
- [x] Port ecommerce order inventory event consumption and native leased-worker handler. Thirteen actual TypeScript event comparisons cover legal transitions, paid decrement, paid/fulfilled cancellation restock, pending cancellation, multi-item rollback and retry. Native memory/PostgreSQL fixtures additionally enforce positive quantities/legal transitions, atomic durable receipts, payload identity conflicts, tenant isolation and concurrent redelivery. A real PostgreSQL queue fixture verifies stock is unchanged on redelivery after an expired unacknowledged lease and reopening the store. Generated transition/event producer wiring remains below.
- [x] Define typed core ecommerce module contracts and provider-selecting creation DTOs. Twenty-seven actual TS controller/router metadata fixtures compare canonical methods/paths/names, HMAC, params/query/body and response statuses. Response-only Stripe secrets are explicit on creation and excluded from reads; checkout/import/webhook/compliance registries and live module wiring remain separate.
- [x] Add tenant-checked PostgreSQL queue enqueue inside an application transaction and atomic ecommerce order transition/event production. Restricted-role live tests cover queue-write and tenant failures rolling back order/audit changes, concurrent transition winners, real paid/fulfilled/cancelled worker delivery and stock restoration. Existing PostgreSQL worker lease/retry/isolation/runner fixtures pass after sharing the insert path.
- [x] Mount core ecommerce HTTP/MCP routes through the tenant-bound PostgreSQL adapter, audited request actors, Stripe/PayPal selection and atomic order/event production. Live TCP fixtures cover all 27 application routes plus health, provider wire requests, JSON query filters/pages, typed-body validation, response-only secrets, MCP discovery/signed tools/Origin rejection, RLS and real inventory delivery. Strict workspace tests/clippy and archive-installed adapter compilation pass; generated CLI lifecycle and remaining ecommerce controllers stay open below.
- [x] Generate the core ecommerce-stripe CLI module in Rust/mixed applications with provider configuration, offline contracts, HTTP/MCP startup, both SDKs and a separate inventory worker. Live generated fixtures verify Stripe/PayPal wire requests, HMAC/MCP authorization, RLS, encrypted/audited orders, paid/cancelled stock updates, worker controls, restart persistence, relocation and source-preserving init/sync/rename/delete.
- [x] Port Stripe Tax and flat-rate shipping calculators. Thirty-two actual TypeScript comparisons cover USD form fields, jurisdiction breakdown, omitted line2, free/domestic/international rates, seven-percent estimated fallback and integer rounding. Native checks reject negative/overflow amounts and flag malformed, oversized, negative and redirected provider responses as estimates without exposing provider bodies.
- [x] Port bulk catalog import with typed HTTP/MCP contracts and both SDKs. Nine actual TS controller operations cover external-ID upserts, optional-field preservation, variant reparenting, default shipping/stock and live-stock preservation. Generated PostgreSQL HTTP/SDK/MCP re-imports retain adjusted inventory and reject null stock; full CLI lifecycle and native restricted-role regression fixtures pass. Partial import failures remain resumable, matching TS sequential semantics.
- [x] Wire checkout through generated HTTP/MCP and both SDK schemas: stock/current variant prices, checked integer totals, Stripe Tax/explicit estimate, flat shipping, order/cart association, pending order/payment resume, provider switching, stale basket replacement and provider failure. Eight actual TS controller comparisons run against owned PostgreSQL/provider service boundaries. Cross-process races produce one order/payment/provider call; ten parallel carts finish without pool starvation. Tenant/namespace/drop-release lock tests, verified actor audit checks and complete generated lifecycle fixtures pass. Cart retention and cancelled-order provider-reference reconciliation preserve current TS behavior; webhook/capture guarantees remain below.
- [x] Generate ecommerce inventory worker Compose orchestration with the HTTP image, isolated runtime credentials, container readiness and module ownership. Sync preserves worker overrides; rename/delete follow the owning module and preserve sibling workers. Native generated lifecycle and actual Linux Compose fixtures pass with restricted-role PostgreSQL, HTTP/MCP discovery/authentication, paid/cancelled inventory delivery and SIGTERM for both processes.
- [x] Complete ecommerce persistence compliance hooks: protected address export with explicit audited plaintext access, tenant-scoped whole-order erasure and opt-in framework retention policies. Twenty actual encrypted ORM/native comparisons, memory/restricted PostgreSQL concurrency/RLS/audit/rollback checks, strict Clippy, 879 packaged sources and generated mixed HTTP/MCP/SDK/provider/worker/lifecycle validation pass. The TS preset defines no compliance controller or default retention period; its non-nullable address is retained by anonymization and removed by whole-order deletion. Native behavior preserves these boundaries. (`blueprint/ecommerce-stripe`)
- [x] Port ecommerce Stripe/PayPal native clients and payment services: PaymentIntent creation/resume, direct Connect fees, OAuth/Orders/signature verification, confirmation/failure and capture-claim rollback. Actual TS conformance covers 15 Stripe scenarios, 22 PayPal client cases and 19 wrapper operations; live provider fixtures validate wire requests, single capture, retry rollback, response bounds, credential redaction and response-only client secrets. Embedded source archives compile both providers.
- [x] Add durable provider webhook bookkeeping with tenant-RLS PostgreSQL storage and owned memory fixtures. Ten actual TS ledger operations verify new/unfinished/processed replay, provider-scoped identity and repeated completion. Native concurrent receipts have one fresh winner; persistence survives reopening, tenant/namespace isolation and changed-type identity rejection pass. Raw provider payloads are not persisted. This gate records processing state; provider authentication and serialized business handling remain below.
- [x] Add raw-body Stripe webhook authentication with constant-time HMAC verification, multiple signatures, secret redaction and the SDK replay window. Twenty official Stripe 22.6.2 SDK comparisons verify exact bytes, timestamp boundaries (including its future-time behavior), rotated signatures and malformed headers. Route/business processing remains below.
- [x] Mount and generate Stripe/PayPal callbacks with raw-byte/provider API verification, typed metadata and both SDKs. Thirty-one application contracts match actual TS metadata. Eleven actual TS controller comparisons cover paid/failed/mismatch, verification, replay and capture rollback/retry over restricted PostgreSQL/native provider boundaries; native checks additionally cover original PayPal event preservation, tenant isolation, cross-process simultaneous/same-payment events with one capture, public-route session independence, SDK/MCP guards and real inventory delivery. Full generated HTTP/MCP startup and source-preserving lifecycle checks pass. PayPal amount reconciliation requires exact nonnegative cents; malformed/fractional-cent amounts do not authorize paid transitions.
- [ ] Complete interrupted external capture and cancelled-order provider-reference reconciliation. Authenticated PayPal and Stripe reads now support audited repair of verified completed captures, with stale-snapshot checks and protection against delayed claim release. Terminal Stripe cancellation with no charge or an expanded verified failed/unpaid charge now supports audited failure repair and blocks later confirmation; PayPal VOIDED orders with explicit empty monetary histories also support audited failure repair. Durable tenant/provider/deployment/merchant/mode bindings now record explicit administrative ownership assertions with audited generation checks; they do not yet verify providers or authorize cancelled-order repair. Configured checkout resume now rejects missing/mismatched/cancelled durable orders before/after provider reads while retaining references and generic fallback. A read-only direct Stripe inspector now verifies account/mode and bound intent fields, rechecks generation/deadline after provider and database waits, and rejects Connect before I/O; it grants no repair or monetary authority. Separate direct and explicitly scoped Connected Stripe inspectors verify account/mode and generation. A direct or Connected Stripe operator flow now repairs terminal no-charge intents for unchanged cancelled-order/payment/binding snapshots, retaining references and recording tenant receipts atomically with audit; ambiguous COMMIT outcomes require receipt inspection. The separate PayPal operator flow also verifies exact merchant/environment/order evidence and explicit empty monetary histories before atomic failure/audit/receipt publication for an already-cancelled local order (migration12). Pending/ambiguous claims, paid/authorized prior charges, omitted PayPal histories and broader cancelled-order provider-reference reconciliation remain open. Ordinary operations preserve TS provisional succeeded claims and failed-payment confirmation semantics. (`blueprint/implementations/ecommerce/{stripe,paypal}`, `blueprint/ecommerce-stripe`)
- [x] Support JWT allowlists spanning RSA/EC/EdDSA key families: verify the configured policy and JWK metadata first, then narrow only the decode operation to its selected allowed algorithm. Regression and generated CAC JWKS tests pass.
- [x] Port TypeScript request-HMAC canonicalization and authorization headers with constant-time verification, normalized timestamps and optional clock-skew checks. Sixteen actual TypeScript signer/header-generator fixtures cover absent/text/buffer/raw-JSON/Unicode/timezone cases; atomic distributed nonce replay policy remains application-owned.
- [x] Add CAC provider DTOs/traits, internal HMAC-declared lookup contracts and the free mock provider. Actual TypeScript provider conformance passes for descriptor/licensing, all existing mock codes, exact matching and unknown codes. The authenticated runtime and generated CAC module are now validated separately below.
- [x] Port CAC code-set provider interfaces and the existing mock procedure-code provider, preserving descriptor/licensing metadata and lookup behavior; do not invent or bundle licensed clinical data. (`blueprint/interfaces/cac`, `blueprint/implementations/cac/base`, generated CAC module) Generated Rust/mixed modules now mount request-HMAC lookup and JWKS/system-role-protected empty-entity compliance routes, health and docs. Both SDKs, claim-policy failures, shared key caching, embedded-workspace upgrades, coexistence, source preservation, rename/sync/delete and source-archive compiler checks pass.
- [x] Audit BullMQ producer/consumer options, retry/retention behavior, waiting/active inspection and tenant Redis-prefix/ACL handling; cover missing Rust worker behavior and cross-language interoperability requirements explicitly. (`blueprint/implementations/worker/bullmq`)
- [x] Match actual BullMQ interval/cron scheduler lifecycle in both directions, including three-iteration limits, failed/successful jobs and inspection/removal. Twelve scenarios reproduce and fix fetch-time scheduling and zero disabling the start limit, cover active cancellation and sequence continuity after preprocessor rejection, and run in CI. All 66 isolated protocol tests, strict adapter Clippy and all 752 packaged assets pass. Broader scheduler metadata/date/cron/update options remain separate gates.
- [x] Preserve scheduler log/history/size metadata in templates, first jobs and subsequent iterations through a shared encoder. Twelve actual-library scheduler scenarios pass; the eight bidirectional lifecycle cases also verify options on every job, newest-two log retention and bounded failure history. All 66 protocol tests, strict adapter Clippy, 752 packaged assets and the rebuilt CLI pass. Creation timestamps and cron scheduled-time metadata now also pass every-iteration comparisons; all twelve scheduler scenarios, 66 protocol tests, strict adapter Clippy and the rebuilt CLI pass. Fresh generated-worker controls/lifecycle and the full companion service HTTP/MCP matrix pass. Date/other scheduler metadata remain open.
- [x] Match BullMQ simple/TTL/delayed-replacement deduplication through actual TypeScript/native single/bulk producers and both consumers. Six scenarios cover cross-producer keys, latest delayed data, completion release and expiry. Four additional active-job keep-last scenarios pass both actual consumers with cross-producer submissions, newest-payload follow-up and release after completion or terminal failure. Repeat schedulers remain separate gates.
- [x] Preserve BullMQ failed reasons and bounded histories across immediate/delayed retries and terminal failure through existing lock-checked transition scripts. Sixteen actual TS/native worker scenarios cover recovered/terminal jobs and absent/zero/one/two-entry limits. Full wire regression, 66 isolated protocol tests, five adapter unit tests, strict adapter Clippy and all 751 packaged source assets pass. Fresh generated-worker histories, controls/restart/lifecycle and the full companion service HTTP/MCP startup matrix also pass.
- [x] Audit the configured Better Auth 1.7.6 plugin and HTTP surface against actual source/library execution. Capture 70 HTTP route descriptors, four server-only helpers and 49 protocol checks with an owned UUID/SQL adapter boundary. Cover real discovery/surfacing controllers, signed/revoked session cookies, credentials and rotation, Origin/callback checks, EdDSA/JWKS claims, organization defaults, configured role denials and disabled flows. This reference audit does not imply native protocol or real-ORM parity.
- [x] Add isolated native Better Auth password/cookie primitives and direct bidirectional library comparisons. Scrypt matches N=16384/r=16/p=1, 64-byte keys, NFKC normalization and text salt encoding. Cookies match HMAC-SHA256, standard padded base64, JavaScript URI encoding, duplicate/quoted/malformed cookie parsing, secure names and 24-hour/transient/cleared options. All 322 comparisons pass, strict crate clippy passes, and source/templates are packaged. Endpoint/session persistence remains open.
- [x] Add atomic memory/encrypted PostgreSQL storage for all eleven configured Better Auth models. Independent schema migration ledger, forced tenant RLS, runtime-role checks, model/record-bound encryption, rotating blind unique indexes, shared/exclusive transaction locks and transactional audit records are implemented. Live PostgreSQL verifies rollback/read-only enforcement, tenant isolation, secrets absent at rest, signing-key restoration, key rotation and ciphertext transplant rejection. Auth endpoint behavior, relationship/cascade policy and indexed pagination beyond bounded 4096-row model scans remain open.
- [x] Audit actual configured session lifecycle beyond configuration values. Sixty checks cover exact one-hour refresh, refresh suppression, stable tokens/creation time, creation-based freshness despite extended expiry, scoped/unknown/foreign token revocation, revoke-others/all, replay, transient signed cookies, expiry cleanup and the precise expiry millisecond. Configured POST get-session is rejected with 405 because deferred refresh is disabled. Native session endpoint comparisons remain open.
- [x] Add indexed encrypted auth lookups and keyset pagination beyond the explicit 4096-row model-scan bound. All production auth callers use scoped indexes or pages, including key rotation and oldest-batch retention. Memory/restricted PostgreSQL exercise larger account/session/member/role/team/invitation/verification/key stores, with index backfill, rotation, RLS and atomicity. Actual TS comparisons, strict Clippy, generated HTTP/MCP/frontend clients and lifecycle checks pass. Configured JWKS selection retains the upstream first-100-record limit; global cleanup remains linear and responses allocate their returned matches.
- [x] Use blind-indexed exact user email/subscription and organization-slug lookups, plus bounded primary-key user joins. Larger user directories pass memory/restricted PostgreSQL HTTP/MCP and 979 actual TS comparisons. Token and relationship lookups are also indexed; session collections, credentials, linked accounts and basic membership queries use scoped keyset pages. Organization/member/role/team/invitation queries now use scoped indexes; provider/password and compliance use scoped indexes, and verification cleanup uses keyset pages. Keyring and retention callers now use bounded pages; see the completed storage gate above.
- [x] Implement the native persistent session backend: random 32-character tokens, 24-hour lifetime, one-hour refresh, creation-based freshness, refresh suppression, scoped/unknown/foreign revocation, revoke-others/all, sign-out and transient expiry cleanup. Forty-eight native body/storage comparisons execute the actual configured HTTP handlers with shared seeded credentials; the fixture includes 63 checks. Memory and live encrypted PostgreSQL lifecycle tests pass. Native credential operations, production HTTP cookie/CSRF/Origin integration and indexed session lookup remain open.
- [x] Implement native email signup/signin over encrypted auth records. Signup commits user/account/session atomically and rechecks normalized-email uniqueness under the write lock. Bounded blocking scrypt jobs hold capacity through cancellation and run outside tenant write locks; signin rechecks account credentials afterward. Thirty-nine comparisons match actual configured handler bodies/stored records, callbackURL casing, errors, case normalization and UTF-16 password limits. Memory/live PostgreSQL cover concurrent signup; a controlled password-change race rejects stale verification without creating a session. Production HTTP validation/cookies/Origin/CSRF, remaining credential/account operations and indexed lookup remain open.
- [x] Match native authenticated password verification and rotation against actual configured handlers. Eighty-one credential comparisons cover password errors, no-revocation retention, all-own-session replacement and other-user isolation. Memory/live PostgreSQL exercise encrypted credential updates and session replacement. Blocking derivation stays outside write locks and authoritative session/account rechecks reject concurrent changes. Production HTTP cookies/CSRF/Origin, user/account operations and CLI integration remain open.
- [x] Match native trusted-origin/callback and mutation/login CSRF policies against 2,822 actual library/HTTP cases. Cover exact/wildcard/custom origins, UTF-16 wildcard units, relative callback separators/traversal, Cookie header presence, Origin/Referer fallback, null same-origin Fetch Metadata and cross-site login navigation. Production adapter wiring remains open.
- [x] Add native signup/signin/password verification/rotation HTTP routes over atomic encrypted storage. Compare 122 actual HTTP body/storage/cookie/callback outcomes and 63 full parser/schema/guard/method errors. Preserve transient replacement-cookie flags, additional-field JSON presence, ignored private inputs, forwarded-IP normalization and current-request agent/IP metadata. Live memory/restricted-role PostgreSQL exercise signup, signin, rotation and revocation through TCP alongside MCP startup/discovery/invocation/shutdown. Existing 161 session/JWT HTTP comparisons remain green. Remaining raw routes, production rate limiting and CLI preset stay open.
- [x] Port configured Better Auth default production per-IP/per-path rate limits, rolling windows, special endpoint rules, fallback IP buckets, atomic concurrency and retry headers. Compare 153 actual production HTTP outcomes, including guard/parser order and normalized IP/path keys. Bounded process-global memory indexes preserve insertion eviction. A live TCP burst admits exactly three of 32 simultaneous signin requests and shuts down cleanly. Configurable positive default windows/budgets and exact-path overrides (including disabling selected endpoints) additionally match 174 actual Better Auth HTTP outcomes; built-in sensitive rules retain precedence over configured defaults. Ordered wildcard/exact custom rules additionally pass 969 actual HTTP comparisons; replacement preserves rule order and wildcard patterns compile at configuration time. Asynchronous request-metadata callbacks now support inherited rules, disablement and replacement budgets/windows, with 1,485 actual HTTP comparisons across all rate profiles. Plugin rule registration/precedence and inherited callback limits additionally pass the expanded 1,947-case actual HTTP fixture. Async custom counter storage additionally passes 2,409 actual HTTP comparisons, including effective-rule/key traces, disabled-rule bypass and optional/zero retry values. Storage errors fail closed without memory fallback. Applications own backend atomicity/expiry; body-reading callbacks and built-in external counter backends remain part of the broader customization gate.
- [x] Add native update-user, list-accounts and unlink-account HTTP routes. Fifty actual response/cookie/storage comparisons cover scoped accounts, token/hash filtering, ECMAScript scope trimming, opaque profile updates, ignored private fields, transient cookies and full errors. Atomic unlinking preserves one remaining login under concurrency; revoked sessions cannot mutate profiles. Live memory/restricted-role encrypted PostgreSQL exercise profile HTTP beside MCP. Remaining user/account/provider operations and real ORM conformance stay open.
- [x] Match the four configured-disabled reset-email, verification-email, change-email and delete-user HTTP routes. Compare 128 actual response/cookie/storage outcomes, including schema/Origin order, authoritative session requirements and empty 404 deletion responses. Live memory/encrypted PostgreSQL verify their HTTP responses alongside MCP. Actual token reset/verification and remaining user/account/provider operations remain open.
- [x] Add native single-use token password reset. Forty-four actual HTTP/storage comparisons cover body/query tokens, validation order, UTF-16 limits, newest duplicate selection/invalidation, expired/missing-user consumption, exact expiry, missing credential creation and retained sessions. Bounded hashing stays outside tenant locks; concurrent consumption permits one winner. Live memory/restricted-role PostgreSQL exercise reset and password verification over HTTP beside MCP. Email verification/callback routes remain open.
- [x] Add GET reset-token and configured-disabled deletion callbacks. Fifty-two actual HTTP/storage comparisons cover query validation aggregation, raw encoded token segments, trusted destinations, configured redirect authority, query replacement/encoding, JSON redirect content type, expired-verification cleanup and non-consuming GET behavior. Live memory/restricted-role PostgreSQL follow GET redirects with POST reset alongside MCP. Email verification and remaining user/account/provider operations remain open.
- [x] Match configured HS256 email-token verification against 96 actual JOSE/HTTP outcomes. Distinguish forged/malformed tokens, expiry and handler schema failures; preserve critical-header behavior, optional iat/nbf/exp ordering, padding/noncanonical bits, BOMs, duplicate keys and numeric overflow using local raw JSON parsing. Native email-verification HTTP/user/session/redirect integration remains open.
- [x] Add signed email-verification HTTP integration. Fifty-two actual response/storage/cookie/header comparisons cover idempotent verification, confirmation/verified/legacy change tokens, authoritative session checks, reuse/creation/transient cookies, error-query appending, raw relative and byte-string success redirects, whitespace normalization and empty malformed-claim 500 responses. Live memory/restricted-role PostgreSQL verify HTTP mutations, forgery rejection, cookies and subsequent session/JWT access beside MCP. Configured email delivery/change requests remain disabled; provider/org/account operations and CLI preset remain open.
- [x] Complete configured Better Auth user/account operations over native credential/session HTTP, actual handler/JOSE/provider comparisons and real MikroORM 7.2.3/adapter 0.5.11 encrypted PostgreSQL conformance. Broader optional hooks/adapters remain a separate gate.
- [x] Add native Ed25519 JWT issuance/public JWK primitives with configured claims and 15-minute lifetime. Twenty-six comparisons execute the actual configured Better Auth payload/signing/JWKS and JOSE verification. Private PKCS#8 restoration preserves IDs/signatures; wrong issuer/audience, exact expiry and forged signatures fail verification. The subsequent keyring checkpoint covers encrypted persistence/selection/rotation; session-authenticated HTTP integration remains open.
- [x] Add durable native keyring selection and rotation over encrypted auth storage. First-key initialization serializes concurrent callers; service reconstruction preserves keys, selection keeps actual insertion ties, and retired public keys obey the exact 30-day grace boundary. All-expired stores publish an empty set until issuance provisions a new live key. Twenty-seven actual Better Auth/JOSE comparisons and memory/live-PostgreSQL lifecycle tests pass, including corrupt key rejection. Session-authenticated token/JWKS HTTP integration remains open.
- [x] Integrate nine native raw session/JWT HTTP endpoints with signed cookies, authoritative storage, refresh/freshness/revocation, credentialed CORS and Origin/callback guards. Include session-authenticated token issuance, public JWKS and the get-session JWT response header. Compare 161 actual configured HTTP body/storage/header/JWT outcomes; live memory/restricted-role encrypted PostgreSQL validate HTTP and MCP startup/discovery/invocation, auth, CORS, CSRF and graceful shutdown. HTTPS extraction/clearing uses secure names/attributes. Raw auth routes stay outside contract/MCP discovery. Credential HTTP, remaining routes and CLI preset remain open.
- [x] Complete Better Auth custom callback exchange and generated client integration over the native session-authenticated token/JWKS endpoints. Reconciled the previously implemented popup helpers and shared/generated client factories: the current generated PostgreSQL fixture revalidates signed callback exchange, independent JWT/JWKS verification, all eight default/custom/root/URL-override client scenarios, restart and HTTP/MCP lifecycle. Broader optional hooks/client plugins and unified mixed SDK behavior remain separate gates.
- [x] Correct the configured Better Auth invitation lifetime to 48 hours in seconds. An operator-seeded dynamic invitation grant exercises the actual handler and validates persisted expiry; default-role denials remain intact.
- [x] Port configured Better Auth organization/team/member/invitation/dynamic-role endpoints, permission checks, default teams, membership limits and active organization/team. Actual surfacing service comparisons, encrypted PostgreSQL/real ORM conformance and registered HTTP/MCP role/permission routing pass.
- [x] Port configured Google authorization-code/PKCE/state, callback/account linking and token lifecycle behavior, including verified profile mapping, protected provider transport and actual comparisons. Configured callback URLs also honor custom/root/base-URL override namespaces.
- [x] Correct configured Google sign-in's required-name mapping. A valid cryptographically verified Google ID token previously failed with `400 MISSING_FIELD` because `firstName` was absent. Map Google given/family names into required application fields, using empty strings when optional/malformed external name claims are absent. Execute real Google RS256/JWKS verification and Better Auth handlers with only Google's certificate HTTP boundary owned; verify sign-in/session/account reuse and reject forged signatures, wrong issuer/audience/nonce/key ID, expired and stale/future-issued tokens. Native Google OAuth and real ORM conformance remain open.
- [x] Integrate configured Better Auth classified Account/Jwks export/erasure over HTTP/MCP with current-store system-role authorization. Seven-calendar-year retention and encrypted ORM/native compliance comparisons pass, including partial reads and atomic erasure rollback.
- [x] Harden the actual Better Auth callback helpers before native porting: enforce configured trusted callback origins, derive token-exchange authority from configured base URL, escape query data embedded in script JSON, and clear the configured secure session cookie. Forty checks execute actual server handlers and browser scripts under HTTP/HTTPS with real session/JWT handlers, forged Host headers and malicious callback/script inputs.
- [x] Add native application-owned HTTP namespaces for the raw Better Auth API. Internal mounts inherit global middleware, preserve raw request bodies, rebase through nested routers, reject contract/documentation/MCP namespace collisions, and remain absent from OpenAPI/SDK/MCP contracts. Live TCP tests verify cookie-authenticated HTTP alongside the unchanged MCP discovery/invocation surface and graceful shutdown. This is routing infrastructure; native Better Auth handlers remain open.
- [x] Add Better Auth module CLI generation/lifecycle, registered HTTP/MCP application routes and SDK contracts. Relocation, preserved sync/reinit, restricted PostgreSQL, shared/separate/disabled MCP, JWT/HMAC, restart, rename/delete and both SDKs pass; all 1,016 CLI tests pass. Raw authentication routes remain separate from registered MCP tools.
- [x] Port both custom Better Auth callback helpers: 92 actual Express/TS HTTP/HTTPS comparisons, independently verified JWTs, cookie clearing, script escaping, duplicate queries, Accept/HEAD, corrupt keys and expiry pass. Fresh generated PostgreSQL HTTP/MCP/lifecycle, 27 native tests, strict feature-matrix clippy, prior callback/raw HTTP regressions and 749 packaged sources pass.
- [x] Add additional Scalar sources and default/custom/root auth namespaces across HTTP/MCP, helpers, docs, Google/reset callbacks, generated configuration and default self-JWKS.
- [ ] Complete broader Better Auth ORM/hooks/client/options coverage, typed/custom JWT/JWKS adapters and indexed storage/pagination; audit dynamic/encoded namespace semantics separately. Trusted async custom JWT signing now bypasses local key/storage side effects with actual Better Auth claim/default comparisons; external signing for authenticated HTTP sessions and popup callbacks, with explicit remote-key metadata and local JWKS suppression, is also implemented. Ordinary unfiltered/unsorted organization-member pages now use indexed counts and bounded member/user hydration, with a 4,200-member instrumentation test, restricted PostgreSQL isolation/rotation/concurrency checks and 59 actual ORM/native checks passing. Generated PostgreSQL services now select an explicit member-query profile, with 434 actual ORM/native HTTP comparisons passing in both UTC and verified America/Los_Angeles sessions for filters, sorting, UUID/timestamp/LIKE semantics and fractional pagination. Filtered/sorted projections fail explicitly above 4,096 candidates or 1 MiB; ordinary indexed pages remain independent of that cap. A fresh CLI-generated preset passes HTTP/MCP shared/separate/disabled listeners, actual blueprint/generated auth clients, both SDKs, encrypted sessions, restart and rename/delete lifecycle with this profile. An explicit unexpected-only API-error observer mode now matches actual handled versus unexpected failure dispatch, including session/revocation and dynamic-permission boundaries; historical all-5xx callbacks and sanitized built-in tracing remain available. Signup user-creation failure now matches 422, while account/session/transaction failures retain unexpected empty 500 responses; signout suppresses storage failures and clears the exact three reference cookies without claiming persisted revocation. Optional JWT-header suppression skips signing/key creation while preserving explicit token/JWKS endpoints. Configurable whole-millisecond JWKS grace periods preserve default expiry/signing behavior. Fixed root JWKS mounts preserve namespace/auth checks; async payload/subject callbacks and typed relative/absolute JWT expiration options preserve claim precedence and cookie identity. Typed tenant-bound custom JWKS read/create hooks, additive public-only reads and trusted exact key-ID signing constraints now have actual JOSE/native coverage. Broader adapter/options, other fault paths, exact default logging counts and dynamic/encoded namespaces remain open.
- [x] Validate generated-module init/change/sync/delete, contracts, SDKs and mixed-app operation for every added module family. The eight-variant mixed-app matrix preserves a TypeScript library and user code, validates component names, compiles both SDKs from OpenAPI/contracts and checks rename/deletion ownership. Family runtime receipts cover HTTP/MCP/storage/provider behavior; relay and captured storefront lifecycle have separate fixtures below. Archive installation, documentation and CI coverage are updated; deployment/provider completeness stays in its own gates.

- [x] Support Rust/mixed relay session-ingest and app-claim hooks, route-scoped HMAC, encrypted atomic nonce/ticket consumption, browser session cookies and safe redirects. Actual TS ORM/native comparisons, PostgreSQL conformance, live HTTP/MCP, generated SDK calls/builds, relocation, preserved sync/reinit, rename and deletion pass; this CLI surface has no separate published blueprint package.
- [x] Support captured storefront initialization, sync, rename/description updates and deletion in Rust/Node/Bun/mixed apps. Preserve static paths across Rust-to-mixed migration and keep captures outside Cargo/package/API generation; hosting remains a deployment concern.

- [x] Audit all exported HTTP methods (including WebDAV/link/unlink/subscribe variants) and generated custom-router extension behavior inside built-in Rust modules; preserve actual mounted contracts and fail explicitly for unsupported combinations.

- [x] Preserve the actual HTTP method for authentication on implicit HEAD-to-GET dispatch, while retaining explicit HEAD precedence and local-handler compatibility; include request-HMAC regression coverage.

- [x] Validate actual service HTTP and MCP startup, initialization/discovery/tool invocation and authenticated dispatch for generated generic services, IAM, CAC, base/Twilio messaging and custom routers. Live fixtures cover all 27 HTTP methods, signed JSON/forms, bearer/session/JWT forwarding, tenant rejection, revocation, RBAC, Origin rejection, graceful shutdown and archive-installed enabled/disabled/custom-path startup. Reuse these checks for each additional generated module family.

- [x] Align JSON query arrays/objects/null across native MCP and both generated TypeScript SDK surfaces. Forty actual TypeScript MCP-generator/universal-SDK comparisons cover empty arrays, Unicode/reserved characters and single percent encoding. Remove duplicate URL encoding in both TS transports; live native HTTP/MCP and generated SDK regressions pass. Rust SDK query maps continue to accept explicitly serialized string values.

- [x] Complete multipart MCP calls and expand actual TypeScript generator conformance fixtures. Thirty-two calls use real FastMCP, the repository Zod validator/generator and the official MCP client over its in-memory transport, proxy to live HTTP and match native body encoding. Native guarded HTTP-extractor calls and all 18 MCP tests pass; CI runs both layers. Versioned APIs and broader HTTP/schema conformance remain separate open gates.

- [x] Execute actual configured billing schema resolvers, Zod runtime/OpenAPI and application mappers. Sixty-six route contracts, 28 real validator/OpenAPI DTO schemas and 229 configured mapper cases pass. Correct optional Stripe payload contracts, HTTP unknown-field stripping, Stripe local plan/subscription IDs and portal owner preservation; generated HTTP/MCP, both SDKs and verified-webhook-created portal reads pass. Record intentional stricter native coercion/provider-object checks and corrected cache provider-ID behavior; Redis interop and crash reconciliation remain open.

- [x] Port managed payments onboarding/status helper over the instance gateway. Sixteen actual TypeScript/native comparisons validate instance-key signatures, request bodies, readiness DTOs, single-use onboarding requests, invalid redirects, failure statuses, Retry-After hints and malformed/oversized responses. No automatic onboarding retry or vendor credential is introduced. Native response and redirect validation is stricter; remote error bodies are redacted. Other managed capability helpers remain open.

- [x] Port managed SMS client, typed send/event DTOs and GSM/UCS accounting. Fifty-five actual TypeScript/native cases cover all GSM extensions, UTF-16 surrogate-pair and ten-segment boundaries, E.164, ECMAScript whitespace, default/promotional purpose, deferred refusal, signed JSON wire bytes, fresh nonces, opt-out/refusal statuses and retry hints. Native response DTOs are validated and remote errors redacted. CLI managed-capability registration/event stubs for native modules remain in scope; client coverage alone does not complete infra add/remove parity.

- [x] Port managed WhatsApp template/text/listing client and delivery DTOs. Twenty-one actual TypeScript/native comparisons cover template component pass-through, empty listings, recipient/text validation, Unicode whitespace, platform policy refusals (403/409/422/429), signed exact JSON bodies, nonce freshness and retry hints. Typed native responses reject malformed data. CLI capability registration/event routes remain open.

- [x] Port managed voice start/read/end and event DTOs. Forty-seven actual TypeScript/native comparisons cover catalog flow names, E.164, attribute key/value/count/UTF-16/byte limits, reserved prefixes, call-ID isolation, status, empty 200/204 hang-up responses, exact signed bodies, nonce freshness, refusal statuses and retry hints. Empty-response support is opt-in; existing provider JSON response validation remains strict. CLI capability wiring/event mounting remains open.

- [x] Port managed email send/suppression and delivery DTOs. Fifty-one actual TypeScript/native comparisons cover mailbox rules, scalar/array recipients and replies, 50-recipient bounds, UTF-16 subject length, UTF-8 body size, tag limits/reserved names, suppression lowercase and encodeURIComponent paths, signed body bytes, gateway refusals and retry hints. Test servers collect complete bytes before UTF-8 decoding, including a 512 KiB multibyte body. Native email responses remain typed; CLI capability/event wiring and model gateway remain open.

- [x] Add reusable platform-event verification with an asynchronous replaceable nonce-store interface and bounded atomic memory default. Thirty-four actual TypeScript/native comparisons cover exact raw bytes, method/path/key binding, minimal event shape, skew boundaries, replay, malformed payloads and same-event-ID redelivery. One hundred concurrent deliveries have one nonce winner. Native retains future-dated nonces through their full inclusive validity window, closing an actual TS replay gap, and bounds bodies/storage without evicting valid nonces. Memory is single-process; replica callers must supply shared storage, and event-ID business deduplication remains handler-owned. Generated capability routes/wiring remain open.

- [x] Add contract-preserving native route decoration for platform capability dispatch and other middleware. Standard and extended HTTP methods, dynamic paths, nesting, existing-handler fallback and request validation pass; live MCP and direct HTTP both execute the decorated route. All 29 Axum unit and two live MCP tests pass with strict clippy.

- [x] Add trusted managed-settings resolution and explicit fixed Docker gateway-mock transport. Fourteen actual TypeScript/native configuration cases cover missing/partial/complete settings, loopback, fixed mock opt-in, remote TLS, invalid opt-in and credential redaction; normal transport continues to reject remote plaintext. Generated Stripe billing uses the validated gateway constructor and passes HTTP/MCP, both SDKs, callbacks, restart, CLI lifecycle and invalid-opt-in startup rejection. Actual Docker capability add/remove/startup wiring remains open.

- [x] Add native managed-capability CLI wiring for email, SMS, WhatsApp, voice and payments. `infra add/remove` resolves the registered native crate path, stages typed client factories, handler stubs, callback contracts and shared HTTP/MCP middleware, and records source/dependency ownership. Clients share a gateway connection pool; SMS defers unmanaged refusal as the TypeScript registration does. Fresh relocated mixed applications compile both SDKs and pass HTTP/MCP startup, actual-TypeScript-signed callbacks for every feature, forgery/staleness/replay rejection, authenticated MCP boundaries, dry-run and stacked removal preservation. Untouched files restore byte for byte; human handler/Cargo/library/env edits survive. All 60 CLI infrastructure tests pass, including 31 existing TypeScript capability tests. Linux Compose verifies all five outbound clients over signed Docker-DNS calls and signed gateway-to-service callbacks, plus HTTP/MCP startup. Archive-installed CLI generation, service HTTP/MCP settings, callback validation and contract/SDK exports pass. Ownership snapshots are Git-ignored and private on Unix because restoration records may contain pre-existing local settings. Capability lifecycle rename/delete, safe removal without local ownership records, shared replica nonce storage, and in-place S3/Redis CLI wiring remain open.

- [x] Dispatch managed callbacks using Axum-decoded feature parameters while authenticating the exact request URL path. Generated runtime accepts a correctly signed percent-encoded feature and rejects a signature over a different decoded URL; HTTP/MCP, SDK and source-preservation regressions pass. This exact-URL binding is stronger than the TS controller reconstruction of its decoded feature path.

- [x] Exercise native capability decoration on a generated Stripe billing module with its existing payment-event route. All five capabilities stack without duplicate contracts; email/SMS/WhatsApp/voice callbacks reject replay, and payments retain PostgreSQL receipt processing and subscription business effects. Full billing HTTP/MCP, both SDKs, managed Stripe, restart and CLI lifecycle regressions pass.

- [x] Preserve native capability hooks and custom source through service sync, rename and deletion. Renames move private ownership records, update Cargo restoration snapshots and local callback targets, and update generated config/smoke-test names without rewriting customized files. Registered custom crate locations survive sync and rename. A fresh-clone recovery path removes generated hooks without a local ownership record while retaining dependencies/settings of unknown origin. Deletion removes ownership records and releases or retains the shared mock according to remaining manifest capabilities. Generated relocated mixed-app fixtures pass HTTP/MCP before/after rename, SDKs, six service smoke tests, custom nested crate paths, clone recovery and deletion; 62 infrastructure tests pass. Multi-service mock baseline/target preservation and human edits to managed dependency entries remain under review.

- [x] Retain human edits to managed dependency entries across stacked capability commands, including custom paths, extra features and explicit dependency removal. Private ownership records remember the last human entry before generated edits; restoration no longer mistakes a later generated snapshot for user ownership. Three native ownership/lifecycle tests pass. The preceding lifecycle checkpoint also passes all 958 CLI tests and the full generated billing HTTP/MCP, SDK, persistence, managed gateway and rename/delete fixture. Multi-service mock ownership and callback-target preservation remain open.

- [x] Track the local gateway mock at application scope so a custom pre-existing mock survives both service removal orders and deletion even when the first capability owner leaves early. Removing the active owner promotes another registered service; removing an inactive owner preserves the existing target. Rename handling retains exact host boundaries and URL suffixes. Real CLI/Compose configuration tests pass both orders, callback promotion, deletion and custom baseline restoration. Cargo ownership comparisons now use TOML values, avoiding false human overrides when sync changes inline dependencies into tables; custom features/paths/removals remain preserved. All 960 CLI tests and the expanded generated HTTP/MCP/SDK/lifecycle fixture pass. Edits made to the shared mock while capabilities are active still need ownership-aware merging; local-mock routing supports one target per feature.

- [x] Merge developer changes into shared gateway restoration records while capabilities remain active. Changes to images, environment keys and nested extension metadata survive later additions/removals/deletion; deleted keys stay deleted. Customized generated mocks become developer-owned and remain without stale generated callback targets. Explicit mock removal updates the baseline even if a later add recreates it. Real CLI/Compose ownership fixtures and all 960 CLI tests pass.

- [x] Preserve Docker Compose standard long network/port/volume forms and short dependency lists while editing native and TypeScript infrastructure. Network aliases/IP assignments, named ports and mount options round-trip through CLI edits; dependency required/restart metadata and build arguments/secrets are retained. Application rename retains structured options, and port allocation handles address bindings, protocols and ranges. All 964 CLI tests, real normalized Compose ownership edits and generated HTTP/MCP, SDK and capability lifecycle checks pass.

- [x] Generate a preserved `src/capabilities/nonce_store.rs` factory returning `Arc<dyn PlatformEventNonceStore>`. Callback dispatch creates one verifier with that store for every feature; application code can supply shared atomic storage without editing the owned dispatch section. A customized capacity-two store survives sync/rename, accepts email and SMS, then returns 503 for voice, proving shared capacity rather than a new store per callback. Default stub removal and custom factory retention pass alongside generated HTTP/MCP, SDK, source/dependency ownership, clone recovery and custom crate lifecycle checks. All 960 CLI tests pass; the default remains single-process memory and applications must configure shared storage for replicas.

- [x] Wire native in-place Redis/S3 infrastructure commands and honor init/change infrastructure flags. Editable typed factories, relative Cargo dependencies, local settings, manifest resources and shared Compose backends support registered nested crate paths, sync, rename, removal, deletion and clone recovery. Customized factories/dependencies/settings and adopted backends remain application-owned. Generic service metadata edits preserve source. Fix relative directory paths and update ejection to use the source parent; capability removal no longer restores unrelated removed infrastructure settings or discards adopted/deleted gateway dependencies. All 967 CLI tests, real CLI/Compose ownership fixtures and relocated generated Redis/MinIO binary round-trips pass; HTTP/MCP enabled/disabled/custom-path startup and both SDK compilers pass. Factories currently expose raw adapters with explicit local S3 credentials; default production AWS credential-chain configuration and generated encryption/compliance wiring remain open.

- [x] Complete generated Redis/S3 encryption/compliance factories and actual TypeScript/native encrypted-storage comparisons, as covered by the later JSON/queue checkpoints. Complete helper/worker coverage remains tracked separately.

Packaged CLI validation also passes native Redis/S3 generation alongside all five managed capabilities, relocated compiler checks, service HTTP/MCP settings, signed callbacks and contract/SDK exports. Fresh IAM, CAC and base/Twilio messaging runtime fixtures pass HTTP/MCP and lifecycle checks; the IAM fixture now rejects an actually unsupported module instead of the now-supported billing module.

- [x] Add production S3 environment resolution and an async generated factory using the official AWS default credential chain with profile/SSO/process/container-role support. Complete S3 key pairs override the chain; partial pairs follow the TypeScript fallback. Both factories honor namespace and presign caps. Thirty-one actual TypeScript/native configuration cases cover endpoint/region/key combinations, prefixes, numeric/radix/fractional values and ECMAScript whitespace. An owned ECS credential endpoint supplies session credentials used to sign actual SDK object/tag requests under the configured prefix. Generated relocated factories round-trip data through real Redis/MinIO using standard AWS environment credentials, pass HTTP/MCP settings and both SDK compilers. All 967 CLI tests, 18 S3 unit tests, the actual S3 backend test and strict all-target clippy pass. Preserve the synchronous explicit-local factory for compatibility; encrypted/compliance-aware cache and object-store wiring remains open.

Fresh IAM, CAC, base/Twilio messaging and ecommerce runtime/lifecycle fixtures pass. Together with billing and generic managed/local-infrastructure services, these validate HTTP/MCP initialization, discovery, health/tool invocation, SDK compilation and graceful service shutdown. Ecommerce comparisons use the isolated Stripe SDK through `STRIPE_SDK_PATH`.

The production S3 checkpoint also passes archive-installed CLI generation, portable framework compilation, HTTP/MCP settings, signed capabilities and contract/SDK exports. The package-source inventory now checks 268 embedded source/template assets, including the new S3 example directory and infrastructure/capability factories.

- [x] Align native Redis list queues with the actual TypeScript LPUSH/RPOP wire layout and add non-destructive batch peeks in dequeue order. Keep an explicit LegacyNative RPUSH/LPOP mode for existing native backlogs; producers/consumers must use one consistent layout. Actual TypeScript queue methods and native clients exchange data through the isolated Redis backend in both directions, covering batch/single FIFO, mixed producers, peeks and legacy backlog reads. Strict all-target clippy and all six Redis tests (including the actual backend fixture) pass. This closes list orientation interoperability; JSON/encryption/compliance, complete TTL/cache metadata and BullMQ worker wire conformance remain open.

- [x] Add shared fallible `JsonRecordCodec`, typed `JsonCache` and async `JsonObjectStore` wrappers plus explicit generated encrypted factories. Per-operation validated tenant context selects authenticated v2/v3 envelopes; encrypted reads without context, wrong tenants, malformed payloads and serialization failures fail closed. Plaintext JSON migration and key rotation work, including false/zero/null values. Pre-serialize cache batches before writing. Actual TypeScript Redis methods and native clients exchange encrypted JSON in both directions against Redis, covering rotation, expiry, corruption and tenant rejection. Relocated generated services use the factories against real Redis/MinIO with encrypted at-rest checks, metadata preservation and wrong-tenant rejection, then pass HTTP/MCP enabled/disabled/custom endpoint startup and both SDK compilers. All 967 CLI tests, five core tests and six non-ignored Redis tests pass; strict infrastructure all-target clippy and 273 packaged source assets pass. Direct TypeScript/native S3 storage comparisons, encrypted queue convenience methods and complete TTL/cache metadata remain open.

- [x] Add generated `encrypted_queue(name)` and typed enqueue/dequeue/batch/peek helpers, with TypeScript-compatible FIFO envelopes and explicit decode-after-removal semantics. Add transactional GET/PTTL JSON cache reads retaining subsecond expiry and persistent-key metadata. Actual TypeScript/native encrypted cache/queue exchanges pass v2/v3, both producer directions, batch order, non-destructive wrong-tenant peeks and false/zero/null preservation. Actual TypeScript S3 methods over their injected SDK boundary and signed native AWS SDK requests exchange v2/v3 JSON in both directions, including prefixes, content types, rotation, plaintext migration, misses and tenant/corruption rejection. Real MinIO encrypted factories remain covered by the generated runtime fixture. Fresh relocated encrypted-queue/service runtime passes HTTP/MCP settings, shutdown and both SDK compilers; strict infrastructure all-target clippy, six non-ignored Redis tests and 274 packaged source assets pass. The prior encrypted-factory checkpoint also passed archive-installed CLI generation, HTTP/MCP, managed callbacks and SDK exports. Per-record batch TTLs, complete S3 helper coverage and worker/BullMQ wire parity remain open.

- [x] Complete typed Redis per-record TTL batches with prevalidated keys/expirations and transactional value/expiry reads. `put_records` supports a separate TTL or configured default per entry; `read_records` keeps order, misses and persistent-key metadata; literal-prefix SCAN reads expose decoded records with expiry. Actual TypeScript/native batch writes and reads compare values and relative expirations in both directions, including encrypted envelopes. Invalid TTL/key batches leave the first valid entry unwritten, empty batches work, and glob characters are matched literally. The full Redis fixture, including the actual backend, passes all seven tests; strict all-target clippy and the extended encrypted cache/queue conformance harness pass. Retain the intentional native semantics of precise milliseconds, explicit persistent expiry, misses as Option and falsy-value preservation rather than TypeScript's rounding/filtering accidents. Worker/BullMQ interoperability remains open.

- [x] Add typed S3 JSON batch writes/reads and single/batch deletes, defaulting absent content types to application/json while preserving custom types/tags and ordered misses. Serialize/encrypt all JSON before starting batch writes; backend partial-write behavior remains explicit. Actual TS object-store methods (including JSON stream-upload aliases) and signed native AWS SDK calls pass encrypted single/batch exchange, false/zero/null payloads, deletes, missing objects and empty batches. A serialization-failure regression proves no S3 write occurs and serializer details stay redacted. Generated encrypted factories pass real MinIO single/batch writes, metadata/default-type reads, missing keys, deletes and wrong-tenant rejection, then HTTP/MCP settings/shutdown and both SDK compilers. Strict S3 all-target clippy, 19 unit tests and encryption-only feature tests pass; all 274 embedded source assets package correctly. Complete binary/stream/presign helper parity, worker/BullMQ interoperability and release gates remain tracked separately.

- [x] Wire platform Redis key/channel namespace conformance into native leased workers and Pub/Sub controls. Resolve REDIS_KEY_PREFIX before FORKLAUNCH_REDIS_KEY_PREFIX with actual TypeScript-compatible whitespace/separator behavior; preserve dedicated-cache keys and native cluster hash tags. Add injected RedisKeyScope/connect_scoped APIs and sanitized NOPERM errors. A real restricted ACL fixture compares actual TS prefix helpers, enqueues/claims/acks native jobs, fences tenants, delivers Pub/Sub controls and denies foreign keys/channels and FLUSHDB. The runner fails promptly on NOPERM rather than looping. The fixture exposed redis 0.32 PubSub subscribe silently discarding server-error payloads; add a bounded subscription-ack preflight without publishing control messages, and verify denied subscription startup fails. All seven Redis-worker unit tests and both actual Redis integration tests pass, as do strict all-target clippy, 276 packaged assets and generated worker runtime/control/retention/batches/relocation/rename/backend-change/delete regressions. This closes platform prefix/ACL startup behavior; BullMQ wire compatibility remains open and is being implemented separately.

- [x] Add an optional official BullMQ Rust protocol adapter and actual TypeScript/native interoperability fixture. Both producer/consumer directions pass single/batch jobs, fixed/exponential retries, terminal failures, duplicate event IDs, default retention, and waiting/active inspection against a real restricted Redis ACL. A denied native consumer fails promptly and drains. Strict all-feature blueprint workspace clippy, seven worker unit tests, both actual Redis integration tests, formatting and 278 packaged source assets pass. CI now runs infrastructure JSON/encryption and worker ACL/BullMQ conformance fixtures using pinned BullMQ 6.3.10. Generated BullMQ lifecycle support, native failure observers and complete option/retention coverage remain open.

- [x] Generate native BullMQ workers through init/change/sync/rename/delete with the official protocol adapter, preserved application handlers, retries/backoff/concurrency/timeouts, inspection and scoped optional Pub/Sub controls. Add HTTP health/readiness and authenticated pause/resume/drain with active-job shutdown, plus a watch-based control transition API. Per-attempt native failure observers retain original events through retries, automatic removal and graceful drain; consumer peeks work before start. Actual TS/native fixed/exponential retry and default/count-based retention fixtures pass both directions under restricted Redis ACLs. Fresh relocated generation passes explicit stub failures, real processing/retries, HTTP controls, service HTTP/MCP initialization/discovery/health/enabled/disabled/custom-path checks, contracts/SDKs and source-preserving lifecycle changes. A nested workers/team/jobs registration passes sync/reinit/backend conversion/rename/delete, closing hardcoded worker crate/dependency paths. All 967 CLI tests, strict Redis/database-worker clippy, three control-transition tests and 283 packaged assets pass. E2E CI includes the pinned BullMQ runtime fixture. Age-based retention, stalled-job/custom-option behavior and broader worker conformance remain separate gates.

- [x] Bind additional typed MCP tools to the real Streamable HTTP runtime. ManualTool::typed derives discovery metadata from FrameworkSchema, parses/validates before application work, supplies fresh headers for operation authorization and preserves transport authentication. Reject manual/contract name collisions at startup; normalize explicitly public handler failures, redact serializer details, bound invocation time/output and preserve object/scalar/array results. Eleven MCP tests exercise initialization/discovery/invocation, HTTP/manual coexistence, schema/type/range/unknown-field rejection before side effects, tenant credentials per call, timeout, output limits and serializer failures. Strict MCP/Axum and whole-blueprint all-feature clippy, formatting and 285 embedded assets pass. Fresh generated generic HTTP/MCP and SDK fixtures pass after the new dependency. Fresh IAM, CAC, base/Twilio messaging, billing/Stripe and ecommerce runtime/lifecycle suites also pass HTTP/MCP startup and authenticated dispatch. Separate listeners, multipart and complete actual-generator conformance remain open.

- [x] Add configurable separate MCP listener orchestration to generated generic, IAM, CAC, messaging, billing and ecommerce executables. MCP_LISTENER=separate uses the actual TypeScript HTTP-port-plus-2000 fallback; MCP_PORT/MCP_HOST select explicit/ephemeral ports and hostname/IP binding. Preserve the native shared-listener default explicitly. The second router exposes only MCP while tools retain internal HTTP validation/authentication; both listeners bind before serving, then drain together with a 30-second bound. Live two-listener tests verify route credentials, denied calls, concurrent active HTTP/MCP completion and closed ports. Fresh generated startup covers shared/separate/disabled/custom endpoints, explicit/ephemeral/default-offset ports, hostname binding, invalid settings and occupied-port rejection without a leftover HTTP listener. Generated BullMQ runtime and nested lifecycle regressions also pass. All 967 CLI tests, 31 Axum unit tests, two live MCP integration tests, eleven MCP tests, strict Axum/MCP clippy, formatting and 287 packaged assets pass. Multipart and remaining actual-generator conformance are still open.

- [x] Fix managed-capability wiring for the new serve_with_default_mcp entrypoint while retaining legacy into_router_with_default_mcp mounts. Insert the capability router exactly once and preserve byte-for-byte removal; reject ambiguous/missing mounts. A regression exercises all six current generated entrypoint templates plus the legacy call. All 968 CLI tests pass, and fresh billing/Stripe and ecommerce HTTP/MCP/SDK/capability/lifecycle fixtures pass after the correction. IAM, CAC and base/Twilio messaging fixtures also pass with coordinated listener startup.

- [x] Add MCP multipart text-field transport, File/Binary JSON-argument encoding and URL-encoded composite values matching the actual TypeScript generator. Generate collision-checked MIME boundaries and escape field names/line breaks; reject non-string multipart parts and SSE request bodies. Thirty-two actual-generator/body-discriminator comparisons and fifteen MCP unit/transport tests pass, including real Axum Multipart/Form/Bytes extractors behind route authorization. Strict full-framework all-feature/all-target clippy, formatting and 289 embedded assets pass. Expand the archive-installed CLI fixture with official BullMQ generation and assertions for typed, separate-listener and multipart MCP sources. Streaming, arbitrary multipart upload parts and broader MCP option conformance remain open.

- [x] Complete multipart uploads in both CLI SDK targets through contract and OpenAPI generation. TypeScript accepts FormData or typed field records and uses the actual shared safeStringify source for multipart/URL-encoded values; files, repeated names, JSON composites and special JavaScript values match thirteen actual universal SDK comparisons. Normalize header casing and let encoders own boundaries. Rust SDKs consume native reqwest forms, preserve authentication/hooks and repair reqwest zero-byte empty forms with a closing MIME delimiter. Preserve multipart bytes in both HTTP adapters. The framework Rust request builder now encodes owned multipart text/file parts and exposes dynamic content types; the framework TypeScript generator clears conflicting multipart headers. Strict standalone Rust/TypeScript compilation and standard MIME parsing pass; fresh relocated SDKs call Rust and TypeScript services successfully from both artifact formats. All 969 CLI tests, 31 Axum unit/two live MCP tests, fifteen MCP tests, both SDK unit suites, strict framework workspace clippy, formatting, 290 embedded assets and a fresh archive-installed CLI fixture pass. The archive fixture also typechecks its generated TypeScript SDK and verifies the bundled shared stringifier, all four workers, modules, capability callbacks and HTTP/MCP startup. Multipart responses, automatic typed extraction, SSE and remaining recursive/realtime SDK tasks remain open.

- [x] Add transport-independent bounded SSE encoding/decoding with arbitrary chunk boundaries, UTF-8/BOM, CR/LF/CRLF, multiline data, comments, IDs/resets, event names and retry metadata. Reject metadata injection, fail closed on event overflow and discard unterminated EOF frames according to the HTML standard. Eighteen HTTP tests, strict HTTP clippy, formatting and 292 embedded assets pass. Thirty-five actual TypeScript universal SDK comparisons pass for common terminated frames; CI now runs them. Document existing TS multiline/whitespace/EOF differences explicitly. Generated streaming clients, cancellation/reconnection and full typed runtime behavior remain open.

- [x] Generate bounded pull-driven SSE responses in CLI TypeScript/Rust SDKs from both contract and OpenAPI artifacts. Return typed async iterables or boxed owned Rust streams, preserve authentication/hooks, enforce configurable 1 MiB default event bounds and response MIME, deserialize Rust envelopes and release responses on iterator return/throw, abort, close/drop and terminal failures. Preserve complete events before later oversized events even when coalesced in one chunk. Live relocated service tests verify delivery before EOF, split UTF-8, BOM/multiline/CRLF/bare-CR framing, ID resets, auth before handler work, finite/truncated streams, wrong media, type/transport/size failures, unstarted cancellation, server stream cleanup and graceful HTTP/MCP shutdown. Both artifact formats pass, including strict generated-SDK clippy; the streams are boxed to keep response enums small, and request-hook type aliases close an existing clippy warning. All 970 CLI tests, nineteen HTTP codec tests, 31 Axum/two live MCP tests, fifteen MCP tests, both framework SDK suites, strict full-framework clippy, formatting and 298 embedded source/template checks pass. Thirty-five actual TS universal SDK/native codec/generated SDK SSE comparisons and thirteen form comparisons pass; ordinary HTTP/multipart/MCP SDK regressions pass against Rust/TypeScript servers from both artifacts. A fresh archive-installed CLI also passes portable generation, all four workers/modules, TypeScript SDK compilation, capability callbacks and HTTP/MCP startup; E2E CI now includes the dedicated streaming fixture. Framework-native typed response decoding, remaining emitter/schema conformance, recursive models and realtime/mixed SDK work stay open.

- [x] Support guarded recursive CLI SDK models through contract and OpenAPI artifacts: self/mutual records, optional/null fields, recursive arrays/dictionaries and object/array unions. Preserve named graph edges with bounded local reference processing; inline acyclic definitions to retain existing types. Reject external/missing references, validation siblings and unguarded aliases before writes. Bound cached wide-graph traversal as well as initial expansion, and omit Rust Default implementations across required recursive edges. Rust vectors avoid redundant boxes; collection aliases use transparent wrappers. Rebase standalone contract schema pointers when exporting OpenAPI components, escaping component names and retaining literal examples/defaults/constants. Fresh relocated clients pass strict TypeScript compilation (including a rejected invalid recursive request), strict Rust SDK clippy, recursive HTTP/MCP invocation and graceful shutdown through both artifact sources. All 977 CLI tests, 31 Axum/two live MCP tests, nineteen HTTP codec tests, fifteen MCP tests, six OpenAPI tests, both framework SDK suites, strict full-framework clippy/formatting and 299 embedded assets pass. Ordinary HTTP/multipart/SSE regressions and 40 query/thirteen form/35 SSE actual TypeScript comparisons pass. The expanded archive-installed CLI fixture compiles recursive clients through both artifact formats alongside all four worker types/modules and passes HTTP/MCP listener settings and managed callbacks. CI includes the recursive runtime fixture. Framework-native typed responses/recursive models, broader schema constraints, multipart responses and typed realtime/mixed SDKs remain open.

- [x] Share standalone-schema relocation between OpenAPI and MCP through the optional forklaunch-contract/json-schema feature. Rewrite schema-pointer edges only, retaining examples/defaults/constants/enums/extension data and properties named $ref; escape body/query/header/path field locations. Handle tuple/conditional and unevaluated schemas, and preserve embedded $id resource bases. CLI imports explicitly reject resource IDs and dynamic references until their resolver is implemented. Live recursive tool discovery preserves an example containing a literal $ref while HTTP/MCP invocations and both SDKs pass through both artifact formats. All 977 CLI tests, 31 Axum/two live MCP tests, sixteen MCP tests, five contract/five OpenAPI tests, nineteen HTTP tests, both SDK suites, strict full-framework clippy/formatting and 299 embedded assets pass. The contract crate also checks without default features. A fresh archive-installed CLI passes recursive client compilation through both artifacts, all worker/module compilation, HTTP/MCP shared/separate listeners and all five managed callbacks. Rust field omission/null distinctions, framework-native response/recursive models, resource/anchor resolution and broader realtime/mixed SDK work remain open.

- [x] Preserve generated Rust SDK field presence: omit absent optional fields, reject null for non-nullable optional fields, distinguish optional null from omission and reject missing required nullable fields, including nested records, arrays and unions. Compare 94 cases against the actual TypeScript validator through contract/OpenAPI generation, strict TypeScript/Rust compilation and both live SDKs against Rust/TypeScript HTTP, with MCP invocation and graceful shutdown. Fix the canonical TypeScript null schema enum to contain JSON null. Box generated JSON model response variants to keep status enums small; update inner-union matching accordingly. All 977 CLI tests, fourteen existing/new Zod validator tests, ordinary HTTP/multipart/recursive/SSE regressions, formatting and 299 embedded asset checks pass. A fresh archive-installed CLI compiles both SDK artifact formats, all four worker types/modules and passes HTTP/MCP listener configurations and all five capability callbacks. CI includes the presence fixture and pinned actual-validator dependencies. Framework-native typed response/recursive generation and broader schema/realtime/mixed SDK tasks remain open.

- [x] Share the bounded local schema graph, Rust/TypeScript type compiler, Serde model renderer and Rust client/SSE templates between CLI and native framework generators. Retain request/field JSON schemas and response kind/MIME/schema metadata in SdkGenerationPlan. Add checked generate_rust_http_sdk_source with typed status responses, guarded recursive models, field presence, auth/hooks, multipart uploads and bounded owned SSE streams. Reject duplicate/runtime-reserved names, invalid media/statuses and unsupported schema graphs before source output. Bound literal data depth/node/bytes before cloning and stream-count serialization bytes; reject oversized JSON schema strings before parsing. Native clients pass strict compilation and the same 94 actual validator cases against both language servers, recursive tree/array/dictionary/union/mutual-record calls and live SSE cancellation/error/resource-cleanup checks with HTTP/MCP startup/discovery/invocation/shutdown. All 978 CLI tests, eight native codegen/four Rust SDK/three TypeScript SDK tests, full framework all-feature test suites, strict workspace clippy/formatting and 311 embedded assets pass. Existing native request-builder/media and ordinary HTTP/multipart SDK regressions pass. An archive-installed CLI compiles all four workers/modules and both SDK artifact formats, passes listener settings/capability callbacks, and invokes the native recursive generator entirely from bundled sources with generated-SDK warnings denied. Native TypeScript typed response/recursive emission, advanced schema constraints/resources, multipart responses and realtime/mixed SDK work remain open.

Native typed TypeScript HTTP SDK checkpoint: the same CLI model/runtime now handles status/business-error unions, recursive aliases, exact presence/null and bounded SSE. The 95-case validator, recursive and streaming fixtures exercise both native generators and both CLI artifact sources. Real media tests cover Blob/ArrayBuffer/sliced bytes, URL-encoded/multipart records, quoted MIME parameters, escaped paths, invalid JSON and status narrowing. Correct the messaging callback fixture to sign record composites as single JSON values, matching the actual TS universal SDK and MCP generator; explicit URLSearchParams retains repeated fields. All five generated module runtime suites (IAM, CAC, base/Twilio messaging, base/Stripe billing and ecommerce) pass HTTP/MCP discovery/invocation, SDK checks and their authenticated persistence/provider/lifecycle checks. Broader parity gates above remain open.

- [x] Share checked TypeScript HTTP generation with CLI types/runtime/templates; validate typed media/status errors, 95-case presence, recursive and SSE clients, all five module HTTP/MCP runtime suites and portable native generation from an archive install. All 979 CLI tests, nine codegen/four Rust SDK/four TypeScript SDK tests, strict workspace clippy/formatting and 319 embedded assets pass. Both artifact sources preserve quoted MIME parameters and escaped metadata.
- [x] Adapt SSE response ownership/cancellation to cross-fetch's Node byte streams. Own an abort controller, forward/detach caller signals and abort upstream requests on cancellation; live tests cover both CLI artifact formats, native injection/imported fetch, pending reads, unstarted cancellation and exact server cleanup.

- [x] Preserve Blob/FormData uploads across native and custom fetch adapters through shared body serialization in checked TypeScript and dependency-free native clients. Actual native/injected/imported cross-fetch tests cover file bytes, sliced views, repeated/empty multipart files, encoder-owned boundaries, URL-encoded composites, quoted media and status/business-error decoding.


- [x] Validate official BullMQ lazy age retention in both TS/native producer/consumer directions and abandoned-job recovery across languages, including maximum stalled/start attempts. Preserve original failure events even when preprocessor failures immediately remove their jobs, through a feature-gated outcome-data hook in a pinned vendored protocol crate. Keep existing failure notifications, Redis scripts and wire semantics; retain upstream MIT/provenance metadata and package the source in generated apps. Broader advanced worker options remain a separate gate.

- [x] Carry supported BullMQ queue defaults through the producer, preserving actual TypeScript retry/backoff overrides and ignored default job IDs. Persist job metadata and read Node log-limit aliases through an optional protocol patch. Actual cross-language metadata, size rejection, log retention, delay/priority/LIFO order and concurrency checks pass alongside native tests, 66 isolated protocol tests, generated worker lifecycle and HTTP/MCP startup. Broader advanced options remain open.

- [x] Validate BullMQ rate limits with concurrent workers and age retention cleanup limits against actual TypeScript/native consumers/producers. Measure start times and remaining old/fresh job identities/counts; strict worker clippy/formatting passes. Repeat/deduplication and broader options remain open.

IAM directory checkpoint (2026-10-11): real PostgreSQL transactions, strict workspace Clippy/tests, all 999 CLI tests and 422 embedded Rust assets pass. A fresh archive install compiles portable services, all worker/module families and both SDK generators, and passes the HTTP/MCP listener matrix and five signed capability callbacks. Generated IAM exercises thirty configured directory routes with request HMAC, asymmetric JWT/RBAC, storage-resolved organization scope, atomic batch rejection, authenticated MCP calls, persistent sessions and lifecycle operations. Full provider, compliance, mixed orchestration and release/deployment gates remain open.

IAM configured discovery/compliance checkpoint: direct conformance checks all 33 base-module routes, their actual RBAC constants and 22 request/response schema layouts. Generated IAM discovers 40 MCP tools (33 configured operations plus seven legacy session/health routes). HTTP/MCP verify public discovery, system-role authorization and the configured empty compliance behavior alongside all CRUD, both live SDKs, scoped atomic rejection and lifecycle checks. Generic entity compliance extensions and enabled Better Auth behavior remain open.

Better Auth foundations archive checkpoint: fresh packaged install `kSwMp4`
includes all native auth source/migrations and compiles/tests them after relocation.
Packaged live Axum HTTP/MCP integration, generated listener startup matrix and all
five native managed callbacks pass. Portable services/workers, both SDK generators
and strict generated consumers pass. Checkout CLI restored afterward. This does
not claim a generated Better Auth preset or its raw API endpoints are complete.

Better Auth HTTP archive checkpoint: packaged install `pUGSo8` at `ba1da0e00`
passes relocated native auth tests, including live session/JWT HTTP and MCP plus
secure cookies. Portable modules/workers, both SDK generators/strict consumers,
the complete HTTP/MCP startup/listener matrix and five signed managed callbacks
pass. Checkout CLI restored afterward. This snapshot includes the nine session/JWT
routes; the subsequent credential HTTP work and complete Better Auth preset are
separate gates.

Fresh packaged install `ac1c0T` validates the twenty-route auth checkpoint at
`54901dd98`, including relocated native auth tests and live HTTP/MCP, listener
startup configurations, portable modules/workers, CLI/native SDKs and all five
managed callbacks. Checkout CLI restored. Token-reset additions are validated
separately and are newer than this archive snapshot.

- [x] Match the configured server-only initial-password helper against 36 actual Better Auth API/storage outcomes, with strict credential identity, empty/missing passwords, session refresh/expiry and validation ordering. Verify one concurrent assignment wins in memory/restricted PostgreSQL and deterministic revocation during derivation prevents a write. Keep this helper server-only; remaining provider/org/JWT helper and CLI preset gates remain open.

Fresh archive `oLl8Px` at `953969c17` passes relocated auth tests (13 passed,
one explicit-PostgreSQL test ignored), live auth HTTP/MCP, all generated listener
startup configurations and five managed callbacks. Portable services/workers and
both SDK generators pass; checkout CLI restored. This snapshot includes 24 raw
auth routes and the server-only password helper, before organization additions.

- [x] Port organization creation, slug checks, listing, full graphs and active selection with actual configured default team/owner/keyed membership and session side effects. All 134 actual HTTP/storage/cookie comparisons pass; memory and restricted PostgreSQL check concurrent slug creation and HTTP/JWT organization claims beside MCP. Remaining organization/member/team/invitation/dynamic-role operations are still tracked by the open organization endpoint gate.

- [x] Add organization metadata/update/delete/permission routes with scoped stored dynamic grants, platform-only static-role denials, per-role composite authorization and actual deletion/session side effects. All 212 actual HTTP/storage/cookie comparisons pass, including validation/alias and malformed stored-grant errors. Memory and restricted PostgreSQL validate concurrent deletion and live granted HTTP mutations beside JWT/MCP lifecycle. Member/team/invitation/dynamic-role management and the complete generated preset remain open.

- [x] Port active member/role reads, role updates, removal and organization departure with exact configured owner safeguards, creator bypass ordering, scoped grants and team/session cleanup. All 146 actual HTTP/storage/cookie comparisons pass. Memory and restricted PostgreSQL validate a concurrent departure leaves one owner and preserves team counters; live HTTP membership operations run beside JWT/MCP lifecycle. Member listing/addition, team/invitation/dynamic-role management and generated preset gates remain open.

- [x] Add trusted server-only member creation with explicit-org/session-fallback behavior, exact role joining, duplicate/user/team/error ordering, configured membership limit and keyed team/counter effects. All 40 actual API/storage comparisons pass. Memory and restricted PostgreSQL verify duplicate and last-slot races; live HTTP rejects a public add-member route. Member listing, team/invitation/dynamic-role management and remaining helper/preset gates stay open.

Fresh archive `PHoCdV` at `a030eb345` passes relocated native auth tests (14 passed,
one explicit-PostgreSQL test ignored), auth HTTP/JWT/MCP and the complete generated
HTTP/MCP listener/startup matrix. Portable services/workers, both SDK generators
and five signed capability callbacks pass. This snapshot includes 38 raw routes
and server-only member/password helpers, before member listing. Checkout CLI is
rebuilt from the subsequent listing sources.

- [x] Port scoped member listing with schema/error ordering, public-user joins, filtered totals, stable en-US ICU sorting and ECMAScript Number/slice pagination. All 340 actual HTTP/storage/cookie comparisons pass, including unknown fields, malformed filter operators and orphan users. Memory/restricted PostgreSQL and live HTTP/JWT/MCP cover the new route. Real ORM collation/query conformance and remaining team/invitation/dynamic-role/preset gates stay open.

Fresh archive `YdLskR` at `05723e998` passes portable auth tests (14 passed, one
explicit-PostgreSQL test ignored), including ICU member listing, live auth HTTP/
JWT/MCP, all generated listener/startup configurations, portable workers/services,
both SDK generators and five signed capability callbacks. Checkout CLI is rebuilt
from the subsequent team sources.

- [x] Add organization/user team lists, team-member reads and active-team selection with exact scope, membership, permission, projection and cookie rules. All 318 actual HTTP/storage/cookie comparisons pass, including transient cookies, timestamp changes, retained memberships and malformed grants. Memory/restricted PostgreSQL and live HTTP/JWT/MCP validate team/session persistence. Team mutations, invitation/dynamic-role management, helpers and preset gates remain open.

- [x] Port team creation/update/removal with scoped dynamic grants, mutation-specific error ordering, field stripping, active/last-team protections and live pending-invitation cleanup. All 110 actual HTTP/storage/cookie comparisons pass. Memory/restricted PostgreSQL validate concurrent removal preserves the last team, cleanup and live HTTP mutations beside JWT/MCP. Team-member mutations, invitations, dynamic roles and remaining preset/provider gates remain open.

- [x] Add team-member addition/removal with required/coerced user IDs, scoped grants, shared keyed membership creation and exact counter/session effects. All 164 actual HTTP/storage/cookie comparisons and the 40-case server-only addition regression pass. Memory/restricted PostgreSQL verify duplicate-add and competing-remove races; live HTTP executes both routes beside JWT/MCP. Invitation/dynamic-role/provider/helper and generated preset gates remain open.
- [x] Match raw auth numeric parsing and serialized metadata against 114 actual HTTP/storage/cookie comparisons: overflow, IEEE-754 rounding, exponent formatting, negative zero, duplicate/index/insertion keys, literal serde marker keys, reserved metadata keys and update-only ISO date revival. Preserve update persistence before strict metadata response failures. All 233 framework and 101 blueprint workspace tests pass with unified JSON features; explicit restricted PostgreSQL auth, 96 JOSE cases and HTTP/organization/team/password regressions pass.
- [x] Audit literal serde marker keys across shared generic request, MCP and persistence deserializers under unified arbitrary-precision/raw-value features. The pinned transitive parser patch preserves literal objects across these boundaries; see [RUST-JSON-BOUNDARY-AUDIT.md](RUST-JSON-BOUNDARY-AUDIT.md). Unpaired surrogate/deep nesting and real ORM metadata conformance remain open before universal raw JSON parity.

Fresh archive `5DF6N8` at `f014f7547` passes portable auth tests, live auth
HTTP/JWT/MCP and the complete generated HTTP/MCP listener/startup matrix.
Portable services/workers, both SDK generators and five signed capability
callbacks pass. This snapshot contains 48 raw routes and both completed
server-only helpers. Checkout CLI rebuilt afterward (16.84 seconds).

- [x] Port invitation lookup, organization lists and received-user lists with configured verified-email/error ordering, recipient scope, expiry boundaries, joined inviter details and adapter limit-before-status filtering. All 258 actual HTTP/storage/cookie comparisons pass, including HEAD/POST/OPTIONS fallback behavior. Memory/restricted PostgreSQL and live HTTP/JWT/MCP cover all three read routes. Invitation mutations, dynamic roles and remaining helper/provider/preset gates stay open.

- [x] Port invitation rejection/cancellation with verified-recipient and scoped permission ordering, exact error messages, expiry/status distinctions and retained membership/session state. All 318 actual HTTP/storage/cookie comparisons and 258 invitation-read regressions pass. Memory/restricted PostgreSQL cover repeated/concurrent cancellation; live HTTP runs both routes beside JWT/MCP. Invitation creation/acceptance and remaining dynamic-role/provider/helper/preset gates stay open.

Fresh archive `v8nGhW` at `51973fa19` passes relocated auth tests, live HTTP/JWT/
MCP and the full generated service HTTP/MCP listener/startup matrix. Portable
services/workers, both SDK generators and five signed callbacks pass. The
snapshot contains 53 raw routes, including JSON metadata and invitation
reads/rejection/cancellation. Checkout CLI rebuilt from subsequent creation
sources afterward.

- [x] Port invitation creation/resend with exact scoped authorization, verbatim role/creator rules, duplicate-member/invitation ordering, team validation, 48-hour expiry and bounded pending-invitation admission. All 196 actual HTTP/storage/cookie comparisons and invitation/member regressions pass. Resend preserves stored role/teams and precedes team/limit checks. Memory/restricted PostgreSQL verify concurrent last-slot admission and retained team/member state; live HTTP runs creation/resend beside JWT/MCP. Acceptance, dynamic roles and remaining provider/helper/ORM/callback/preset gates stay open.

- [x] Port invitation acceptance with recipient/verification/expiry/limit ordering, duplicate-member and verbatim-role behavior, keyed team enrollment, active scope and exact cookie emission. All 186 actual HTTP/storage/cookie comparisons and invitation/team/organization regressions pass. Memory/restricted PostgreSQL validate competing acceptance, last-slot membership admission and complete rollback after a later missing team. Live HTTP executes no-team/single-team transient acceptance beside JWT/MCP. Configured invitation lifecycle is complete; dynamic-role/provider/helper/ORM/callback/full-preset gates stay open.

- [x] Port dynamic role listing/lookup with explicit ac:read grants, raw membership, nullish organization fallback, exact case-sensitive name/ID selectors and union validation. All 272 actual HTTP/storage/cookie comparisons pass, including first-100 listing, later direct reads, malformed later JSON and literal serde keys. Memory/restricted PostgreSQL and live HTTP/JWT/MCP verify parsed permission output retains stored strings. Role mutations and remaining provider/helper/ORM/callback/full-preset gates stay open.

- [x] Port dynamic role creation with nullish scope, lowercase/predefined-name ordering, raw membership, ac:create and per-action grants, platform-only resource validation and ordered missingPermissions errors. All 212 actual HTTP/storage/cookie comparisons and role-read/permission regressions pass. Memory/restricted PostgreSQL verify atomic competing duplicate creation and stored permission JSON; live HTTP executes creation beside JWT/MCP. Blueprint workspace checks cover structured error details with explicit HTTP field selection. Role update/deletion and remaining provider/helper/ORM/callback/full-preset gates stay open.

- [x] Port dynamic role update/deletion with exact schema/selector ordering, ac:update/ac:delete and per-action grants, parsed prior permissions, rename/duplicate handling, first-100 assignment filtering and predefined-name versus ID safeguards. All 332 update and 324 deletion actual HTTP/storage/cookie comparisons pass, including duplicate rows, malformed later JSON, zero/overflow permissions and adapter timestamp projection. Memory/restricted PostgreSQL verify atomic deletion, blocked assigned roles, retained member role strings and duplicate updates/deletes; live HTTP executes both routes beside JWT/MCP. Configured dynamic-role lifecycle is complete; provider/helper/ORM/callback/shared-parser/full-preset gates stay open.

Fresh archive `bnXBOp` at `10349eea0` passes portable Better Auth tests and live
HTTP/JWT/MCP, the complete generated HTTP/MCP listener/startup matrix, portable
services/all four worker backends, CLI/native SDK targets and all five capability
callbacks. This archive includes sixty raw auth routes and the two completed
server-only helpers, including the configured invitation/dynamic-role lifecycle.
Remaining provider/helper/ORM/callback/shared-parser/full-preset gates stay open.

- [x] Add configured server-only JWT verification for persisted Ed25519 keys, token algorithm aliases, issuer/audience/claim behavior, first-100 retired-key selection and exact JOSE/WebCrypto metadata/encoding rules. All 159 actual API and 144 key-store comparisons pass. Memory/restricted PostgreSQL verify no key minting, retired public-key verification and independence from corrupt private material; live HTTP confirms both JWT helper paths remain absent beside JWT/MCP. Signing and additional-algorithm provisioning/interop remain open with provider/ORM/callback/shared-parser/full-preset gates.

- [x] Add the configured EdDSA JSON server-only signing helper with exact schema/default-duration/claim ordering, persistent minting before claim failures, issuer/audience overrides and numeric key rotation. All 129 actual API and 134 key-state comparisons pass with independently verified signatures. Memory/restricted PostgreSQL exercise signing/verifying beside key lifecycle and live HTTP/JWT/MCP. All four configured helper entrypoints are implemented; additional algorithm/key provisioning/interop, typed date/function options and custom callbacks remain open alongside provider/ORM/shared-parser/full-preset gates.

- [x] Merge main through b5852a5a1 and extend its OSV dependency audit to Cargo lockfile registry releases. Parse legacy/v3/v4 records, exclude workspace/path packages and generated targets/bundled sources, preserve RustSec severity/fixed-version reporting and offline cache behavior, and report malformed/future locks plus unsupported Git/private-registry sources explicitly. Mixed-language root manifests require matching ecosystem locks. All 1,016 CLI tests pass, including 15 dependency-audit tests. Git commit-based advisory queries remain unsupported and explicitly reported.

- [x] Add persistent Ed25519-alias, ES256/P-256 and ES384/P-384 JWT key provisioning, restoration, public JWK publication, session/server signing and server verification. All 151 actual signing API/165 key-state and 213 actual verification API/182 key-store comparisons pass, including curve/import/algorithm failures. Memory/restricted PostgreSQL validate concurrent single-key initialization, restart/reuse, rotation and retired-key verification. All 17 native tests pass (one explicit PostgreSQL test separately passes), including live HTTP/JWT/MCP; strict all-feature clippy, checkout CLI and 709 embedded sources pass. RSA/ES512 and broader typed/provider/callback/ORM/preset gates remain open.

- [x] Add ES512/P-521 PKCS8 generation/restoration, public JWK coordinates, randomized SHA-512 signing and strict signature verification. All 159 actual signing API/176 key-state and 240 actual verification API/201 key-store comparisons pass. Shared memory/restricted PostgreSQL profile lifecycle covers P-521 concurrency, restart/session reuse, rotation and retired verification. Strict default/all-feature clippy, rebuilt CLI and 709 embedded sources pass. RSA, typed options and remaining provider/ORM/callback/full-preset work remain open.

- [x] Add RS256/384/512 and PS256/384/512 persistent key profiles, native ring signatures and public-key verification with matching hash/algorithm/PSS salt enforcement. Actual comparisons pass 189 signing API/216 key-state and 393 verification API/309 key-store operations, including 2048/3072-bit provisioning and invalid modulus options. Memory/restricted PostgreSQL cover atomic initialization, restart/reuse, rotation and retired verification; live HTTP/JWT/MCP and 27 prior keyring comparisons pass. Move prime generation off async executor threads and prefer the default EdDSA key during session issuance. Five final keyring tests, strict all-feature clippy, rebuilt CLI and 711 embedded sources pass. RSA provisioning above 4096 bits, typed options/custom adapters, providers/ORM/shared-parser/full-preset remain open.

Fresh archive `n6okrp` at `74213970a` passes 18 portable auth tests (one explicit
PostgreSQL test ignored), including live HTTP/JWT/MCP and all asymmetric signing
profiles. The complete generated HTTP/MCP startup/listener matrix, portable
services/all four worker backends, CLI/native SDK targets and all five signed
capability callbacks pass. This snapshot includes sixty raw auth routes, four
configured server helper entrypoints and Ed25519/ECDSA/RSA/PSS provisioning.
Larger RSA/typed options, Google OAuth, ORM/callback/shared-parser and full-preset
gates remain open.

- [x] Add configured database OAuth state primitives with random 32-character correlation and 128-character PKCE verifier, signed five-minute state cookies, ten-minute persisted expiry, secure names and atomic one-time consumption. All 66 actual TypeScript state/cookie and 66 storage comparisons pass through real endpoint context/Better Call/memory adapter, including global cleanup, malformed/legacy state and link coercion. Memory/restricted PostgreSQL verify races, retained state on failed correlation and exact expiry boundaries. Strict all-feature clippy, rebuilt CLI and 715 embedded sources pass. Google authorization/callback HTTP wiring and provider token/account lifecycle remain open; raw-route count stays sixty.

- [x] Add configured Google authorization URL primitives with S256 PKCE, default/appended scopes, configured callback URI, login hints, reserved-key protection and exact URLSearchParams overwrite/encoding/JavaScript numeric-key ordering. All 37 actual Google provider comparisons pass, including missing credentials/verifier errors. Strict all-feature clippy, rebuilt CLI and 717 embedded sources pass; CI includes the provider fixture. This adds no HTTP route; social-login validation, ID-token user/account/session integration, code exchange/callback and token lifecycle remain open.

- [x] Add Google ID-token verification primitives with 147 actual-provider comparisons: RS256, trusted supplied JWKS, exact issuer/audience/nonce and numeric claim boundaries, all-selected-key import failures, duplicate key IDs and public/private WebCrypto import behavior. Strict all-feature/all-target clippy, rebuilt CLI and 719 embedded sources pass; CI runs the fixture. HTTP certificate fetching and persistent Google user/account/session integration remain open.

- [x] Port configured Google social sign-in HTTP authorization/state and direct ID-token persistence with 165 actual HTTP/storage comparisons, nested request schemas, protected state fields, first-selected-key verifier behavior, bounded trusted certificate transport, profile/token reuse and safe verified-email linking. Native concurrent signup/orphan ownership and encrypted restricted PostgreSQL token/tenant checks pass. Authorization-code callback, explicit linking and remaining token/account routes remain open.

- [x] Port Google authorization-code/PKCE callbacks with 226 actual HTTP/storage/wire comparisons: trusted bounded token transport with redirects/retries disabled, one-time state and exact expiry, POST normalization, callback/new-user/error redirects, access/refresh/ID-token persistence and explicit callback account linking/scope merge. Malformed provider-token failures match upstream empty-500/cookie-header behavior. Restricted encrypted PostgreSQL linking/token/tenant checks pass. Strict default/all-feature/all-target clippy, 21 native auth tests, rebuilt CLI and 725 embedded sources pass. Link initiation and access/refresh/account-info HTTP APIs, remaining raw routes, full ORM/application hooks and CLI preset remain open; raw-route count is 62/70.

- [x] Repeat fresh relocated archive validation after Google HTTP/state/code callbacks (`VwxVQI`, snapshot `f1a966960`). Portable services/all four worker backends, CLI/native SDKs, 21 portable auth tests, complete generated HTTP/MCP listener/endpoint/port/startup-rejection/shutdown matrix, MCP initialize/discover/invoke/Origin checks and all five signed capability callbacks pass. Checkout CLI restored after archive installation. Snapshot includes 62 raw auth routes; later link initiation is separate.

- [x] Port public Google `/link-social` initiation/direct linking, with owned server state, same-owner token updates, verified-email/matching-email policy and cross-owner conflict rejection. Combined social/link/session-field HTTP comparisons pass 289 checks; real browser link-to-callback path passes 236 response/storage/wire checks. Direct linking passes memory and encrypted restricted PostgreSQL. Port configured `/update-session` record/schema/protected-field/no-editable-fields behavior. Access/refresh/account-info and OpenAPI/error APIs remain open; raw-route count is 64/70.

- [x] Port Google access-token, refresh-token and account-info HTTP APIs: 157 actual response/storage/wire comparisons, live token rotation and unlink-during-refresh protection, 22 native tests, restricted encrypted PostgreSQL, 161 HTTP/289 social regressions, strict clippy and 728 packaged assets pass. Raw routes: 67/70; error/OpenAPI/reference and broader parity gates remain open.

- [x] Complete the configured error HTML/production redirect, OpenAPI schema and disabled-reference routes: all 70 raw routes implemented; 27 development + 27 production full-response comparisons, 22 native tests and strict clippy pass. The schema is the fixed preset declarative contract, compared with the actual TS generator. Application surfacing/compliance/retention, ORM conformance and CLI preset remain open.

- [x] Revalidate archive-installed all-70-raw-route snapshot `4f0540ba6` (`IqcM1T`): 22 portable auth tests, all services/four workers, CLI/native SDKs, complete generated HTTP/MCP startup/listener/shutdown matrix and all five actual-TypeScript-signed callbacks pass. Checkout CLI restored.

- [x] Port configured application role/permission surfacing service: 37 actual TS result comparisons, shared memory/restricted PostgreSQL expiry/scope/raw-permission checks, ciphertext inspection and tenant isolation pass; strict clippy, restored CLI and 734 packaged assets pass. HTTP/MCP application routing and CLI preset remain open.

- [x] Add real MikroORM 7.2.3/adapter 0.5.9/PostgreSQL conformance and repair the TS compliant-field adapter boundary. All 24 auth/encryption/audit/tenant/transaction/restart/native-interop checks pass; strict isolated adapter typing, 161 HTTP/157 token/27 public/37 surfacing regressions and 734 packaged assets pass. Complete ORM/hooks/compliance/retention and CLI preset remain open.

- [x] Port configured classified export/erasure and seven-calendar-year retention services: 14 real encrypted-ORM/native compliance comparisons (including partial reads), 41 calendar/timezone/ORM/native retention comparisons, shared memory/restricted PostgreSQL ownership/atomic rollback/tenant/ciphertext checks, all 26 native tests, strict clippy, rebuilt CLI and 739 embedded assets pass. HTTP/MCP application routing, custom callback helpers and CLI preset remain open.

- [x] Mount all five configured Better Auth application contracts over HTTP/MCP with HMAC/JWKS and current-store system-role resolution. Five actual controller contracts/nine response schemas match; live TCP initialization/discovery/signed tools/role revocation/opaque exports/erasure/shutdown, all 27 native tests, strict clippy and 742 packaged assets pass. Native CLI preset/custom callbacks/full ORM hooks remain open.

- [x] Verify fresh archive installation `XXfFfM` at `f288c81c4`: generated Better Auth service/SDK, all 27 packaged auth tests, complete shared/separate/disabled/custom HTTP/MCP startup matrix and all five actual-signed capability callbacks pass alongside every portable service/four worker backends. Restore checkout CLI afterward.

- [x] Add typed additional Scalar documentation sources and mount BetterAuth's full raw API beside application contracts. Fix the actual TS preset's double base-path docs bug and escape Scalar/Swagger titles/configuration. Twelve actual Scalar/TS generator/registration comparisons, live native and fresh generated PostgreSQL docs/HTTP/MCP checks, all 27 auth/38 framework tests, strict clippy and 751 packaged files pass. Nondefault auth base paths and complete ORM/hooks/client integration remain open.

- [x] Support configured fixed non-root BetterAuth namespaces throughout native/generated HTTP routes, helpers, docs, Google/reset callbacks, canonical-origin JWTs and default JWKS discovery. Repair actual TS helper/provider path configuration. All 343 actual comparisons, three live application namespaces, fresh generated PostgreSQL custom/restart/self-JWKS checks, 28 native/1,016 CLI tests, strict clippy, 161 HTTP/40 callbacks/12 Scalar regressions and 751 packaged sources pass. Root/dynamic/encoded namespaces remain open.

- [x] Support root-mounted BetterAuth without swallowing application/docs/MCP routes. Framework declared-namespace merges preserve nesting and reject contract/docs/MCP collisions; actual TS root catch-all uses known auth namespaces. All 450 actual comparisons/630 raw dispatch probes, five native application configurations, fresh generated PostgreSQL root/self-JWKS/startup/lifecycle, 28 native/40 framework tests, strict clippy and 751 packaged sources pass.

- [x] Revalidate fresh archive installation `EklWnG` at `f20252656`: all portable services/four workers, both SDK targets, all 28 packaged auth tests, full HTTP/MCP startup/listener/shutdown matrix and all five actual-signed native capability callbacks pass. Restore checkout CLI afterward.

- [x] Validate actual shared and generated TypeScript BetterAuth frontend clients against native PostgreSQL over default/custom/root/URL-override namespaces. Add `authBasePath` to both CLI generation paths; 80 live client requests cover additional fields, sessions, profile update, independent JWT/JWKS and signout/signin. Fresh standalone dependency setup/TypeScript generation, HTTP/MCP/self-JWKS/SDK/persistence/lifecycle, all 1,016 CLI tests and 751 packaged source checks pass. Unified mixed SDK and broader custom client/plugins/hooks remain open.


Mixed HTTP SDK checkpoint (2026-10-12): `sdk generate --from contract` now imports
TypeScript OpenAPI alongside native contracts; `--from openapi` imports both
languages' OpenAPI. Both target languages include every selected service, while
libraries and TypeScript observability services stay excluded. Versioned source
tracking prevents lifecycle regeneration from silently removing previously imported
clients when an export disappears. Artifact and source-state paths are contained
inside the application. Check mode does not write files. Watch follows the manifest,
custom paths and TypeScript exports in contract mode, retains output on errors and
recovers on subsequent changes. All 1,023 CLI tests and 752 embedded-source checks
pass. `rust_mixed_sdk_runtime.sh` runs real CLI export, relocation, lifecycle refresh,
watch loss/recovery, strict TypeScript compilation, generated Rust SDK Clippy and
both live SDK targets against native and tsx HTTP services. Native HTTP/MCP discovery,
health invocation, listener modes, startup rejection and graceful shutdown pass.
The TypeScript server/export hook is fixture-owned; mixed TypeScript scaffolding,
project lifecycle routing and dev orchestration remain open.


Mixed lifecycle checkpoint (2026-10-12): the previous app-wide Rust dispatch
removed a TypeScript service registration during `sync service` and reported
success. Service/worker/library sync and deletion now resolve the selected
project's language, router deletion resolves its owning project, and native sync
rejects non-native projects before cleanup. Application language, SDK targets,
Cargo workspace and artifact paths now belong to the shared manifest model, so
TypeScript project views and generated module conversions retain them. Service
deletion also preserves the configured application name. TypeScript router
removal handles generated async startup functions, tolerates unrelated zero-arg
calls and validates rewrites before removing source files. The actual generated
mixed fixture verifies all three project kinds, rename roundtrips, router failure
preservation and every shared metadata field. All 1,025 CLI tests and existing
native sync/delete runtime regressions pass. The fixture supplies a Cargo workspace
for its TypeScript-first app; automatic hybrid scaffolding and app-wide sync are
still open.


TypeScript-first native attachment checkpoint (2026-10-12): native init now
creates the root Cargo workspace for existing Node/Bun applications and marks
them mixed while preserving TypeScript runtime/tooling/source and existing Cargo
members/comments/settings. Missing inherited package settings use application
defaults; incompatible workspace editions, conflicting aliases and escaping
output paths reject before writes. Dry-run is read-only. Native services and
modules use `Dockerfile.rust` plus a dedicated ignore file, preserving the existing
TypeScript image; native workers inherit the same build selection. Actual Node
and Bun applications each accept service/Redis-worker/library/CAC generation,
retain their TypeScript trees byte-for-byte, survive relocation and compile every
native crate. Both native services pass the full HTTP/MCP listener matrix. The
mixed native image builds all binaries and passes health/OpenAPI, MCP discovery
and tool invocation, Origin rejection and graceful shutdown. All 1,030 CLI tests,
752 source-package checks and the mixed lifecycle regression pass. CI runs the
attachment and container fixture; the user guide now documents the migration.
Rust-first TypeScript scaffolding, custom workspace locations, application-wide
sync and development orchestration remain separate work.


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

### Explicit persistence compliance metadata (2026-10-12)

`forklaunch-core-macros` supplies the core-reexported `ComplianceMetadata` derive.
Named structs declare entity names and each field's none/PII/PHI/PCI classification,
storage name and nullability. Unannotated fields remain unclassified and non-nullable.
Anonymization eligibility follows the TypeScript rule: classified and nullable.
Rust Option types, encryption settings and serde renames cannot silently authorize
field erasure. Application adapters must check storage metadata and tenant scope;
generic retention enforcement and compliance data-service integration remain open.

Core/macro unit and integration tests and three documentation examples pass,
including compile-fail coverage. Strict all-target clippy passes. The derive handles
lifetimes/type/const generics, where clauses, raw identifiers and renamed dependencies;
unknown/duplicate attributes and duplicate storage names fail compilation.

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


Server-only backup-code viewing checkpoint (2026-10-14): opaque zeroizing trusted
domain result, read-only built-in/custom decode, current-generation/lease/deadline
rechecks and explicit caller authorization.26backup/14actual and restrictedPG pass;
strict full/minimal Clippy green. No public HTTP/MCP route. See inventory/security
receipt. Latest CLI1122 tests pass, production CLI builds24.07s (four existing
warnings), and1285 embedded source/template package entries pass; working-source
checks include current drafts, not an immutable checkpoint.660/682 (96.8%),22open.


Aggregate bound billing/Portal checkpoint (2026-10-14): one explicit selector
chooses Checkout/Portal independently, preserving legacy/Checkout-only profiles.
Portal create and IDs-only inspection pass restrictedPG/HTTP/MCP and4generated
SDKcalls;32config/artifact cases and actual base/Stripe main compile,6units,
Date/default66routes28schemas and strictClippy pass. Root CI wrapper passes3.49s.
Cached Portal reread and other families remain open; see inventory.660/682 (96.8%).


SDK own-property checkpoint (2026-10-14): bounded private AJV compilation view
repairs __proto__ without changing public schema/reference targets; draft04 socket
resource reset uses id. Dual-use default/const/enum targets requiring repair reject
explicitly, preserving injected defaults/equality/options.128cases+controls,
18reference/13body and strictsource/socket types pass.660/682 (96.8%) unchanged.


OTP background checkpoint (2026-10-14): trusted optional app-owned scheduling
preserves defaultawait, committedproof and sanitized error behavior, without
automatic runtime/spawn.3focused/36affected and5actual checks plusstrictClippy
pass. Other background callsites remain open;660/682 (96.8%),22open unchanged.


Tuple consumer checkpoint (2026-10-14): explicit response dialect context chooses
owned2020 only when declared, preserving legacy draft7/unmarked3.1 and injected
capabilities.17realHTTP positional date/binary/ref conversions and prior SDK
regressions pass. Public inventory619/1694 refreshed; strict12roots/619exports
pass(actualTS7.0.2). Emitter/Rust tuplemodels remain separate;660/682 (96.8%).


Explicit2020 tuple emission checkpoint (2026-10-14): additive openapi2020 emits
closed/rest/nested tuple positions under explicitdialect, preserving legacy/TypeBox.
1620actual/AJV comparisons and20realHTTP SDKcases plusprior regressions pass;
619/1696 inventory updated and strict12roots/619exports pass. Rootcallbacks0;
nested callbacks and Rust typedtuple models remain open.660/682 (96.8%).


Stored Portal checkpoint (2026-10-14): provenance/currentCustomer/alias/receipt
checks and original5minTTL gate direct local reread, no providerrefresh. PGmutation/
auditexpiry andHTTP/MCP410/noURL +6SDKcalls pass;32artifacts/generatedmaincompile
andstrictClippy pass. observedAt completionclock differs from preceding finalSELECT
linearization. Otherfamilies remainopen;660/682 (96.8%),22open unchanged.


Password-reset checkpoint (2026-10-14): optional trusted delivery creates fixed1h/
24base62 compatibleproof, same missinguser response, boundedowned context and
commit-beforeprepare. Reusable app-owned scheduler preservesOTP aliases/precedence.
9actual/focusedHTTP/domain/restrictedPG andstrictClippy pass. GETchecks/redirects,
POSTconsumes; broaderemail/options remain open.660/682 (96.8%),22open unchanged.


Fixed Rust tuple checkpoint (2026-10-14): named exactsequence models/customSerde
coverempty through128positions, requiredrecursive edges/defaultsuppression and
legacy/explicit2020 constraints.79modelcases/native46/fullCLI1125/strictClippy
and1294embeddedfiles pass. Implicitdraft7 prefix annotation no longer narrows
model incorrectly. Variable/rest shapes remainopen;660/682 (96.8%),22open.


Bound Plan read checkpoint (2026-10-14): sealedcompletedprovenance+exactcurrent
DTO/alias/profile, freshPlan/Product terms and finaldeadline preserve localUUID
with Product-only enrichment.65actualTS/newPG5.54s/creationPG12.85s andstrict
Clippy pass. No automaticroute/publisher capability;660/682 (96.8%),22open.


Email delivery checkpoint (2026-10-14): trusted explicit endpoint awaits callback
independently ofbackgroundscheduler; compatible1h addressproof, anonymousfloor/
privacy and authenticatederror behavior pass11actual/3focused/PG/liveHTTPMCP/
strictClippy checks. Signup/signin/hooks remainopen;660/682 (96.8%),22open.


Variable Rust tuple checkpoint (2026-10-14): typedprefix/rest andnestedOption
presence preserve contiguousJSON values, boundschecked beforewriter.892modelcases,
fixed79/native46/SDKRust5/strictClippy/fullCLI1125 andgeneratedRust-Dwarnings pass.
RecursiveTS rest spelling fixed withrealcompilerproof.660/682 (96.8%),22open.


Aggregate Plan-read checkpoint (2026-10-14): explicitplanRead selects provenance
GET with no publisher connection; mixedcreation retains separatepublisher. Local
startupgrant/configvalidation,46artifactcases/actualgeneratedmain andHTTP/MCP/
2SDKcalls pass; rootCIwrapper2.46s, strictClippy green. Legacy66routes unchanged
andnotclaimedread-only.660/682 (96.8%),22open unchanged.


Pure input export checkpoint (2026-10-14): opt-in supported-node traversal avoids
named Zod callbacks and computed defaults, rejects coercion/unsupported nodes,
preserves nullable false schemas and enforces finite budgets.577 actual/AJV and
138 controls plus existing tuple/SDK/union/Never regressions pass. Default API
unchanged; NativeEnum/getter sandbox remain outside profile. Public inventory
620/1698 refreshed. Broad checklist660/682(96.8%),22open unchanged.


### Trusted signup/signin verification policy (2026-10-14)

Trusted required signup/signin policy now has actual13/focusedHTTP/restrictedPG/fullIAM198 evidence; callback transaction-visibility boundary remains explicit. Checklist660/682(96.8%),22open unchanged.


### Bound completed Subscription read (2026-10-14)

Library completed Subscription read now has actualTS66/restrictedPG/provider/provenance race evidence; aggregate route and lifecycle synchronization remain separate. Checklist660/682(96.8%),22open unchanged.


### Generated Rust tuple internal names (2026-10-14)

Reserved generated internal names prevent recursive model collisions;920 variable/79 fixed/47 codegen/1126 CLI and strict generated compilation pass. Checklist660/682(96.8%),22open unchanged.


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

### 2026-10-14 — Reserved Rust model names

Reserved Self/self/SELF model names now normalize idempotently to Self_. Shared
and CLI graph seeds preserve existing TypeScript SelfSchemaRef1/SelfSchema1Ref1
names. Generated strict Rust and TypeScript fixture: 144 cases across 30 models;
variable tuples: 920; codegen: 49; full CLI: 1,128; strict Clippy passed.
Frozen four-path manifest /private/tmp/fl-reserved-type-source.json verified.
Builtin/prelude shadowing remains a separate audit. Checklist: 660/682 (96.8%),
22 broader gates open.

### 2026-10-14 — Aggregate PaymentLink read surface

Explicit paymentLinkRead trusted-price selection now exposes
GET /bound-payment-links/:publicId through the shared authenticated HTTP/MCP
registry and generated TypeScript/Rust SDKs. Sole read selection requires no
publisher; creation selections still require their own publisher. Startup checks
direct client configuration and local SELECT/audit INSERT privileges without
provider I/O. Completed actor-owned creation provenance is required. Current
provider fields may show active=false while local amount/status stay unchanged;
this is not payment, entitlement or lifecycle synchronization. Existing 66 legacy
routes remain available; selection does not make the entire service read-only.

Restricted PostgreSQL/HTTP/MCP/two actual SDK calls passed (2.39s), config units9,
rendered artifacts78 plus base/Stripe templates, strict all-feature/all-target
Clippy and feature-off Clippy passed. Independent root CI wrapper passed2.65s
and releases Cargo's lock before nested SDK compilation. Frozen nine-path
manifest /private/tmp/fl-payment-link-read-route-final.json verified; evidence
/private/tmp/fl-payment-link-route-{live,units,artifacts,clippy,default-clippy,root-wrapper}.log.
Checklist remains660/682(96.8%),22 broad gates open.

### 2026-10-14 — Awaited email-verification hooks

Independent trusted before/after builders now cover ordinary verification and
the existing change-email verification after-hook branch. Hooks always run
outside database transactions and bypass background dispatch. Before failure
leaves the user unverified; after failure preserves committed verification and
session changes, returns sanitized empty500 and suppresses cookies/redirects.
Already-verified replay, change-email confirmation and legacy branches retain
their observed hook skips. Writer ID/email rechecks reject stale callback changes;
this is a stronger stale-callback boundary than the upstream flow. Native
cancellation preserves the corresponding pre/post-commit state, without claiming
JavaScript disconnect equivalence. Public-user context validates borrowed fields
before cloning, owns bounded zeroized JSON, exposes no request/credentials or
Debug/Serde; no-hook paths skip projection. Blocking hooks/panics are not caught.

Actual Better Auth oracle12 passed; native2 covers14 branches and before/after
cancellation; affected email5 passed3.59s; restricted PostgreSQL1 passed132.89s;
strict all-feature/all-target18.09s and minimal2.95s passed. Frozen eight-path
manifest /private/tmp/fl-email-observer-freeze.json verified (SHA256
42e07c6aa2f787a9637249e1ce58461fef21f779265e827506d5a141c02b7f5c).
Evidence: /private/tmp/fl-email-verification-hooks-actual.log and
/private/tmp/fl-email-observer-{native,regression,pg,clippy,minimal-clippy}.log.
Auto-signin after verification remains separate; checklist660/682(96.8%),22open.

### 2026-10-14 — PaymentLink reservation admission before cloning

The public reservation method now checks borrowed intent data before recursive
cloning/canonicalization: depth32,8192 values and exact65536 serialized UTF-8 bytes
for the full envelope. A bounded writer measures escaped keys/values and UTF-8
without constructing the envelope Value. Ordinary valid canonical hashes and
commit/recovery handling are unchanged; extreme depth/width are explicit new
admission limits. Boundary65536 accepted/65537 rejected, large plain/Unicode
values preserved, hostile nesting/width/keys/escaped values rejected.
Focused2 units, full direct creation/concurrency/lost-COMMIT PostgreSQL matrix
with zero-write rejection checks17.77s and strict base/Stripe Clippy16.97s pass.
Owned roles0. Frozen3 manifest /private/tmp/fl-payment-link-admission-final.json
verified SHA256156f2713429e947702fa953e5adae5310ebee779d53615beace6014f80131565.
Logs /private/tmp/fl-payment-link-admission-{units,pg,clippy}.log. Existing CI
creation matrix covers the added integration cases; no dependencies/migrations.
Broader billing gates remain open;660/682(96.8%),22open.

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

### 2026-10-15 — Optional sign-in after email verification

AuthHttp.with_auto_sign_in_after_verification enables ordinary newly verified
proof login after the awaited after-hook; defaultfalse. Current same-user session
is reused; anonymous/invalid/foreign/expired sessions create a target session.
Already-verified replay and change-email flows retain their independent behavior.
Redirects carry the login cookie; signed transient preference suppresses Max-Age
without shortening the configured persisted lifetime. Final writer rechecks
current target ID/email/verified state before granting a session; this stronger
stale-hook boundary is explicit. No transaction spans the application callback.

Expired cleanup commits independently before session insertion. Unexpected
insert/commit failure retains verified state/cleanup, returns sanitized empty500
and suppresses pending cookies. Native Result<()> storage cannot represent
upstream createSession=null/APIError; that different clear-cookie/error behavior
is documented as unsupported rather than mapped onto all storage failures.
Actual15, native2(17 lifecycle branches+cancellation), affected email7(3.66s),
restricted PostgreSQL1(137.88s) and strict full18.74s/minimal4.26s pass; owned
roles/records/audits0. Eight-path /private/tmp/fl-verification-auto-freeze.json
verified SHA2564ca4ee6e6413cbe17a2b130ca28282cdd189c6a74b1305e909ed2a09411ba5f2.
Logs /private/tmp/fl-verification-auto-{actual,native,regression,pg,clippy,minimal-clippy}.log.
No dependencies/locks; checklist660/682(96.8%),22broad gates remain open.

### 2026-10-15 — Completed immutable workspace cleanup

After committed runtime receipt9b84f9a33, removed the exact owned6f77 checkpoint
and generated archive roots (212646681 logical bytes), then 182 exclusive
nonexecutable cache files in 80 groups (417385757 logical bytes).
Total 630032438 logical bytes; no APFS physical-space claim. Guards verified
exact owned roots, no symlink/open-file/active-process use, current-source cache
exclusion and retained archive SHA256 before/after. Executables, live workspaces
and active agent caches preserved. Evidence /private/tmp/fl-oct14-payment-link-
cleanup.log and /private/tmp/fl-oct14-payment-link-cache-cleanup.{json,log}.

### 2026-10-15 — Bounded completed-PaymentLink listing

list_bound_created_payment_links validates the whole explicit UUID selection
before provider I/O, retains original provenance snapshots for final reobservation,
preserves order/duplicates and filters only actual boolean provider active=true.
No account-wide provider scan, cache-derived authority, writes or lifecycle
synchronization. The trusted limit defaults100/max1000; nonempty/non-nil IDs,
8MiB evidence/result budgets and one shared5s provider context bound the request.
No-ID/empty enumeration is outside this supported subset. Per-item reads are not
an atomic multi-record/provider snapshot.

Actual TS37 comparisons (13new list cases), budget unit1, restricted PostgreSQL
list6.27s (including150 records, missing/foreign/profile denial, ordered duplicates,
inactive filtering, alias race and expiry), historical getter5.55s and strict
base/Stripe Clippy9.16s pass. The older deep-intent fixture now seeds historical
encrypted data via owned migration-owner SQL with atomic trigger restoration,
explicitly sorted canonical hashes and a successful positive control; new deep
reservations remain rejected. Frozen7 /private/tmp/fl-payment-link-list-final.json
verified SHA25688e805997d17acdefd3135403b32e22707980f6c3dc2ffd9dfa13705d35adc5e.
Logs /private/tmp/fl-payment-link-list-{ts,unit,pg-final,clippy-final}.log.
Owned fixture roles0. HTTP/MCP list wiring remains next;660/682(96.8%),22open.

### 2026-10-15 — PaymentLink list HTTP/MCP/SDK surface

Existing explicit paymentLinkRead selection now adds GET /bound-payment-links
with required typed array-of-UUID ids and trusted optional maxRecords(default100,
max1000). Runtime uses one JSON-encoded query entry, matching existing generated
TS client/native list convention; TS callers pass an array, Rust callers serialize
into the current string query map. Repeated query keys/unknown keys are rejected.
This is not a claim of repeated-array TS-controller wire equivalence. Metadata
reflects the selected limit. Existing path-only HMAC convention is unchanged.

HTTP/MCP plus two actual TS/two actual Rust getter/list calls passed2.81s,
including pre-provider query/auth denials. Artifact88 plus generated base/Stripe
binaries, units9, strict all-feature/all-target Clippy3.26s and feature-off7.08s
pass. Existing66legacy routes remain unchanged; no publisher/write authority.
Frozen7 /private/tmp/fl-payment-link-list-route-final.json verified SHA256
bdeb6560d5d327acf6c9a96b1059ef7c7c4f207c654ca338cc0e97ba7c02c9eb.
Logs /private/tmp/fl-payment-link-list-route-{live,artifacts,units-final,clippy-final,default}.log.
Existing CI direct-executable payment_link_read wrapper runs this expanded test
without holding Cargo's lock across SDK compilation; no extra wrapper needed.
Checklist660/682(96.8%),22broad gates remain open.

- 2026-10-15: Bounded credential policy and reset/verification TTLs validated: actual17, focused3, full IAM217, restricted PostgreSQL and strict full/minimal Clippy pass. Fresh-clock timing and broader auth remain open; 660/682 unchanged.

- 2026-10-15: Raw Zod date assertions now pass 70,000 actual/AJV plus 70,000 native comparisons, 16 HTTP/64 socket checks, strict Clippy and configured TypeScript/inventory. Regex and remaining formats stay open; 660/682 unchanged.

- 2026-10-15: Subscription creation HTTP/MCP/both SDKs pass real restricted PostgreSQL role separation, 12 unit tests, 136 rendered artifact cases and strict full/minimal Clippy. Explicit service tenant authority and separate publisher required; broader billing remains open, 660/682 unchanged.

- 2026-10-15: Raw duration assertions validated with34,626 actual/AJV and34,626 native observations,17 HTTP/68 socket checks, strict Clippy and configured types/inventory. Other string formats and regex remain open;660/682 unchanged.

- 2026-10-15: Raw time precision assertions pass 108,336 actual/AJV and 108,336 native observations, 19 HTTP/76 socket checks, strict Clippy and configured types/inventory. Datetime/regex and broader constraints remain open; 660/682 unchanged.

- 2026-10-15: Raw datetime precision/local/offset options pass 86,592 actual/AJV and 86,592 native observations, 20 HTTP/80 socket checks, strict Clippy and configured types/inventory. Remaining format/regex constraints stay open; 660/682 unchanged.

- 2026-10-15: IP/CIDR versioned assertions pass 244,444 actual/AJV and 244,444 native observations, 22 HTTP/88 socket checks, strict Clippy and configured types/inventory. Regex/JWT/emoji and broader constraints remain open; 660/682 unchanged.

- 2026-10-15: Independent subscription recovery passes restricted PostgreSQL, HTTP/MCP, both SDKs, creation regression, 13 units, 152 artifact cases and strict full/minimal Clippy. No recovery provider POST; broader billing gate remains open, 660/682 unchanged.

- 2026-10-15: Pinned Unicode16 emoji assertions pass 2,224,436 actual/AJV and 2,224,436 native observations, 23 HTTP/92 socket checks, strict Clippy/types/inventory and licensed data package inclusion. Regex/JWT and broader constraints stay open; 660/682 unchanged.

- 2026-10-15: Fresh HTTP auth clock propagation validated against actual3, pre-review full IAM218/restrictedPG, final delayed email proof and8 email regressions, strict full/minimal Clippy. Secondary organization rechecks remain queued;660/682 unchanged.

- 2026-10-15: Raw JWT syntax/header assertions pass 83,494 actual/AJV and 83,494 native observations, 25 HTTP/100 socket checks, strict Clippy and configured types/inventory. Authentication verification is unchanged; arbitrary regex and broader validator constraints remain open, 660/682 unchanged.

- 2026-10-15: All23 secondary organization session rechecks use fresh HTTP clock/policy;24 focused cases, full IAM220/3ignored, restricted PostgreSQL and strict full/minimal Clippy pass. Explicit-time APIs preserved; broader auth remains open,660/682 unchanged.

- 2026-10-15: Explicit bound Portal local expiration passes immutable migration16 history, restricted PostgreSQL races/uncertain commit, HTTP/MCP/ten SDK calls,14 units,166 artifacts and strict full/minimal Clippy. No remote URL revocation claim; broader billing stays open,660/682 unchanged.

- 2026-10-15: Immutable69cee runtime passes portable services/four workers, HTTP/MCP startup/shutdown, five capability callbacks, IAM220/3ignored and both SDK regeneration/strict consumers. All2,889 source files verified; Unicode packaging omission separately fixed/validated in f2b306269. Owned workspaces cleaned;660/682 unchanged.

- 2026-10-15: Default-off bounded regex kernel passes178,182 semantic/1,717 constructor comparisons,540 upstream tests, bounded resource tests, strict common checks and74-entry licensed source package. Checked schema/HTTP/MCP/WS integration remains open;660/682 unchanged.

- 2026-10-15: Trusted scalar organization policy passes24 actual cases, full IAM221/3ignored, restricted PostgreSQL and strict full/minimal Clippy. Default/zero/count-window/hydration behavior and concurrent admissions validated; broader auth remains open,660/682 unchanged.

- 2026-10-15: Immutable c045e5814 packages/installs all50 crates through local sparse registry, verifies optional bounded regex outcomes and HTTP/MCP startup/shutdown. Release tests10 pass, exact worktree cleaned and artifacts retained. Public publication and broader parity remain open;660/682 unchanged.

- 2026-10-15: PaymentLink metadata-only expiration library passes actual TS6, official Stripe SDK wire, two restricted PostgreSQL tests, existing read/list regressions and strict full/minimal Clippy. Migration17 journals one dispatch and retains unresolved claims; separate publisher completes local tombstone. HTTP/MCP/SDK exposure and recovery remain open;660/682 unchanged.

- 2026-10-15: Trusted invitation expiration/count/recipient-verification policy passes19 actual reference cases (including explicitly pending cancellation observations), focused/organization native checks, restricted PostgreSQL and strict full/minimal Clippy. CI compiler paths corrected for12 auth fixtures. Cancellation/delivery and broader auth remain open;660/682 unchanged.

- 2026-10-15: Explicit PaymentLink expiration HTTP/MCP/both SDKs pass restricted PostgreSQL role/startup matrices, concurrent single dispatch and pending inspection;15 units,166 generated artifacts and strict full/minimal Clippy pass. Event-linked recovery and broader billing remain open;660/682 unchanged.

- 2026-10-15: Checked regex exporter/contract/HTTP/MCP/WebSocket/composition and generated SDK integration passes376 native tests, strict nine-crate Clippy, actual2720 comparisons, generated consumers and canonical366 with zero normalized OpenAPI differences. Explicit host-JS policy and bounded native Operational errors remain distinct; inventory625/1708. Broader schema parity remains open;660/682 unchanged.

- 2026-10-15: Trusted cancellation-on-reinvite preserves committed cancellation before later admission errors, resend precedence and existing100/101 duplicate windows. Actual10, native organization5, restricted PostgreSQL and strict full/minimal checks pass; both commit-acknowledgement boundaries are covered. Delivery remains next;660/682 unchanged.

- 2026-10-15: Event-linked PaymentLink expiration recovery library passes official SDK wire, restricted PostgreSQL, prior expiration regression and strict full/minimal Clippy. Authenticated reads recover publication-only proof; no provider POST or retry. Explicit HTTP/MCP/SDK exposure remains next;660/682 unchanged.

- 2026-10-15: Ordered runtime formats pass65,703 actual/native comparisons,76 all-feature/71 minimal tests,267 HTTP/126 MCP and both generated SDKs. Typed registration and JWT algorithm resource bounds are covered; broader schema composition remains open,660/682 unchanged.

- 2026-10-15: Optional invitation delivery passes actual9, final organization6, restricted PostgreSQL and strict full/minimal Clippy. Callback ordering/commit uncertainty and sanitized delivery failures covered; stale generated executables cleaned after lease release. Broader auth remains open,660/682 unchanged.

- 2026-10-15: Unconstrained Zod factory exports no longer falsely require objects; actual/AJV11,106 comparisons and strict12-root TypeScript pass. Explicit overrides preserved; native/SDK follow-up queued,660/682 unchanged.

- 2026-10-15: Independent PaymentLink expiration recovery passes restricted PostgreSQL HTTP/MCP/both SDKs, expiration regression,16 units,186 actual artifacts and strict full/minimal Clippy. No provider POST or reservation INSERT authority required; broader billing stays open,660/682 unchanged.

- 2026-10-15: Trusted scalar team policies pass actual27, final organization7, restricted PostgreSQL concurrency and strict full/minimal checks. Default-team/zero/window/last-team behaviors covered; stronger native failure rollback remains explicit. Broader auth remains open,660/682 unchanged.

- 2026-10-15: Generated Stripe Subscription update preflight passes actual84, native/real PostgreSQL HTTP, legacy170, artifacts186 and strict checks. Owner/enums reject before provider I/O; configured-vs-baseline-vs-recovery audit recorded. Broader billing/error parity remains open,660/682 unchanged.

- 2026-10-15: Immutable e0af2cc0d CLI package passes3,005-file integrity, optional regex forwarding, portable modules/four workers, strict SDKs, IAM223/3ignored, generated HTTP/MCP lifecycle and five capability callbacks. Exact owned roots cleaned; later source changes retain separate receipts,660/682 unchanged.

- 2026-10-15: Bounded JSON intersections pass12,096 comparisons in each number mode,83 full/78 minimal tests,291 HTTP/138 MCP and both SDKs. Merge/status/reference identity and atomic failure/resource behavior covered; broader schema gates remain open,660/682 unchanged.

- 2026-10-15: Organization deletion/dynamic-role controls pass actual16, final native organization8, restricted PostgreSQL and strict full/all-target plus minimal-library Clippy. Quota concurrency and malformed deletion ordering covered; broader auth remains open,660/682 unchanged. Separate minimal-all-target keyring feature-gating failure recorded.

- 2026-10-15: Configured Subscription ownership errors now match TS500 text/correlation behavior; actual12 HTTP, signed native PostgreSQL HTTP, service2, legacy170, artifacts186 and strict full/minimal checks pass. Generic semantics and malformed-input400 preserved; broader billing stays open,660/682 unchanged.

- 2026-10-15: Trusted creator-role lifecycle passes actual14, native organization9, restricted PostgreSQL concurrency and strict full/minimal all-target checks. Exact role semantics, first100 counts and HTTP propagation covered; broader access-control/options/hooks remain open,660/682 unchanged.

- 2026-10-15: Bounded enums/discriminated unions pass5,838 comparisons per numeric mode,12,096 intersection regressions per mode,90 full/85 minimal tests,327 HTTP/156 MCP and both SDKs. Strict checks pass; committed worktree cleaned with proof retained. Broader schema gates remain open,660/682 unchanged.

- 2026-10-15: Configured cache-first Checkout/PaymentLink GET passes actual369 plus48 HTTP cases, native Stripe2/restricted-PG HTTP1, legacy170, artifacts186 and strict full/minimal checks. Provider429/status/text/correlation and read-only public identity behavior covered; cache date coercion/lists and broader billing remain open,660/682 unchanged.

- 2026-10-15: Unicode16 case transforms pass2,224,128 scalar plus13,855 context/order comparisons,95 full/89 minimal tests,351 HTTP/168 MCP and both SDKs. Licensed source/package inclusion and strict checks pass; committed worktree cleaned. Numeric length bounds and broader schema gates remain open,660/682 unchanged.


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
