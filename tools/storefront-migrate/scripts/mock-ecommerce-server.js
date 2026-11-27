#!/usr/bin/env node
/**
 * Contract-faithful stand-in for the real ecommerce-stripe module's GET
 * /product and GET /variant, used ONLY to verify bridge.js's filter/sort/
 * search wiring end-to-end (HMAC signing, query mapping, DOM re-render)
 * when the real module cannot be booted.
 *
 * This exists because the real module could not be started locally: its
 * `Orm` DI singleton (blueprint/ecommerce-stripe/registrations.ts) is
 * built with `new MikroORM(mikroOrmOptionsConfig)` and never `.init()`'d,
 * so `Orm.em` is always undefined and every DB-touching route 500s
 * unconditionally — reproduced directly via
 * `pnpm vitest run __test__/purchaseLoop.test.ts` after fixing that
 * test's own missing-env-var walls (ENCRYPTION_KEY, then STRIPE_API_KEY,
 * then PAYPAL_BASE_URL), landing on:
 *   TypeError: Cannot read properties of undefined (reading 'fork')
 *     at registrations.ts:290  (Orm.em.fork(...))
 * That's a pre-existing bug under blueprint/, unrelated to this ticket,
 * reported rather than fixed (see the handoff for FOR-<filters-sort-search>).
 *
 * This server therefore does NOT stand in for "the module works" — it
 * stands in for "here is what a correctly-implemented GET /product and
 * GET /variant, matching the real schemas and the real HMAC scheme byte
 * for byte, would return," so bridge.js's client-side logic can be
 * verified against something that will reject a wrong signature exactly
 * like the real module would.
 *
 * Schemas mirrored from (read, not guessed):
 *   blueprint/ecommerce-stripe/api/controllers/product.controller.ts
 *   blueprint/implementations/ecommerce/base/domain/schemas/zod/product.schema.ts
 *   blueprint/implementations/ecommerce/base/domain/schemas/zod/variant.schema.ts
 *   framework/core/src/http/createHmacToken.ts
 *   framework/core/src/http/middleware/request/auth.middleware.ts
 *
 * Usage: node mock-ecommerce-server.js [port] [hmac-secret] [fixture]
 *   fixture: "coffee" (default) | "wellness"
 */
const http = require('http');
const crypto = require('crypto');
const { URL } = require('url');

const PORT = parseInt(process.argv[2] || '8901', 10);
const HMAC_SECRET = process.argv[3] || 'bridge-verify-secret';
const FIXTURE = process.argv[4] || 'coffee';

// ---------------------------------------------------------------------
// Fixtures — small, realistic catalogs shaped exactly like the real
// module's response DTOs (ProductMapper / VariantMapper).
// ---------------------------------------------------------------------
const FIXTURES = {
  coffee: {
    products: [
      { id: 'p1', externalId: 'e1', handle: 'chocolate-hazelnut-coffee', title: 'Chocolate Hazelnut Coffee', productType: 'Coffee', vendor: 'Death Wish Coffee', images: [{ src: 'https://picsum.photos/seed/p1/600/600', position: 1 }], createdAt: '2024-01-01T00:00:00Z' },
      { id: 'p2', externalId: 'e2', handle: 'medium-roast-coffee', title: 'Medium Roast Coffee', productType: 'Coffee', vendor: 'Death Wish Coffee', images: [{ src: 'https://picsum.photos/seed/p2/600/600', position: 1 }], createdAt: '2024-02-01T00:00:00Z' },
      { id: 'p3', externalId: 'e3', handle: 'valhalla-java-coffee', title: 'Valhalla Java Coffee', productType: 'Coffee', vendor: 'Death Wish Coffee', images: [{ src: 'https://picsum.photos/seed/p3/600/600', position: 1 }], createdAt: '2024-03-01T00:00:00Z' },
      { id: 'p4', externalId: 'e4', handle: 'og-death-wish-coffee', title: 'OG Death Wish Coffee', productType: 'Coffee', vendor: 'Death Wish Coffee', images: [{ src: 'https://picsum.photos/seed/p4/600/600', position: 1 }], createdAt: '2024-04-01T00:00:00Z' },
      { id: 'p5', externalId: 'e5', handle: 'death-wish-mug', title: 'Death Wish Mug', productType: 'Merch', vendor: 'Death Wish Coffee', images: [{ src: 'https://picsum.photos/seed/p5/600/600', position: 1 }], createdAt: '2024-05-01T00:00:00Z' }
    ],
    variants: [
      { id: 'v1', productId: 'p1', externalId: 've1', title: '14oz Ground', priceCents: 1799, optionValues: { Format: 'Ground' } },
      { id: 'v2', productId: 'p2', externalId: 've2', title: '14oz Ground', priceCents: 1599, optionValues: { Format: 'Ground' } },
      { id: 'v3', productId: 'p3', externalId: 've3', title: '16oz Whole Bean', priceCents: 2199, compareAtPriceCents: 2499, optionValues: { Format: 'Whole Bean' } },
      { id: 'v4', productId: 'p4', externalId: 've4', title: '16oz Ground', priceCents: 1999, optionValues: { Format: 'Ground' } },
      { id: 'v5', productId: 'p5', externalId: 've5', title: 'One Size', priceCents: 1499 }
    ],
    // p5 (mug) intentionally has no inventory record -> "out of stock" per
    // product.controller.ts's documented semantics (no record = not in stock).
    inStock: { v1: true, v2: true, v3: false, v4: true, v5: false }
  },
  wellness: {
    products: [
      { id: 'w1', externalId: 'we1', handle: 'shine-organic-lube', title: 'Shine Organic Lube', productType: 'Lubricant', vendor: 'Maude', images: [{ src: 'https://picsum.photos/seed/w1/600/600', position: 1 }], createdAt: '2024-01-15T00:00:00Z' },
      { id: 'w2', externalId: 'we2', handle: 'shine-silicone-lube', title: 'Shine Silicone Lube', productType: 'Lubricant', vendor: 'Maude', images: [{ src: 'https://picsum.photos/seed/w2/600/600', position: 1 }], createdAt: '2024-02-15T00:00:00Z' },
      { id: 'w3', externalId: 'we3', handle: 'vibe-massager', title: 'Vibe Massager', productType: 'Device', vendor: 'Maude', images: [{ src: 'https://picsum.photos/seed/w3/600/600', position: 1 }], createdAt: '2024-03-15T00:00:00Z' },
      { id: 'w4', externalId: 'we4', handle: 'number-2-massage-candle', title: 'Number 2 Massage Candle', productType: 'Candle', vendor: 'Maude', images: [{ src: 'https://picsum.photos/seed/w4/600/600', position: 1 }], createdAt: '2024-04-15T00:00:00Z' }
    ],
    variants: [
      { id: 'x1', productId: 'w1', externalId: 'xe1', title: '4 fl oz / Charcoal', priceCents: 1600, optionValues: { color: 'charcoal' } },
      { id: 'x2', productId: 'w1', externalId: 'xe2', title: '8 fl oz / Green', priceCents: 2400, optionValues: { color: 'green' } },
      { id: 'x3', productId: 'w2', externalId: 'xe3', title: '4 fl oz / Charcoal', priceCents: 1800, optionValues: { color: 'charcoal' } },
      { id: 'x4', productId: 'w3', externalId: 'xe4', title: 'Standard / Green', priceCents: 13500, optionValues: { color: 'green' } },
      { id: 'x5', productId: 'w4', externalId: 'xe5', title: 'Standard / Charcoal', priceCents: 3200, optionValues: { color: 'charcoal' } }
    ],
    inStock: { x1: true, x2: true, x3: false, x4: true, x5: true }
  }
};

const DATA = FIXTURES[FIXTURE];
if (!DATA) {
  console.error('unknown fixture "' + FIXTURE + '" — use "coffee" or "wellness"');
  process.exit(1);
}

// ---------------------------------------------------------------------
// HMAC verification — byte-for-byte the same algorithm as
// framework/core/src/http/createHmacToken.ts + auth.middleware.ts.
// ---------------------------------------------------------------------
function computeSignature(method, signedPath, bodyString, ts, nonce) {
  const hmac = crypto.createHmac('sha256', HMAC_SECRET);
  hmac.update(`${method}\n${signedPath}\n${bodyString}${ts}\n${nonce}`);
  return hmac.digest('base64');
}

function verifyAuth(req) {
  const header = req.headers['authorization'];
  if (!header) return { ok: false, reason: 'missing authorization header' };
  const [prefix, ...parts] = header.split(' ');
  if (prefix !== 'HMAC' || parts.length < 4) return { ok: false, reason: 'malformed HMAC header' };
  const map = {};
  parts.forEach((p) => {
    const idx = p.indexOf('=');
    if (idx === -1) return;
    map[p.slice(0, idx)] = p.slice(idx + 1);
  });
  if (!map.keyId || !map.ts || !map.nonce || !map.signature) {
    return { ok: false, reason: 'missing keyId/ts/nonce/signature' };
  }
  // GET requests here always have no body -> bodyString is the literal
  // "undefined" text, matching createHmacToken.ts's `${bodyString}` when
  // bodyString itself is JS `undefined` (see bridge.js's signHmacGet).
  const expected = computeSignature(req.method, '/', 'undefined', map.ts, map.nonce);
  if (expected !== map.signature) {
    return { ok: false, reason: 'signature mismatch', expected, got: map.signature };
  }
  return { ok: true };
}

// ---------------------------------------------------------------------
// GET /product — mirrors product.controller.ts's listProducts exactly,
// including the variant-level narrowing done there (price/stock/option).
// ---------------------------------------------------------------------
function listProducts(query) {
  const title = query.get('title');
  const minPriceCents = query.get('minPriceCents') != null ? Number(query.get('minPriceCents')) : null;
  const maxPriceCents = query.get('maxPriceCents') != null ? Number(query.get('maxPriceCents')) : null;
  const inStockParam = query.get('inStock');
  const inStock = inStockParam == null ? null : inStockParam === 'true';
  const optionName = query.get('optionName');
  const optionValue = query.get('optionValue');

  const hasVariantFilter = minPriceCents != null || maxPriceCents != null || inStock != null || (optionName != null && optionValue != null);

  let productIds = null;
  if (hasVariantFilter) {
    let candidates = DATA.variants.slice();
    if (minPriceCents != null) candidates = candidates.filter((v) => v.priceCents >= minPriceCents);
    if (maxPriceCents != null) candidates = candidates.filter((v) => v.priceCents <= maxPriceCents);
    if (optionName != null && optionValue != null) {
      candidates = candidates.filter((v) => v.optionValues && v.optionValues[optionName] === optionValue);
    }
    if (inStock != null) {
      candidates = candidates.filter((v) => (DATA.inStock[v.id] === true) === inStock);
    }
    productIds = [...new Set(candidates.map((v) => v.productId))];
  }

  let products = DATA.products.slice();
  if (productIds) products = products.filter((p) => productIds.includes(p.id));
  if (title) {
    const needle = title.toLowerCase();
    products = products.filter((p) => p.title.toLowerCase().includes(needle));
  }
  return products;
}

function listVariants() {
  return DATA.variants;
}

// ---------------------------------------------------------------------
const server = http.createServer((req, res) => {
  const u = new URL(req.url, `http://localhost:${PORT}`);
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Headers', 'authorization, content-type');
  res.setHeader('Access-Control-Allow-Methods', 'GET, OPTIONS');
  if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }

  const auth = verifyAuth(req);
  if (!auth.ok) {
    console.log(`[mock] 403 ${req.method} ${u.pathname}${u.search} — ${auth.reason}`);
    res.writeHead(403, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: 'Invalid Authorization signature.', reason: auth.reason }));
    return;
  }

  if (req.method === 'GET' && u.pathname === '/product') {
    const result = listProducts(u.searchParams);
    console.log(`[mock] 200 GET /product${u.search} -> ${result.length} product(s): ${result.map((p) => p.title).join(', ')}`);
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify(result));
    return;
  }

  if (req.method === 'GET' && u.pathname === '/variant') {
    const result = listVariants();
    console.log(`[mock] 200 GET /variant -> ${result.length} variant(s)`);
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify(result));
    return;
  }

  res.writeHead(404, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ error: 'not found' }));
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`mock ecommerce server (fixture=${FIXTURE}) on http://127.0.0.1:${PORT}  (hmac secret set)`);
});
