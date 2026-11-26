# Stripe ecommerce scoped inspection

`StripeClient::verify_direct_context(account_id, livemode)` authenticates the direct account with GET `/v1/account` and the key mode with GET `/v1/balance`. Expectations must come from trusted server configuration. It never infers mode from a key prefix. The sealed observation belongs to one immutable client generation and expires five seconds after verification starts. Cloning the client preserves that generation; constructing another client does not. Verification and intent reads use the existing bounded HTTP transport, and reject stale observations after awaited provider calls. Restricted keys lacking read permissions fail closed.

With the `postgres` feature, `StripePaymentService::inspect_bound_payment_intent(scope, payment_id)` resolves an explicitly provisioned tenant-bound administrative binding, retrieves the intent, checks exact object ID/type/mode/amount/canonical lowercase currency/order metadata, then rechecks the full binding including its generation and the verification deadline after the final database read. It rejects services configured with Connect before provider I/O. The result contains only reference, status, amount, currency and mode; no client secret or raw provider/customer/address data is returned. The low-level `retrieve_intent_in_context` method returns sensitive raw provider data to trusted callers and must not be logged or persisted indiscriminately.

This opt-in inspection has no public HTTP route and performs no provider or local payment/order writes. Administrative binding provisioning remains an operator assertion, not provider proof by itself. The authenticated read validates the configured account and the bound object under that account; it does not prove that the operator assigned the right tenant or grant authority to move funds. No cancellation, refund, capture, order transition, automatic reference clearing, or cancelled-order repair follows from the returned status. Existing reconciliation APIs retain their separate evidence requirements and do not automatically use this context. Managed gateway context integration remains separate work; PayPal has its own [scoped order context](../ecommerce-paypal/README.md), and Connect uses the separate explicit APIs below.

Freshness is an observation window, not a lock on Stripe or a guarantee about state after the final local check. Account settings/default API versions and provider state can change outside the process. Existing client credentials and transport configuration are immutable locally; remote rotations or trusted proxy reconfiguration are not a remote generation protocol. The direct client uses the account's configured default API version, as its existing ecommerce calls do.

Provider contracts: [retrieve account](https://docs.stripe.com/api/accounts/retrieve), [retrieve balance](https://docs.stripe.com/api/balance/balance_retrieve), [balance mode](https://docs.stripe.com/api/balance/balance_object), and [PaymentIntent fields](https://docs.stripe.com/api/payment_intents/object). The official pinned Stripe SDK/native fixture compares the exact three GET paths and absence of a Connect header; local fixtures do not claim live Stripe or cloud validation.

## Cancelled-order terminal repair

The separate operator method `reconcile_bound_cancelled_order_payment(scope, payment_id, actor, receipt_id)` supports only direct, administratively bound Stripe payments whose local order is already cancelled. It verifies the direct account/mode, exact payment and order metadata, terminal provider `canceled` status, positive provider cancellation time, zero received/capturable amounts, and **explicitly null** `latest_charge`. An absent charge field, string/expanded charge (even failed), nonterminal intent, local succeeded payment, or prior capture-reconciliation audit requires review. The provider [cancel contract](https://docs.stripe.com/api/payment_intents/cancel) describes terminal behavior; this method only issues GETs and never calls that cancellation endpoint.

A sealed snapshot carries the exact payment, order row version and administrative binding. The final transaction locks payment, order, then binding, checks the cancelled order identity/version and exact binding scope/generation, rejects changed payment snapshots and capture history, and atomically writes local `failed` status, the existing terminal-cancellation audit and a scoped receipt. Provider reference, order status and encrypted order fields are retained. Locks are not held during provider I/O. The verification deadline is checked after lock waits and writes immediately before COMMIT; the local decision is made under those locks, not at a guaranteed remote-state instant. Other provider/account changes outside this process remain outside this observation window. Existing generic reconciliation APIs remain unchanged and are not silently upgraded to this profile.

Generate and retain a nonnil receipt UUID before invocation. `Recorded` confirms the commit; `RecordedEvidenceNoLongerCurrent` confirms a written receipt whose verification window elapsed by the postcommit check. `ecommerce_cancellation_commit_unknown`, cancellation of the awaiting task, or a lost response can leave the commit outcome unknown: query `scoped_cancellation_receipt` for that UUID. Absence is only an observation; neither an existing nor absent receipt authorizes another provider operation. Reusing a receipt UUID fails atomically rather than duplicating the terminal audit. This API does not clear provider references, reopen payment attempts, refund/capture/cancel funds, or transition an order. Managed scoped repair remains unsupported; the separate Connect counterpart is described below. PayPal has its own [VOIDED order repair](../ecommerce-paypal/README.md) with provider-specific evidence and receipt requirements.


## Explicit connected-account counterparts

`verify_connected_context(connect, expected_account, livemode)` performs the same account/mode verification using the **same exact `Stripe-Account` header on all GETs**. It rejects a trusted expected account differing from the configured Connect account before I/O. The opaque connected token copies the full Connect configuration (account and fee setting) and binds the immutable client generation and five-second deadline. `retrieve_intent_in_connected_context` rejects a different client or Connect configuration before sending a request, uses the token's copied account for retrieval, and checks freshness and returned object/mode afterward. Copying the fee setting into context identity grants no fee/financial-operation authority.

`inspect_bound_connected_payment_intent` and `reconcile_bound_connected_cancelled_order_payment` require a configured Connect account and share the direct counterparts' exact durable binding, evidence, generation, snapshot, deadline and transaction/receipt checks. The direct APIs still reject Connect; neither profile falls back to the other. Account/mode expectations remain trusted configuration and administrative bindings, not caller fields or key-prefix inference. The connected repair still requires explicit null latest charge, zero received/capturable amounts, a cancelled local order, and no captured/succeeded local history. No provider POST or reference clearing is added.

This is the repository's existing Accounts-v1/PaymentIntents-v1 compatibility profile. Stripe documents [per-request connected-account authentication](https://docs.stripe.com/connect/authentication) and [connected account balance retrieval](https://docs.stripe.com/connect/account-balances#check-a-connected-accounts-balance); the pinned official SDK's `accounts.retrieveCurrent` sends `/v1/account`. The owned SDK/native fixture checks both direct and connected triples, including each header. Local fixtures are not live Stripe validation, and Accounts-v2 migration is separate work.


## Interrupted confirmation after full capture

With `postgres`, `reconcile_bound_captured_payment(scope, payment_id, actor,
receipt_id)` adopts an already fully captured direct-account payment. Retain the
receipt UUID before calling. The method authenticates account/mode and reads the
exact PaymentIntent with expanded Charge/balance transaction. Immutable local
payment, noncancelled order version and administrative binding generation must
match through locked publication. Full captured/received amount, canonical
currency, paid/captured flags and zero refunded amount/no dispute are mandatory.
The initial profile admits only same-currency non-FX balance evidence with exact
charge source and pending/available bookkeeping state; legitimate FX, unexpanded
or ambiguous evidence require manual review. Bookkeeping is not settlement or a
lock against later remote refunds/disputes.

Apply additive ecommerce migration11. Payment status becomes succeeded with the
existing reconciliation audit and a tenant-scoped durable receipt in one
transaction. The order and provider references are retained. No provider POST,
capture, refund or retry occurs. Unknown COMMIT outcomes require
`scoped_capture_receipt(receipt_id)` inspection; recorded-but-expired observations
return `RecordedEvidenceNoLongerCurrent`. The explicit Connected counterpart is
`reconcile_bound_connected_captured_payment(scope, payment_id, actor, receipt_id)`.
It binds the sealed client/account/configuration and sends the exact
`Stripe-Account` on each provider GET. The direct method still rejects Connect.
Connected direct-charge evidence requires explicit null delegation/source-transfer
fields and rejects destination/transfer ambiguity or missing critical fields.
Coherent historical application fees between zero and gross are admitted
independently of current `fee_bps`; fees/net payout/fee ownership are not inferred.
The same non-FX gross evidence, atomic receipt and ambiguity rules apply. Managed
capture adoption remains separate. No default HTTP/MCP route is added.
