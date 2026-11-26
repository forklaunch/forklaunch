# {{{name}}}

Persistent ecommerce service with HTTP and MCP, HMAC catalog/cart/order/payment routes, bounded catalog pages, Stripe/optional PayPal payment creation and an inventory worker.

The crate follows the generated service layout. `config.rs` reads trusted settings, `registrations.rs` builds the store, order-event queue, payment providers and `AppState`, and `api/routes/<resource>.rs` binds each ecommerce contract to the controller in `api/controllers/<resource>.rs`. Controllers are the module's own code: each extracts its request, calls the audited ecommerce service (or a checkout, payment or webhook flow) and maps errors to the contract's responses, so edit one to customize that operation. `forklaunch-ecommerce-stripe` keeps the catalog, checkout, provider and audit logic. `bootstrapper.rs` runs the inventory worker for `--worker`. Routers added with `forklaunch init router` live in `api/routes/extensions`.

Apply `cargo run -p {{{name}}} -- --migrate` with a schema-owner connection. Generated Compose runs the HTTP service and a separate `<module>-inventory-worker` from the same image. The worker shares database/tenant/encryption settings and exposes container readiness on port 9090 without a host port. Sync preserves worker settings; rename/delete follow the owning module. Run the HTTP service with `cargo run -p {{{name}}}` and a separate inventory process with `cargo run -p {{{name}}} -- --worker`. HTTP health is `/health`; MCP is `/mcp` on the same listener. Offline `FORKLAUNCH_MODE=contract` exports routes without provider/storage credentials. Configure the trusted tenant, encryption keys and providers in `.env.example`; request fields cannot change the database tenant.

Runtime database roles must have no SUPERUSER/BYPASSRLS. Grant schema USAGE on `fl_ecommerce,fl_worker`, SELECT/INSERT/UPDATE/DELETE on `fl_ecommerce.records,fl_ecommerce.order_retention,fl_worker.jobs`, SELECT/INSERT on `fl_ecommerce.inventory_event_receipts`, SELECT/INSERT/UPDATE on `fl_ecommerce.webhook_events`, INSERT on `fl_ecommerce.pii_read_audit`, SELECT on `fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.payment_cancellation_audit`, and USAGE on `fl_worker` sequences. Apply migrations separately with the owner role. Addresses are encrypted and plaintext reads audit the verified service actor.

Order transitions and their inventory jobs commit together. The worker decrements on paid, restocks cancellation from paid/fulfilled and ignores stock for pending cancellation. All item deltas and durable receipts commit together, preserving stock on retry/redelivery. The worker uses the same tenant and database, with optional HTTP readiness and bearer-authenticated pause/resume/drain controls.

Stripe Connect requires both `STRIPE_CONNECTED_ACCOUNT_ID` and `STRIPE_PLATFORM_FEE_BPS` (`0` for no platform fee). Client secrets appear only in Stripe creation responses. The optional `STRIPE_API_BASE` accepts HTTPS proxies or explicit loopback fixtures; unset, the Stripe API is called directly. Stripe callbacks require `STRIPE_WEBHOOK_SECRET`. PayPal requires its client ID/secret/base URL and `PAYPAL_WEBHOOK_ID` together.

This template covers 31 application routes, including external-ID catalog imports that preserve live stock and USD checkout with Stripe Tax/estimated fallback and domestic/international shipping. Provider capture reconciliation and compliance controllers remain tracked in the Rust parity plan; native database tables are not a shared MikroORM schema. Add custom HTTP routers through `routes::contracts()` and `routes::register_router(...)` and regenerate both SDKs after changing contracts.

Checkout leaves the basket intact until a paid webhook clears it, matching the TypeScript controller. Empty-cart retries resume a pending order/payment and preserve its original provider; a nonempty cart replaces an earlier pending order. A failed provider initiation leaves the order pending and the basket intact. Concurrent requests use tenant-scoped PostgreSQL operation locks; manual payment creation returns 409 if that order already has a pending provider reference. Separate bounded coordination connections prevent application-pool starvation (at most eight coordination connections, plus the configured ten application connections per HTTP process). Four checkout/webhook operations enter coordination at once, with a 15-second lock wait bound. Pending provider references from cancelled orders still require reconciliation; this template does not claim exactly-once external charging.

Provider callbacks are public at `/webhook/stripe` and `/webhook/paypal`; they verify provider signatures before creating durable tenant/provider event receipts. Stripe verifies the exact request bytes with the SDK timestamp window. PayPal sends the complete original event to its verification API. Successful matching payments atomically mark pending orders paid and enqueue inventory delivery, then clear the basket. Replay is serialized across processes. Amount/currency mismatches preserve the TypeScript payment-confirmed/order-pending behavior. PayPal decimal reconciliation requires exact nonnegative cents; malformed/fractional-cent values cannot authorize an order transition.

PayPal capture uses the TypeScript wrapper’s provisional succeeded claim and rolls it back on provider errors. A process interruption during capture requires external reconciliation. Event receipts and operation locks do not make external charging exactly once.


The native `forklaunch_ecommerce_base::compliance` service provides tenant-bound
Order export and whole-record erasure hooks. Construct it with an existing store
and an explicit `OrderSubjectField`: `Id` matches the TypeScript generic walk's
fallback, while `CustomerId` matches a TypeScript `{ Order: 'customerId' }`
override. Exported shipping addresses serialize as opaque objects; use their
`deanon` method with a trusted audit sink for explicit plaintext access. Database
reads and audit inserts commit together; failed audits or decryption roll back
an erasure. A nonempty `failures` list means the operation is incomplete.
Unclassified carts, payments and event receipts are not cascaded. These are
trusted application hooks; this preset defines no new public compliance route.
The store implements the framework `RetentionStore` API for explicit
`RetentionPolicy::for_entity::<OrderCompliance>` policies; the preset imposes no
default retention duration. Resolve the calendar cutoff in trusted application
configuration and pass the store's tenant to `retention::enforce`. Dry runs count,
and mutations use bounded oldest-first batches. The configured address is
non-nullable: an anonymization policy records completion without clearing that
address, matching TypeScript; a delete policy erases the whole order. Migration
`5_order_retention.sql` stores tenant-isolated markers that survive ordinary
order updates and disappear on deletion. Memory development stores use
`InMemoryEcommerceStore::for_tenant` to bind retention requests. Treat any reported
errors or exhausted batch budget as incomplete enforcement.


Migration 6 adds audited capture reconciliation. Custom PayPal integrations can
call `PaypalPaymentService::reconcile_captured_payment(payment_id, actor_context)`
to read authenticated provider evidence and repair a matching local payment.
Only completed, final, single-unit/single-capture evidence with matching order
reference, amount and currency is accepted. No capture, refund or order delivery
is initiated. Reconciliation operators additionally need INSERT on
`fl_ecommerce.payment_reconciliation_audit`. Ordinary claim release reads that
table so a delayed failure cannot undo a verified capture. Pending/refunded or
ambiguous evidence and cancelled-order provider references require manual review.


Stripe integrations can use `StripePaymentService::reconcile_captured_payment`
with the same operator context and audit grants. It verifies a fully captured,
unrefunded, undisputed intent/charge in the configured Connect account before
repairing local payment state. It does not create/capture/refund charges or change
orders; mismatched or incomplete evidence remains a review case.

Migration 7 adds `payment_cancellation_audit`. Runtime roles need SELECT and
reconciliation operators additionally need INSERT. Apply the migration and grants
before updating a running service. `StripePaymentService::reconcile_canceled_payment`
accepts only terminal canceled intents with no charge, zero received/capturable
amounts and matching local identifiers/amount/currency. It marks the local payment
failed, preserves its reference and audits the actor atomically. Later confirmation
or contradictory capture repair cannot reopen it. The provider is only read;
no cancellation/refund or order transition is issued. Prior charges, ambiguous
states and PayPal cancellation require review. Evidence APIs remain server-only.
