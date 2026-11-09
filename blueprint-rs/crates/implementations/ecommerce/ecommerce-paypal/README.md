# PayPal scoped order inspection and reconciliation

`PaypalClient::for_environment(client_id, secret, environment)` pins PayPal's
fixed Sandbox/Live HTTPS origin. Legacy `new`/`with_transport` remain unprofiled
for scoped reads. `with_trusted_environment_transport` is an explicit trusted
proxy/fixture boundary: the caller attests environment. Never take a transport,
environment or merchant expectation from an untrusted request.

`verify_order_context(environment, merchant_id, order_id)` performs OAuth
client-credentials authentication then one exact order GET. It requires a single
CAPTURE purchase unit with the exact PayPal merchant ID, reference, canonical
currency and positive amount. Missing/email-only payee is rejected. OAuth
`app_id` is not merchant identity; environment is constructor configuration,
not a response livemode attestation. The opaque observation belongs to one
immutable client generation and expires five seconds after authentication starts.
It returns redacted order fields, with no reusable token or raw payer data.
COMPLETED alone proves neither capture nor settlement/refund/dispute absence.

With `postgres`, `PaypalPaymentService::inspect_bound_order(scope, payment_id)`
uses an existing administratively provisioned tenant binding. It checks exact
provider/merchant/reference/amount/currency and rechecks the complete binding
and current local relation after provider I/O, then freshness after database
waits. This API makes no local writes, requires no new migration and performs
no capture/refund/patch/order transition. OAuth POST is authentication only.
It grants no financial repair or generic merchant-credential authority; broader
captured/cancelled reconciliation needs a separate proof and atomic receipt.

Primary contracts: [authentication](https://developer.paypal.com/api/rest/authentication),
[request environments](https://developer.paypal.com/api/make-api-requests/),
[payee](https://developer.paypal.com/sdk/orders/v2/definitions/payee/) and
[pinned Orders schema](https://developer.paypal.com/api/orders/v2/schema.json).
Owned schema/wire and restricted PostgreSQL fixtures validate this boundary;
no live PayPal deployment is claimed.

With `postgres`, `reconcile_bound_voided_order_payment` offers explicit operator
repair for a nonsucceeded payment whose local order is already cancelled. It
requires the authenticated exact order/payee and explicitly present empty
captures, authorizations and refunds arrays; omitted or nonempty histories stay
manual. Payment, order version and full binding generation are checked under
locks. Migration12 adds tenant-scoped, unique provider-scope receipts; payment
failure, cancellation audit and receipt commit atomically. Retain the receipt ID
before calling and inspect it after an ambiguous commit. References and order
state are preserved; no provider financial request or cancellation timestamp is
invented. This is operator bookkeeping from fresh evidence, not settlement proof.
