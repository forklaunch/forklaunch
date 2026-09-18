#!/usr/bin/env bun
/**
 * migrate — CLI for migrating a Shopify shop's catalog into a real,
 * running ForkLaunch ecommerce module.
 *
 * Commands:
 *   pull <shop-url>              Fetch the shop's public catalog to a raw file.
 *   normalize <raw.json>         Reshape raw -> normalized model + gap report.
 *   brand <shop-url>             Best-effort logo/color/name extraction, for
 *                                 the local demo storefront only.
 *   import <normalized.json>     Load the normalized catalog into a real
 *                                 ecommerce-stripe server via /catalog-import.
 *
 * Run with: bun src/cli.ts <command> [args]
 */
import { createHmac, randomUUID, createHash } from 'node:crypto';
import { writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { normalizeShopify } from './normalize.ts';
import { extractBrand } from './brand.ts';
import type { GapNote, NormalizedCatalog, NormalizedProduct, NormalizedVariant } from './model.ts';

function shopSlug(url: string): string {
  return url.replace(/^https?:\/\//, '').replace(/\/.*$/, '').replace(/[^a-z0-9]+/gi, '-').toLowerCase();
}

function out(path: string, data: string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
}

async function cmdPull(shopUrl: string) {
  const base = shopUrl.replace(/\/$/, '');
  const slug = shopSlug(shopUrl);
  // Shopify returns the full catalog; page 2+ comes back empty for small shops.
  const all: any[] = [];
  for (let page = 1; page <= 20; page++) {
    const url = `${base}/products.json?limit=250&page=${page}`;
    const res = await fetch(url, { headers: { 'user-agent': 'forklaunch-migrate/0.1' } });
    if (!res.ok) throw new Error(`pull failed: ${res.status} ${res.statusText} for ${url}`);
    const body = (await res.json()) as any;
    const products = body.products ?? [];
    if (products.length === 0) break;
    all.push(...products);
    if (products.length < 250) break;
  }
  const payload = { __pulledAt: new Date().toISOString(), products: all };
  const path = join('data', slug, 'raw.json');
  out(path, JSON.stringify(payload, null, 2));
  console.log(`pulled ${all.length} products from ${base}`);
  console.log(`  -> ${path}`);
}

function groupCounts(gaps: GapNote[]): Record<string, number> {
  const c: Record<string, number> = {};
  for (const g of gaps) c[g.kind] = (c[g.kind] ?? 0) + 1;
  return c;
}

function cmdNormalize(rawPath: string) {
  const raw = JSON.parse(readFileSync(rawPath, 'utf8'));
  // Recover shop base url from the raw file's directory slug is unreliable;
  // read it from the first product's handle-free source if present, else arg.
  const shopArg = process.argv[4];
  const base = shopArg ?? inferBase(raw);
  const slug = shopSlug(base);
  const { catalog, gaps } = normalizeShopify(raw, slug, base);

  const dir = join('data', slug);
  out(join(dir, 'normalized.json'), JSON.stringify(catalog, null, 2));

  const counts = groupCounts(gaps);
  const report = [
    `# Migration gap report — ${slug}`,
    ``,
    `Source: ${catalog.source.sourceUrl}`,
    `Pulled: ${catalog.source.pulledAt}`,
    ``,
    `- Raw products:      ${catalog.source.rawProductCount}`,
    `- Kept products:     ${catalog.source.keptProductCount}`,
    `- Filtered (junk):   ${catalog.source.filteredProductCount}`,
    `- Total variants:    ${catalog.products.reduce((n, p) => n + p.variants.length, 0)}`,
    ``,
    `## Gap / attention notes (${gaps.length})`,
    ...Object.entries(counts).map(([k, n]) => `- **${k}**: ${n}`),
    ``,
    `## Detail`,
    ...gaps.map((g) => `- [${g.kind}] ${g.detail}`),
    ``,
  ].join('\n');
  out(join(dir, 'gap-report.md'), report);

  console.log(`normalized ${catalog.source.keptProductCount} products (dropped ${catalog.source.filteredProductCount})`);
  console.log(`  -> ${join(dir, 'normalized.json')}`);
  console.log(`  -> ${join(dir, 'gap-report.md')}`);
  console.log(`\ngap summary:`);
  for (const [k, n] of Object.entries(counts)) console.log(`  ${k}: ${n}`);
}

async function cmdBrand(shopUrl: string) {
  const slug = shopSlug(shopUrl);
  const brand = await extractBrand(shopUrl);
  const path = join('data', slug, 'brand.json');
  out(path, JSON.stringify(brand, null, 2));
  console.log(`brand: ${brand.name ?? '(no name found)'}${brand.markSrc ? ', mark found' : ', no mark found'}${brand.primaryColor ? `, color ${brand.primaryColor}` : ''}${brand.heroImageSrc ? ', hero image found' : ''}${brand.fontFamily ? `, font ${brand.fontFamily}` : ''}`);
  console.log(`  -> ${path}`);
}

function inferBase(raw: any): string {
  const first = (raw.products ?? [])[0];
  if (first?.vendor) {
    // best-effort; caller should pass the base url explicitly for accuracy
  }
  return 'https://unknown-shop';
}

/**
 * Signs an HMAC authorization header the same way ForkLaunch's
 * generateHmacAuthHeaders/createHmacToken do (framework/core/src/http).
 *
 * IMPORTANT: `path` must be the path as the *handler* sees it, not the
 * external request URL. ForkLaunch routers mount at a prefix (here,
 * '/catalog-import') and the POST handler sits at '/' inside that router —
 * Express rewrites req.path to be relative to the mount, so the signature
 * has to be computed over '/', not '/catalog-import'. Signing the wrong
 * path is indistinguishable from a bad secret: the server just returns
 * 403 "Invalid Authorization signature" either way.
 */
function signHmac(secretKey: string, method: string, signedPath: string, body: unknown) {
  const timestamp = new Date();
  const nonce = randomUUID();
  const bodyString = body ? `${JSON.stringify(body)}\n` : '';
  const hmac = createHmac('sha256', secretKey);
  hmac.update(`${method}\n${signedPath}\n${bodyString}${timestamp.toISOString()}\n${nonce}`);
  const signature = hmac.digest('base64');
  return `HMAC keyId=default ts=${timestamp.toISOString()} nonce=${nonce} signature=${signature}`;
}

/** Maps our source-agnostic normalized catalog onto the real module's
 *  /catalog-import request schema (ecommerce-stripe/api/controllers/catalogImport.controller.ts). */
export function toImportPayload(cat: NormalizedCatalog) {
  return {
    products: cat.products.map((p) => ({
      externalId: p.externalId,
      handle: p.handle,
      sourceUrl: p.sourceUrl,
      title: p.title,
      descriptionHtml: p.descriptionHtml,
      vendor: p.vendor,
      productType: p.productType,
      tags: p.tags,
      options: p.options,
      images: p.images,
      variants: p.variants.map((v) => ({
        externalId: v.externalId,
        sku: v.sku || undefined,
        title: v.title,
        optionValues: v.optionValues,
        priceCents: v.priceCents,
        compareAtPriceCents: v.compareAtPriceCents ?? undefined,
        requiresShipping: v.requiresShipping,
        // Use the REAL on-hand count when we have it (an Admin pull fills
        // inventoryQuantity). Only when it's null — a public products.json pull,
        // which carries no stock numbers — do we seed a placeholder from the
        // `available` boolean, to be reconciled before cutover. A migration
        // that has the real number should never ship the placeholder.
        initialStock: v.inventoryQuantity != null ? v.inventoryQuantity : (v.available ? 100 : 0)
      }))
    }))
  };
}

// A real store's full catalog is far bigger than the server's 100KB JSON
// body limit (52 Gorilla Mind products alone was ~286KB) — send it in
// batches instead. This also matches Guild's own migration design doc,
// which models migration as a batched, resumable job, not one giant call.
const IMPORT_BATCH_SIZE = 10;

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}

// Mock mode: the throwaway local demo server (src/server.ts) — no auth,
// single endpoint, no size limit worth worrying about.
async function cmdImportMock(normalizedPath: string, serverUrl: string) {
  const cat: NormalizedCatalog = JSON.parse(readFileSync(normalizedPath, 'utf8'));
  const res = await fetch(`${serverUrl.replace(/\/$/, '')}/api/import`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(cat)
  });
  const body = await res.json();
  if (!res.ok) throw new Error(`import failed: ${JSON.stringify(body)}`);
  console.log(`imported ${body.products} products / ${body.variants} variants -> ${serverUrl}`);
}

// Real mode: the real ecommerce-stripe module — HMAC-authenticated,
// batched to stay under its request-size limit.
async function cmdImportReal(normalizedPath: string, serverUrl: string, secretKey: string) {
  const cat: NormalizedCatalog = JSON.parse(readFileSync(normalizedPath, 'utf8'));
  const fullPayload = toImportPayload(cat);
  const batches = chunk(fullPayload.products, IMPORT_BATCH_SIZE);

  const requestPath = '/catalog-import';
  const signedPath = '/'; // see signHmac() docstring — NOT requestPath

  let productsImported = 0;
  let variantsImported = 0;

  for (let i = 0; i < batches.length; i++) {
    const payload = { products: batches[i] };
    const authorization = signHmac(secretKey, 'POST', signedPath, payload);

    const res = await fetch(`${serverUrl.replace(/\/$/, '')}${requestPath}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization },
      body: JSON.stringify(payload)
    });
    const body = await res.json();
    if (!res.ok) {
      throw new Error(
        `import failed on batch ${i + 1}/${batches.length} (${res.status}): ${JSON.stringify(body)}\n` +
        `  Already imported before this batch: ${productsImported} products / ${variantsImported} variants.\n` +
        `  Re-running this command is safe — imports upsert by externalId, nothing already loaded gets duplicated.`
      );
    }
    productsImported += body.productsImported;
    variantsImported += body.variantsImported;
    console.log(`  batch ${i + 1}/${batches.length}: +${body.productsImported} products / +${body.variantsImported} variants`);
  }

  console.log(`imported ${productsImported} products / ${variantsImported} variants -> ${serverUrl}${requestPath}`);
}

// ---- media re-hosting -----------------------------------------------------
// A real cutover must not hotlink the source store's CDN (see
// catalog-import-contract.md, "Media re-hosting" — explicitly the migration
// tool's responsibility, not the import endpoint's). This downloads every
// distinct catalog image, gives the migrated store its own copy, and rewrites
// each image src to an owned URL. Deduplicates identical URLs, runs a bounded
// pool, retries transient failures, and NEVER aborts the whole run for one bad
// image — unreachable images are reported and left pointing at their original
// URL so the failure is visible rather than a silent 404.
const REHOST_CONCURRENCY = 8;
function sleep(ms: number) { return new Promise((r) => setTimeout(r, ms)); }

function extFor(url: string, contentType: string | null): string {
  const ct = contentType && /image\/(png|jpe?g|webp|avif|gif|svg\+xml)/i.exec(contentType);
  if (ct) return ct[1].toLowerCase().replace('jpeg', 'jpg').replace('svg+xml', 'svg');
  const m = /\.(png|jpe?g|webp|avif|gif|svg)(?:[?#]|$)/i.exec(url);
  return m ? m[1].toLowerCase().replace('jpeg', 'jpg') : 'img';
}

async function fetchImage(url: string, tries = 3): Promise<{ buf: Buffer; ext: string } | null> {
  for (let i = 0; i < tries; i++) {
    try {
      const res = await fetch(url, { headers: { 'user-agent': 'forklaunch-migrate/0.1' } });
      if (!res.ok) {
        if (res.status === 429 || res.status >= 500) { await sleep(500 * (i + 1)); continue; }
        return null; // 4xx (gone/forbidden) — retrying won't help
      }
      const buf = Buffer.from(await res.arrayBuffer());
      if (!buf.length) return null;
      return { buf, ext: extFor(url, res.headers.get('content-type')) };
    } catch { await sleep(500 * (i + 1)); }
  }
  return null;
}

async function cmdRehost(normalizedPath: string, opts: { out?: string; base?: string }) {
  const cat: NormalizedCatalog = JSON.parse(readFileSync(normalizedPath, 'utf8'));
  const outDir = opts.out ?? join(dirname(normalizedPath), 'rehosted');
  const mediaDir = join(outDir, 'media');
  mkdirSync(mediaDir, { recursive: true });

  const urls = new Set<string>();
  for (const p of cat.products) for (const img of p.images) if (img.src) urls.add(img.src);
  const list = [...urls];
  console.log(`rehosting ${list.length} distinct images from ${cat.products.length} products` +
    ` (concurrency ${REHOST_CONCURRENCY})...`);

  const map = new Map<string, string>();          // original url -> new url
  const failures: { url: string; reason: string }[] = [];
  let idx = 0, done = 0;

  async function worker() {
    while (idx < list.length) {
      const url = list[idx++];
      const hash = createHash('sha1').update(url).digest('hex').slice(0, 16);
      const got = await fetchImage(url);
      if (!got) { failures.push({ url, reason: 'unreachable' }); map.set(url, url); }
      else {
        const file = `${hash}.${got.ext}`;
        writeFileSync(join(mediaDir, file), got.buf);
        map.set(url, opts.base ? `${opts.base.replace(/\/$/, '')}/${file}` : `media/${file}`);
      }
      if (++done % 25 === 0 || done === list.length) console.log(`  ${done}/${list.length}`);
    }
  }
  await Promise.all(Array.from({ length: REHOST_CONCURRENCY }, worker));

  let rewritten = 0;
  for (const p of cat.products) for (const img of p.images) {
    const nu = map.get(img.src);
    if (nu && nu !== img.src) { img.src = nu; rewritten++; }
  }
  const outCat = normalizedPath.replace(/\.json$/, '') + '.rehosted.json';
  writeFileSync(outCat, JSON.stringify(cat, null, 2));
  const report = {
    distinctImages: list.length,
    downloaded: list.length - failures.length,
    failed: failures.length,
    rewrittenSrcs: rewritten,
    base: opts.base ?? null,
    mediaDir, rehostedCatalog: outCat,
    failures: failures.slice(0, 50),
  };
  writeFileSync(join(outDir, 'rehost-report.json'), JSON.stringify(report, null, 2));
  console.log(`rehosted ${report.downloaded}/${list.length} images ` +
    `(${failures.length} failed), rewrote ${rewritten} src refs`);
  console.log(`  media -> ${mediaDir}`);
  console.log(`  catalog -> ${outCat}`);
  if (failures.length) console.log(`  ${failures.length} unreachable images left at source URL (see rehost-report.json)`);
}

// ---- credentialed Admin API pull (real inventory + SKUs) ------------------
// The public products.json feed cannot expose stock counts, unpublished
// products, or reliable SKUs. The Admin GraphQL API can, with a merchant's
// access token — and that is what turns a seeded demo into an EXACT migration:
// real inventoryQuantity per variant flows straight through normalize -> import
// -> the module's on-hand stock, with no placeholder. normalizeAdminProduct is
// a pure function (unit-tested against a fixture); adminGraphql/cmdPullAdmin are
// the token-gated live adapters (their end-to-end run needs a real dev-store
// token, but the mapping they depend on is fully tested without one).
const ADMIN_API_VERSION = '2024-10';

function weightToGrams(value: number, unit: string): number {
  switch ((unit || '').toUpperCase()) {
    case 'KILOGRAMS': return Math.round(value * 1000);
    case 'GRAMS': return Math.round(value);
    case 'OUNCES': return Math.round(value * 28.3495);
    case 'POUNDS': return Math.round(value * 453.592);
    default: return Math.round(value || 0);
  }
}

function moneyToCents(m: string | number | null | undefined): number | null {
  if (m == null || m === '') return null;
  const n = typeof m === 'number' ? m : parseFloat(m);
  return Number.isFinite(n) ? Math.round(n * 100) : null;
}

/** Map one Shopify Admin GraphQL Product node onto the normalized model. Pure
 *  and total — every field defaulted so a sparse node never throws. */
export function normalizeAdminProduct(node: any, shopBaseUrl: string): NormalizedProduct {
  const handle = node.handle ?? '';
  const options = (node.options ?? []).map((o: any) => ({
    name: o.name ?? '', isPackQuantity: false, values: o.values ?? [],
  }));
  const images = (node.images?.edges ?? [])
    .map((e: any, i: number) => ({ src: e.node?.url ?? '', position: i + 1 }))
    .filter((im: any) => im.src);
  const variants: NormalizedVariant[] = (node.variants?.edges ?? []).map((e: any) => {
    const v = e.node ?? {};
    const optionValues: Record<string, string> = {};
    for (const so of v.selectedOptions ?? []) if (so?.name) optionValues[so.name] = so.value ?? '';
    const inv = v.inventoryItem ?? {};
    const weight = inv.measurement?.weight;
    return {
      externalId: String(v.legacyResourceId ?? v.id ?? ''),
      sku: v.sku ?? '',
      title: v.title ?? '',
      optionValues,
      priceCents: moneyToCents(v.price) ?? 0,
      compareAtPriceCents: moneyToCents(v.compareAtPrice),
      available: v.availableForSale ?? true,
      // The whole point of the Admin pull: the real number, not a placeholder.
      inventoryQuantity: typeof v.inventoryQuantity === 'number' ? v.inventoryQuantity : null,
      requiresShipping: inv.requiresShipping ?? true,
      grams: weight ? weightToGrams(weight.value, weight.unit) : 0,
    };
  });
  return {
    externalId: String(node.legacyResourceId ?? node.id ?? ''),
    handle,
    sourceUrl: `${shopBaseUrl.replace(/\/$/, '')}/products/${handle}`,
    title: node.title ?? '',
    descriptionHtml: node.descriptionHtml ?? '',
    vendor: node.vendor ?? '',
    productType: node.productType ?? '',
    tags: node.tags ?? [],
    options,
    images,
    variants,
  };
}

const ADMIN_PRODUCTS_QUERY = `query($cursor: String) {
  products(first: 50, after: $cursor) {
    edges { node {
      legacyResourceId handle title descriptionHtml vendor productType tags status
      options { name values }
      images(first: 50) { edges { node { url altText } } pageInfo { hasNextPage } }
      variants(first: 100) { edges { node {
        legacyResourceId sku title price compareAtPrice inventoryQuantity availableForSale
        selectedOptions { name value }
        inventoryItem { requiresShipping measurement { weight { value unit } } }
      } } pageInfo { hasNextPage } }
    } }
    pageInfo { hasNextPage endCursor }
  }
}`;

async function adminGraphql(shop: string, token: string, query: string, variables: unknown): Promise<any> {
  // ADMIN_API_ENDPOINT overrides the live Shopify URL — used only to point the
  // pull at a mock server in tests. Unset in real use, so production always
  // hits the merchant's real Admin API.
  const url = process.env.ADMIN_API_ENDPOINT ||
    `https://${shop}/admin/api/${ADMIN_API_VERSION}/graphql.json`;
  for (let attempt = 0; attempt < 6; attempt++) {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'X-Shopify-Access-Token': token },
      body: JSON.stringify({ query, variables }),
    });
    if (res.status === 429) { await sleep(2000 * (attempt + 1)); continue; } // REST-style rate cap
    if (res.status === 401 || res.status === 403)
      throw new Error(`Admin API auth failed (${res.status}) — check the access token and its scopes ` +
        `(needs read_products, read_inventory).`);
    const body: any = await res.json();
    if (body.errors) {
      // GraphQL cost limiter returns THROTTLED as an error, not a 429 — back off and retry.
      if (JSON.stringify(body.errors).includes('THROTTLED')) { await sleep(2000 * (attempt + 1)); continue; }
      throw new Error(`Admin API error: ${JSON.stringify(body.errors)}`);
    }
    return body.data;
  }
  throw new Error('Admin API: exhausted retries against the cost limiter');
}

async function cmdPullAdmin(shopUrl: string, token: string) {
  const shop = shopUrl.replace(/^https?:\/\//, '').replace(/\/.*$/, '');
  const base = `https://${shop}`;
  const products: NormalizedProduct[] = [];
  let cursor: string | null = null;
  do {
    const data = await adminGraphql(shop, token, ADMIN_PRODUCTS_QUERY, { cursor });
    for (const edge of data.products.edges) products.push(normalizeAdminProduct(edge.node, base));
    cursor = data.products.pageInfo.hasNextPage ? data.products.pageInfo.endCursor : null;
    console.log(`  pulled ${products.length} products...`);
    if (cursor) await sleep(500);
  } while (cursor);

  const cat: NormalizedCatalog = {
    source: {
      shop, sourceUrl: base, platform: 'shopify', pulledAt: new Date().toISOString(),
      rawProductCount: products.length, keptProductCount: products.length, filteredProductCount: 0,
    },
    products,
  };
  const path = join('data', shopSlug(shopUrl), 'normalized.json');
  out(path, JSON.stringify(cat, null, 2));
  const totalV = products.reduce((s, p) => s + p.variants.length, 0);
  const withStock = products.reduce((s, p) => s + p.variants.filter((v) => v.inventoryQuantity != null).length, 0);
  console.log(`admin pull: ${products.length} products / ${totalV} variants ` +
    `(${withStock} with real stock counts) -> ${path}`);
  console.log('  (Admin pull writes normalized.json directly — no separate normalize step needed.)');
}

// ---- parity verification (cutover gate) -----------------------------------
// Before a store goes live on ForkLaunch, prove the imported catalog matches
// the source EXACTLY — counts, prices, SKUs, variant sets. Loading data and
// trusting it is not a migration; a cutover gate that blocks on any mismatch
// is. diffCatalogs is a pure function (fully unit-testable) keyed on the
// source platform's stable externalIds so re-ordering or re-pagination can't
// create false diffs. The command compares the source normalized.json against
// a READBACK of what the module actually stored (a normalized dump of the
// module's list/get endpoints — that thin fetch adapter is the one piece that
// needs the running module, and is intentionally kept separate from this
// pure, testable core).
interface ParityReport {
  pass: boolean;
  sourceProducts: number;
  targetProducts: number;
  matchedProducts: number;
  missingFromTarget: string[];
  extraInTarget: string[];
  productMismatches: Array<{ externalId: string; handle: string; field: string; source: unknown; target: unknown }>;
  variantMismatches: Array<{ product: string; variant: string; field: string; source: unknown; target: unknown }>;
  parityScore: number;
}

export function diffCatalogs(source: NormalizedCatalog, target: NormalizedCatalog): ParityReport {
  const sMap = new Map(source.products.map((p) => [p.externalId, p]));
  const tMap = new Map(target.products.map((p) => [p.externalId, p]));
  const missingFromTarget: string[] = [];
  const extraInTarget: string[] = [];
  const productMismatches: ParityReport['productMismatches'] = [];
  const variantMismatches: ParityReport['variantMismatches'] = [];

  for (const id of sMap.keys()) if (!tMap.has(id)) missingFromTarget.push(id);
  for (const id of tMap.keys()) if (!sMap.has(id)) extraInTarget.push(id);

  let matched = 0;
  for (const [id, sp] of sMap) {
    const tp = tMap.get(id);
    if (!tp) continue;
    matched++;
    if (sp.title !== tp.title)
      productMismatches.push({ externalId: id, handle: sp.handle, field: 'title', source: sp.title, target: tp.title });

    const sv = new Map(sp.variants.map((v) => [v.externalId, v]));
    const tv = new Map(tp.variants.map((v) => [v.externalId, v]));
    for (const [vid, svar] of sv) {
      const tvar = tv.get(vid);
      if (!tvar) { variantMismatches.push({ product: id, variant: vid, field: 'presence', source: 'present', target: 'missing' }); continue; }
      if (svar.priceCents !== tvar.priceCents)
        variantMismatches.push({ product: id, variant: vid, field: 'priceCents', source: svar.priceCents, target: tvar.priceCents });
      if ((svar.sku || '') !== (tvar.sku || ''))
        variantMismatches.push({ product: id, variant: vid, field: 'sku', source: svar.sku, target: tvar.sku });
    }
    for (const vid of tv.keys()) if (!sv.has(vid))
      variantMismatches.push({ product: id, variant: vid, field: 'presence', source: 'missing', target: 'extra' });
  }

  const totalChecks = sMap.size + [...sMap.values()].reduce((s, p) => s + p.variants.length, 0);
  const problems = missingFromTarget.length + extraInTarget.length + productMismatches.length + variantMismatches.length;
  return {
    pass: problems === 0,
    sourceProducts: sMap.size,
    targetProducts: tMap.size,
    matchedProducts: matched,
    missingFromTarget, extraInTarget, productMismatches, variantMismatches,
    parityScore: totalChecks ? Math.max(0, 1 - problems / totalChecks) : 1,
  };
}

function cmdVerify(sourcePath: string, readbackPath: string) {
  const source: NormalizedCatalog = JSON.parse(readFileSync(sourcePath, 'utf8'));
  const target: NormalizedCatalog = JSON.parse(readFileSync(readbackPath, 'utf8'));
  const report = diffCatalogs(source, target);
  const outPath = join(dirname(sourcePath), 'parity-report.json');
  out(outPath, JSON.stringify(report, null, 2));
  console.log(`parity: ${report.matchedProducts}/${report.sourceProducts} products matched, ` +
    `score ${(report.parityScore * 100).toFixed(2)}%`);
  if (report.missingFromTarget.length) console.log(`  ✗ ${report.missingFromTarget.length} products missing from target`);
  if (report.extraInTarget.length) console.log(`  ✗ ${report.extraInTarget.length} extra products in target`);
  if (report.productMismatches.length) console.log(`  ✗ ${report.productMismatches.length} product field mismatches`);
  if (report.variantMismatches.length) console.log(`  ✗ ${report.variantMismatches.length} variant mismatches`);
  console.log(report.pass
    ? '  PASS — exact parity, safe to cut over'
    : `  FAIL — do NOT cut over. See ${outPath}`);
  if (!report.pass) process.exit(2);
}

if (import.meta.main) {
  const [, , cmd, arg] = process.argv;
  try {
  if (cmd === 'pull' && arg) await cmdPull(arg);
  else if (cmd === 'pull-admin' && arg) {
    const tokenIdx = process.argv.indexOf('--token');
    const token = (tokenIdx > -1 ? process.argv[tokenIdx + 1] : '') || process.env.SHOPIFY_ADMIN_TOKEN || '';
    if (!token) { console.error('pull-admin needs --token <access-token> or SHOPIFY_ADMIN_TOKEN env'); process.exit(1); }
    await cmdPullAdmin(arg, token);
  }
  else if (cmd === 'normalize' && arg) cmdNormalize(arg);
  else if (cmd === 'brand' && arg) await cmdBrand(arg);
  else if (cmd === 'rehost' && arg) {
    const outIdx = process.argv.indexOf('--out');
    const baseIdx = process.argv.indexOf('--base');
    await cmdRehost(arg, {
      out: outIdx > -1 ? process.argv[outIdx + 1] : undefined,
      base: baseIdx > -1 ? process.argv[baseIdx + 1] : undefined,
    });
  }
  else if (cmd === 'verify' && arg) {
    const readback = process.argv[4];
    if (!readback) { console.error('verify needs: <source-normalized.json> <module-readback.json>'); process.exit(1); }
    cmdVerify(arg, readback);
  }
  else if (cmd === 'import' && arg) {
    const serverUrl = process.argv[4] ?? 'http://localhost:8001';
    const secretKey = process.argv[5] ?? process.env.HMAC_SECRET_KEY;
    // No secret key -> mock mode (the local demo server has no auth at all).
    // Secret key given -> real mode (the real module requires HMAC auth).
    if (!secretKey) {
      await cmdImportMock(arg, serverUrl);
    } else {
      await cmdImportReal(arg, serverUrl, secretKey);
    }
  }
  else {
    console.log('usage:');
    console.log('  bun src/cli.ts pull <shop-url>');
    console.log('  bun src/cli.ts pull-admin <shop-url> --token <admin-access-token>   (real inventory + SKUs)');
    console.log('  bun src/cli.ts normalize <raw.json> <shop-url>');
    console.log('  bun src/cli.ts brand <shop-url>');
    console.log('  bun src/cli.ts rehost <normalized.json> [--out <dir>] [--base <url>]');
    console.log('  bun src/cli.ts verify <source-normalized.json> <module-readback.json>');
    console.log('  bun src/cli.ts import <normalized.json> [server-url] [hmac-secret]');
    process.exit(1);
  }
  } catch (e: any) {
    console.error('error:', e.message);
    process.exit(1);
  }
}
