# Turnover — Ecommerce Mock Migrations (for a fresh Claude Code session)

Paste this into Claude Code opened in the `forklaunch` repo to pick up with full context.

## Goal of this task

Build **full end-to-end mock migrations of two real Shopify shops through the ForkLaunch framework** to (1) prove the migration works cleanly on real catalogs, (2) surface the ownership boundary (what the module owns vs. what Guild builds on top), and (3) inform how the ecommerce engine gets fleshed out. The overriding priority is getting **core functionality up and running**, not polish.

Two test shops (pull their catalogs via the public Shopify feed, `<url>/products.json`):
- **Gorilla Mind** — https://gorillamind.com  (supplements + gym gear; simple variants; subscriptions)
- **Olipop** — https://drinkolipop.com  (beverages; pack-size variants; subscribe-and-save)

Both return their full catalog in one request. Flow to build: pull catalog → normalize → import into the module's data model → run the purchase loop against it.

## What the module is

A drop-in ecommerce module for the **`forklaunch` blueprint repo** (not the platform). Lives at `blueprint/interfaces/ecommerce/` → `blueprint/implementations/ecommerce/{base,stripe}/` → a deployable `ecommerce-stripe/` service + an `ecommerce-worker/`. Mirrors the `billing-stripe` / `iam-base` / `sample-worker` patterns one-for-one.

- **Service:** REST/CRUD APIs + the order state machine (`pending → paid → fulfilled → shipped → delivered`, `+ cancelled`). Postgres source of truth, Redis for cart/sessions.
- **Worker:** BullMQ jobs reacting to order transitions (inventory, fulfillment, notifications, invoices, dunning, reconciliation).
- **Stripe** behind a provider-agnostic interface.

Full plan + the 23 tickets (ECOM-01…23) are in `ecommerce-module-plan.md` and `ecommerce-tickets.md`. **Committed floor = the purchase loop:** ECOM-01, 02, 03, 04, 06, 07, 08, 09, 10, 12.

## Scope decisions already made

- **Payments:** pull the standard methods into v1. Stripe's Payment Element covers cards, Apple Pay, Google Pay, and BNPL (Klarna/Affirm/Afterpay) with minimal work. **PayPal is the one separate provider build; Venmo comes bundled with PayPal.**
- **Also pulling in:** subscriptions (leverage `billing-stripe`'s subscription primitives) and reviews.
- **Deferred (don't build now):** i18n, B2B/wholesale, loyalty, affiliate, AI discovery, support chat, 3PL/fulfillment channels, multi-location inventory, tax filing/VAT, accounting sync, gift cards, CMS/theme editor, merchant admin UI.

## Design gotchas found in live testing (build for these)

- **Variant shapes vary.** Some shops encode attributes (e.g., color) in **tags** rather than options — must be normalized, not dropped. Products can have 0, 1, or 2 option dimensions with arbitrary names — don't hardcode "size/color."
- **"Size" isn't always size.** Olipop's option is actually **pack quantity** (1 can / 4-pack / 6-pack / 12-pack). Model packs vs. physical size correctly.
- **Subscriptions are everywhere** in target merchants — reinforces building subscribe-and-save.
- **Filter junk on import.** Real catalogs contain non-product entries (fake "content"/"donation"/test products with placeholder variants). The import must drop these so nothing looks mangled.
- **Preserve product handles/URLs** through import so SEO redirects can map old links later.

## Ownership boundary (what the mock should confirm)

- **Ours (the module):** catalog / variant / inventory import, order → checkout → payment engine, subscriptions, the read APIs a storefront consumes.
- **Guild's (on top):** storefront rendering, theme/branding, merchant-facing screens, SEO redirects, admin UI.
- Watch the handoffs: bulk-import API (ours), handle/URL preservation (data model ours, redirect wiring Guild's), where the admin UI lives.

## Convention reminders (ForkLaunch)

Follow the `billing-stripe` / `iam-base` patterns: import framework primitives from `@{{app-name}}/core` (never `@forklaunch/*` directly), natural object-notation schemas (not `z.object`), compile-time compliance (`defineComplianceEntity` + `fp.*.compliance('pii'|'pci'|...)`), route `access` levels, const-as-const enums, CLI-first scaffolding (`forklaunch init ...`, never hand-build service dirs). See `.claude/skills/` in the platform repo for the skill index.

## Suggested first steps

1. Read `ecommerce-module-plan.md` and `ecommerce-tickets.md`.
2. Scaffold the module skeleton per the `billing-stripe` pattern (interfaces → base/stripe impl → service + worker).
3. Start with ECOM-01/02 (product + variant + inventory data model), building in the variant/tag/pack flexibility above.
4. Build a catalog importer that pulls `products.json`, normalizes, filters junk, and loads Gorilla Mind + Olipop.
5. Stand up the purchase loop (cart → checkout → order → paid) against the imported catalog.

## Reference docs (saved in the repos)

- `Ecommerce-v1-Scope-Reconciliation.md` — plan/tickets vs. the guides, deferrals, ownership map, payment nuance.
- `Migration-Test-Readout.md` — the live catalog-pull results and fidelity checklist.
