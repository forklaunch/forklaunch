# Running the ecommerce backend yourself — what it needs

This is for whoever stands up their own copy of the ForkLaunch ecommerce
backend (`ecommerce-stripe`) — no shared, always-on server from ForkLaunch
required. Run it on your own infrastructure.

## What has to already exist

- A Postgres database, reachable from wherever this runs
- A Redis instance, reachable from wherever this runs (cart caching and the
  background worker's event queue both require it — the service won't boot
  without `REDIS_URL` set)
- Node + pnpm to build and run it
- A Stripe account (test or live) if you want real payment to work
- A PayPal developer account (sandbox or live) if you want PayPal payment
  to work

There is no Dockerfile for this yet — it runs as a plain Node process, same
as the rest of this codebase's services. You'll build your own deployment
around the commands below however your infrastructure normally does that.

## Required environment variables

| Variable | What it's for |
|---|---|
| `DB_HOST`, `DB_PORT`, `DB_NAME`, `DB_USER`, `DB_PASSWORD` | Your Postgres connection |
| `NODE_ENV` | Standard Node environment name |
| `HOST`, `PORT` | Where this service itself listens |
| `VERSION` | API version string, used in the URL path |
| `DOCS_PATH` | Where the API reference is served, e.g. `/docs` |
| `ENCRYPTION_KEY` | Encrypts sensitive fields at rest (this module tags customer/payment-related fields for automatic encryption) |
| `HMAC_SECRET_KEY` | The secret used to authenticate calls to `/catalog-import` — generate your own, keep it private, it's yours to control since you're running the server |
| `JWKS_PUBLIC_KEY_URL` | Public-key URL used to verify normal user-facing auth tokens (separate from the HMAC secret above, which is only for service-to-service calls) |
| `STRIPE_API_KEY` | Your Stripe secret key |
| `STRIPE_WEBHOOK_SECRET` | Your Stripe webhook signing secret |
| `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`, `PAYPAL_BASE_URL` | Your PayPal app credentials + API base (sandbox or live) |
| `REDIS_URL` | Redis connection string — used for the cart cache and the order-event queue the background worker consumes |
| `ORDER_EVENT_QUEUE` | Queue name for order-transition events between the service and the worker — any string, just has to match on both sides |
| `OTEL_SERVICE_NAME`, `OTEL_LEVEL`, `OTEL_EXPORTER_OTLP_ENDPOINT` | Logging/observability export — point at your own collector, or leave logging local if you don't have one yet |

**Important:** since you're the one running this, you generate
`HMAC_SECRET_KEY` and `ENCRYPTION_KEY` yourself — nobody hands those to you.
Whatever tool calls `/catalog-import` needs the same `HMAC_SECRET_KEY` value
you set here.

## Commands, in order

```
pnpm install
pnpm run migrate:up      # creates the database tables
pnpm run build
pnpm run start            # or: pnpm run dev, for a version that reloads on file changes
pnpm run worker            # separate process — consumes order-transition events
```

The service prints its own URL and API-docs path on startup. The worker is
a second, separate process — order transitions still work without it
running, but inventory won't adjust automatically until it's up.

## Payment providers — what's actually wired in today

Both **Stripe** and **PayPal** are connected in the version of this service
that runs out of the box. The payment endpoint accepts an optional
`provider` field (`stripe` or `paypal`, defaults to `stripe`) so callers
pick per-request — neither provider replaces the other.

## What this service includes

- Catalog search/filtering — title, price range, in-stock, option-value —
  in addition to fetching by ID
- A unified checkout endpoint (`POST /checkout`) — cart to order in one
  call, stock validated first, real tax and shipping cost included (see
  below), instead of separate create-order and take-payment steps
- Real tax at checkout via **Stripe Tax**, with a flagged flat-rate
  fallback (never a silent $0) if Stripe Tax is unreachable
- Real shipping cost at checkout — flat/table-rate (free above a
  configurable threshold, otherwise a domestic/international flat rate) —
  not live carrier rates yet, see below
- **Promo codes** — percent, fixed, or free-shipping discounts, with usage
  limits, minimum-subtotal gates, and expiry; redemption is atomic (safe
  under concurrent checkouts)
- **Gift cards** — redeemed as a tender against the final total, partial
  redemption supported, balance decrement is atomic
- **Product reviews** — ratings, a moderation queue (new reviews start
  pending, never auto-published), verified-buyer badge when tied to a real
  order
- A background worker (run separately: `pnpm run worker`) that reacts to
  order status transitions over Redis and adjusts inventory (paid ->
  decrement, cancelled-from-paid -> restock)
- Cart is Redis-cached (read-through/write-through); falls back to
  Postgres-only (with a 750ms timeout, so a request never hangs) if Redis
  is unreachable at request time

## What this service does not include yet

- No real shipping labels, live carrier rates, or tracking — checkout has
  a real flat-rate cost, but nothing buys a label or talks to a carrier
- No invoicing or email/notification side effects — the background worker
  above only adjusts inventory today
- No returns flow — no RMA state machine, no self-service return/exchange
- No dunning (failed-payment retry) or payment/payout reconciliation
- No customer accounts — `customerId` is a bare reference today, no saved
  profile, addresses, or payment methods
- No subscriptions billing engine — the data model exists, but nothing
  turns a recurring cycle into a real order yet
- No customer or historical order migration — only the product catalog
  loads through `/catalog-import` today
