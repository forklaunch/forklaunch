# Billing HTTP and MCP profiles

The `http` feature provides the handlers generated billing modules bind:
`BillingHttp::handler` returns one operation's handler and `BillingRoutes`
mounts the routed operations with the backend's authentication and formats.
`BillingHttp::router` mounts every operation with its packaged handler.

The default `BillingBackend::base` and `BillingBackend::stripe` retain the existing
billing routes and identifier behavior. Request date normalization is separately
selected with `BillingRequestDateProfile`.

## BoundCheckoutV1

The optional `bound-checkout` feature provides three additional internal,
request-HMAC-authenticated operations. HTTP and MCP use the same handlers and
`bound_checkout_contracts()` registry; generated contract/OpenAPI/SDK artifacts
select that same registry. Existing billing routes remain unchanged.

| Operation | HTTP route | Meaning |
| --- | --- | --- |
| `createBoundCheckoutSession` | POST `/bound-checkout/operations/:operationId` | One guarded creation attempt with visible caller operation UUID |
| `getBoundCreatedCheckoutSession` | GET `/bound-checkout/sessions/:id` | Provenance-aware read by published local UUID |
| `inspectBoundCheckoutOperation` | GET `/bound-checkout/operations/:operationId` | Actor-owned historical IDs/state; no URL or dispatch capability |

The create body has exactly `localCustomerId`, `localPlanId`, and `quantity`.
Both IDs are non-nil UUIDs; quantity is an integer from 1 through
9,007,199,254,740,991. No caller `stripeFields`, provider ID, redirect, account,
fee, or delegation option is accepted. The Customer must already have a trusted
registered provider relation; the Plan must have a completed bound creation
operation authorized by server configuration. Neither cache nor mutable provider
fields can establish these relations. An authenticated internal service key is
trusted to select business records within the configured tenant; this is not
per-end-user authorization or verification of an external IAM Party lifecycle.

For generated Stripe billing services, explicitly set all three variables:

- `BILLING_IDENTITY_PROFILE=bound-checkout-v1`
- `BILLING_BOUND_CHECKOUT_CONFIG` to the finite JSON configuration below
- `BILLING_PUBLISHER_DATABASE_URL` to a distinct, explicitly trusted publisher role

```json
{
  "deploymentLabel": "production",
  "accountId": "acct_CONFIGURED",
  "livemode": true,
  "plans": [{"localPlanId": "LOCAL-PLAN-UUID", "creationOperationId": "COMPLETED-PLAN-OPERATION-UUID"}],
  "successUrl": "https://your-app.example/success",
  "cancelUrl": "https://your-app.example/cancel"
}
```

The UUID placeholders must be replaced with real provisioned UUIDs. Configuration
is bounded to 64 KiB, rejects unknown fields, and is not included in errors or
SDK metadata. `TENANT_ID` remains the fixed server tenant. Partial configuration,
unknown profiles, base/managed transports, and `BILLING_DATABASE_BACKUP=true`
fail startup. Legacy mode rejects stray bound-profile settings. The generated
library retains its legacy registry APIs and additionally exports
`contract_registry_with_profiles` for explicit artifact selection.

Library applications construct `BoundCheckoutConfig`, an explicitly validated
`TrustedCheckoutPublisher`, and `BoundCheckoutV1::trusted`, then await
`BillingBackend::with_bound_checkout` **after** selecting backup settings.
Publisher actor derivation retains the original validated pools and role/database
boundary. Every request derives its actor from the verified HMAC principal and
validates publication against that request's runtime store. Never expose actor
selection or publisher credentials to request DTOs.

Apply billing migrations through 15 with a migration role. Runtime and publisher
must be distinct non-superuser, non-BYPASSRLS roles; runtime must neither inherit
the publisher/migration owner nor execute `publish_bound_checkout`. The publisher
is trusted authority and needs explicit EXECUTE on that publication function.
Follow the billing base/Stripe creation-journal grant requirements for records,
Customer/Plan bindings, Checkout intents/claims, and audit tables. Runtime needs
journal SELECT for reads/inspection and the documented reservation privileges;
read audits must be writable. Private dependency guards and completed-transition
RLS remain enforced even with broader table grants. No automatic grants or
migrations run at startup.

A fresh successful creation returns HTTP 200 with `state: "created"`, operation
and public UUIDs, and the DTO. A completed duplicate returns HTTP 200 with
`state: "completed"` and IDs only. Unknown dispatch/COMMIT and expired post-write
observations return HTTP 202 with the operation UUID and explicit pending or
`recordedEvidenceNoLongerCurrent` state. Inspect before deciding further action;
a missing receipt never authorizes provider retry. Existing pending operations
retain dependency claims and never automatically dispatch again. Inspection does
not return a historical URL as if refreshed. The separate GET returns the stored
DTO with current verified provider fields and its original expiry, not a newly
issued session URL.

Creation is neither payment nor entitlement proof. Provider observations are
bounded in time and do not lock remote configuration. The finite direct-only
creation profile and its separate publisher transaction retain their existing
ambiguity/recovery boundaries. This namespace does not switch all five billing
families to configured TypeScript parity and adds no financial mutation alias
resolution, account-wide list scanning, or automatic legacy data migration.

## BoundBillingV1 aggregate

Select `BILLING_IDENTITY_PROFILE=bound-billing-v1` and supply
`BILLING_BOUND_BILLING_CONFIG` with one shared scope and explicitly selected domains:

```json
{
  "deploymentLabel": "production",
  "accountId": "acct_CONFIGURED",
  "livemode": true,
  "checkout": {
    "plans": [{"localPlanId": "LOCAL-PLAN-UUID", "creationOperationId": "COMPLETED-PLAN-OPERATION-UUID"}],
    "successUrl": "https://your-app.example/success",
    "cancelUrl": "https://your-app.example/cancel"
  },
  "portal": {
    "configurationId": "bpc_CONFIGURED",
    "returnUrl": "https://your-app.example/account"
  }
}
```

At least one of `checkout`, `portal`, `planRead`, `subscriptionRead`, and `paymentLinkRead` is required; omit an unselected domain.
Null, empty, unknown, or partial sections fail. The shared scope is server-owned;
no DTO may override it. When `checkout` or `portal` is selected,
`BILLING_PUBLISHER_DATABASE_URL` is the single explicit separate publisher
connection. Its role must have each selected
publication function grant, independently validated before listening. Mixed old
`BILLING_BOUND_CHECKOUT_CONFIG` and aggregate configuration fail. The existing
`bound-checkout-v1` spelling, its configuration, and legacy registry APIs remain
supported; they do not select Portal implicitly.

Library callers construct `BoundBillingConfig` and `BoundBillingV1::trusted`
with exactly the selected creation publisher capabilities (none for `planRead`, `subscriptionRead`, or `paymentLinkRead`), then call
`with_bound_billing`. Artifact callers use `contract_registry_with_bound_billing`
with the same configuration; a selector alone cannot guess selected families.
The existing optional `bound-checkout` Cargo feature also enables this additive
aggregate. No new dependency, migration, or automatic publisher grant is needed.

Portal adds `createBoundPortalSession` (POST) and `inspectBoundPortalOperation`
(GET), both at `/bound-portal/operations/:operationId`. The body contains exactly
`localCustomerId`, a registered local Customer UUID. The server pins configuration
and return URL. HTTP, MCP, and generated SDKs share these contracts and the
same trusted service-key/tenant authority boundary described above. Creation uses
the existing Portal journal, customer claims, direct provider validation, and
separate trusted publisher. Fresh creation may return a URL; completed duplicates
and inspection return IDs only. Uncertain effects remain pending, without retry
or claim release. These routes confer no payment or entitlement proof.

`getStoredBoundPortalSession` adds GET `/bound-portal/sessions/:publicId` under
the same authenticated actor and server policy. It returns
`{state: "stored", observedAt, record}` without provider GET or POST. This is the
original stored DTO, not a new or refreshed session. An actor-owned completed
receipt, matching current Portal record and alias, and unchanged registered
Customer body/binding are all required. No generic cache or mutable provider
field can establish authority. Changed server configuration, scope/API version,
missing provenance, and managed transport fail closed.

The sealed original `expiresAt` must be exactly five minutes after its original
`createdAt`. Reads reject future creation times, backward clocks, and expiry,
including expiry during audit/database waits; expired reads return HTTP 410
without a URL. No TTL is extended. The data snapshot linearizes at the final
matching SELECT after audit waits. `observedAt` records the subsequent completion-
clock TTL check after audit COMMIT; it does not mean data remained unchanged
through COMMIT or afterward. Current alias generation is checked across this
read, without claiming the alias was never administratively rebound historically.

The stored URL may already be invalid remotely. Its local five-minute policy
proves neither remote validity nor payment/entitlement state. Creation duplicates
and operation inspection still return IDs only, even when a separate explicit
stored-read operation is permitted. This preserves that separation while adding
an authorized bounded form of the configured TypeScript cache reread. Subscription and PaymentLink route integration
remain separate gaps; Plan has only the explicitly selected read capability below.


### Explicit Plan provenance reads without a publisher

Set `BILLING_IDENTITY_PROFILE=bound-billing-v1` with, for example:

```json
{"deploymentLabel":"your-deployment","accountId":"acct_your_account","livemode":false,"planRead":{"trustedProducts":["your-product-id"]}}
```

`planRead` requires a nonempty bounded product allowlist and adds only
`getBoundCreatedPlan`, GET `/bound-plans/:publicId`. It does not select Plan
creation, mutation, listing, or publisher authority. A Plan-only selection rejects
`BILLING_PUBLISHER_DATABASE_URL`; combining it with Checkout or Portal creation
requires that connection and the corresponding separate publication grants.
Unknown, null, partial, or mixed old/new configuration fails before listening.

Startup checks the local direct Stripe client configuration, schema USAGE,
SELECT on `fl_billing.records`, `provider_bindings`, and `plan_creation_intents`,
and INSERT on `sensitive_read_audit`. It makes no provider request and neither
creates nor assumes publication privileges. Missing permissions, base/managed
transport, or backup fallback fail. Revoked grants after startup still fail reads.
HTTP/MCP and generated artifacts/both SDKs use the same selected registry.

The getter requires the authenticated actor's completed direct Plan journal,
the original normalized product allowlist, and an unchanged current durable
Plan DTO/alias. It verifies account/mode and expanded Plan/Product identity and
terms, then returns the original public UUID/local fields with fresh Product
presentation in `stripeFields`. No mutable `externalId`, cache entry, or raw
administrative alias supplies missing provenance. Only read audits are written;
no provider POST, retry, or compensation occurs. Data linearizes at the final
matching database SELECT; provider context expiry is rechecked after audit
COMMIT. Later remote/local changes remain possible, and no payment or
entitlement claim follows.

The existing 66 legacy routes remain available unchanged: selecting only this
new read capability does **not** make the entire generated service read-only.
Its internal HMAC service key/tenant authority is the same existing boundary,
not new end-user or cross-service IAM ownership proof.


## Direct Subscription provenance read

The same `bound-billing-v1` aggregate accepts an explicit read capability:

```json
{"deploymentLabel":"your-deployment","accountId":"acct_your_account","livemode":false,"subscriptionRead":{"plans":[{"localPlanId":"11111111-1111-4111-8111-111111111111","creationOperationId":"22222222-2222-4222-8222-222222222222"}],"collectionPolicy":"automatic_default_incomplete_v1"}}
```

This adds `getBoundCreatedSubscription`, `GET /bound-subscriptions/:publicId`,
through the shared HTTP/MCP/artifact registry. Read-only selections require no
publisher connection and reject a stray publisher setting; mixed creation selections
retain their separate publisher requirements. Startup validates direct transport
and local schema usage, SELECT on records, provider bindings, Customer bindings,
Subscription and Plan creation journals, and INSERT on sensitive-read audit. Startup
makes no provider requests. This does not remove the existing 66 legacy routes or
make the whole service read-only.

The authenticated actor must own the completed Subscription receipt. The trusted
Plan catalog may have a different actor. Current encrypted Customer/Plan records,
bindings, and completed Plan provenance must match sealed creation dependencies;
external Party association remains an administrative assertion, not IAM ownership
proof. Fresh authenticated Customer, Plan, and Subscription reads must match the
finite original terms. Only unchanged active/incomplete or incomplete-to-active
states qualify; other provider changes require another workflow. The response
refreshes `stripeFields` only: local UUID, lifecycle, party and product fields remain
unchanged. There is no payment or entitlement claim, provider POST, creation alias
fallback, or automatic lifecycle synchronization. The data snapshot linearizes at
the final matching database SELECT; context freshness is checked after audit commit.
Later local or provider changes remain possible.


### Direct completed PaymentLink reads without a publisher

An explicit `paymentLinkRead` capability uses the same aggregate selector:

```json
{"deploymentLabel":"your-deployment","accountId":"acct_your_account","livemode":false,"paymentLinkRead":{"trustedPrices":["price_your_reference"]}}
```

It adds `getBoundCreatedPaymentLink`, GET `/bound-payment-links/:publicId` to the
shared HTTP/MCP/artifact registry. Read-only selections reject a publisher setting;
mixed creation selections still require their existing separate publisher grants.
Startup checks direct local Stripe configuration, schema USAGE, SELECT on records,
provider bindings and payment-link creation intents, and sensitive-read audit INSERT.
No provider request occurs during startup. Managed/base/backup transports, partial
or unknown options, and absent grants fail before listening.

The authenticated actor must own a completed direct creation journal. The original
normalized trusted price set, current local receipt/body and alias scope/generation
must match. Administrative-only aliases and managed creation envelopes do not qualify.
An authenticated exact id/kind/mode provider read refreshes `stripeFields`; local
amount/status and other fields stay unchanged, even when provider `active` is false.
No line-item, price, paid/entitlement, cache, creation, or mutation authority is inferred.
Read evidence is bounded and provider freshness is checked after the final audit commit;
data linearizes at the last matching SELECT and can change afterward. The existing
66 legacy routes remain available unchanged; this is not whole-service read-only mode.

The same selector also adds `listBoundCreatedPaymentLinks`, GET
`/bound-payment-links?ids=<JSON-array-of-UUIDs>`. `ids` is required and nonempty;
unknown or repeated query keys are rejected. The contract/MCP/TypeScript SDK input
is an array; the generated TypeScript SDK encodes it as one JSON query value. The
current Rust SDK query map requires an explicitly serialized JSON array. This is
the native generated-client convention, not a claim of repeated-key TypeScript
controller wire parity. Existing HMAC authentication signs the path, not query contents.

Optional trusted `paymentLinkRead.maxRecords` defaults to 100 and accepts 1..1000;
callers cannot override it. Metadata projects the configured maximum. Every selected
completed local proof is checked before provider I/O, retaining input order and
duplicates. Only provider `active: true` records are returned; missing/nonboolean
active is an error, and local amount/status are unchanged. The whole remote phase
shares one five-second context. Evidence and result aggregates each have an 8 MiB
limit. Omitted/empty selection enumeration, account scans and pagination are unsupported.
Each original proof is reobserved after I/O; this is not an atomic multi-record snapshot.

### Explicit direct PaymentLink creation

`bound-billing-v1` can separately select
`"paymentLink":{"trustedPrices":["price_your_reference"]}` under the common
trusted deployment/account/mode scope. `paymentLinkRead` does not enable creation.
This domain uses its existing ordinary-role immutable creation journal, not a
separate publisher. A publisher URL is rejected unless another selected domain
requires it. Startup requires schema USAGE, INSERT on records/provider bindings,
SELECT/INSERT/UPDATE on payment-link creation intents, and INSERT on creation and
binding audits. No provider call occurs during startup. Creation alone does not
require record/binding SELECT or sensitive-read audit INSERT; selecting reads adds
their grants separately. Base, backup and managed transports fail closed.

POST `/bound-payment-links` (`createBoundPaymentLink`) accepts exactly operationId,
amount, currency, paymentMethods and lineItems (`price`, `quantity`). The operation
UUID is caller-visible and bound to the authenticated actor, tenant, profile and
canonical request. Prices must belong to the server allowlist; currency is three
uppercase letters, paymentMethods has 1..32 safe identifiers, and lineItems has
1..20 items with integral quantity 1..999999. No supplied public ID, raw provider
options, fees, transfer/delegation or status is accepted. The server sets PENDING.
Amount is a declared local value, not independently verified price or charged total.

Successful publication or a completed duplicate returns HTTP200 with only state,
operationId and publicId. HTTP202 `unresolved` or `recordedEvidenceNoLongerCurrent`
may already have provider/local effects; it is not rollback proof or retry authority.
GET `/bound-payment-link-operations/:operationId` returns actor-owned historical
pending/completed status and IDs only, or404. Inspection never revives a URL or
provider DTO, proves current ownership/validity, or dispatches another POST.
Changed payload/actor conflicts; pending repeats do not redispatch. Existing66
legacy routes remain available; this explicit selector is not full default mapper parity.

### Explicit direct Plan creation

`bound-billing-v1` separately selects `"plan":{"trustedProducts":["your_product"]}`
under the common trusted deployment/account/mode scope. `planRead` does not enable
creation; creation does not enable reads. The ordinary runtime role needs schema
USAGE, record/binding INSERT, plan_creation_intents SELECT/INSERT/UPDATE and
plan_creation_audit/provider_binding_audit INSERT, checked individually before
listening without provider I/O. No separate publisher is used; a publisher URL is
rejected unless another selected creation domain requires it. Base, backup and
managed transports fail closed.

POST `/bound-plans` (`createBoundPlan`) accepts a visible operationId UUID, name,
active, unitAmount, currency, cadence and productId; description/features are
optional and nullable. unitAmount is an integral minor-unit amount from zero to
9007199254740991; currency is three uppercase letters; cadence is day/week/month/year.
The product reference must belong to the server allowlist and is independently
read under the pinned authenticated provider account/mode before the one Plan POST.
The finite per-unit, licensed, single-interval profile verifies its exact readback.
There is no caller public ID, externalId, raw Stripe options, nested product creation,
delegated ownership, payment success or entitlement claim.

Confirmed publication or a completed duplicate returns HTTP200 IDs only
(state, operationId, publicId). HTTP202 unresolved/recordedEvidenceNoLongerCurrent
can have provider/local effects; inspect rather than retrying or assuming rollback.
GET `/bound-plan-operations/:operationId` (`inspectBoundPlanOperation`) returns only
actor-owned historical pending/completed status and IDs, or404. It performs no
provider calls and provides neither current ownership proof nor redispatch authority.
Changed actor/request conflicts; pending repeats do not issue another POST.
Existing 66 legacy routes remain unchanged and available, so this is an explicit
capability rather than a whole-service restricted profile or full default TS parity.

### Explicit direct Subscription creation

`bound-billing-v1` separately selects `subscription` with exactly
`authority: "internal_service_tenant_v1"`, `collectionPolicy:
"automatic_default_incomplete_v1"` and trusted `plans` entries containing
`localPlanId`/`creationOperationId`. This authorizes an authenticated internal
service to subscribe registered Customers within this configured tenant; it does
not establish end-user consent, external IAM Party ownership or its current lifecycle.
Customer association is an explicit administrative business assertion, corroborated
by exact authenticated provider Customer reads. `subscriptionRead` alone grants no
creation capability. The server-selected Plan must have completed creation provenance.

A separate validated `TrustedSubscriptionPublisher` is mandatory. The additive
`BoundBillingV1::trusted_with_subscription` constructor accepts it; the original
`trusted(config, checkout, portal)` remains compatible and rejects a selected
Subscription creation capability without its publisher. Generated startup uses the
explicit `BILLING_PUBLISHER_DATABASE_URL`, never the runtime pool as a fallback.
Both actor-scoped store and publisher wrappers retain their validated tenant/pools,
and the publisher checks the same authenticated actor before dispatch/publication.
Startup checks direct transport, role/database separation, function execution and
all local dependency/read/reservation/audit grants, with no provider I/O. Runtime
must not inherit the publisher role or execute publication itself. Base, managed,
backup and incomplete configuration fail before listening.

POST `/bound-subscriptions` (`createBoundSubscription`) accepts exactly operationId,
localCustomerId, localPlanId, quantity (integral 1..9007199254740991) and optional
nullable description (at most 500 UTF-8 bytes, no NUL). It reserves four immutable
dependency claims before one provider POST. The fixed automatic/default-incomplete
profile and exact Customer/Plan/Subscription evidence retain the library's limitations;
creation or active status does not prove payment, invoice total or entitlement.
There is no arbitrary provider-option forwarding, reassignment, retry or recovery route.

Confirmed/historical completion returns HTTP200 IDs only (state, operationId,
publicId); HTTP202 unresolved/recordedEvidenceNoLongerCurrent can already have
provider/local effects. GET `/bound-subscription-operations/:operationId`
(`inspectBoundSubscriptionOperation`) returns only actor-owned historical status
and IDs, or404; it supplies neither current ownership evidence nor redispatch
capability. Pending operations retain their dependency claims until trusted
validated publication. Existing 66 legacy routes remain unchanged and available.

### Explicit interrupted Subscription recovery

`subscriptionRecovery` is a separate aggregate selector with the same trusted
plans, fixed collectionPolicy and explicit `authority: "internal_service_tenant_v1"`.
It requires a separate Subscription publisher but does not enable creation. If
creation and recovery are both selected, their one supplied publisher is used for
both capabilities and the common inspection route appears once. Recovery-only
runtime grants require dependency SELECT/UPDATE locks, intent and claims SELECT,
and read/creation audit INSERT; neither intent nor claims INSERT is needed.
The publisher's existing role separation and dependency grants remain mandatory.
Startup checks local configuration/grants without contacting the provider.

POST `/bound-subscription-operations/:operationId/recover`
(`recoverBoundSubscriptionOperation`) accepts exactly `{ "eventId": "evt_..." }`.
It uses the existing same-actor pending journal, its four unchanged dependency
claims, an authenticated `customer.subscription.created` Event with the exact
server idempotency request key, and fresh Customer/Plan/Subscription reads.
The event must match the recorded rendering API version and restricted reservation
window (-5seconds..+5minutes), with current evidence and final deadline checks.
This is request-key linkage, not proof of process origin, payment or entitlement.
Metadata alone, absent request keys and broader ambiguous events are unsupported.

Recovery performs no provider POST and never reconstructs a dispatch ticket.
Only the separately trusted publisher can atomically publish the local UUID,
receipt/binding and release claims. HTTP200 returns IDs only; HTTP202 requires
receipt inspection because commit outcome or freshness can be uncertain.
Concurrent attempts can return a conflict/uncertainty while the one publication
succeeds. Completed repeats return historical IDs without asserting current
provider state. GET inspection is the same actor-owned IDs-only operation used by
creation; neither endpoint returns an old URL or authorizes a financial retry.
Default66 routes and generic TS behavior remain unchanged; this does not close
all interrupted billing or provider mutation parity.

### Explicit local Portal expiration

`bound-billing-v1` accepts a separate `portalLocalExpiration` selector containing
`configurationId` and `returnUrl` under the shared trusted account/mode/deployment.
It enables `POST /bound-portal/sessions/:publicId/expire` and
`GET /bound-portal/sessions/:publicId/expiration`. It requires no publisher and
implies neither creation nor stored-read authority. Existing legacy routes remain.

First expiration requires the completed actor-owned creation receipt, unchanged
Customer and alias/record proof, and the original five-minute local lifetime.
Migration 16 appends an immutable tombstone and audit in one transaction. Stored
bound reads reject it even if the mutable record is restored. The original Stripe
URL may remain usable remotely until the provider expires it: this capability
makes no provider calls and cannot revoke a URL already given to a client.

Confirmed writes and historical repeats return `localExpired`. Lost COMMIT
acknowledgements return 202 `localExpirationUnknown`; inspect or repeat the same
local operation. `notRecorded` does not prove rollback while an earlier transaction
may still be finishing. Historical status/repeats require the same actor/profile
but do not assert current Customer ownership or remote URL validity. Creation
inspection remains IDs-only and unchanged.

After migration 16, every existing stored Portal read deployment also needs SELECT
on `portal_local_expirations`; selected Portal startup checks this grant. This
adds no expiration/write permission.

Before listening, the selected expiration runtime needs SELECT on the creation journal and
expiration table, INSERT on the expiration table and sensitive-read audit, and
SELECT/UPDATE on records, provider bindings and Customer bindings for proof locks.
Use the existing distinct non-owner runtime role. Migration owners and privileged
DB administrators remain trusted; no protection against stolen runtime write
credentials is claimed. No expiration UPDATE/DELETE policy is granted, and mutation
and TRUNCATE triggers reject history removal even with broader table grants.


### Explicit PaymentLink metadata expiration

`bound-billing-v1` accepts `paymentLinkExpiration` with `trustedPrices` and
`authority: "internal_service_tenant_v1"`. This is internal service tenant
authority, not end-user authorization. Selection requires a distinct trusted
expiration publisher and enables POST `/bound-payment-links/:publicId/expire`
with `{operationId}` plus GET
`/bound-payment-links/:publicId/expiration-operations/:operationId`.
Read and creation selectors do not infer this authority. Legacy routes remain.

The provider effect is exactly `metadata.status=EXPIRED`; it neither sets
`active=false` nor revokes a provider URL. A confirmed publication blocks the
created-only bound reader/list locally. POST returns 200 `completed`, or 202
`pending`/`recordedEvidenceNoLongerCurrent` after an uncertain or stale outcome.
There is no automatic retry or recovery endpoint. Inspection performs no provider
I/O and returns IDs/status only, never the URL or provider body. A missing receipt
is not proof that the provider effect or an earlier transaction rolled back.

Migration 17 is required. Startup checks each runtime SELECT on records, bindings,
creation/expiration journals and publication identity; UPDATE on records/bindings
for proof locks; INSERT on expiration journal, dependency claims, expiration audit
and sensitive-read audit. The separate publisher requires its validated publication
function permission, schema usage, record/binding SELECT and UPDATE, and journal
and publication-identity SELECT. Role separation and same-database checks remain
mandatory. Startup makes no provider calls; incomplete, managed, or backup profiles
fail before listening. Publisher credentials are trusted administrative authority.


### Independent PaymentLink expiration recovery

`paymentLinkExpirationRecovery` selects only POST
`/bound-payment-links/:publicId/expiration-operations/:operationId/recover`
with exactly `{eventId}` and the shared IDs-only inspection GET. It requires
`trustedPrices` and `authority: "internal_service_tenant_v1"`, plus the distinct
expiration publisher. It does not select the expiration/provider-update route.
If both selectors are enabled, their normalized trusted-price sets must agree;
the shared inspection route has one unambiguous policy.

Recovery-only startup requires the existing provenance reads and audit writes,
record/binding UPDATE permission for locks, and SELECT on expiration dependency
claims. It does not require INSERT on expiration intents or claims. Publisher
validation remains identical. Startup performs no provider I/O. The library uses
only authenticated Event/current PaymentLink GETs, exact request-key linkage and
the original claims; it cannot recreate a dispatch ticket or send another POST.
The bounded event-version/time admission policy is not process-origin causality
proof. Responses are 200 historical/completed or 202 pending/stale bookkeeping;
IDs-only inspection performs no provider I/O. No response proves `active=false`,
URL revocation, payment state or entitlement. Existing legacy routes remain.
