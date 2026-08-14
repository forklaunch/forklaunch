#!/usr/bin/env node
/**
 * Unit tests for the URL->file mapping that decides where every captured page
 * lands. The dangerous failure is a COLLISION: two distinct storefront URLs
 * mapping to the same output file, which silently drops one page — a dead/wrong
 * link a client could hit. These tests pin the mapping and prove no collision
 * across a realistic URL set.
 *
 *   node check-complete.test.mjs
 */
// Test the REAL shared mapping (urlmap.js) — the exact one crawl.js uses.
import { createRequire } from 'node:module';
const { pageFileFor: rawPageFileFor } = createRequire(import.meta.url)('./urlmap.js');
const pageFileFor = (p) => { const r = rawPageFileFor(p); return r ? r.file : null; };

let fails = 0;
const ok = (cond, msg) => { if (!cond) { console.log('  FAIL ' + msg); fails++; } else console.log('  ok   ' + msg); };
const eq = (a, b, msg) => ok(a === b, `${msg} (got ${JSON.stringify(a)}, want ${JSON.stringify(b)})`);

// --- exact mapping ---
eq(pageFileFor('/'), 'index.html', 'home');
eq(pageFileFor('/products/sizzle'), 'products/sizzle.html', 'product');
eq(pageFileFor('/products/sizzle/'), 'products/sizzle.html', 'product trailing slash');
eq(pageFileFor('/collections/olive-oil/products/sizzle'), 'products/sizzle.html', 'product via collection path');
eq(pageFileFor('/collections/all'), 'collections/all.html', 'collection');
eq(pageFileFor('/pages/about'), 'pages/about.html', 'page');
eq(pageFileFor('/blogs/recipes/pesto'), 'blogs/recipes-pesto.html', 'blog article');

// --- skipped (must be null, not captured) ---
for (const p of ['/cart', '/checkout', '/account', '/account/login', '/apps/x', '/search?q=a'.split('?')[0]])
  eq(pageFileFor(p), null, `skip ${p}`);
eq(pageFileFor('/agents.md'), null, 'file with extension skipped');

// --- COLLISION CHECK: distinct real DESTINATIONS must never share a file ---
// (Note: /products/a and /collections/x/products/a are the SAME product and
// correctly share a file — that's intentional dedup, so they're not listed as
// distinct here. This set is genuinely different pages.)
const distinctUrls = [
  '/', '/products/a', '/products/b', '/products/a-b', '/products/a_b',
  '/collections/all', '/collections/sale',
  '/pages/about', '/pages/contact', '/blogs/news/one', '/blogs/news/two',
  '/blogs/recipes/one', // /blogs/news/one vs /blogs/recipes/one must differ
  '/about', '/shop',
];
const fileToUrl = new Map();
const collisions = [];
for (const u of distinctUrls) {
  const f = pageFileFor(u);
  if (!f) continue;
  // A collection and its nested product legitimately share a file only when
  // they resolve to the same product; here they don't, so any dup is a bug.
  if (fileToUrl.has(f) && fileToUrl.get(f) !== u) collisions.push(`${u} and ${fileToUrl.get(f)} -> ${f}`);
  else fileToUrl.set(f, u);
}
ok(collisions.length === 0, `no collisions across ${distinctUrls.length} distinct URLs` +
  (collisions.length ? ': ' + collisions.join('; ') : ''));

// Specifically prove the two same-slug-different-blog articles don't collide.
ok(pageFileFor('/blogs/news/one') !== pageFileFor('/blogs/recipes/one'),
   'same slug under different blogs map to different files');

console.log(fails === 0 ? '\nALL PASS' : `\n${fails} FAILURES`);
process.exit(fails === 0 ? 0 : 1);
