#!/usr/bin/env node
/**
 * Emit manifest.json — the one machine-readable file a scaffolding tool (the
 * planned `forklaunch init storefront --from <url>`) needs to read to find
 * out what was captured, without reaching into crawl.js's internals or the
 * catalog pipeline's data/ directory itself.
 *
 * Runs last in the pipeline, after crawl (and catalog, if it ran), so every
 * file it might read already exists — or doesn't, in which case the matching
 * manifest field is null. This script never invents a value for something it
 * didn't capture; a missing input is null in the output, not a guess.
 *
 *   node manifest.js <domain> <outdir> [--url <url>] [--stack "<stack>"]
 *
 * See ../references/manifest-schema.md for what each field means.
 */
const fs = require('fs');
const path = require('path');

const SCHEMA_VERSION = 1;

const argv = process.argv.slice(2);
const domain = argv[0];
const outdir = argv[1];
const urlIdx = argv.indexOf('--url');
const sourceUrl = urlIdx > -1 ? argv[urlIdx + 1] : `https://${domain}`;
const stackIdx = argv.indexOf('--stack');
const stack = stackIdx > -1 ? argv[stackIdx + 1] : null;
// Set by migrate.mjs whenever the catalog step didn't freshly produce
// normalized.json THIS run (skipped, --no-catalog, or pull/normalize
// failed). catalog/data/<slug>/ is keyed only by domain, not by --out, so a
// stale normalized.json from an earlier run of the same domain can still be
// sitting there — this flag is what stops that leftover file from being
// reported as this run's catalog.
const noCatalog = argv.includes('--no-catalog');

function readJson(p) {
  try { return JSON.parse(fs.readFileSync(p, 'utf8')); } catch (_) { return null; }
}

// ---- capturedAt --------------------------------------------------------
// crawl.js/capture.js both stamp startedAt (epoch ms) into their own result
// file; reuse that rather than "now", so re-running manifest.js alone still
// reports when the capture actually happened.
function readCapturedAt() {
  const crawl = readJson(path.join(outdir, 'crawl.json'));
  if (crawl && crawl.startedAt) return new Date(crawl.startedAt).toISOString();
  const capture = readJson(path.join(outdir, 'capture.json'));
  if (capture && capture.startedAt) return new Date(capture.startedAt).toISOString();
  return new Date().toISOString();
}

// ---- pages ---------------------------------------------------------------
// crawl.js writes pages.json alongside crawl.json with the route (original
// storefront path), local file and title it already knows for every page it
// wrote — read that back rather than re-deriving routes from filenames.
// Several file layouts (the generic fallback in crawl.js's pageFileFor)
// collapse multi-segment paths into one dash-joined segment and cannot be
// inverted reliably, so guessing from the filename would risk a wrong route.
function readPages() {
  const pages = readJson(path.join(outdir, 'pages.json'));
  if (Array.isArray(pages) && pages.length) return pages;

  // --single (capture.js) only ever writes site/index.html and keeps no page
  // index of its own. That one page's route is unambiguous, so synthesize it
  // rather than reporting zero pages for a run that did capture something.
  const indexPath = path.join(outdir, 'site', 'index.html');
  if (fs.existsSync(indexPath)) {
    const html = fs.readFileSync(indexPath, 'utf8');
    const m = /<title[^>]*>([^<]*)<\/title>/i.exec(html);
    return [{
      route: '/', file: 'index.html', type: 'home',
      title: m ? (m[1].replace(/\s+/g, ' ').trim() || null) : null,
    }];
  }
  return [];
}

// ---- assets ----------------------------------------------------------------
function readAssets() {
  const crawl = readJson(path.join(outdir, 'crawl.json'));
  if (crawl) return { count: crawl.assets || 0, bytes: crawl.bytes || 0 };
  const capture = readJson(path.join(outdir, 'capture.json'));
  if (capture) return { count: capture.assetsSaved || capture.assets || 0, bytes: capture.bytes || 0 };
  return { count: 0, bytes: 0 };
}

// ---- catalog ---------------------------------------------------------------
// Only read what the catalog pipeline actually produced (cli.ts's `normalize`
// step). --no-catalog, a bun install failure, or a pull/normalize error all
// leave no normalized.json for this domain — that must come out as
// catalog: null, never a fabricated zero.
function readCatalog() {
  if (noCatalog) return null;
  const slug = domain.replace(/\./g, '-');
  const dir = path.join(__dirname, 'catalog', 'data', slug);
  const normalized = readJson(path.join(dir, 'normalized.json'));
  if (!normalized) return null;

  const products = (normalized.products || []).map((p) => {
    const variants = (p.variants || []).map((v) => ({
      sku: v.sku || null,
      title: v.title,
      priceCents: typeof v.priceCents === 'number' ? v.priceCents : null,
      available: !!v.available,
    }));
    const prices = variants.map((v) => v.priceCents).filter((n) => n !== null);
    return {
      handle: p.handle,
      title: p.title,
      // The source platform has no separate "product price" — this is the
      // lowest of the product's own variant prices (a real "from" price,
      // not a guess).
      priceCents: prices.length ? Math.min(...prices) : null,
      variants,
    };
  });

  return {
    productCount: products.length,
    variantCount: products.reduce((n, p) => n + p.variants.length, 0),
    // Shopify's public products.json feed (what cli.ts pulls from) carries
    // neither collection membership nor a shop currency. Left null rather
    // than backfilled from the page crawl, which only captures a
    // page-budget-limited subset of collections and would misrepresent the
    // catalog's true collection count.
    collectionCount: null,
    currency: null,
    pulledAt: normalized.source ? normalized.source.pulledAt || null : null,
    products,
  };
}

// ---- gaps --------------------------------------------------------------
// cli.ts's `normalize` command writes gap-report.md next to normalized.json.
// Parse its summary section (kind -> count); the free-text "## Detail" list
// below it is not parsed, it's for a human reading the file directly.
function readGaps() {
  if (noCatalog) return null;
  const slug = domain.replace(/\./g, '-');
  const reportPath = path.join(__dirname, 'catalog', 'data', slug, 'gap-report.md');
  let text;
  try { text = fs.readFileSync(reportPath, 'utf8'); } catch (_) { return null; }

  const totalMatch = /## Gap \/ attention notes \((\d+)\)/.exec(text);
  const byKind = {};
  const kindRe = /^- \*\*([^*]+)\*\*:\s*(\d+)/gm;
  let m;
  while ((m = kindRe.exec(text))) byKind[m[1]] = parseInt(m[2], 10);

  return { total: totalMatch ? parseInt(totalMatch[1], 10) : null, byKind };
}

const manifest = {
  schemaVersion: SCHEMA_VERSION,
  source: {
    domain,
    url: sourceUrl,
    capturedAt: readCapturedAt(),
    stack: stack || null,
  },
  pages: readPages().map((p) => ({
    route: p.route, file: p.file, type: p.type, title: p.title || null,
  })),
  catalog: readCatalog(),
  assets: readAssets(),
  gaps: readGaps(),
  // Static, not derived per-run — true of every capture this pipeline
  // produces, so a consumer never has to discover it the hard way.
  limits: [
    'Cart, search, variant switching and checkout are visual only — no commerce backend is wired up.',
    'Inventory quantities are never captured — the public catalog exposes only an in-stock boolean, never counts.',
    "Third-party app data (reviews, loyalty balances, subscription contracts) lives in other vendors' systems and does not transfer.",
  ],
};

fs.mkdirSync(outdir, { recursive: true });
fs.writeFileSync(path.join(outdir, 'manifest.json'), JSON.stringify(manifest, null, 2));
console.log(JSON.stringify({ ok: true, pages: manifest.pages.length, hasCatalog: !!manifest.catalog }));
