---
name: ecommerce
description: "Ecommerce module: products, variants, inventory, cart, checkout, orders, Stripe/PayPal payments. HMAC auth, catalog import, the worker, and the purchase loop end to end."
user-invokable: true
---

# ForkLaunch Ecommerce Module

## When to Use This Skill

Use when a project includes the `ecommerce-stripe` module — scaffolding a store,
seeding a catalog, driving a cart to a paid order, wiring Stripe or PayPal, or
debugging why an order is stuck at `pending` or stock did not move.

Scaffold it with `forklaunch init application <name> -m ecommerce-stripe`, or add
it to an existing app with `forklaunch init module`. The module is backend only:
it exposes an API. The storefront that calls it is yours to build or migrate.

## Critical Rules

1. **Every route requires an HMAC signature.** The routes are `access: 'internal'`.
   There is no bearer token, no JWT, no JWKS endpoint. The generated OpenAPI spec
   currently declares `bearer`/`JWT` — **that is wrong**; ignore it and sign.
2. **Sign with the framework's helper, never by hand.**
   `generateHmacAuthHeaders` from `@forklaunch/core/http` matches the server's
   verifier exactly. Hand-rolled signing fails on two undocumented details (the
   digest is base64, and a bodyless request signs the literal string `undefined`)
   and both failures return the same `403 Invalid Authorization signature`.
3. **The signed path is router-relative, with real values and no query string.**
   `POST /cart/items` is signed as `/items`. `PUT /order/abc-123/transition` is
   signed as `/abc-123/transition` — the actual id, never the literal `{id}` or
   `:id`. `GET /product/catalog?limit=50` is signed as `/catalog` — the query
   string is not part of the signature. Every mistake here (full path, placeholder
   id, query string, wrong secret) returns the same `403 Invalid Authorization
   signature`, so it cannot be debugged from the response.
4. **A browser cannot call the module directly — ever.** The secret must stay
   server-side, so any storefront needs a small server-side layer that signs
   requests on the page's behalf (a route-whitelisting proxy). Budget for it: it
   is the largest piece of work in a first integration and nothing else here
   builds it for you. Stripe's Payment Element also needs a **publishable** key
   (`pk_…`) the module never asks for — get it from the Stripe dashboard.
5. **`catalog-import` is the only route to sellable stock.** `POST /variant`
   creates no inventory row, and `PUT /inventory/adjust` on a variant with no row
   returns 500. Import with `initialStock` set.
6. **Run the worker.** `pnpm dev:worker` in its own process. It drains the
   `ORDER_EVENT_QUEUE` and decrements stock after payment. Without it, orders reach
   `paid`, the cart clears, and inventory never moves — with no error anywhere.
7. **The webhook signing secret must be in `.env.local` before the server starts.**
   Get it from `stripe listen` (it prints `whsec_…` on startup) or the Stripe
   dashboard. A missing or stale secret rejects every webhook and orders sit at
   `pending` forever.

## Calling the API

```ts
import { createRequire } from 'module';
const { generateHmacAuthHeaders } = createRequire(import.meta.url)('@forklaunch/core/http');

export async function call(method, fullPath, signedPath, body) {
  const { authorization } = generateHmacAuthHeaders({
    secretKey: process.env.HMAC_SECRET_KEY,
    method, path: signedPath, body, keyId: 'default'
  });
  const res = await fetch(`http://localhost:${process.env.PORT}${fullPath}`, {
    method,
    headers: { authorization, ...(body ? { 'content-type': 'application/json' } : {}) },
    body: body ? JSON.stringify(body) : undefined
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}
```

`fullPath` is what you request; `signedPath` is that path with the router mount
removed. Router mounts: `/product`, `/variant`, `/inventory`, `/catalog-import`,
`/cart`, `/checkout`, `/order`, `/payment`, `/webhook`.

## Seeding a Catalog

```ts
await call('POST', '/catalog-import', '/', { products: [{
  externalId: 'tee-1', handle: 'tee', title: 'Tee',
  options: [{ name: 'Size', isPackQuantity: false, values: ['S', 'M'] }],
  variants: [
    { externalId: 'tee-s', title: 'Tee - S', sku: 'TEE-S', priceCents: 3200,
      optionValues: { Size: 'S' }, initialStock: 10, requiresShipping: true },
    { externalId: 'tee-m', title: 'Tee - M', sku: 'TEE-M', priceCents: 3200,
      optionValues: { Size: 'M' }, initialStock: 25, requiresShipping: true }
  ]
}] });
```

Required and easy to miss: `options[].isPackQuantity` and `variants[].title`.
`externalId` is what a migrated storefront uses to map its own variant ids onto
the module's, so keep it stable.

## The Purchase Loop

```ts
const cart = (await call('POST', '/cart', '/', { customerId: 'shopper-1' })).body;
await call('POST', '/cart/items', '/items', { cartId: cart.id, variantId, quantity: 2 });

const out = (await call('POST', '/checkout', '/', {
  cartId: cart.id, provider: 'stripe',
  shippingAddress: { name, line1, city, state, postalCode, country: 'US' }
})).body;
// out.order.status === 'pending'; out.clientSecret is the Stripe PaymentIntent secret.
```

Confirm the PaymentIntent client-side with Stripe.js (`confirmPayment` with
`redirect: 'if_required'` and a `return_url`), or server-side in a test with
`pm_card_visa`. Then **wait** — the transition to `paid` and the stock decrement
are asynchronous:

1. Stripe sends `payment_intent.succeeded` to `/webhook/stripe`
2. The module marks the order `paid` and emits an order event
3. The worker consumes it and decrements stock

Poll `GET /order/{id}` for `status === 'paid'`, then poll `GET /inventory/{variantId}`
for the decrement. Reading stock the instant the order flips to `paid` lands in
the gap between steps 2 and 3 and reports a false failure.

Shipping is a flat rule in v1: **$5.99 when the subtotal is under $50, free at
$50 and above.** It is not documented anywhere else and not configurable yet.

## Order Lifecycle

`pending → paid → fulfilled → shipped → delivered`, plus `cancelled`.
Transition with `PUT /order/{id}/transition`. There is **no refund state and no
refund endpoint** in v1 — refunds must be issued in the provider dashboard and the
order state will not reflect them.

## PayPal

The module captures on `CHECKOUT.ORDER.APPROVED`, **not** `PAYMENT.CAPTURE.COMPLETED`.
The client must not call `actions.order.capture()` — the module does it when the
webhook arrives, and capturing client-side double-charges. PayPal allows one
webhook URL per app and needs it publicly reachable; locally that means a tunnel.

## Known Issues (as of cli-v1.10.0)

- `pnpm database:setup` runs `pnpm seed`, which needs `persistence/seeder.ts`; the
  scaffold does not generate it. Migrations succeed first, so the database is fine —
  run `pnpm build` first (the workspace libs must be compiled or migrations fail
  with `Cannot find package @<app>/core/lib/index.js`), then `migrate:init` and
  `migrate:up` directly, and ignore the seed failure. Note `migrate:up` is a
  silent no-op when no migration files exist — exit 0, empty database.
- Tax falls back to a flat estimated rate when Stripe Tax is inactive. The response
  is HTTP 200 and the `estimated` flag is dropped before it reaches the caller; the
  only signal is `taxBreakdown[].jurisdiction === 'estimated-fallback'`. Check for it.
- The generated `docker-compose.yaml` omits `STRIPE_API_KEY`, `STRIPE_WEBHOOK_SECRET`,
  `HMAC_SECRET_KEY`, `ENCRYPTION_KEY` and `ORDER_EVENT_QUEUE` from the service
  containers. Run the module with `pnpm dev` + `pnpm dev:worker` until that is fixed.
- Defaults collide: Postgres 5432, Redis 6379, and an MCP server on 10000 that is
  not in `.env.local`. A local Postgres on 5432 fails as `role "postgresql" does not
  exist`, which points at credentials rather than the port. Move `DB_PORT` first.
- `ORDER_EVENT_QUEUE` defaults to `order-events` for every scaffold. Two apps on one
  Redis will consume each other's events. Give each app its own queue name.
