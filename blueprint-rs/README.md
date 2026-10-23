# blueprint-rs

See the [workspace guide](GUIDE.md) for the complete crate/feature catalog, development and operations workflows, API documentation commands, and compatibility boundaries.

Rust blueprint crates for ForkLaunch.

The [crate release guide](../docs/adding-projects/rust-releases.md) covers public
package validation and the publishing workflow.

This workspace mirrors the TypeScript `blueprint` style with shared core primitives, contract and interface crates, SDK generation, and reference implementations for IAM, CAC, messaging, billing, and workers. IAM provides an optional PostgreSQL backend with tenant RLS, Argon2id credentials, persistent sessions, an async Axum adapter, and generated modules. The database worker provides an optional PostgreSQL queue implementing async leases, acknowledgments, retry backoff, crash recovery, and failed-job retention. Both integrations use the `postgres` feature. The database worker also provides a `runtime` feature with a polling runner, lease renewal, bounded queue operations, configurable concurrent deliveries, and graceful drain; newly generated PostgreSQL workers use both features.

The Redis worker provides `leased::RedisLeasedQueue` behind its `redis-client` feature, with atomic claims and batches, delivery leases, renewal, retries, and crash recovery. It uses the same async queue interface and runner as PostgreSQL; newly generated Redis workers use this adapter.

Generic service storage, additional provider clients, billing provider/module wiring, worker interoperability and remaining modules are tracked in [the implementation plan](../plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md). See [Rust project documentation](../docs/adding-projects/rust.md) for setup and current boundaries.

## Commands

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps
```

From the repository root, `bash cli/tests/rust_module_lifecycle.sh` exercises all
eight built-in Rust module variants in a relocated mixed application with a
TypeScript library. It checks idempotent initialization, sync, rename, user-source
preservation, both SDK compilers and deletion of generated artifacts. Set
`FORKLAUNCH_CLI` to a built CLI and `TSC_BIN` to the TypeScript compiler. The
family-specific runtime fixtures separately check HTTP/MCP, storage and provider
behavior; this lifecycle fixture does not require live external services.

## Package Policy

- Interface crates define contracts and traits.
- Implementation crates depend on interfaces, not the other way around.
- Optional integrations should be feature-gated at the implementation crate that owns them.
- Generated TypeScript and Rust SDKs should come from framework contract artifacts.

## Worker administration

`PostgresWorkerQueue` and `RedisLeasedQueue` implement `AsyncWorkerAdmin`: bounded dead-letter listing, atomic requeue with an explicit retry policy, and age-based purging of completed or failed jobs. Operations stay within the queue's trusted tenant. Retry clears prior attempts and failure reasons; purging permanently frees the job identity. Generated worker binaries expose `--dead-letters`, `--retry`, `--purge-completed`, and `--purge-failed` commands. Automatic retention is disabled by default. Enable `retention::run_worker` with an explicit `RetentionPolicy` to coordinate bounded periodic cleanup with job processing and graceful shutdown.

The shared runner supports `control::WorkerControl` for process-local pause/resume and irreversible drain. The optional `http-control` feature adds health/readiness and bearer-authenticated controls; `http::run` coordinates server lifetime, retention, and active handler drain. Generated workers enable the listener only when `WORKER_HTTP_ADDR` is set; see the [Rust project documentation](../docs/adding-projects/rust.md#worker-http-health-and-controls).

Generated durable workers also accept `--control-service`: the same executable
serves health/readiness and optional distributed controls without opening job
storage, claiming work or running retention. `http::serve_management` exposes this
lifecycle to custom applications. A control token requires a signal publisher;
without a token only health/readiness are exposed. Readiness describes the
management process, not remote consumer health. CLI release metadata selects the
control role separately from consumers when generated sources advertise support.
Legacy releases retain their previous command; actual ECS rollout remains open.

The official BullMQ adapter also supports [lazy age retention](https://docs.bullmq.io/guide/queues/auto-removal-of-jobs) and [stalled-job recovery](https://docs.bullmq.io/guide/jobs/stalled), tested against actual TypeScript producers/consumers under Redis ACLs. Configure upstream queue/job retention policies through `BullMqOptions.queue`; age values are seconds. Worker options select lock/stall intervals and maximum stalls/starts. Failure observers receive original processing and preprocessor events even after automatic removal. A feature-gated patch to the pinned official worker emits outcome data before its existing failure notification; both notifications use the same original snapshot and Redis wire semantics. The vendored dependency retains upstream source/license provenance and documents the outcome hook and option compatibility patch in `crates/vendor/bullmq-official/FORKLAUNCH_PATCH.md`.

BullMQ queue defaults also support simple deduplication, TTL throttling and
replacement of delayed jobs (`deduplication` with `ttl`, `extend`, `replace` and
job `delay`). Actual TypeScript/native producers share the same deduplication
keys in both directions, including release after completion and TTL expiry.
`keep_last_if_active` retains the newest duplicate submitted during processing
and runs one follow-up after either successful completion or terminal failure.
Configure these through `BullMqOptions.queue.default_job_options`. Failed
attempts retain their reason and diagnostic history through immediate/delayed
retries and terminal failure; `stack_trace_limit` controls the newest retained
entries (zero clears history). Native entries contain Rust error diagnostics.

The official queue available from `BullMqProducer::inner()` also provides
persistent interval/cron schedulers and inspection/removal. Actual TypeScript
and native workers exchange three-iteration schedules for both success and
failure. The next iteration is scheduled before processing starts, so removal
cancels queued work even while the current job is active; it lets that active job
finish. A preprocessor rejection still advances the schedule. Zero
`max_started_attempts` disables that limit, as in TypeScript. Scheduler templates
and all iterations preserve `keep_logs`, `stack_trace_limit` and `size_limit`;
actual workers verify bounded logs and failure histories in both directions.
Iterations also retain creation timestamps and the scheduled instant for cron.

Worker handlers can override `JobHandler::process_batch` to return failures per job. `RunnerConfig::batch_size` and generated `WORKER_BATCH_SIZE` default to one, preserving single handlers. Concurrent batch sizes are bounded to 1000 total deliveries; all leases renew and batch cancellation preserves expiry recovery. See the [batch handler documentation](../docs/adding-projects/rust.md#worker-batch-handlers).

## Messaging

There are no messaging crates. Services send email and SMS through the managed
gateway clients in `forklaunch_core::http` (`forklaunch infra add`).

## Ecommerce providers

`forklaunch-ecommerce-stripe` creates and retrieves PaymentIntents, resumes checkout without creating another intent, and supports direct Connect charges with bounded integer application fees. `StripePaymentDto` includes an optional response-only client secret; the shared store persists only the provider reference. `forklaunch-ecommerce-paypal` implements OAuth, Orders creation/capture and remote webhook signature verification. Both clients use a bounded TLS transport with redirects and automatic retries disabled, and sanitized errors.

Both payment wrappers share the base ecommerce store. PayPal uses its atomic confirmation claim before capture and releases the claim on provider failure, matching the TypeScript wrapper. The claim provisionally marks a payment succeeded; process interruption during capture requires reconciliation. Generated provider webhooks verify signatures, serialize durable event handling and drive atomic order/inventory delivery. Interrupted-capture reconciliation remains tracked in the [implementation plan](../plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md). Actual TypeScript conformance fixtures cover [Stripe](../framework-rs/parity/rust_ecommerce_stripe_parity.mjs) and [PayPal](../framework-rs/parity/rust_ecommerce_paypal_parity.mjs), alongside native live HTTP tests.

`BaseEcommerceService::search_products` applies the storefront's variant price, option and stock filters before the product query. `list_catalog` returns products with variants and optional stock in stable createdAt/id order, with limits clamped to 1–100. PostgreSQL pages before loading product DTOs and scopes bulk variant/stock reads to that page. Integral pagination parameters and storage errors are checked explicitly; missing inventory remains absent. The [catalog conformance fixture](../framework-rs/parity/rust_ecommerce_catalog_parity.mjs) executes the actual TypeScript product controllers.

The ecommerce base `worker` feature exposes `events::OrderInventoryHandler` for the leased database/Redis/Kafka runner. Paid events decrement inventory; cancellation from paid/fulfilled restocks it. All item adjustments and an event receipt commit together, so retry after a failed item or lost queue acknowledgment cannot apply partial or duplicate stock changes. Receipt identity ignores delivery metadata but rejects reused IDs with different logical payloads. Workers validate positive quantities, legal transitions and matching job/event/queue identities. PostgreSQL runtime roles also need `SELECT, INSERT` on `fl_ecommerce.inventory_event_receipts`; use the owner migration command to apply versioned tables/indexes.

With both `postgres` and `worker` enabled, `PostgresEcommerceStore::transition_order_with_event` updates the order and enqueues its inventory event in one tenant-bound transaction. Apply both ecommerce and database-worker migrations with an owner connection. Runtime roles need the documented table/sequence grants for both schemas. Queue-write failures roll back the transition and its PII read audit. The underlying `PostgresWorkerQueue::enqueue_in_transaction` checks the caller transaction's tenant and leaves commit/rollback ownership with the caller.

`forklaunch-ecommerce-stripe`'s `http` feature (`EcommerceHttp`) serves 31 application routes plus health, including import, checkout and provider callbacks. It creates request-scoped audited stores from verified principals, selects Stripe/PayPal from the payment request and uses atomic order/event production for transitions. `contracts::module_registry()` adds the health route and documented validation/storage/provider errors to the core TypeScript-compatible route registry. Checked JSON bodies return 400 for invalid typed inputs; query arrays use the SDK/MCP JSON wire form. The [live adapter fixture](crates/implementations/ecommerce/ecommerce-stripe/tests/http_router.rs) starts HTTP and MCP against restricted-role PostgreSQL and owned provider endpoints. Generated module fixtures cover checkout/import/provider callbacks, both SDKs, worker delivery and lifecycle. Compliance and automated worker orchestration remain in the implementation plan.

Billing base services are available in `forklaunch_billing_base::services` with typed asynchronous interfaces in `forklaunch_billing_interface::services`. Plans and subscriptions are durable; checkout sessions, payment links and portal sessions use a configurable TTL cache independent of provider expiry, with optional database backups. The memory and PostgreSQL stores preserve TypeScript lifecycle differences: checkout completion retains backup status, payment completion removes backup rows, and explicit portal/checkout expiry removes only the cache entry. PostgreSQL requires a restricted runtime role, encrypts provider payloads and opaque session URIs, and commits plaintext reads with actor audits. Provider-ID lookup reads only indexed metadata. The [actual TS base comparison](../framework-rs/parity/rust_billing_base_parity.mjs) covers 168 operations over generic mapper/cache/ORM boundaries. Configured Stripe mappers, real Stripe service wiring and generated billing HTTP/MCP modules remain tracked; the older root-level billing classes are reference scaffolds.

BullMQ producers inherit configured job defaults while the wrapper overrides retry attempts/backoff and intentionally leaves job IDs auto-generated, matching TypeScript. Metadata includes log limits, stack-trace limits, timestamps and size limits. Actual cross-language fixtures exercise persisted options, oversized-job rejection, bounded log retention, delayed/priority/LIFO ordering and worker concurrency. Repeat/deduplication and rate limiting have actual cross-language fixtures. The [options audit](../plan/RUST-BULLMQ-OPTIONS-AUDIT.md) maps the checked wrapper surface and records remaining upstream-option gaps.

IAM directory parity now includes typed user/role/permission/organization CRUD interfaces, transactional memory and PostgreSQL stores, nested relationship hydration, encrypted sensitive fields, retained-key uniqueness claims and atomic actor audits. Generated IAM modules expose thirty configured directory operations over HTTP and MCP alongside persistent session endpoints. Opaque tokens remain default; the optional `jwt-sessions` feature adds `RsaJwtSessionIssuer` and `PostgresIamService::with_jwt_session_issuer` for RS256 tokens with explicit issuer/audience/key ID, unique IDs, tenant binding and server-resolved grant snapshots. The full token digest remains authoritative for local session validation and revocation. Generated IAM configures the issuer only when all four `SESSION_JWT_*` variables are present; matching public-key publication is application-owned. Directory users and authentication identities remain separate, matching the configured TypeScript CRUD mapper's omission of passwords. Optional base browser cookie/CSRF routes use the shared session layer (see below); verification/reset and OAuth/OIDC integration remain separate gates; the Better Auth module has its own lifecycle implementation. Base discovery returns 204; its system-role compliance routes return 404 under the configured all-`none` entity policy.

See the [Rust/TypeScript compatibility baseline](../docs/adding-projects/rust-compatibility.md) for source package versions, artifact formats and implementation choices.

### Application-configured IAM compliance

IAM directory compliance defaults to no classified entities. Applications can
register their own `ComplianceMetadata` types independently of encryption:

```rust,ignore
use forklaunch_core::persistence::compliance::ComplianceMetadata;
use forklaunch_iam_base::directory::{DirectoryKind, compliance::DirectoryCompliance};

#[derive(ComplianceMetadata)]
#[compliance(entity = "User")]
struct UserCompliance {
    id: String,
    #[compliance(level = "pii")]
    email: String,
}

let compliance = DirectoryCompliance::default()
    .with_entity::<UserCompliance>(DirectoryKind::User, "id")?;
let iam = forklaunch_iam_base::http::IamHttp::new(sessions, directory, auth, mode, service_info)
    .with_compliance(compliance);
```

The authenticated HTTP and MCP export/erase routes require the system role and
use the store's trusted tenant. Export selects IDs and explicitly classified
fields. Erasure deletes whole matching records, following the TypeScript
compliance service. Each entity runs in its own transaction; incomplete storage
operations return an error through the router. Direct service callers must check
`failures`, including when other entities committed successfully.

A subject field other than `id` must contain an explicit scalar string ID.
These lookups use the directory store's bounded listing (currently 4,096 records
for PostgreSQL); relation inference and scalable indexed traversal remain
separate work. See the [executable example](crates/implementations/iam/iam-base/examples/directory_compliance.rs).


BullMQ queue metadata options are available as
`BullMqOptions.queue.streams_max_len_events` (default 10,000) and
`skip_metas_update` (default false). Event stream trimming is approximate and uses
the shared queue metadata. Skipping metadata updates supports constructing a
read-only queue handle; subsequent job operations still need write permissions.
Queue and worker construction check Redis's reported minimum version (5) unless
`queue.skip_version_check` is true; the wrapper applies this setting to both.
Native errors remain sanitized and startup/operation deadlines still apply.

The blueprint workspace shares the framework's pinned literal-key JSON parser.
The root Cargo patch applies transitively to HTTP/MCP, workers and JSON record
persistence. When consuming these source crates from another workspace, copy the
workspace patch described in the [JSON boundary audit](../plan/RUST-JSON-BOUNDARY-AUDIT.md);
dependency manifests cannot apply Cargo patches to their callers.

Better Auth storage supports `AuthTransaction::list_page(model, after, limit)`
for bounded insertion-order pagination over models larger than 4,096 rows.
Limits are 1..=4,096; `AuthPage.next` is present only when another row exists.
Keep the transaction open across pages for a stable view against other writers.
Cursors are model-scoped positions, not authorization credentials. Replacing a
record keeps its position, deleting a cursor's record does not invalidate the
continuation, and rollback restores records and indexes together.

Migration `0002_record_pagination.sql` adds the tenant/model/sequence index;
apply migrations with the schema owner before serving requests with the runtime
role. PostgreSQL pages decrypt at most limit+1 records and retain tenant RLS and
record-bound encryption. The memory reference backend maintains insertion-order
and unique-field indexes, but still clones its transaction snapshot in memory.
The existing `list()` remains capped and fails closed for oversized models.
Production auth paths use indexed lookups or bounded paging, including sessions,
accounts, members, provider verification records and key rotation. Collection
responses still require memory proportional to their returned result.

Better Auth session-token authentication now uses exact unique indexes in memory
and rotating encrypted blind indexes in PostgreSQL. Fetch, refresh, single-token
revocation and sign-out no longer scan the whole session model. Unknown tokens
remain indistinguishable from revoked tokens; corrupt/transplanted index matches
are checked against the decrypted record before returning it.

`PostgresAuthStore::new` backfills existing session-token claims under the tenant
write lock in 256-record pages before returning the store. The public
`reindex_session_tokens()` operation also supports maintenance/key rotation.
Backfill is transactional: duplicate legacy tokens or undecryptable rows reject
startup and roll back the claims. This uniqueness check is deliberately stricter
than the configured TypeScript entity's unindexed token field. Roll out
index-aware writers together; older binaries do not maintain token claims and
must not keep writing after backfill. Session-list/revoke-all and account/member
queries use the corresponding indexed lookup paths.


## Ecommerce compliance and retention

`forklaunch_ecommerce_base::compliance` provides tenant-scoped classified Order
export and erasure. `OrderSubjectField::Id` preserves the configured TS default;
select `CustomerId` explicitly for customer-scoped application policy. Exported
shipping addresses remain opaque `ProtectedAddress` values (ordinary serialization
emits `{}`). Explicit `deanon` requires an audit callback before disclosure.
PostgreSQL erasure audits and deletes matching orders atomically; unrelated carts,
payments, provider receipts and other tenants are not removed.

`retention::OrderCompliance` supplies metadata to the framework retention hooks.
There is no default retention period. Required shipping addresses are non-nullable:
anonymization keeps their value and records completion, matching the TS metadata.
Deletion removes the order. The PostgreSQL adapter uses a tenant-scoped operation
lease, bounded oldest-first batches and RLS-protected completion markers; dry runs
count without mutation. Cutoffs are exclusive and cannot be in the future.
Migrations and the generated restricted-role grants include the retention marker
table. Provider cancellation/capture reconciliation remains a separate concern.


Durable customer CRUD is available through
`forklaunch_billing_interface::customers::CustomerService`, implemented by
`BaseBillingService` over both memory and PostgreSQL stores. Customer IDs may be
supplied by the server or generated on creation. Email and name are encrypted in
PostgreSQL with record/field-bound envelopes; reads and updates require an atomic
actor audit. Updates distinguish an omitted name from explicit `null` (clear).
`list_customers(None)` lists records within the store's existing bounded limit;
`Some(&[])` returns no records. Customers never use the expiring session cache.
Migration 3 extends existing billing databases without changing earlier migration
checksums. This is the native customer-persistence interface planned for the port;
the configured TypeScript billing module has five services and no standalone
customer service.

`StripeBillingClient` supports `StripeResource::Customer` through its create,
retrieve, update, list/list-all and delete helpers, using either direct Stripe
authentication or the instance-signed managed transport. The same client already
supports checkout creation, retrieval, update and expiry. Applications can persist
a provider customer ID in the native customer's `id` field and pass it to checkout.
Provider requests and local persistence remain separate operations; interrupted
provider/local-write reconciliation is still an open gate.


Billing webhook migration 4 records intent before executing a verified callback.
Callback errors, timeout/cancellation and process interruption leave a durable
attempt. Subsequent deliveries return 503 and do not repeat ambiguous effects.
Successful processing atomically inserts the final receipt and removes intent.
Existing migration-2 receipts still deduplicate without executing the callback.
Runtime roles additionally need SELECT/INSERT/DELETE on
`fl_billing.webhook_attempts`; reconciliation operators need INSERT on
`fl_billing.webhook_reconciliation_audit`.

Use `PostgresBillingStore::pending_webhooks(1..=1000)` to inspect outstanding
attempts. This list includes active callbacks; resolution cannot take an active
callback's event lock. After checking and repairing external/local state, call
`resolve_webhook(&attempt, WebhookResolution::Applied)` to suppress replay, or
`RetryAllowed` only after verifying re-execution is safe. The actor and correlation
context are required; the inspected attempt ID protects against stale decisions.
The audit and receipt/removal commit together. No raw payloads or provider secrets
are retained by these tables. This is an explicit operational recovery API;
provider repairs and interrupted outbound API mutations remain separate work.


PayPal recovery is available through
`PaypalPaymentService::reconcile_captured_payment(payment_id, actor_context)`.
It retrieves the provider order with configured merchant credentials, verifies a
completed final capture against the local order reference, amount and currency,
and atomically repairs/audits the payment. It never calls capture or triggers order
transitions/delivery. The current implementation accepts one purchase unit and
one final capture, matching this client's create-order format. Other layouts,
pending/refunded states and mismatched evidence require review. Monetary comparison
uses exact decimal arithmetic under the existing amountCents/100 contract.

`PaymentReconciliationStore` implements optimistic snapshot checks and atomic
audits for memory/PostgreSQL. Migration 6 adds tenant-RLS audit metadata; runtime
roles need SELECT and reconciliation operators need INSERT. A delayed failed
capture cannot release a payment that has recorded verified capture evidence.
The trusted server-only `VerifiedCapture` API must not accept unverified public
request bodies. No new public endpoint is enabled. Ordinary PayPal operations
retain the TypeScript claim/failure semantics until explicit reconciliation;
cancelled-order references and broader outbound recovery remain open.


Stripe recovery uses `StripePaymentService::reconcile_captured_payment` with the
same audited payment store. It retrieves the PaymentIntent with `latest_charge`
expanded, preserving the configured Connect account. Both intent and charge must
match the local amount/currency and provider reference; order metadata must match,
and the charge must be fully captured, paid, unrefunded and undisputed. Pending,
partial, refunded, disputed or substituted evidence requires review. The helper
only reads provider state and repairs the payment; it does not create/capture/
refund charges or deliver orders. Client secrets/raw provider data never enter
the reconciliation audit.

`StripePaymentService::reconcile_canceled_payment` repairs an interrupted local
claim only from a terminal canceled intent with zero received/capturable amount,
an explicit null latest charge, a positive cancellation timestamp, and matching
order/amount/currency. It preserves the provider reference, sets the payment to
failed and atomically records an operator audit. Migration 7 adds the tenant-RLS
`payment_cancellation_audit` table; runtime roles need SELECT and reconciliation
operators additionally need INSERT. Apply the migration and grants before serving
new code. Recorded cancellation blocks later confirmations/claims and contradictory
capture repairs; verified capture evidence also blocks cancellation repair.

The helper performs a provider read only. It does not cancel/refund funds, reopen
an attempt or change the order. This narrow policy relies on Stripe's documented
[terminal cancellation behavior](https://docs.stripe.com/api/payment_intents/cancel).
Canceled intents with any prior charge, pending states, PayPal cancellation and
ambiguous evidence still require review. `VerifiedCancellation` is a trusted
server-only API and must never accept unauthenticated evidence from a request.


Better Auth's trusted server-only keyring helper supports external signers through
`KeyRing::sign_jwt_with_signer` and the async `keyring::custom::JwtSigner` trait.
The explicitly supplied callback receives JSON claims with the custom-sign path's
expiration/issuer/audience defaults. It bypasses local JOSE claim setters and all
key-store access; callback errors become a fixed sanitized signing error. The
signer owns algorithm selection, claim policy and key publication. Bound network
waits in the signer or at the call site. Non-finite JavaScript claims cannot be
represented by this JSON callback API and are rejected.

This server-only helper does not reconfigure session token routes, remote JWKS
verification or plugin discovery metadata. Configuring those surfaces for custom
signers remains part of the broader JWT/JWKS adapter work. Serialized claim options
cannot replace the typed server callback.


For authenticated HTTP session tokens, construct `http::ExternalJwtSigner::new`
with a public JWKS URL, explicit asymmetric algorithm and a typed `JwtSigner`,
then pass it to `AuthHttp::with_external_jwt_signer`. `/token` and the
`get-session` response header use the callback after persistent session validation.
The callback receives the configured email/organization claims, subject, issuance
and expiry times, issuer and audience. No local signing key is created; `/jwks`
returns 404 as in Better Auth's remote-JWKS mode. The application must publish the
external public keys and configure JWT verification separately. Popup callback helpers exchange through the same configured auth router. This
does not configure OpenID discovery metadata. The key URL
is configuration metadata and is never fetched by this adapter; signing algorithms,
key rotation and callback deadlines remain the external signer's responsibility.


### Base IAM browser sessions

Enable `forklaunch-iam-base/http` with `browser-sessions` and construct
`BrowserIam::new(service, store, "__Host-iam", "https://your-app.example")` from
`forklaunch_iam_base::http::browser`. Supply a durable shared `tower_sessions::SessionStore`
with a dedicated namespace for the tenant/application. MemoryStore is suitable
only for tests/development. Generated modules enable encrypted PostgreSQL storage
only when `SESSION_COOKIE_ORIGIN` is explicitly configured.
Use `browser.mount(router)` to reserve `/auth/browser` in a ForkLaunch router;
HTTP middleware and ordinary HTTP/MCP services remain available, while browser
login/logout and CSRF endpoints are excluded from contract/SDK/MCP exports.

GET `/auth/browser/csrf` returns `{ "csrfToken": "..." }` and an anonymous secure
cookie. POST `/auth/browser/login` with ordinary email/password JSON, that cookie,
`x-csrf-token` and the exact configured HTTPS Origin. Successful login rotates the
cookie ID and CSRF token; the response contains only the new CSRF token. GET
`/auth/browser/session` returns current subject, roles, permissions and expiry.
POST `/auth/browser/logout` requires the current CSRF proof and revokes the
underlying IAM session before clearing the cookie. All endpoint responses use
`Cache-Control: no-store`. Cookies are Secure, HttpOnly, SameSite=Strict and path
`/`; applications own TLS termination and trusted-origin CORS configuration.

The server-side cookie store contains the IAM bearer credential and must be
protected as secret storage. It never appears in a browser response or cookie.
Every authentication resolves the persistent IAM ledger and current user/grants,
so password changes, disabled users, revocation and tenant isolation apply.
Re-login revokes the prior credential. For application-owned cookie-authenticated
contract routes, use `browser.cookie_sessions()` in `RouterConfig.cookie_sessions`
and the matching `AuthConfig::cookie` name. Existing bearer endpoints continue to
require their bearer header. Cookie records expire after thirty minutes without
writes; IAM credentials retain their one-hour maximum. OAuth/OIDC flows remain separate work.

`forklaunch-iam-base/browser-sessions` supplies `PostgresCookieStore`, using the
existing tenant RLS and encryption key ring. Migration 3 adds the cookie table;
runtime roles need CRUD on it. Cookie IDs are SHA-256 hashed, and encrypted
records bind namespace, ID and expiry. The store rejects privileged roles,
oversized records and modified/swapped ciphertext. Creation retries random ID
collisions; updates cannot resurrect deleted or expired sessions. The
`purge_expired` maintenance method is scoped to the trusted tenant/namespace.
Generated IAM uses the service name as namespace and `__Host-iam` as cookie name,
so a rename requires re-login. Cookie writes and IAM writes remain separate
transactions; failed storage can leave an unreachable credential until expiry.

Direct bound BillingPortal creation is available through the Stripe crate's
`postgres-identity` profile. It verifies registered Customer/configuration
evidence, retains a separate public UUID and uses durable dependency claims plus
separate-role atomic publication. Historical inspection never refreshes a bearer
URL; uncertain dispatch remains inspectable without automatic POST retry. See
[the provider guide](crates/implementations/billing/billing-stripe/README.md#direct-bound-portal-creation)
for migrations12/13, caller authorization and publication-role requirements.

The same explicit Stripe profile supports bound Checkout creation from registered
Customer and completed Plan provenance. Migrations14/15 add four durable dependency
claims and separate-role UUID publication. Exact Price/line-item evidence and
provider expiry are preserved; open-session creation does not grant entitlement.
See [the Checkout provider guide](crates/implementations/billing/billing-stripe/README.md#direct-bound-checkout-creation).

Generated billing services default to legacy request dates. Set
`BILLING_REQUEST_DATE_PROFILE=utc-iso-and-epoch` (or use
`BillingBackend::with_request_dates(BillingRequestDateProfile::UtcIsoAndEpoch)`)
to accept numeric epoch milliseconds and the deterministic UTC/offset ISO subset
on declared startDate/endDate/expiresAt fields through HTTP and MCP. Fractions
truncate toward zero. The UTC year range is 0000–9999; locale and offsetless
datetime strings return an explicit unsupported-profile 400. Null and invalid
dates remain invalid. Omitted fields and opaque provider payloads are preserved;
responses retain ISO date-time schemas. Contract request metadata and generated
SDKs expose string-or-number admission with profile annotations. This is a bounded
profile, not all JavaScript Date formats. Runtime environment selection persists
across regeneration/sync.

Billing contract/OpenAPI export uses the same `BILLING_REQUEST_DATE_PROFILE` as
runtime startup. Library callers can explicitly select
`contract_registry_with_request_dates`; the existing `contract_registry()` stays
legacy. Application-defined custom routes are unaffected by package date projection.

Generated Stripe billing can explicitly select `BILLING_IDENTITY_PROFILE=bound-checkout-v1`
with bounded `BILLING_BOUND_CHECKOUT_CONFIG` and a separate
`BILLING_PUBLISHER_DATABASE_URL`. See the
[HTTP profile guide](crates/implementations/billing/billing-stripe/HTTP.md).
The profile adds create/get/inspect Checkout operations over HTTP/MCP and both
SDKs, preserving local UUIDs and durable creation evidence. Unknown outcomes return
202 without redispatch; historical inspection returns IDs without reviving URLs.
Internal HMAC authenticates the calling service; application code must authorize
individual users and Customer/Party ownership. Default routes stay legacy.
