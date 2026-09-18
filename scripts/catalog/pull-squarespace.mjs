#!/usr/bin/env node
/**
 * pull-squarespace — Squarespace commerce catalog -> normalized.json
 *
 *   node pull-squarespace.mjs https://www.the-store.com [/shop] [--out data/<dir>]
 *
 * Squarespace serves every collection page as JSON with `?format=json`; a
 * commerce collection's `items` carry the product, its variants, prices in
 * integer cents and stock. This walks the pagination and writes the same
 * `normalized.json` shape that `cli.ts import` sends to `POST /catalog-import`
 * (see references/catalog-import-contract.md), so a Squarespace store's
 * catalog lands in the module through the identical import path Shopify uses.
 *
 * heroserve-fl.ts binds the captured pages' own `.sqs-add-to-cart-button` to
 * the module and renders products the crawl never captured from this catalog,
 * so everything imported here is sellable on the clone.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const args = process.argv.slice(2);
const base = (args[0] || '').replace(/\/$/, '');
if (!/^https?:\/\//.test(base)) {
  console.error('usage: node pull-squarespace.mjs https://www.the-store.com [/shop] [--out DIR]');
  process.exit(2);
}
const shopPath = args.find((a, i) => i > 0 && a.startsWith('/')) || '/shop';
const outIdx = args.indexOf('--out');
const host = new URL(base).host.replace(/^www\./, '');
const outDir = outIdx >= 0 ? args[outIdx + 1] : join('data', host.replace(/\./g, '-'));

const UA = 'Mozilla/5.0 (Macintosh) storefront-migrate/0.1';
async function getJson(url) {
  const r = await fetch(url, { headers: { 'User-Agent': UA, Accept: 'application/json' } });
  if (!r.ok) throw new Error(`${r.status} ${r.statusText} for ${url}`);
  return r.json();
}

// Walk /shop?format=json, /shop?format=json&offset=N ... until nextPage is false.
const raw = [];
let url = `${base}${shopPath}?format=json`;
let page = 1;
for (;;) {
  const j = await getJson(url);
  const items = (j.items || []).filter((it) => it.recordType === 11 || Array.isArray(it.variants));
  raw.push(...items);
  process.stderr.write(`page ${page}: +${items.length} products (${raw.length} so far)\n`);
  const next = j.pagination && j.pagination.nextPage && j.pagination.nextPageUrl;
  if (!next) break;
  url = `${base}${next}${next.includes('?') ? '&' : '?'}format=json`;
  page++;
  await new Promise((r) => setTimeout(r, 400)); // polite
}

const strip = (html) => (html || '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();

const products = raw.map((it) => {
  const optionNames = it.variantOptionOrdering || [];
  const variants = (it.variants || []).map((v, i) => {
    const attrs = v.attributes || {};
    const title = Object.values(attrs).join(' / ') || 'Default Title';
    const price = v.onSale && v.salePrice ? v.salePrice : v.price;
    return {
      externalId: String(v.id),
      sku: v.sku || undefined,
      title,
      optionValues: Object.keys(attrs).length ? attrs : undefined,
      priceCents: Math.round(price || 0),
      compareAtPriceCents: v.onSale && v.salePrice ? Math.round(v.price) : undefined,
      requiresShipping: !it.fulfilledExternally,
      // cli.ts import seeds stock from these two, the same way it does for a
      // Shopify pull: the real count when the platform gives one, otherwise a
      // placeholder from `available`. Squarespace `unlimited` has no count.
      inventoryQuantity: v.unlimited ? null : Math.max(0, Number(v.qtyInStock) || 0),
      available: v.unlimited || Number(v.qtyInStock) > 0,
    };
  });
  const options = optionNames.length
    ? optionNames.map((name) => ({
        name,
        isPackQuantity: false,
        values: [...new Set(variants.map((v) => (v.optionValues || {})[name]).filter(Boolean))],
      }))
    : [{ name: 'Title', isPackQuantity: false, values: ['Default Title'] }];
  const images = [it.assetUrl, ...((it.items || []).map((m) => m.assetUrl))]
    .filter(Boolean)
    .map((src, i) => ({ src, position: i + 1 }));
  return {
    externalId: String(it.id),
    handle: it.urlId,
    sourceUrl: `${base}${it.fullUrl}`,
    title: it.title,
    descriptionHtml: it.body || (it.excerpt ? `<p>${strip(it.excerpt)}</p>` : ''),
    vendor: host,
    productType: it.productType === 1 ? 'physical' : it.productType === 2 ? 'digital' : '',
    tags: [],
    options,
    images,
    variants: variants.length ? variants : [{ externalId: `${it.id}-default`, title: 'Default Title', priceCents: Math.round(it.priceCents || 0), inventoryQuantity: null, available: true }],
  };
});

mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, 'raw.json'), JSON.stringify(raw, null, 1));
writeFileSync(join(outDir, 'normalized.json'), JSON.stringify({ source: { platform: 'squarespace', url: base, pulledAt: new Date().toISOString() }, products }, null, 1));
const nv = products.reduce((n, p) => n + p.variants.length, 0);
console.log(`pulled ${raw.length} products from ${base}${shopPath}`);
console.log(`normalized ${products.length} products / ${nv} variants`);
console.log(`  -> ${join(outDir, 'normalized.json')}`);
