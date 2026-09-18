# From clone to a store that takes money

The skill gives you the front half: a faithful, browsable clone of a Shopify
or Squarespace storefront. This is the whole path from there to a store running on ForkLaunch
with real cart, checkout and payment. Six steps. Each says what you type and
what you get.

Read this after `MANUAL.md`. Nothing here is needed to show a client their
clone; all of it is needed before that clone can sell anything.

## 0. What you have after the clone

`node scripts/bin/migrate.mjs https://www.the-store.com --clean` leaves, in
`scripts/output/the-store.com/`:

| File | What it is |
|---|---|
| `site/` | the clone: pages, images, fonts, scripts, all local |
| `manifest.json` | the handoff record ForkLaunch reads: pages, catalog summary, limits |
| `features.json` | the verification report |
| `../../catalog/data/the-store-com/normalized.json` | the catalog: products, variants, prices, images, in the module's import shape |

The clone's cart already works offline (add, change, remove, subtotal). Its
checkout stops at a page that says so. That is the seam everything below fills.

## 1. Stand up a ForkLaunch app with the ecommerce module

```bash
forklaunch init application my-store -m ecommerce-stripe
cd my-store
```

Then follow the ecommerce skill that ships with the CLI (it is in your skills
pack after `forklaunch init`): Postgres and Redis up, `.env.local` filled in,
`pnpm build`, migrations, `pnpm dev` **and** `pnpm dev:worker` in a second
terminal. Note from `.env.local`: the module's port (`PORT`) and
`HMAC_SECRET_KEY`. You will need both twice below. A blank agent with only the
skill docs gets through this step on its own.

## 2. Import the catalog into the module

```bash
cd tools/storefront-migrate/scripts/catalog
bun cli.ts import data/the-store-com/normalized.json http://localhost:<PORT> <HMAC_SECRET_KEY>
```

Every product and variant lands in the module with `initialStock` set, so it is
sellable. `externalId` on each variant is the source platform's variant id (Shopify's,
or Squarespace's); that is how the clone's add-to-cart buttons map onto the
module's variants in step 3.

For a Squarespace store there is no `/products.json`; pull the catalog from
the store's own JSON first, then import the same way:

```bash
node pull-squarespace.mjs https://www.the-store.com /shop
bun cli.ts import data/the-store-com/normalized.json http://localhost:<PORT> <HMAC_SECRET_KEY>
```

Done on graza.co (79 products), gorillamind.com (51) and, for Squarespace,
swaticouture.com (379 products).

## 3. Serve the clone against the module

```bash
cd tools/storefront-migrate/scripts
bun catalog/heroserve-fl.ts output/the-store.com/site 4173 http://localhost:<PORT> <HMAC_SECRET_KEY> <STRIPE_PUBLISHABLE_KEY>
```

Open http://localhost:4173. This is the same server the verification used, now
with a backend. What each action on the page does:

| On the clone | Hits the module |
|---|---|
| Add to cart | `POST /cart` (first time) then `POST /cart/items` |
| Cart drawer / cart page | `GET /cart/{id}` presented in Shopify's `cart.js` shape, so the theme draws it |
| Checkout | `POST /checkout` → order + Stripe PaymentIntent |
| Pay | Stripe Payment Element in the browser (needs the `pk_…` key above), confirmed by webhook |
| Order status | `GET /order/{id}` |
| A product the crawl never captured | `GET /product/handle/{handle}` renders a page from the catalog |

The HMAC secret never reaches the browser: heroserve signs every module call
server-side. Pass a PayPal client id as a sixth argument for the PayPal button.

Prove it before anyone looks. Two gates, run from `tools/storefront-migrate/scripts`
with the module, its worker and `stripe listen` (step 5) all running:

```bash
node check-wired.mjs http://localhost:4173
node check-purchase.mjs --store http://localhost:4173 --db <DB_NAME> --pg postgresql://<DB_USER>@localhost:<DB_PORT>
```

`check-wired` (10 checks) proves the served pages drive the module: shim
running, add to cart lands server-side, no dead links, checkout collects an
address. `check-purchase` (12 checks) proves a purchase goes all the way:
pending order, test card to `paid` by webhook, stock down by the quantity
ordered, a declined card leaving both untouched, and nothing on the page
phoning a third party. It reads the module's database directly, never the
page, so pass the `DB_NAME`, `DB_USER` and `DB_PORT` from the module's
`.env.local`; its defaults are one development machine's values. Both exit
non-zero on any failed check, and a store is not ready to show until both
pass.

Proven with real Stripe test payments: graza.co and gorillamind.com pass all 12
`check-purchase` checks, including a declined card leaving stock untouched and
the return URL verified with Stripe rather than trusted; the Squarespace clone
of swaticouture.com goes through its own theme's Add to Cart button to `paid`
with stock decremented. PayPal is wired in the module; carrying it through
this server is next.

## 4. Register the clone as a ForkLaunch project

```bash
forklaunch init storefront --from tools/storefront-migrate/scripts/output/the-store.com/manifest.json
```

This command is coming to the CLI (not in 1.10.0; check `forklaunch init
--help` on yours). Until it lands the manifest is the handoff: it is produced
and validated on every run, and `references/manifest-schema.md` says exactly
what the command reads.

## 5. Payments and webhooks for real

Stripe: put the account's secret key and the webhook signing secret in the
module's `.env.local` before it starts. Locally, forward Stripe's events to the
module and use the signing secret it prints:

```bash
stripe listen --forward-to localhost:<PORT>/webhook/stripe
```

The module rejects every webhook, and no order ever reaches `paid`, if that
secret and `STRIPE_WEBHOOK_SECRET` in `.env.local` differ. PayPal: one webhook URL per app, publicly reachable, so
locally that means a tunnel. Details and the traps are in the ecommerce skill's
"Critical Rules".

## 6. Deploy

`forklaunch deploy create` provisions the app. The clone's server (heroserve)
is a dev server today (secret on the command line, no TLS); giving it a
production home — secret from the environment, TLS, a real domain — is the next
piece of work.

## What still will not come across

Reviews, loyalty, subscriptions and chat widgets live in their vendors'
systems; the clone shows their captured markup and marks them as policy in the
report. Refunds have no endpoint in the module yet. Automatic discounts are not
in the public catalog feed, so prices import at list.
