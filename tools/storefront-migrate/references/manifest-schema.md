# manifest.json — schema reference

`manifest.json` is written at the root of a run's output directory (sibling
of `site/` and `crawl.json`), by `../scripts/manifest.js`, as the last step
of `bin/migrate.mjs`. It is the one file the `forklaunch init
storefront --from <path>` CLI command reads — everything it
describes already exists on disk by the time manifest.js runs; it does not
capture anything new.

A working reference implementation of everything below exists at
`../scripts/manifest.js` and is safe to read end-to-end alongside this doc.

## Ground rule: null over guess

Every field below that manifest.js could not read from something the
pipeline actually produced is `null`, never a fabricated value — no
zero-filled counts, no inferred currency, no interpolated page. If a field is
`null`, the corresponding capture step either didn't run (`--no-catalog`,
`--single`) or failed partway through. Treat `null` as "not captured", not as
"empty".

## Top-level shape

```ts
{
  schemaVersion: number;
  source: {
    domain: string;
    url: string;
    capturedAt: string;       // ISO 8601
    stack: string | null;
  };
  pages: Array<{
    route: string;
    file: string;
    type: 'home' | 'collection' | 'product' | 'page';
    title: string | null;
  }>;
  catalog: {
    productCount: number;
    variantCount: number;
    collectionCount: null;
    currency: null;
    pulledAt: string | null;
    products: Array<{
      handle: string;
      title: string;
      priceCents: number | null;
      variants: Array<{
        sku: string | null;
        title: string;
        priceCents: number | null;
        available: boolean;
      }>;
    }>;
  } | null;
  assets: { count: number; bytes: number };
  gaps: { total: number | null; byKind: Record<string, number> } | null;
  limits: string[];
}
```

## Field by field

### `schemaVersion`

Currently always `1`. Bump this if the shape below ever changes in a way
that isn't purely additive, so a consumer can tell an old manifest from a new
one instead of guessing from which fields happen to be present.

### `source`

- `domain` — the bare domain passed on the command line (e.g. `graza.co`,
  no `www.`, no scheme).
- `url` — the URL actually captured; `--url` if given, else
  `https://<domain>`.
- `capturedAt` — when the capture ran, taken from `crawl.json`'s (or
  `capture.json`'s) own `startedAt` timestamp, not from when manifest.js
  itself ran. Re-running manifest.js alone against old output still reports
  the original capture time.
- `stack` — the storefront platform/theme string `preflight.js` detected
  (e.g. `"Shopify"`), passed in via `--stack`. `null` if preflight didn't run
  (`--no-preflight`) or didn't recognize the stack.

### `pages`

One entry per page actually written under `site/`. Read back from
`pages.json` (written by `crawl.js` alongside `crawl.json`), not re-derived
from filenames — several file-layout rules in `crawl.js`'s `pageFileFor`
(the generic multi-segment fallback in particular) collapse a path into a
filename that can't be inverted reliably, so guessing the route from the
file would risk reporting a route that was never actually captured.

- `route` — the original storefront path, e.g. `/collections/all`,
  `/products/olive-oil`, `/`. Exactly what was requested, before any
  local-file rewriting.
- `file` — the path of the saved page, relative to `site/`, e.g.
  `collections/all.html`. Every entry here corresponds to a real file at
  `site/<file>`; there are no phantom entries and no captured page missing
  from this list.
- `type` — `"home"` for `/`, `"product"` for anything under `/products/`
  (including the `/collections/x/products/y` nested form Shopify also
  serves), `"collection"` for `/collections/*`, `"page"` for everything else
  (info pages, and the generic-fallback paths `crawl.js` also mirrors).
- `title` — the page's `<title>` text, whitespace-collapsed. `null` if the
  page had no `<title>` tag or it was empty.

`capture.js`'s `--single` mode (homepage only, no page index of its own)
falls back to synthesizing the one unambiguous entry — `{ route: "/", file:
"index.html", type: "home" }` — by reading `site/index.html` directly,
rather than reporting zero pages for a run that did capture something.

### `catalog`

`null` whenever the catalog step didn't produce fresh output this run:
`--no-catalog`, bun not installed, or the `pull`/`normalize` step failing.
This is tracked by `bin/migrate.mjs` (a `catalogReady` flag passed to
manifest.js as `--no-catalog` when false) rather than by manifest.js merely
checking whether `normalized.json` exists on disk — that file is keyed only
by domain, not by this run's `--out`, so a stale file from an earlier
migration of the same domain must not be reported as this run's catalog.

When present, read from `scripts/catalog/data/<domain-with-dashes>/normalized.json`
(the output of `cli.ts normalize`):

- `productCount` / `variantCount` — counted directly from the products/
  variants that made it into `normalized.json` (i.e. after `cli.ts`'s own
  junk filtering).
- `collectionCount` — always `null`. Shopify's public `products.json` feed
  (what `cli.ts pull` reads) carries no collection membership at all; the
  page crawl does see some collections, but only the page-budget-limited
  subset it happened to capture, and backfilling from that would
  misrepresent the catalog's true collection count. Left `null` rather than
  reporting a partial number that looks authoritative.
- `currency` — always `null` for the same reason: not present in the public
  feed `cli.ts` pulls from.
- `pulledAt` — `normalized.json`'s own `source.pulledAt`, i.e. when
  `cli.ts pull` ran (which may predate this migrate.mjs run, if the catalog
  step was skipped this time but ran on an earlier run of the same domain
  and this run's `catalogReady` is true).
- `products[].priceCents` — the lowest of the product's own variant prices.
  The source platform has no separate "product price" field; this is a real
  "from" price computed from real variant data, not a guess. `null` if the
  product somehow has no variants with a known price.
- `products[].variants[].sku` — `null` when the source data had no SKU
  (`normalize.ts` already flags this as a `missing-sku` gap; see `gaps`
  below). Never fabricated.

### `assets`

- `count` — number of distinct asset URLs captured (`crawl.json`'s
  `assets`, or `capture.json`'s `assetsSaved`/`assets` for `--single`).
- `bytes` — total bytes written to `site/_a/`.

Both `0` if no result file could be read at all (shouldn't happen in
practice — `manifest.js` only runs after a successful capture step — but
this is the honest fallback rather than a crash).

### `gaps`

`null` under the same conditions as `catalog` (no fresh catalog step this
run). When present, parsed from `gap-report.md` (written by `cli.ts
normalize` next to `normalized.json`):

- `total` — the count from the report's `## Gap / attention notes (N)`
  header. `null` if that header couldn't be parsed (report format changed
  out from under this parser — better to surface `null` than a wrong count).
- `byKind` — `{ [gapKind]: count }`, parsed from the report's summary
  bullets (`- **missing-sku**: 3`, etc). This is the *summary* section only;
  the report's free-text `## Detail` list (one line per individual gap, e.g.
  which product handle) is for a human reading `gap-report.md` directly and
  is intentionally not duplicated into the manifest.

### `limits`

A static array, identical on every run — true of every capture this
pipeline produces, not derived from what happened to be captured this time:

```json
[
  "Cart, search, variant switching and checkout are visual only — no commerce backend is wired up.",
  "Inventory quantities are never captured — the public catalog exposes only an in-stock boolean, never counts.",
  "Third-party app data (reviews, loyalty balances, subscription contracts) lives in other vendors' systems and does not transfer."
]
```

If the pipeline gains a genuinely new capability (e.g. real inventory
counts via an admin API), remove the matching line here — don't leave a
limit listed that no longer applies.

## Regenerating manifest.json alone

`manifest.js` can be re-run standalone against an existing output directory
without re-crawling anything:

```
node scripts/manifest.js <domain> <outdir> [--url <url>] [--stack "<stack>"] [--no-catalog]
```

It only reads `crawl.json`/`capture.json`, `pages.json`, and (unless
`--no-catalog`) the catalog pipeline's `normalized.json`/`gap-report.md` —
all safe to re-read as many times as needed.
