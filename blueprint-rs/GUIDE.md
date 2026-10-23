# blueprint-rs guide

Blueprint development and operations reference for the current source workspace.

## Crate catalog

The source links expose the public API. Build the searchable Rust API reference
with the commands below. Feature names come from each crate's `Cargo.toml`; enabling
a feature makes its API available but does not configure credentials or start a service.

Shared module types (errors, paging, audit and session context) and managed
platform clients live in the framework, as they do for TypeScript in
`@forklaunch/core/http`: `forklaunch_core::domain` and `forklaunch_core::http`
behind the `domain`, `provider-http`, `stripe-webhook` and `instance-gateway`
features. Each implementation crate's `http` feature provides the module's
contract-bound handlers; generated modules bind them in thin controllers and
routes, as TypeScript modules call their implementation packages.

| Crate | Responsibility | Optional features and defaults |
| --- | --- | --- |
| [forklaunch-iam-interface](crates/interfaces/iam-interface/src/lib.rs) | IAM identities, sessions, roles, permissions and service DTOs/traits. | —; default: none |
| [forklaunch-cac-interface](crates/interfaces/cac-interface/src/lib.rs) | Code-set/category contracts and service types. | —; default: none |
| [forklaunch-ecommerce-interface](crates/interfaces/ecommerce-interface/src/lib.rs) | Product, variant, inventory, cart, order and payment contracts/types. | —; default: none |
| [forklaunch-billing-interface](crates/interfaces/billing-interface/src/lib.rs) | Customer, plan, subscription, checkout and billing contracts/types. | —; default: none |
| [forklaunch-worker-interface](crates/interfaces/worker-interface/src/lib.rs) | Queue/job, lease, administration and handler types. | —; default: none |
| [forklaunch-iam-base](crates/implementations/iam/iam-base/src/lib.rs) | Directory/auth service implementations, optional PostgreSQL persistence and HTTP handlers. | `postgres`, `jwt-sessions`, `browser-sessions`, `oidc-sessions`, `http`; default: none |
| [forklaunch-cac-base](crates/implementations/cac/cac-base/src/lib.rs) | Code-set service implementations and HTTP handlers. | `http`; default: none |
| [forklaunch-ecommerce-base](crates/implementations/ecommerce/ecommerce-base/src/lib.rs) | Atomic local commerce storage/business rules, compliance and retention. | `worker`, `postgres`; default: none |
| [forklaunch-ecommerce-stripe](crates/implementations/ecommerce/ecommerce-stripe/src/lib.rs) | Stripe checkout/capture/webhook integration and the commerce HTTP handlers (with PayPal). | `postgres`, `http`; default: none |
| [forklaunch-ecommerce-paypal](crates/implementations/ecommerce/ecommerce-paypal/src/lib.rs) | PayPal checkout/capture integration. | —; default: none |
| [forklaunch-billing-base](crates/implementations/billing/billing-base/src/lib.rs) | Billing services, store seams and optional PostgreSQL persistence. | `postgres`; default: none |
| [forklaunch-billing-stripe](crates/implementations/billing/billing-stripe/src/lib.rs) | Stripe provider, optional remote client and the billing HTTP handlers. | `stripe-client`, `postgres-reconciliation`, `postgres-identity`, `http`, `typescript-routing`, `bound-checkout`; default: none |
| [forklaunch-worker-database](crates/implementations/worker/worker-database/src/lib.rs) | Database queue, shared runtime/control/retention and PostgreSQL adapter. | `http-control`, `runtime`, `postgres`; default: none |
| [forklaunch-worker-redis](crates/implementations/worker/worker-redis/src/lib.rs) | Native leased Redis queue and optional official BullMQ protocol adapter. | `bullmq`, `redis-client`; default: none |
| [forklaunch-worker-kafka](crates/implementations/worker/worker-kafka/src/lib.rs) | Kafka worker interfaces and optional broker adapter. | `kafka-client`; default: none |

## Choose a generated module

The [generation guide](../docs/adding-projects/rust.md) covers application init,
adding modules, migrations, development, SDKs and mixed-language lifecycle.
Rust modules are `iam-base`, `cac-base`, `ecommerce-stripe`, `billing-base` and
`billing-stripe`. Better Auth and relay are TypeScript modules; Rust services call
them through generated Rust SDKs. Messaging is not a module in either language:
services send email and SMS through the managed gateway (`forklaunch infra add`).
Storefront capture has its own lifecycle outside Cargo APIs.

Interfaces own contracts, DTOs and service traits; base crates implement local
business behavior and storage seams; provider crates implement external effects;
Axum crates connect those services to registered HTTP/MCP operations. Choose the
implementation explicitly. Reference memory stores are useful for tests but do
not establish production durability or cross-process isolation.

## IAM

The [PostgreSQL IAM guide](../docs/adding-projects/rust.md#postgresql-iam) documents the
base module's runtime environment. Directory CRUD does not turn an ignored
password field into a credential: provider integration and generic mapper/filter
extensions remain tracked separately. Use the authentication service's credential
and session lifecycle rather than inserting directory records to simulate login.
Better Auth and relay are TypeScript modules.

## CAC

[CAC](../docs/adding-projects/rust.md#cac-module) exposes code-set operations through
the service interface and its generated HTTP layer. Keep custom route registration
in application source so exported contracts and serving behavior agree.

Managed capability clients (email, SMS, voice, WhatsApp, payments and models) are
`forklaunch_core::http` modules that use the instance gateway; see the
[capability configuration guide](../docs/adding-projects/rust.md#managed-capabilities).
Provider HTTP transport, Stripe webhook verification and gateway clients each
have independent feature gates.

## Billing and ecommerce

Billing services compose customer, subscription, plan and checkout storage with
an optional Stripe provider. Configure provider credentials and the local store
before mounting the Axum adapter. The [compatibility baseline](../docs/adding-projects/rust-compatibility.md)
and [parity inventory](../plan/RUST-TYPESCRIPT-PARITY-INVENTORY.md) record configured
mapper differences and timestamp/status quirks. External provider effects and
local transactions are not atomically committed together. Interrupted plan
replacement/provider writes and receipt reconciliation remain open: do not claim
exactly-once external financial effects from a successful local transaction.

Ecommerce splits product/variant/inventory/cart/order/payment behavior from
Stripe and PayPal capture. See [provider setup and boundaries](README.md#ecommerce-providers).
The base store owns tenant isolation, local concurrency and durable event state;
provider retries require the documented provider references and idempotency
behavior. Interrupted external capture and cancelled-order reference
reconciliation remain tracked work. Inspect local and provider state before
repeating an uncertain capture.

Ecommerce compliance exports classified order addresses as protected values;
explicit disclosure requires an audit callback. Subject mapping defaults to the
actual configured TS entity-ID behavior; choose customer-ID mapping explicitly
when the application requires it. Required addresses are not nullable, so an
anonymization pass records completion without clearing them. No retention period
is enabled implicitly. Retention and erasure are tenant-scoped and atomic in the
PostgreSQL adapter. See the compliance and retention sections in [the README](README.md).

## Worker execution and shutdown

The shared database worker runtime coordinates bounded concurrency, lease
renewal, retries, administration, optional retention and graceful drain. Native
Redis leases and official BullMQ are distinct wire protocols; use `bullmq` for
actual TS BullMQ interoperability. Kafka uses its broker adapter and explicit
acknowledgment behavior. Use the matching implementation's feature gate from the
catalog, not a generic memory worker in production.

The [worker guide](../docs/adding-projects/rust.md#postgresql-worker-queue) covers
database, Redis, BullMQ and Kafka startup. The
[administration guide](README.md#worker-administration) covers dead letters,
requeue, purge, pause/resume, drain and retention. Generated worker HTTP controls
are optional and bearer-authenticated; they are not the service MCP listener.
Automatic retention is disabled unless configured. Keep handler side effects
idempotent because a crash or expired lease can cause redelivery. Graceful drain
stops acquiring new work and lets the configured active work/shutdown policy run.

## Persistence and application ownership

Run migrations using an administrative connection, then start services with
restricted tenant-aware connections and required audit grants. Keep encryption
keys and service credentials outside generated source. Startup failure after an
upgrade commonly needs a migration, a missing runtime secret, or a corrected
restricted-role grant; bypassing RLS is not a repair.

Sync and module reinitialization preserve user-editable Rust source. Review
embedded library changes separately from application hooks, provider policy and
custom routes. Regenerate contracts and both SDK targets after route changes.
Generated module fixtures exercise relocation, rename and deletion; preserve the
application's own tests as well. Application hook correctness remains the
application's responsibility even when the generic scaffold passes its fixtures.

## Build API docs and run conformance

From the repository root:

```sh
CARGO_INCREMENTAL=0 RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path blueprint-rs/Cargo.toml --workspace --all-features --no-deps
cargo test --manifest-path blueprint-rs/Cargo.toml --workspace --all-features
cargo clippy --manifest-path blueprint-rs/Cargo.toml --workspace --all-targets --all-features -- -D warnings
```

Open `blueprint-rs/target/doc/forklaunch_iam_base/index.html` and select a
crate to browse its API. Real PostgreSQL/Redis/Kafka/provider-fixture checks need
the environment specified in `cli/tests/rust_*` scripts; memory tests and ignored
integration tests do not replace those checks. See
[validation commands](../docs/adding-projects/rust.md#validate-the-port),
[compatibility](../docs/adding-projects/rust-compatibility.md), and the
[implementation checklist](../plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md) for
remaining gates. Publishing and ECS deployment are not established by a local
doc, source-archive or container build.
