---
name: storefront-migrate
description: Migrate a live Shopify (or similar) storefront onto ForkLaunch — capture every public page as a browsable, visually faithful clone, pull and import the product catalog, and wire the clone's cart and checkout to the real ecommerce module so it takes actual orders. Also does capture-only when a visual demo is all that's wanted. Use this whenever a store URL shows up alongside words like migrate, clone, mirror, copy, reproduce, move over, or "what would this look like on our platform" — and also for the individual pieces: capturing a store's pages, pulling or importing a Shopify catalog, checking capture fidelity or dead links, or standing a captured storefront up against a running ForkLaunch backend. Reach for it even when the request names only one of those steps rather than a whole migration.
---

# Storefront capture

Turns a live storefront URL into a browsable local clone. Front half of the
ForkLaunch migration pipeline: reproduce what the shopper sees, then wire the
commerce backend underneath it.

## Running it

One command. Dependencies install themselves on first run (~150MB Chromium
download, once).

```bash
node scripts/bin/migrate.mjs <store-url> --clean
```

It captures, then serves at **http://127.0.0.1:4173**. Give the user that URL.

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

**Relay the verdict to the user before proceeding.** Setting expectations up
front is the whole point of the feature — especially if someone is watching a
live run.

Useful flags:

| Flag | Use |
|---|---|
| `--clean` | hide vendor marketing popups AND make cart/account/checkout inert so a click can't jump to the live store. **Use for demos.** |
| `--pages N` | product/collection page budget (default 20). **Not a hard total** — every destination in the site nav is captured *on top* of this (up to 12 more), so nothing in the menu links back to the live store. `--pages 3` on a typical store really captures ~13; the default really captures ~31. |
| `--no-preflight` | skip the assessment |
| `--single` | homepage only — faster, but NOT browsable (links go to the live site) |
| `--out DIR` | where to write. Defaults to `output/<domain>/` **relative to this skill's `scripts/` directory, not your current directory** — pass `--out` explicitly if you want it somewhere you'll find it. |
| `--no-serve` | capture only |
| `--no-catalog` | skip the product-data pull (see Requirements — needs bun) |
| `--measure` | score fidelity vs live (dev signal, not the deliverable) |

### How long it takes, and why it looks stuck

Roughly **20–35 seconds per page**, so a default-scope run is **10–20 minutes**
and 100MB+. A measured 13-page capture took 253s and 116MB.

**HTML pages are only written at the very end of the crawl.** Mid-run the
output directory fills with assets and contains *zero* browsable pages. This
looks exactly like a hang. It isn't — but if you kill it, you get nothing
usable and there is no resume. For a quick first look, use `--pages 3`
(~13 pages, ~4 minutes) rather than the default.

Progress prints one line per captured page. Long silences during asset
downloads are normal.

### Did it work?

The run ends with `✓ N pages, N assets, NMB` and writes `manifest.json` beside
`site/`. That file is the record of what actually came across — page list,
catalog counts, and a `gaps` array naming anything that did not. If
`manifest.json` is absent, the run did not finish. If `catalog` is `null`,
the product-data pull did not run (almost always missing bun — see
Requirements); the clone is still browsable, but there is no product data to
import into ForkLaunch.

## Reporting back

Tell the user the local URL and what was captured (page count, asset count).
Say plainly that navigation works but cart/search/checkout do not.

**Always serve over HTTP.** Opening the HTML with `file://` breaks module
scripts and CORS, and the clone will look broken for reasons unrelated to the
capture. `migrate.mjs` serves automatically; to serve later:

```bash
python3 scripts/serve.py 4173 <outdir>/site
```

## What it does and does not do

**Does:** reproduce the visual storefront — layout, styling, webfonts, imagery,
product photography, prices, swatches, size grids — for any store that renders
in a browser. Works on classic Liquid themes and on headless React storefronts
(Hydrogen, Next.js) alike. Page-to-page navigation works.

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

**Check that your CLI has it** — `forklaunch init --help` should list
`storefront`. If it doesn't, your CLI predates the command and you'll need a
build that includes it. Older notes describing this command as "planned" are
out of date.

### Pointing the clone at the backend

Once the module is running and the catalog is imported:

```bash
node scripts/bin/migrate.mjs <store-url> --clean --api http://localhost:<port>
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
bun scripts/catalog/heroserve-fl.ts <capture>/site <port> <module-url> <hmac-secret>
```

It intercepts the captured pages' native Shopify cart calls, maps them onto the
module's `/cart`, `/cart/items` and `/checkout` endpoints with HMAC auth, and
serves an order-confirmation page at `/__fl/checkout`. A shopper browses the
migrated storefront and every commerce action lands in the real module — the
same one the catalog was imported into.

### Getting the catalog in

The three commands below are the whole pipeline. Their argument shapes are easy
to get wrong, so they're written out exactly:

```bash
# 1. pull — needs a FULL url; a bare domain fails with "fetch() URL is invalid"
bun scripts/catalog/cli.ts pull https://thestore.com

# 2. normalize — takes the path to raw.json, not a shop name
bun scripts/catalog/cli.ts normalize data/<slug>/raw.json

# 3. import — POSITIONAL args, not flags: <normalized.json> <module-url> <secret>
bun scripts/catalog/cli.ts import data/<slug>/normalized.json http://localhost:8001 <hmac-secret>
```

`normalize` writes to `data/<slug>/` derived from the raw file, and falls back
to `data/unknown-shop/` when it can't infer the shop — harmless, but check the
path it prints rather than assuming, or the import step won't find the file.

Public-catalog pulls carry no real stock counts (Shopify's public feed exposes
only an in-stock boolean), so imported inventory is a placeholder. Use
`pull-admin <shop> --token <t>` with the merchant's read-only Admin token when
the numbers need to be real.

## When a store won't capture

**Client-rendered / headless stores are fine** — a real browser runs their
JavaScript. Being "headless" is not an obstacle.

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
node scripts/crawl.js <domain> <outdir> --complete --clean
# 2. list exactly what is missing
node scripts/check-complete.mjs <outdir> <domain>
# 3. one-time: build the bookmarklet and have the operator save it as a
#    bookmark (drag to bookmarks bar / New Bookmark with this as the URL)
node scripts/make-bookmarklet.mjs
# 4. the operator opens each missing page in their browser (logged in if
#    the store is gated) and CLICKS THE BOOKMARKLET — it downloads that
#    page's DOM as flcap__<path>.html. Pace it like a person.
# 5. ingest everything the operator downloaded
node scripts/import-folder.mjs <outdir> [downloads-dir] [--move]
# 6. resume — imported pages are treated as captured; rewrite runs over them
node scripts/crawl.js <domain> <outdir> --complete --clean
# 7. re-run the gate until it is green
node scripts/check-complete.mjs <outdir> <domain>
```

`import-dom.mjs` is the single-page primitive (URL + HTML on stdin) the
folder importer is built on, for scripted one-offs. If the site blocks even
the real browser, respect that and stop — what stays out of bounds is
fingerprint spoofing, proxy rotation, or CAPTCHA solving to defeat a block,
never the polite human-paced browsing above. Assisted pages keep absolute
asset URLs (they load from the live CDN — the clone is browsable; full asset
localization for assisted pages is a known follow-up).

## Fidelity, honestly

Some storefronts render differently on every load (A/B tests, personalization —
one measured store changed 38% of its elements between two loads five seconds
apart). For those, "matches the live site" has no fixed answer. The tool
captures **one coherent version**; judge it against that capture, not against a
moving live page. `scripts/check_browsable.js <domain> <outdir>` verifies a
clone stands on its own without comparing to live.

## Requirements

**Node 18+ and Python 3.** Playwright and its Chromium build install
themselves on first run (~150MB, once).

**bun — required for the product catalog, and it does NOT self-install.** The
catalog pipeline is TypeScript executed by bun. Without it the capture still
succeeds and the clone is still browsable, but `manifest.json` comes back with
`catalog: null` and there is no product data to import into ForkLaunch. There
is no loud error — check the manifest. Install from https://bun.sh if
`command -v bun` finds nothing.

Cart, checkout and filters additionally need the ForkLaunch ecommerce module
running — see **Next: getting it into ForkLaunch** above for what that costs.
