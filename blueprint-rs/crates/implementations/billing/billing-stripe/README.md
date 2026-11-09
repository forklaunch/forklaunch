# Stripe billing

The optional `http` feature serves billing over HTTP and MCP; see [HTTP.md](HTTP.md).

Payment-link update, expire, success and failure, and checkout-session expire, success and failure, read the cached record first and call Stripe with its stored Stripe id (`providerFields.id`, `plink_...`/`cs_...`); public ids stay in the API. A record with no stored provider object falls back to its own id only when that id is itself a Stripe id (the default service stores records under their Stripe id). Anything else fails with `billing_provider_reference_missing` before any Stripe call.

`StripeBillingClient::verify_context(expected_account, expected_livemode)` verifies a direct account using authenticated `GET /v1/account` and `GET /v1/balance`. Expectations must come from trusted server configuration. The account response must match the expected account ID and the balance must explicitly report the expected boolean mode. No account or mode is inferred from API key prefixes. Restricted keys without both read permissions fail closed. Requests use the client's existing bounded HTTPS/trusted-proxy transport and pinned API version; no Connect account header or provider mutation is added.

The returned `VerifiedStripeContext` has no public constructor, clone or serializer. `ensure_current` requires the same immutable client generation and less than five seconds elapsed since verification started. A cloned client retains the generation; a newly constructed client or version reconfiguration requires verification again. Future binding lookups/provider mutations must obtain fresh verification for each operation and recheck immediately before use. This observed context is not a financial-operation capability, proof of local ownership, or guarantee that a remote credential/proxy has not changed after the reads. Existing legacy billing trait methods are not gated by this API.

Managed verification uses the companion's signed JSON `POST /payments/context` endpoint. Its account/mode/mapping-generation/API-version snapshot and opaque token are bound to the configured instance, immutable platform credential/API base and five-second lifetime. `retrieve_checkout_in_context` submits that token to `/payments/context/checkout-read`; `retrieve_payment_link_in_context` uses `/payments/context/payment-link-read`; the platform pins the same account, rechecks mode/generation/expiry, and returns a matching scope envelope. Native code checks its client generation and elapsed time again. Old gateways without the protocol fail closed. The raw Stripe allowlist still excludes account and balance: those are internal verification reads only. The companion generation migration is required; credential/base rotation and account remapping invalidate outstanding contexts. These read observations grant no financial mutation capability.

Provider definitions: [account retrieval](https://docs.stripe.com/api/accounts/retrieve), [balance retrieval](https://docs.stripe.com/api/balance/balance_retrieve), and [balance mode](https://docs.stripe.com/api/balance/balance_object). The repository's pinned official Stripe SDK uses `/v1/account` for `accounts.retrieve()` without an ID, and `/v1/balance` for `balance.retrieve()`; the owned SDK/native oracle checks both requests.

With `postgres-identity`, `StripeBillingService<PostgresBillingStore>::get_bound_checkout_session` is an opt-in durable read by public UUID. It verifies direct or managed account/mode, resolves an explicitly provisioned tenant/deployment/account/mode checkout binding, retrieves only its provider reference, then joins the exact binding generation to the encrypted durable row and audits decryption. Freshness is checked after network and database work. Provider object, ID, subscription mode, live mode and customer must match; the customer must match the durable local customer relation. Expanded customers require the customer object type and an absent or false boolean deletion flag. A replaced alias, missing ownership, malformed evidence or unavailable audit fails closed. The returned DTO uses the public UUID and fresh provider fields; storage keys and ciphertext remain unchanged.

This method requires existing administrative durable bindings and the base billing grants, including sensitive-read audit INSERT. Its joined record/binding query provides a statement snapshot, not a lock or capability for later financial operations. It performs no provider mutation, automatic backfill, new-record UUID migration or HTTP route wiring; cache-only and existing trait reads remain separate. Portal reads and mutation integration remain unsupported by this opt-in read; the separate payment-link API is described below.

`get_bound_payment_link` adds an opt-in direct or managed durable payment-link read under `postgres-identity`. It uses the same explicitly provisioned tenant/deployment/account/mode alias and exact-generation audited encrypted-row read, and verifies `payment_link` object/ID/live mode under a fresh context. It returns the public UUID and fresh provider fields while preserving local amount/status and storage ciphertext. Payment links have no configured local customer relation to compare. Managed clients require the dedicated signed `/payments/context/payment-link-read` endpoint and exact matching scope envelope. Old gateways fail closed. The default service still uses provider IDs; this API does not automatically convert existing callers, provision aliases, add HTTP routes, or authorize mutations. Context expiry after database waits also denies the result.

With `postgres-reconciliation`, `acknowledge_already_paused_effect` is an explicit operator workflow for `customer.subscription.paused`. It verifies the configured account/mode, retrieves the exact pending event and current subscription with authenticated GET requests, and requires both provider objects to be paused with the exact sealed binding/customer identity. The owned local subscription must already have `active=false`; exact row/binding versions and pending-attempt identity protect the atomic audit/receipt update. No subscription/provider write, event replay, financial action, or public route is added. Managed transport uses the dedicated signed `/payments/context/paused-subscription-read` endpoint, which retrieves the event and subscription under the same pinned account/API version and checks binding generation and expiry after every await. Old gateways fail closed. The returned scope must exactly match the verified context. An event `account` may be absent/null under this authenticated retrieval scope; when present it must match. Non-null event `context` is unsupported. This is authenticated GET evidence, not proof of the original webhook HMAC or a causal pause epoch.

Authenticated GET evidence is not verification of the original webhook HMAC. Stripe exposes no pause-generation token: matching old-event/current paused state does not prove an uninterrupted pause or that the interrupted callback caused the local state. This API only acknowledges an already-satisfied effect. Event creation must precede the stored attempt and inspection time. [Stripe event retrieval](https://docs.stripe.com/api/events/retrieve) supports events created within the last 30 days; older/unavailable events, missing bindings, changed customers and contradictory state remain manual review. Paused status is distinct from merely pausing collection.

The same immutable client context has a five-second verification window checked after provider and database awaits, including immediately before COMMIT. A late post-COMMIT check returns `RecordedEvidenceNoLongerCurrent`, explicitly recording that a receipt was committed. Actual COMMIT timing is not guaranteed; timeout/cancellation/COMMIT errors require the store's read-only receipt query. See the base persistence guide for additional explicit trusted-operator locking/audit permissions.


## Verified direct creation

Enable `postgres-identity` for the explicit creation APIs. They retain public UUIDs separately from Stripe
references. `create_bound_plan` verifies an approved Product and journals one Plan
creation. `register_verified_stripe_customer` verifies Customer existence/account/
mode and records a separately server-authorized local Customer/Party association.
Neither email equality nor provider metadata proves the business association.
Party authorization and lifecycle in another service remain application-owned.

`create_bound_subscription` requires that Customer registration, a completed
unchanged bound Plan creation, an explicit operation UUID and a trusted profile:

```rust,ignore
use forklaunch_billing_stripe::bound_subscription_creation::{
    BoundSubscriptionCreateRequest, BoundSubscriptionCreationProfile,
    SubscriptionCollectionPolicy,
};

let profile = BoundSubscriptionCreationProfile::trusted(
    scope,
    vec![(local_plan_id, completed_plan_operation_id)],
    SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
)?;
let publisher = store.trusted_subscription_publisher(publisher_pool).await?;
let outcome = service.create_bound_subscription(
    &profile,
    &publisher,
    operation_id,
    BoundSubscriptionCreateRequest {
        local_customer_id,
        local_plan_id,
        quantity: 1,
        description: None,
    },
).await?;
```

Apply the billing migrations through11 using the migration owner. Provision
separate non-owner/non-BYPASSRLS runtime and publisher credentials; runtime must
not inherit publisher or migration owner. Explicitly grant the publisher the
publication-function permission and required base-store access. The constructor
checks role separation and matching configured/observed database identity; it
never falls back to runtime credentials. Publisher credentials are trusted server
authority and must stay outside request data. The SQL function and native caller
share one transaction, including post-await freshness checks before COMMIT.

The finite collection policy selects `charge_automatically` and
`default_incomplete`, with one verified licensed/per-unit Plan. Fresh exact
Customer/Plan and Subscription evidence must match. `active` means observed
subscription service state; it does not establish a paid invoice or charged total.
The profile rejects unsupported trial, discount, tax, delegation and ambiguous
item/evidence states. It adds no default HTTP route or managed creation support.

Confirmed reservation holds durable claims on Customer/Plan bodies and binding
generations before the single provider POST. Successful publication writes UUID,
ownership binding, audit and encrypted receipt before releasing those claims.
`Unresolved` retains the operation for explicit inspection and never retries the
provider write. `RecordedEvidenceNoLongerCurrent` means a receipt was committed
before evidence expired. `inspect_bound_subscription_creation` returns historical
pending/completed state without dispatch authority or a current-payment claim.
For an interrupted pending operation, call
`recover_bound_subscription(&profile, &publisher, operation_id, event_id)` with
an exact `customer.subscription.created` event ID. Recovery performs provider
GETs only and does not reconstruct dispatch authority. It requires the original
server-generated request key, unchanged local dependencies and trusted profile,
matching event rendering API version/account/mode, and fresh exact Customer,
Plan and Subscription evidence. It admits only creation timestamps within the
explicit reservation window (-5 seconds/+5 minutes), and rejects status regression
from active to incomplete. This restricted window is application policy, not a
Stripe guarantee. Key linkage identifies a request, not its originating process.
Missing/automatic event request keys, old/incompatible event versions or ambiguous
evidence require manual review; no POST retry, refund or claim release occurs.
`inspect_subscription_recovery_evidence` returns the encrypted historical event
receipt bound to the operation, public UUID and payload hash. It grants no new
provider authority. General recovery, managed creation and ownership-aware
mutations remain separate work.

## Direct bound Portal creation

With `postgres-identity`, `BoundPortalCreationProfile::trusted` fixes an explicit
Portal configuration ID and HTTPS return URL. The trusted application separately
authorizes its local Customer/Party to access that configuration.
`create_bound_portal_session` verifies direct account/mode, registered Customer
and active configuration before reserving one immutable encrypted intent and
server idempotency key. It sends one exact session POST, verifies customer/config/
mode/return URL and atomically publishes a local UUID record, provider binding and
encrypted receipt through a separately credentialed `TrustedPortalPublisher`.

Apply additive billing migrations12/13 with the same migration owner as the
existing private dependency guards. Runtime must neither inherit the publisher
role nor execute `publish_bound_portal`. Customer record/binding claims survive
uncertain dispatch or COMMIT and block mutation until verified publication;
Subscription completion cannot release Portal claims.
`inspect_bound_portal_creation` returns pending/completed IDs without a bearer
URL or reusable dispatch key. A completed duplicate never returns an old URL as
fresh. Retain the operation ID and inspect uncertain outcomes; there is no
automatic POST retry or operator force-release in this profile.

Configuration GET is an observation, not a lock on remote configuration changes.
The five-minute local expiry is application policy, not a Stripe URL lifetime
guarantee. A Portal session grants access to configured features; it proves
neither payment nor entitlement. Managed transport, arbitrary flow/delegation
options and automatic default HTTP route wiring remain separate work.

## Direct bound Checkout creation

`BoundCheckoutCreationProfile` authorizes completed local Plan provenance and
HTTPS success/cancel URLs. `BoundCheckoutCreateRequest` selects local Customer/
Plan UUIDs and quantity. The application authorizes Customer access; no arbitrary
provider options or caller-owned provider ID are forwarded.
`create_bound_checkout_session` verifies direct account/mode, registered Customer,
expanded Plan/Product and an independently retrieved Price. Stripe's Price
retrieval accepts Plan IDs; the returned Price must exactly match the completed
Plan receipt. Four durable Customer/Plan record/binding claims precede one hosted
subscription/card session POST. A bounded line-items GET must return exactly the
verified Price, quantity and undiscounted/untaxed totals.

Billing migrations14/15 add separate immutable Checkout intent/claims and
`TrustedCheckoutPublisher` publication, with local UUID, encrypted record/receipt
and provider binding in one transaction. Runtime cannot complete/release claims;
Portal/Subscription publication cannot release Checkout claims. Uncertain dispatch
or COMMIT stays inspectable without automatic POST retry. Historical inspection
and completion return IDs without a bearer URL. Provider `expires_at` is retained;
it is not replaced by a local five-minute TTL.

This profile admits open/unpaid sessions with exact finite totals. A zero-total
`no_payment_required` or already-completed session remains unresolved. Remote
reads are observations, not locks on later changes. Proposed-session totals prove
neither later payment nor subscription entitlement. Generic routes and managed
transport remain separate; the durable APIs require explicit server wiring.

For Checkout sessions published by the direct bound creation profile, use
`get_bound_created_checkout_session`. It preserves the local Customer UUID while
verifying the provider customer through the completed creation receipt and current
Customer registration/body generation. It requires the original actor and explicit
SELECT access to `checkout_creation_intents` plus existing binding/record/audit
permissions. It rejects changed records, registrations and provider customers and
rechecks context freshness after database work. It is direct-only and grants no
mutation authority. The existing `get_bound_checkout_session` retains its legacy
provider-customer representation and grant requirements; generated default routes
still require separate explicit profile wiring.
