# storefront-migrate, explained in five minutes

This is the short version for someone who has to explain the tool to a colleague or a prospect. MANUAL.md says how to run it. WIRING.md says how to connect the result to the ForkLaunch commerce module. This page says what it actually does and where the line between the two sits.

## What the skill produces

You give it one thing: the URL of a Shopify store (Squarespace works too; see "Other platforms"). It hands back a folder that is a working, browsable copy of that store, served on localhost. Every page a shopper can reach from the navigation is there. Menus open, carousels slide, variant pickers switch images and prices, search and collection filters respond, and the cart drawer holds items. Nothing in the copy calls the original store or Shopify. It is a self-contained demo the prospect can click through as if it were their own site.

What it does NOT do by default: take money. Cart, checkout and account pages are visual only until a backend is wired in. That is deliberate. The skill is the migration. The commerce module is the product.

## How the migration works

The pipeline is one command and runs hands off. Under the hood it is six stages.

1. **Assess.** It fetches the storefront, identifies the platform and theme, estimates page count and time, and prints a GREEN, AMBER or RED verdict before spending any real time. RED means do not bother, and it says why.

2. **Capture.** A real browser walks the site from the navigation outward: home, every collection, every product linked from those collections, the content pages, and the cart. For each page it saves the rendered HTML plus every asset the page needed (images, fonts, scripts, stylesheets, videos), and rewrites the links so they point at local files. Third-party tags (analytics, chat widgets, consent banners, review widgets) are recorded but not fetched. The copy stays offline.

3. **Pull the catalog.** Separately from the pages, it reads the store's public product feed and normalizes it into one JSON file: products, variants, prices, options, images, inventory. This file is what the commerce module imports later. It is produced now so the demo and the backend agree on the same catalog.

4. **Always-safe repairs.** Media is shrunk to 720p so the clone loads fast, and web fonts are localized. Idempotent, so re-running never degrades quality.

5. **Verify against the live site.** This is the part that makes it one-shot rather than best-effort. A checker loads the LIVE store and the clone side by side and builds an inventory of every feature the live store has: each navigation link, each variant picker, each carousel, each collection filter, each search box, each add-to-cart form, each footer link. For every feature it asserts two things about the clone: the feature exists, and it responds when used. It never compares values, only behaviour, because prices and stock change minute to minute. Anything the live site samples inconsistently (A/B tests, personalization) is measured twice and only the stable intersection is required.

6. **Repair and loop.** Every failed assertion maps to a named repair: re-capture a page that was missed, fix a link that was rewritten inside a script, fetch a lazily loaded chunk the crawl never saw, neutralize a popup that froze open, and so on. The loop runs repair, then verify, until the missing-feature list is empty or the budget runs out. A round that makes things worse is rolled back.

The exit code is the contract. 0 means every feature the live store has, the clone has and it works. 1 means the report names what is still missing. 2 means the checker itself could not run, and nothing has been proved either way. Some things are listed as policy rather than failure: consent banners, third-party popups, hosted checkout, account login. Those belong to vendors, not to the storefront, and the report says so instead of pretending.

## What "fidelity" means here

Feature fidelity, not pixel fidelity. The claim is: everything a shopper can do on the live store, they can do on the copy, and it reacts the same way. The claim is NOT that a screenshot matches. Themes render differently on different days, so a pixel gate would fail on every store and prove nothing.

## Other platforms

The capture works on any site. The catalog pull and the cart bridge are
per-platform adapters: Shopify is the production path, and a Squarespace
adapter (catalog from the store's own JSON, the theme's Add to Cart bound to
the module) was proven on a live enquiry-only store that could not take
orders on its own platform. WooCommerce and BigCommerce are the same shape of
work. Search-driven sites (a map and filters over an API) are out of scope: the
clone captures pages, not someone else's backend.

## Getting to real transactions

The clone is a frontend. To sell through it, three things happen, all described step by step in WIRING.md:

1. **Stand up the ForkLaunch commerce module** (the ecommerce module we shipped: catalog, cart, orders, Stripe and PayPal payment, inventory worker, webhooks).
2. **Import the catalog** the skill pulled in stage 3 into the module. One command, signed with the module's HMAC key. Products and variants land in the module's database with their prices and stock.
3. **Serve the clone wired to the module.** The same server that serves the clone takes a module URL, the HMAC key and the Stripe publishable key. From then on, the theme's own add-to-cart, cart drawer, checkout and payment calls are intercepted and routed to the module instead of Shopify. The shopper sees the same pages. The money goes through Stripe on our side.

We proved this loop end to end on two migrated Shopify stores, graza.co and gorillamind.com, against the module in test mode: add to cart reaches the module, checkout creates a priced pending order, a test card payment moves it to paid via the Stripe webhook, the worker decrements stock by the quantity ordered, a declined card leaves the order pending and stock untouched, and the return URL is verified with Stripe rather than trusted. Twelve checks, all passing, on both stores. The Squarespace clone of swaticouture.com passed the same purchase path through its own theme's Add to Cart button: order paid, stock decremented.

Going from test mode to production is the module's deployment story, not the skill's: live Stripe keys, a real domain in front of the served clone, and the module deployed with its worker and webhook endpoint reachable from Stripe. WIRING.md marks which of those steps have been run for real and which have not.

## One-line summary

Point it at a Shopify store (or a Squarespace one), get a verified click-through copy within the hour with a report that names anything it could not reproduce; import the catalog it pulled into the commerce module and the same copy starts taking payments.
