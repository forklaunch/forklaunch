---
name: storefront-migrate
description: Migrate a live Shopify or Squarespace storefront onto ForkLaunch — capture every public page as a browsable, visually faithful clone, pull and import the product catalog, and wire the clone's cart and checkout to the real ecommerce module so it takes actual orders. Also does capture-only when a visual demo is all that's wanted. Use this whenever a store URL shows up alongside words like migrate, clone, mirror, copy, reproduce, move over, or "what would this look like on our platform" — and also for the individual pieces: capturing a store's pages, pulling or importing a Shopify or Squarespace catalog, checking capture fidelity or dead links, or standing a captured storefront up against a running ForkLaunch backend. Reach for it even when the request names only one of those steps rather than a whole migration.
---

# Storefront capture

Turns a live storefront URL into a browsable local clone. Front half of the
ForkLaunch migration pipeline: reproduce what the shopper sees, then wire the
commerce backend underneath it.

## Running it

`${CLAUDE_SKILL_DIR}` is this skill's directory (Claude Code substitutes it).
Every command below is written against it so it works from any working
directory. Output lands in `${CLAUDE_SKILL_DIR}/scripts/output/<store-domain>/`
unless `--out` says otherwise.

One command. Three prerequisites must already be on the machine — bun, ffmpeg,
and Playwright's Chromium (`cd scripts && npm install && npx playwright install
chromium`, a ~150MB download, once). The command checks all three first and
REFUSES to start if one is missing, printing the exact install line; it never
installs anything itself. `MANUAL.md` has the same three lines for a person.

```bash
node ${CLAUDE_SKILL_DIR}/scripts/bin/migrate.mjs <store-url> --clean --no-serve
```

It captures, **verifies against the live storefront, repairs what it finds**,
then exits. `--no-serve` matters for an agent: without it the process finishes
the verify loop and then blocks on "(still serving at http://localhost:4173,
Ctrl-C to stop)", so the exit code never comes back. With it, the last line
printed is `exit code N` and the process exits with that code. Read the code,
THEN serve the result in the background and confirm it answers:

```bash
nohup bun ${CLAUDE_SKILL_DIR}/scripts/catalog/heroserve-fl.ts <outdir>/site <port> > <outdir>/serve.log 2>&1 &
curl -sI http://127.0.0.1:<port>/ | head -1
```

`<outdir>` is `${CLAUDE_SKILL_DIR}/scripts/output/<store-domain>` unless you
passed `--out`; `<port>` is conventionally 4173. `heroserve-fl.ts` is the
server to hand a client: it supplies the Shopify runtime endpoints and the
popup shim. `python3 ${CLAUDE_SKILL_DIR}/scripts/serve.py <port> <outdir>/site`
is only a fallback when bun is missing, and cart, search and filters are inert
under it. Give the user the served URL.

One command is the whole point. Capture, verify and repair all existed before
and all worked; nothing chained them, so every migration needed somebody who
remembered the right order — and the defects they catch are silent, because a
broken capture serves 200s and looks perfect until a client is at the screen.

### How a link is judged present

An internal link is identified by its destination path, never by its text. A
product card's accessible name carries whatever sits inside it — a vendor's
star rating, a sale badge, a price — and all of those legitimately differ
between the live site and an offline clone. Same destination, same feature.
Sale phrasing ("Original price:", "20% off") is stripped from every feature
key for the same reason: it is a value, and values are not compared.

### When the live site changes under you

Shopify stamps every section with the template that rendered it. If the live
page is served from a different template than the capture (an A/B test, or a
theme change since the crawl), the gate reports one condition — `same template
as live` — routes it to a targeted recapture of that route, and skips the
section/control/picker comparison there rather than reporting phantom
defects. If two live samples disagree with each other, the store is A/B
testing and the structural comparison is skipped with that reason. Both are
named in the report; neither is a capture failure.

### What the server does that a static server cannot

`catalog/heroserve-fl.ts` is not a file server. Two things it does are the
difference between a clone that renders and one whose theme JavaScript dies
mid-initialisation:

- **Shopify's runtime API** (`catalog/shopify-runtime.ts`): `/products/<handle>.js`
  and `.json`, `/cart.js`, `/cart/add.js` and friends, `?sections=` and
  `?section_id=` fragments, `/recommendations/products.json`,
  `/search/suggest.json`, and Shopify's telemetry pings. Answered from the
  catalog pull and the captured pages; a local in-memory cart when no module
  is wired. Every Liquid theme assumes these exist.
- **Dead overlays**: a third-party pop-up captured in its open state (a promo
  scratch card, a newsletter form) can never be closed on the clone because its
  vendor script is blocked. The served shim removes known pop-up vendor roots
  and any fixed element at the browser's maximum z-index covering most of the
  viewport, and marks them `data-fl-dead-overlay`.

### What "done" means

Not a percentage, and not a round count. The run finishes when the
**missing-feature list is empty**.

`scripts/check-features.mjs` visits the LIVE storefront, enumerates what it has
— sections, images, fonts, controls by role and accessible name, nav
destinations, variant pickers — then visits the clone and requires each one to
be present and to RESPOND. The specification is the merchant's own site, so it
updates itself when they change it, and features nobody anticipated are covered
without anyone writing an assertion for them.

It asserts a feature EXISTS and RESPONDS. It never asserts a value matches.
Live stock counts, rotating banners, "N people viewing", A/B buckets and
personalisation legitimately differ, and three mechanisms keep them out of the
report: the live page is sampled twice and only what both loads contain becomes
a requirement; every run of digits in a comparison key collapses to `#`, so
"4540 Reviews" and "4561 Reviews" are the same feature; and a control that does
nothing on the live site is not a feature and is never held against the clone.

`--rounds` and `--budget-min` are **crash guards, not goals**. Hitting either is
reported as "ran out of budget with N features still missing", never as success.
A round that makes the score worse is rolled back rather than built on.

### Exit codes mean three different things

| Code | Meaning |
|---|---|
| 0 | every live feature is present and responding |
| 1 | features still missing — the report names each one and its repair |
| 2 | **harness failure**: the gates could not run, so nothing was proved |

2 exists because of a real incident. Playwright's browser was not installed, the
gates produced no output, and the loop read "no assertions" as "nothing to
repair" and declared the storefront needed a person. It was one `npx` command
from green. **A gate that cannot run is not a gate that passed** — every gate
now prints a line beginning `HARNESS-FAIL` and exits 2 when it could not run,
and nothing downstream is allowed to conflate that with a fidelity failure.

### Pre-flight comes first — read it out

Every run opens with an assessment of the target, before any work happens:

```
● GREEN  deathwishcoffee.com
stack: Shopify theme: Dawn
Expect a faithful, browsable clone (~110s, ~12 pages).
```

```
● RED  olipop.com
✗ The store refused an automated browser. This is anti-bot protection
  (Cloudflare / DataDome / similar). We cannot capture it without the
  merchant's cooperation — and we don't build ways around it.
```

GREEN = expect a faithful clone. AMBER = works, caveats named. RED = expect
failure, reason given. It also prints the permanent limits every time.

The preflight verdict (GREEN, AMBER or RED, with reasons) is printed at the
top of the run output; in a one-command run the crawl starts right after it
with no pause. **Quote it verbatim in the report.** If it is RED, stop and
report instead of retrying.

Useful flags:

| Flag | Use |
|---|---|
| `--clean` | hide vendor marketing popups AND make cart/account/checkout inert so a click can't jump to the live store. **Use for demos.** |
| `--pages N` | bounds only the additional product and collection pages (default 20). Navigation pages (home, every menu target, content pages) are always captured, so nothing in the menu links back to the live store. **Not a hard total**: real totals run 30 to 60 pages on a normal store. |
| `--no-preflight` | skip the assessment |
| `--single` | homepage only — faster, but NOT browsable (links go to the live site) |
| `--out DIR` | where to write. Defaults to `output/<domain>/` **relative to this skill's `scripts/` directory, not your current directory** — pass `--out` explicitly if you want it somewhere you'll find it. |
| `--no-serve` | still captures, verifies and repairs, but exits with the verdict's code instead of blocking on the serve; the last line printed is `exit code N`. Use it in every agent-driven run, then serve with `heroserve-fl.ts` as shown above. |
| `--no-catalog` | skip the product-data pull (see Requirements — needs bun) |
| `--measure` | score fidelity vs live (dev signal, not the deliverable) |
| `--no-verify` | capture only, skip the feature gate and the repair loop. The clone is then **unverified** — say so when reporting. |
| `--rounds N` | verify/repair round cap (default 5). A crash guard, not a target. |
| `--budget-min N` | wall-clock cap on the verify/repair loop (default 30). Also a crash guard. |
| `--no-recapture` | do not let the repair loop re-crawl to fill dead links. The re-crawl is targeted: it fetches exactly the dead paths the gate named (about a minute each), never the whole sitemap. It is on by default here because this command is already crawling that store, and off in `finish.mjs`. |
| `--port N` | serve port (default 4173) |
| `--stripe-pk`, `--paypal-id` | publishable keys; turn `/__fl/checkout` into a real card page |

### How long it takes, and why it looks stuck

Roughly **6 to 40 seconds per page** for the crawl (quiet store: 6; ad-heavy: 40), and 100MB+. The preflight
estimate covers the crawl only; the verify and repair loop adds up to
`--budget-min` (default 30) minutes on top of it. Expect **15 to 60 minutes
wall clock** for a typical store, longer for image-heavy ones. A measured
13-page crawl took 253s and 116MB before verify started.

**HTML pages are only written at the very end of the crawl.** Mid-run the
output directory fills with assets and contains *zero* browsable pages. This
looks exactly like a hang. It isn't — but if you kill it, you get nothing
usable and there is no resume. For a quick first look, use `--pages 3` (three
product/collection pages plus the navigation pages) rather than the default.

Progress prints one line per captured page. Long silences during asset
downloads are normal.

### Finishing a capture you already have

`migrate.mjs` runs this itself. Use it directly on an existing capture:

```bash
node ${CLAUDE_SKILL_DIR}/scripts/bin/finish.mjs <outdir>/site --live https://the-store.com [--module <url> --hmac <secret>]
```

It discovers this theme's controls, serves the capture through the same bridge
that ships, runs the feature gate, applies the repair each defect maps to, and
re-gates until the list is empty. Defects map to repairs like this:

| Defect | Repair |
|---|---|
| subresources still fetched from the internet | `catalog/localize-runtime.mjs` |
| a brand typeface falling back | `catalog/localize-fonts.mjs` |
| video over the decode budget | `catalog/shrink-media.mjs` |
| an asset name the page asks for that the crawl saved under a hashed or query-suffixed name | resolved at serve time in `catalog/heroserve-fl.ts` |
| a 404 on **our own** origin — the crawl rewrote the reference but never fetched the file (lazily-imported chunks) | `catalog/refetch-missing.mjs`, which finds the file by name on the live site and puts it where the clone was looking |
| a dead internal link | a targeted re-crawl of exactly those paths (`crawl.js --only /a,/b`) — opt-in |
| a vendor's inline snippet broken by the demo-mode link rewrite (`location.href="#" data-mirror-uncaptured=…` is a syntax error) | `catalog/fix-script-hrefs.mjs`, always run; `crawl.js` no longer rewrites inside `<script>` bodies |

The repair scripts are **idempotent**, which they had to become before a
loop could run them repeatedly: `shrink-media` refuses a file already at or
below the target height (h264 is lossy, and four passes shipped visibly mushy
video), and the two localisers remember what they already fetched and what is
already known unreachable instead of re-reporting "fetched 0, failed 8" every
round.

### Cart and checkout: not configured is not broken

Without a ForkLaunch module behind the bridge, the cart and checkout gates
report **SKIP**, not FAIL, and say what would prove them. They used to fail —
`heroserve` swallows the connection error and answers `{item_count: 0}`, so an
add-to-cart check saw 0 → 0 and called it broken. A red line nobody can act on
teaches everyone to skim past red lines, and that is the one line that must
never be skimmed when a real backend IS attached. `GET /__fl/health` on the
served storefront reports `configured` and `reachable` separately.

### Checkout interception is discovered, not assumed

Add-to-cart interception works on every theme because `POST /cart/add.js` is a
**contract**. Checkout has none, and used to be matched with `[name="checkout"],
[href*="/checkout"], [href="/cart"]` — an accurate description of Dawn and its
descendants and of nothing else. When that guess misses, the button is present,
looks right, and clicking it walks the viewer out of the demo and onto the
merchant's real Shopify checkout, mid-presentation. (graza.co's own cart link is
`<a href="#">`; the old selector matched none of it.)

`scripts/discover-controls.mjs` now classifies every control by role and
accessible name, opening disclosures first so a drawer's Checkout button is
found, and writes `<site>/_fl-controls.json`. The bridge reads that AND runs the
same name matching live in the page, so controls built after load are caught
too. Three layers, weakest last: contract, then name, then selector.

The run may print `note: no checkout control identified by selector`, followed
by "the runtime name matcher is the only thing intercepting checkout on this
capture". That is informational and harmless in a browse-only (`--clean`) run.

### Did it work?

The run ends with `✓ N pages, N assets, NMB` and writes `manifest.json` beside
`site/`. That file is the record of what actually came across — page list,
catalog counts, and a `gaps` array naming anything that did not. If
`manifest.json` is absent, the run did not finish. If `catalog` is `null`,
the product-data pull did not run (almost always missing bun — see
Requirements); the clone is still browsable, but there is no product data to
import into ForkLaunch. When the pull did run, its files are NOT under `--out`:
the catalog step writes `raw.json` and `normalized.json` to
`${CLAUDE_SKILL_DIR}/scripts/catalog/data/<shop-domain-with-dashes>/`
regardless of `--out`.

The verify/repair phase writes two more files beside `site/`:

- **`features.json`** — the machine-readable verdict. `results` is every
  assertion, `missing` is the blocking defects with the repair each maps to,
  `policy` is what we deliberately did not migrate and which vendor owns it.
  Read this before saying anything about fidelity.
- **`feature-inventory.json`** — the cached live requirement, reused across
  repair rounds so a loop costs the merchant's origin one polite pass rather
  than six. Delete it (or pass `--refresh-live` to `check-features.mjs`) to
  re-measure the live site.

## Reporting back

Tell the user the local URL and what was captured (page count, asset count),
and then the verdict — which is now a list, not an impression:

- **exit 0** — every feature the live storefront has, the clone has and
  responds to. Say so, and name the third-party features under `policy` that
  were deliberately not migrated (reviews, loyalty, the hosted checkout).
- **exit 1** — read `features.json` and relay the actual missing features and
  the repair each needs. Do not summarise it as a percentage.
- **exit 2** — the gates could not run. **Nothing was proved about this
  capture.** Say exactly that; do not describe the clone as working or broken,
  because neither was measured.

Say plainly that cart and checkout are visual only until a ForkLaunch module is
attached — the gates report those as SKIP, not PASS, for the same reason.

**Always serve over HTTP.** Opening the HTML with `file://` breaks module
scripts and CORS, and the clone will look broken for reasons unrelated to the
capture. To serve an existing capture (after `--no-serve`, or any time later),
use `heroserve-fl.ts`, in the background so the command returns:

```bash
nohup bun ${CLAUDE_SKILL_DIR}/scripts/catalog/heroserve-fl.ts <outdir>/site <port> > <outdir>/serve.log 2>&1 &
curl -sI http://127.0.0.1:<port>/ | head -1
```

That is the server to hand a client; it supplies the Shopify runtime endpoints
and the popup shim. `python3 ${CLAUDE_SKILL_DIR}/scripts/serve.py <port> <outdir>/site`
is only a fallback when bun is missing, and cart, search and filters are inert
under it.

## What it does and does not do

**Does:** reproduce the visual storefront — layout, styling, webfonts, imagery,
product photography, prices, swatches, size grids — for any store that renders
in a browser. Classic Liquid themes come through whole. Headless React
storefronts (Hydrogen, Next.js) capture the homepage and content pages well but
product and collection pages thin — they render from an API the clone cannot
reach — and the preflight says AMBER for them. Page-to-page navigation works.

**Does not**, without merchant credentials:

- **Cart, search, variant switching, add-to-cart** — visual only. They need a
  backend; that backend is the ForkLaunch ecommerce module.
- **Checkout** — Shopify's is closed and hosted. It gets rebuilt, never migrated.
- **Inventory counts** — the public catalog exposes an in-stock boolean, never
  quantities. Seed real stock from the Admin API at cutover.
- **Fields never rendered** — SKU, weight, tax flags, cost, barcode, metafields.
  Not on the page, so not recoverable by looking at it.
- **Third-party app data** — reviews, loyalty balances, subscription contracts.
  These live in other vendors' databases (Bazaarvoice, Yotpo, Klaviyo), not in
  Shopify, and do not come across.

Do not tell a user this produces a working store. It produces a faithful,
clickable *render* of one.

## Next: getting it into ForkLaunch

`WIRING.md` at the skill root is the short, human version of this section:
six steps, exact commands, tested-or-not per step. Hand it to a person; read
the rest of this section yourself.

Capture is the front half. Everything below turns the render into a store, and
each step is documented in `references/` — **read those files, they are not
optional and nothing else links to them**:

| Step | What it does | Read |
|---|---|---|
| 1. Capture | this document | — |
| 2. Register the project | adds the captured site to an existing ForkLaunch app as a project | `references/manifest-schema.md` |
| 3. Import the catalog | pushes captured products/variants into the ecommerce module over an HMAC-signed endpoint | `references/catalog-import-contract.md` |
| 4. Run the backend | Postgres, Redis, env config, migrations, worker — what cart/checkout/filters actually need | `references/self-hosting.md` |
| 5. Point the clone at it | re-serve with `--api <module-url>` so cart and filters hit the real backend instead of being inert | this document, below |

**Be honest about step 4's cost.** It is not a one-liner: it needs Postgres and
Redis running, roughly twenty environment variables, secrets you generate
yourself (`HMAC_SECRET_KEY`, `ENCRYPTION_KEY`), Stripe/PayPal credentials for
payments, and a separate worker process. `references/self-hosting.md` walks
through it. If someone just wants to *see* the storefront, stop after step 1 —
don't send them down this path unnecessarily.

### Registering the project

`forklaunch init storefront --from <path-to-manifest.json>` registers a capture
as a project in an existing ForkLaunch app, reading the `manifest.json` written
beside `site/`.

**It is not in CLI 1.10.0, the current release** — `forklaunch init --help`
does not list `storefront`. Until it ships, `manifest.json` is the handoff:
it is produced and validated on every run, and `references/manifest-schema.md`
is what the command will read. Do not tell a user the registration step is
available; tell them the manifest is ready for it.

### Pointing the clone at the backend

Once the module is running and the catalog is imported:

```bash
node ${CLAUDE_SKILL_DIR}/scripts/bin/migrate.mjs <store-url> --clean --api http://localhost:<port>
```

`--api` switches the runtime bridge from its offline cart to the real module.
Without it, cart actions are handled locally in the browser and filters/search
do nothing — they are inert by design rather than faked, because a filter that
silently returns wrong results is worse than one that visibly does nothing.

**Already captured the store? Don't re-crawl.** `--api` is baked in at capture
time, so using it on an existing capture means walking the whole store again —
wasteful on a 200+ page site, and another round of load on the merchant's
origin. `heroserve-fl.ts` serves a capture you already have and proxies its
cart and checkout to the module instead:

```bash
bun ${CLAUDE_SKILL_DIR}/scripts/catalog/heroserve-fl.ts <capture>/site <port> <module-url> <hmac-secret> [stripe-publishable-key]
```

It intercepts the captured pages' native Shopify cart calls, maps them onto the
module's `/cart`, `/cart/items` and `/checkout` endpoints with HMAC auth. A
shopper browses the migrated storefront and every commerce action lands in the
real module — the same one the catalog was imported into.

Pass the merchant's Stripe **publishable** key (`pk_…`) as the fifth argument
and `/__fl/checkout` becomes a genuine card page: address form, Stripe Payment
Element, and a confirmation screen once the charge clears. The browser gets
only the publishable key and a per-order client secret, so card details go
straight from the shopper to Stripe and never touch this server or the module.

**Products whose pages were never captured still work.** The catalog import
pulls every product from the source platform's API, while the capture only
saves pages the crawler actually walked to — on graza.co that is 79 products
against 8 captured pages, so most product links would be dead. Any
`/products/<handle>` (or `/collections/<x>/products/<handle>`) that has no
captured file is rendered from the module's own catalog instead: real title,
price, image, variant picker, and an add-to-cart wired to the same bridge the
captured pages use. A handle that isn't in the catalog either still 404s.
That took graza.co from 8 reachable product pages to 79.

#### The two bridges are NOT interchangeable — do not delete `--api`

It is tempting to drop `--api` on the grounds that `heroserve-fl.ts` does the
same job at serve time without a re-crawl. It does not. They overlap on
add-to-cart and diverge everywhere else, and the divergence is in both
directions:

**Only `--api` / `bridge.js` (capture-time) has:**

- **collection filters, sort, and predictive search wired to the module.** This
  is 533 lines of `filtersMain` mapping Shopify's `filter.v.price.*`,
  `filter.v.availability`, `filter.v.option.*` and `sort_by` vocabulary onto
  `GET /product` and `GET /variant`, HMAC-signed from the browser. It is
  careful work — sorts with no backing data are *disabled* rather than silently
  ignored. `heroserve-fl.ts` contains none of it.
- **XMLHttpRequest interception.** `bridge.js` patches `fetch` *and* XHR;
  heroserve's shim patches `fetch` only. A theme that adds to cart over XHR is
  unhandled by heroserve.
- **`/cart/change`, `/cart/update`, `/cart/clear`.** heroserve handles
  `/cart/add` and cart reads. Changing a line quantity or removing an item from
  a captured cart drawer does not reach the module through heroserve.

**Only `heroserve-fl.ts` (serve-time) has:**

- HMAC-signed cart calls with **source-variant-id → module-variant-UUID
  mapping**. `bridge.js`'s backend cart mode forwards raw Shopify paths
  (`<api>/cart/add.js`) to the module, which has no such route.
- the real checkout page, Stripe/PayPal, order polling
- catalog-rendered pages for products the crawl never reached
- content-addressed sibling and query-suffix asset resolution

So: **use `heroserve-fl.ts` for cart and checkout on an existing capture** (no
re-crawl, and it is the one wired correctly to the module's actual routes), and
**re-capture with `--api` only when the demo needs working collection filters
and search**. Closing the gap properly means porting `filtersMain` and the XHR
patch into heroserve's shim; until that is done, deleting `--api` deletes
working functionality with no replacement.

Omit the key and checkout keeps its original one-shot behaviour — it creates
the order and shows the confirmation without collecting payment, which is what
a visual demo without Stripe credentials wants.

> A card charge is not the end of the story: the order only becomes `paid` when
> Stripe's webhook reaches the module, and stock only moves when the worker
> consumes the resulting event. Both must be running, or a paid-looking
> checkout leaves an order stuck at `pending` and inventory untouched.

**Do not report a wired store as working until both gates pass.** With the
module, its worker and `stripe listen --forward-to localhost:<PORT>/webhook/stripe`
running:

```bash
node ${CLAUDE_SKILL_DIR}/scripts/check-wired.mjs http://localhost:<port>
node ${CLAUDE_SKILL_DIR}/scripts/check-purchase.mjs --store http://localhost:<port> --db <DB_NAME> --pg postgresql://<DB_USER>@localhost:<DB_PORT>
```

`check-purchase` reads the module's Postgres directly (a page will happily say
"thank you" for an order that never left `pending`), so its `--db` and `--pg`
must be the values from the module's `.env.local`; the defaults are one
machine's. Relay the two counts (`10/10`, `12/12`) and any FAIL line verbatim.

### Getting the catalog in

The three commands below are the whole pipeline. Their argument shapes are easy
to get wrong, so they're written out exactly:

```bash
# 1. pull — needs a FULL url; a bare domain fails with "fetch() URL is invalid"
bun ${CLAUDE_SKILL_DIR}/scripts/catalog/cli.ts pull https://thestore.com

# 2. normalize — takes the path to raw.json, not a shop name
bun ${CLAUDE_SKILL_DIR}/scripts/catalog/cli.ts normalize data/<slug>/raw.json

# 3. import — POSITIONAL args, not flags: <normalized.json> <module-url> <secret>
bun ${CLAUDE_SKILL_DIR}/scripts/catalog/cli.ts import data/<slug>/normalized.json http://localhost:8001 <hmac-secret>
```

`normalize` writes to `data/<slug>/` derived from the raw file, and falls back
to `data/unknown-shop/` when it can't infer the shop — harmless, but check the
path it prints rather than assuming, or the import step won't find the file.

Whether `migrate.mjs` ran the pull or you did, the output lives in
`${CLAUDE_SKILL_DIR}/scripts/catalog/data/<shop-domain-with-dashes>/`
(`raw.json`, `normalized.json`) regardless of `--out`. Look there, not in the
capture's output directory.

**Squarespace stores** have no `/products.json`; phase `2/4` prints `catalog
step failed` and that is expected. Pull the catalog from the store's own JSON
instead, then import exactly as above:

```bash
# commerce collection path is usually /shop; the preflight's nav list shows it
node ${CLAUDE_SKILL_DIR}/scripts/catalog/pull-squarespace.mjs https://thestore.com /shop
bun ${CLAUDE_SKILL_DIR}/scripts/catalog/cli.ts import data/<slug>/normalized.json http://localhost:8001 <hmac-secret>
```

The crawl discovers products by Shopify's URL shape, so on a Squarespace store
it captures only the product pages the navigation links to directly and the
rest of the shop grid falls to the module-rendered page. Before reporting the
clone as ready, recapture the grid's own product pages — exactly the links the
captured shop page carries, in targeted mode, never a re-crawl:

```bash
links=$(grep -oE 'href="/shop/p/[^"]+"' <outdir>/site/shop.html | cut -d'"' -f2 | awk '!s[$0]++' | head -30 | paste -sd, -)
node ${CLAUDE_SKILL_DIR}/scripts/crawl.js <domain> <outdir> --clean --only "$links"
```

Served with `heroserve-fl.ts` against the module, the captured pages' own
`.sqs-add-to-cart-button` drives the module's cart (the shim binds it), and
`/<collection>/p/<handle>` URLs the crawl never captured render from the
module's catalog. Stores that switched selling off hide those buttons in their
own CSS; the served clone shows them again.

Public-catalog pulls carry no real stock counts (Shopify's public feed exposes
only an in-stock boolean; Squarespace `unlimited` carries no number), so
imported inventory is a placeholder. Use
`pull-admin <shop> --token <t>` with the merchant's read-only Admin token when
the numbers need to be real.

## When a store won't capture

**Client-rendered stores capture** — a real browser runs their JavaScript.
Headless storefronts (Hydrogen, Next.js) are the exception: their product and
collection pages render from an API on every visit and come through thin; the
preflight says AMBER and the section above says what to expect.

**Shops behind a Shopify app proxy, and filter-app collection grids.** Some
stores keep their shop behind an app proxy (`/a/...` or `/apps/...` routes,
often a Nuxt or React app; kettleandfire.com is one), and some draw their
collection grids with a filter app such as Boost. Preflight says AMBER with
reason `app_proxy`. The theme pages capture faithfully; the app's pages come
through thin, with the app's console errors. The report lists those as named
items and the run exits 1. That is the honest result, not a tool failure:
relay the named items as the report gives them.

Two things genuinely stop it:

1. **Anti-bot protection / rate limiting.** Some stores refuse automated
   browsers or throttle a crawl until it stops (the crawler waits out
   rate-limit windows patiently, then keeps what it has). If a human can
   plainly open the site but capture fails or stalls, this is almost
   certainly why. Say so and stop the automated path. **Do not build or
   suggest evasion** — fingerprint spoofing, proxy rotation, CAPTCHA solving —
   against a store the user does not own.
2. **Login-gated catalogs.** Wholesale/B2B/private stores show nothing without
   an account. Rare among consumer storefronts.

**Assisted capture (a real browser, a real person) covers both — and it is
how a walled capture gets FINISHED, not an optional extra.** Rate limiting
is never a reason to deliver a partial migration: these are public pages,
and a person opening them in their own browser is ordinary use of the site.
In a real migration the operator (whoever runs it — an implementer or the
merchant) works with the merchant's consent and captures the missing pages
by hand.

The capture is **operator-driven by design, not agent-driven**: an agent
cannot read a real browser's page content out (the Claude-in-Chrome bridge
blocks page HTML/DOM from being returned — a deliberate exfiltration guard),
and stores' CSP blocks a page from POSTing itself to a local server. So the
transfer rides on a genuine human click via a **bookmarklet** (Chrome blocks
gesture-less downloads; a bookmarklet click is a real gesture, and a Blob
download makes no network request so CSP is irrelevant):

```
# 1. automated capture, as far as it politely gets
node ${CLAUDE_SKILL_DIR}/scripts/crawl.js <domain> <outdir> --complete --clean
# 2. list exactly what is missing
node ${CLAUDE_SKILL_DIR}/scripts/check-complete.mjs <outdir> <domain>
# 3. one-time: build the bookmarklet and have the operator save it as a
#    bookmark (drag to bookmarks bar / New Bookmark with this as the URL)
node ${CLAUDE_SKILL_DIR}/scripts/make-bookmarklet.mjs
# 4. the operator opens each missing page in their browser (logged in if
#    the store is gated) and CLICKS THE BOOKMARKLET — it downloads that
#    page's DOM as flcap__<path>.html. Pace it like a person.
# 5. ingest everything the operator downloaded
node ${CLAUDE_SKILL_DIR}/scripts/import-folder.mjs <outdir> [downloads-dir] [--move]
# 6. resume — imported pages are treated as captured; rewrite runs over them
node ${CLAUDE_SKILL_DIR}/scripts/crawl.js <domain> <outdir> --complete --clean
# 7. re-run the gate until it is green
node ${CLAUDE_SKILL_DIR}/scripts/check-complete.mjs <outdir> <domain>
```

`import-dom.mjs` is the single-page primitive (URL + HTML on stdin) the
folder importer is built on, for scripted one-offs. If the site blocks even
the real browser, respect that and stop — what stays out of bounds is
fingerprint spoofing, proxy rotation, or CAPTCHA solving to defeat a block,
never the polite human-paced browsing above. Assisted pages keep absolute
asset URLs (they load from the live CDN — the clone is browsable; full asset
localization for assisted pages is a known follow-up).

## What will never come across, and why

The feature gate splits its report in two, and the split is the honest part.

**Missing** — things we caused, each with the repair that answers it. The run
is not done while this list is non-empty.

**Deliberately not migrated** — third-party apps: reviews (Okendo, Yotpo,
Judge.me, Loox, Junip), loyalty (Smile), subscriptions (Recharge), chat
(Gorgias), and Shopify's own hosted checkout bundle. Their data lives in those
vendors' databases, not the storefront's, and their scripts are blocked so an
offline demo cannot beacon from a client's machine. These are reported with the
vendor named — never hidden, never counted as failures. A bar that can only be
cleared by abandoning the offline guarantee is not a bar, it is a permanent red
light, and a permanent red light gets ignored.

Two consequences worth knowing when you read a report:

- a blocked tracking pixel is an `<img>` with no pixels. Every "broken image" on
  graza.co's clone was one (bidr.io, roeye.com, dstillery.com). Only broken
  images served from **our** origin are counted.
- a page whose analytics script we refused then throws on the global that script
  was going to define (`fbq is not defined`). Console errors are classified by
  the origin they name and by whether anything of ours actually 404'd, not by a
  vendor keyword list that goes stale.

## Fidelity, honestly

Some storefronts render differently on every load (A/B tests, personalization —
one measured store changed 38% of its elements between two loads five seconds
apart). For those, "matches the live site" has no fixed answer. The tool
captures **one coherent version**; judge it against that capture, not against a
moving live page. `scripts/check_browsable.js <domain> <outdir>` verifies a
clone stands on its own without comparing to live.

## Requirements

**Checked before anything runs.** `migrate.mjs` and `finish.mjs` both call
`scripts/check-prereqs.mjs` up front and refuse to start if something is
missing, naming it and the command that fixes it. That gate exists because all
three of these fail *silently*:

| Missing | What you see instead of an error |
|---|---|
| playwright's chromium | the gates emit **nothing** — which reads as "no failures", not "did not run". This cost an afternoon once. |
| bun | `manifest.json` comes back `catalog: null` and the storefront never serves |
| ffmpeg / ffprobe | `shrink-media` reports every file "FAILED (left as-is)", and `check-budget`'s height probe returns 0 — which compares as `0 <= 720` and **passes** |

A budget gate that passes because it could not measure is worse than no gate,
because it is believed. Run it standalone with
`node scripts/check-prereqs.mjs`.

**Node 18+ and Python 3.** The Playwright npm package installs itself on first
run, but **its ~150MB Chromium build is a separate download that does not**:
`npx playwright install chromium` inside `scripts/`. The prerequisite check
looks for the browser executable on disk, not for the package.

**bun — required for the product catalog, and it does NOT self-install.** The
catalog pipeline is TypeScript executed by bun. Without it the capture still
succeeds and the clone is still browsable, but `manifest.json` comes back with
`catalog: null` and there is no product data to import into ForkLaunch. There
is no loud error — check the manifest. Install from https://bun.sh if
`command -v bun` finds nothing.

Cart, checkout and filters additionally need the ForkLaunch ecommerce module
running — see **Next: getting it into ForkLaunch** above for what that costs.

## Machine notes

- There is no `setsid` on macOS. To keep a server alive after the command
  returns, use `nohup <cmd> > log 2>&1 &`.
- Never run `pkill -f headless_shell` while a migration is still running: it
  kills that run's browser, and there is no resume.
