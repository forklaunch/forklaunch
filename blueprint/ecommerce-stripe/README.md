# ecommerce-stripe

The commerce engine: catalog → cart → checkout → payment → order → fulfilment,
with inventory that can't oversell and payments through Stripe or PayPal.

## What you need running

- **Postgres** — orders, products, inventory, payments
- **Redis** — the cart cache *and* the order-event queue. Not optional; the
  service won't start without it.
- A **Stripe** account, a **PayPal** app, or both

## Setup

```bash
cp .env.example .env.local   # then fill it in — every variable is explained there
pnpm install
pnpm build                   # the app's own core/monitoring libs, needed below
pnpm migrate:init            # first run only — see below
pnpm dev                     # applies migrations, then serves on http://localhost:8000
pnpm dev:worker              # separate process — see below
```

A scaffolded project ships no migrations — generate one from the entities the
first time, then apply it:

```bash
pnpm migrate:init   # writes migrations/Migration<timestamp>.ts from the entities
pnpm migrate:up     # applies it, creating the 8 tables
```

`pnpm dev` runs `migrate:up` on the way in, so once the migration exists you do
not need to run it again by hand. It does not run `migrate:init`, and
`migrate:up` with no migration files is a silent no-op — the service then starts
and fails on the first query with `relation does not exist`.

**The worker is not optional.** Order transitions publish events to Redis, and
the worker is what consumes them to adjust inventory. Without it running,
orders will reach `paid` but stock never decrements.

It is a second process in production too, not just in development. Deploy
`pnpm start` and `pnpm start:worker` alongside each other; running only the
former gives you an API that takes orders and never adjusts stock.

## Getting your payment credentials

**Stripe** — from [dashboard.stripe.com/apikeys](https://dashboard.stripe.com/apikeys):
the secret key (`sk_test_…` / `sk_live_…`) into `STRIPE_API_KEY`. Then add a
webhook endpoint pointing at `https://<your-host>/webhook/stripe` and copy its
signing secret (`whsec_…`) into `STRIPE_WEBHOOK_SECRET`.

**PayPal** — create a *Merchant* app at
[developer.paypal.com](https://developer.paypal.com/dashboard/applications/)
(Sandbox tab while testing). Copy the Client ID and Secret. On the same page
add a webhook pointing at `https://<your-host>/webhook/paypal`, subscribe to
`CHECKOUT.ORDER.APPROVED` and `PAYMENT.CAPTURE.DENIED`, and copy the Webhook
ID.

> Subscribe to **`CHECKOUT.ORDER.APPROVED`**, not `PAYMENT.CAPTURE.COMPLETED`.
> Unlike Stripe, nothing captures a PayPal order until this app does: the
> approval event is what triggers the capture *and* the move to `paid` (see
> webhook.controller.ts's switch). Subscribing to the capture event instead
> means the capture never happens, so the completion event never fires either
> and every order stays `pending`.

> Webhooks need a **publicly reachable HTTPS URL**, so you can only register
> them once the store is deployed. Until then, use the Stripe CLI
> (`stripe listen --forward-to localhost:8000/webhook/stripe`) for local
> testing. **Payments do not complete without webhooks** — the provider
> confirms the charge asynchronously, and that callback is what moves an order
> to `paid`.

## How a purchase flows

```
POST /catalog-import   import products (HMAC-signed — used by the migration tool)
GET  /product          browse
POST /cart             create a cart
POST /cart/items       add items
POST /checkout         validates stock, creates the order + payment intent
     ↓ customer pays with the provider
POST /webhook/stripe   provider confirms → order becomes `paid`
     ↓ order event → Redis queue
     worker            decrements inventory
```

Orders move `pending → paid → fulfilled → shipped → delivered`. `cancelled` is
reachable from `pending`, `paid` and `fulfilled` — but **not** once an order is
`shipped`, which can only go on to `delivered`. Illegal transitions are
rejected.

Stock is checked at checkout, and the decrement the worker performs on `paid`
is an atomic conditional update, so stock can never go negative.

**Stock is not reserved at checkout, though.** The check is a read, and the
decrement happens later, when payment is confirmed. Two orders for the last
unit will both pass the check and both be charged; the first decrement
succeeds and the second fails its condition. Nothing oversells the database,
but a customer can be charged for an order that cannot be fulfilled, and today
that surfaces only as a worker error in the logs. If you are selling scarce
stock, reserve at checkout rather than relying on this.

## Choosing a payment provider

Checkout takes an optional `provider` (`stripe` by default, or `paypal`). Both
can be enabled at once — the customer's choice decides which is used per order.

## Bring-your-own vs. platform mode

By default the account behind `STRIPE_API_KEY` is the merchant of record and
receives the funds — the merchant brings their own Stripe account.

Setting `STRIPE_CONNECTED_ACCOUNT_ID` (an `acct_…` from Stripe Connect)
switches to a **direct charge on that connected account**: the merchant is
still merchant of record and still settles their own funds.
`STRIPE_PLATFORM_FEE_BPS` defaults to `0`, so no application fee is attached
and no cut is taken from merchant sales.

## Testing

```bash
pnpm test
```

The suite uses testcontainers, so **Docker must be running**. It covers the
full purchase loop (import → cart → order → transitions, including rejecting
an illegal transition) and the webhook idempotency gate.
