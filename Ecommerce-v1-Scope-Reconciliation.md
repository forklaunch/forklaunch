# Ecommerce v1 — Scope Reconciliation

*Plan + 23 tickets vs. what Sunil's deck & the 31 guides ask for. Working doc for the Rohin scope conversation.*

## TL;DR

The plan and tickets build **the engine** — catalog → checkout → returns, as a backend service + worker in the blueprint repo. That already satisfies most of what the guides describe on the transactional side. Three things are worth deciding deliberately:

1. **Four of the guides' "must-builds" aren't tickets — and shouldn't be.** Storefront, the Shopify clone/migration, and SEO/redirect are **frontend/tooling that sits on top of the module** (Guild builds these against our APIs). Security is largely handled by the framework + existing tickets. So the module plan is correctly scoped to the engine — just make the boundary explicit.
2. **Decided: pull the expected payment methods, subscriptions, and reviews into v1.** Standard methods (cards, Apple Pay, Google Pay, BNPL) are near-free via Stripe; PayPal + Venmo is one added provider ticket. Subscriptions earn the slot — every test merchant runs subscribe-and-save — and reviews is the #1 requested app for cheap parity. Both are cheap given the plan's provider-agnostic seams.
3. **Everything else the guides ask for is either already a ticket, a fast extension of one, or a legitimate later-tier deferral.**

## Coverage map — 31 guides vs. the plan

**Already in the 23 tickets (the engine):**
catalog + variants + inventory (ECOM-02/03/04/05) · cart (06) · checkout + promos (09/11) · order state machine + payments (07/08/10/12) · fulfillment + shipping stub (13/14) · invoices (15) · notifications (16) · returns (17) · tax calc (18) · reconciliation (19) · dunning (20) · accounts/profiles (21). → maps to: commerce-security (partial), payments-providers (Stripe), shipping-fulfillment (labels/tracking, stub carrier), tax-compliance (calc), accounting-finance (reconciliation only), inventory-operations (single-location), returns-portal (logic), post-purchase (notifications), catalog-search (basic), merchandising-promotions (codes).

**Guild builds on top of the module (not module tickets — confirm the split):**
storefront · clone-shopify-store / commerce-migration · seo-redirect-parity · content-merchandising (CMS/theme editor) · the merchant admin UI (called out as unbuilt in the overview). The module just needs to **expose the right APIs** for these (see "Module-side asks" below).

**Requested but deferred in the plan — candidates to pull into v1:**
subscriptions · payments-providers → PayPal · reviews · marketing-automation (abandoned-cart).

**Legitimately later-tier (leave deferred):**
b2b-wholesale · internationalization · loyalty-rewards · affiliate-referral · ai-discovery-syndication · support-messaging · supplier-integrations · fulfillment-channels (3PL) · multi-location inventory · tax filing/VAT · accounting sync (QuickBooks/Xero) · gift cards · business-monitoring dashboards.

## The decisions to make (recommendations)

**1. Payments — pull the standard methods into v1; PayPal is the one separate build.** Decision: bring the payment methods buyers expect into v1. The nuance that makes this cheap: **Stripe's Payment Element already covers cards, Apple Pay, Google Pay, and BNPL (Klarna / Affirm / Afterpay)** — enabling them is mostly config + domain verification on top of the Stripe work already in ECOM-10, not separate provider builds. **PayPal is the only true separate provider** (its own SDK and checkout flow) → new ticket **ECOM-24 — PayPal provider impl (L)**. **Venmo rides on PayPal** — it's a PayPal-owned funding method (US-only, mobile-first) exposed through the PayPal integration, not available via Stripe — so it comes bundled with ECOM-24 at no extra cost. **Net: one provider ticket (PayPal, incl. Venmo); every other standard method is Stripe configuration.**

**2. Subscriptions — pull a lean version into v1.** It's listed as a v1 gap, but it's the one growth feature our own target merchants actually use, and `billing-stripe` already has subscription primitives to lean on. Recommend **ECOM-25 — subscribe-and-save (M)**: reuse billing-stripe's subscription engine, attach to an order line. Not the full Recharge feature set — just recurring reorders.

**3. Reviews — optional, high ROI.** It's the #1 installed app in Guild's 3.62M-store data and it's mostly CRUD + display (ratings, photos, verified-buyer flag). If there's room after the floor, **ECOM-26 — reviews (M)** buys a lot of perceived parity cheaply. Otherwise explicitly defer.

**4. Catalog search depth — keep v1 basic.** ECOM-03's title/attribute/price/in-stock filter is fine for v1. Guild's catalog-search guide wants typo-tolerant + faceted; note that as **v1.1**, not now.

**5. Promotions depth — keep codes, defer gift cards.** ECOM-11 (%, fixed, free-shipping) covers the common case; gift cards/bundles from merchandising-promotions are later.

## Module-side asks so Guild can build the top layer

The storefront and clone are Guild's, but the module must expose:
- **Read-only catalog/variant/inventory API** the clone can populate from (ECOM-03 covers this — confirm it's SDK-reachable for import, not just internal).
- **Stable product handles / URL slugs** carried through import, so seo-redirect-parity can map old links. Add to the ECOM-01 data-model design.
- **Bulk create** for catalog import (the clone writes hundreds of products) — worth a small ticket or an ECOM-03 acceptance criterion.

## Proposed adjusted scope

- **Committed floor (unchanged):** the purchase loop — ECOM-01, 02, 03, 04, 06, 07, 08, 09, 10, 12.
- **Target (all 23):** unchanged.
- **Additions now in scope:** enable standard methods (cards, Apple Pay, Google Pay, BNPL) via Stripe config → **ECOM-24** PayPal + Venmo provider → **ECOM-25** subscriptions → **ECOM-26** reviews. Plus fold bulk-import + handle-preservation into ECOM-01/03.
- **Explicit deferrals (say them out loud):** i18n, B2B, loyalty, affiliate, AI discovery, support chat, 3PL/fulfillment channels, multi-location inventory, tax filing, accounting sync, gift cards, CMS/theme editor, merchant admin UI.

## Mock migration → ownership map

Next step: run **Olipop and Gorilla Mind end-to-end through the ForkLaunch framework** as mock migrations. The point isn't the demo — it's that every seam we hit assigns an owner, turning the who-builds-what debate into a checklist.

- **Ours (the module):** catalog / variant / inventory import, the order → checkout → payment engine, subscriptions, and the APIs the store reads from.
- **Guild's (on top):** storefront rendering, theme / branding, merchant-facing screens, SEO redirects.
- **Handoffs the mock will expose:** how the catalog gets *into* the module (bulk-import API — ours), how product handles/URLs carry over (data model — ours; redirect wiring — Guild's), and where the admin UI lives.

## Keep-in-mind from the live migration tests

- **Model packs/variants carefully** — Olipop's "Size" is really *quantity per pack*, not physical size (feeds ECOM-01/02).
- **Subscriptions are everywhere** in our target merchants — reinforces pulling ECOM-25 forward.
- **Filter junk on import** — test/"content"/donation entries appear in real catalogs; the clone must drop them (feeds the migration tooling, not the module, but note it).
- **Non-Shopify + locked-down shops** are the next difficulty tier — the provider-agnostic/import design should assume more than one source platform.
