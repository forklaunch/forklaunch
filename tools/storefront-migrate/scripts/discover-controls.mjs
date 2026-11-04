#!/usr/bin/env node
/**
 * discover-controls — work out, per capture, which elements are this theme's
 * cart and checkout controls, instead of guessing them from a selector.
 *
 *   node discover-controls.mjs <site-dir> <served-url> [--routes /,/products/x]
 *
 * WHY THIS EXISTS
 *
 * The bridge in heroserve-fl.ts intercepts two things. Add-to-cart is easy and
 * has always worked everywhere, because `POST /cart/add.js` is a CONTRACT:
 * every Shopify theme, Liquid or headless, posts to that exact path. Intercept
 * the path and you have intercepted the feature, whatever the button looks
 * like.
 *
 * Checkout had no such contract. It was matched with
 *
 *     [name="checkout"], [href*="/checkout"], [href="/cart"]
 *
 * which is a CONVENTION — an accurate description of Dawn and its descendants
 * and of nothing else. A theme whose checkout button is a <button> with an
 * onclick, or an <a> to /checkouts/, or an icon-only control inside a drawer
 * that builds its href in JavaScript, matches none of it. The failure is
 * silent in the worst way: the button is right there, it looks correct, and
 * clicking it leaves the migrated store for the merchant's real checkout — in
 * front of whoever is being shown the demo. That is the single thing most
 * likely to break on an unusual theme.
 *
 * So: find them the way a person does, by what the control SAYS it is. This
 * walks the served capture, classifies every interactive element by role and
 * accessible name using the same code the feature gate uses (inventory.mjs —
 * shared on purpose, so the gate and the bridge can never disagree about what
 * a checkout button is), and writes what it found to
 * `<site>/_fl-controls.json`.
 *
 * heroserve-fl.ts reads that file at startup. It also does the same name-based
 * classification live in the browser, which covers controls this pass never
 * saw — a button inside a drawer that only exists after the drawer opens. The
 * JSON adds the ones a name cannot identify: icon-only controls, keyed by a
 * durable attribute selector instead.
 *
 * Three layers, deliberately, weakest last: contract, then name, then
 * selector.
 */
import { chromium } from 'playwright';
import { writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { SNAPSHOT } from './inventory.mjs';

const SITE = process.argv[2];
const BASE = (process.argv[3] || 'http://localhost:4180').replace(/\/$/, '');
const routesArg = process.argv.indexOf('--routes');

if (!SITE || !existsSync(SITE)) {
  console.error('usage: discover-controls.mjs <site-dir> <served-url> [--routes a,b]');
  process.exit(2);
}

/**
 * A selector that will still find this element on the next page load.
 *
 * Ordered by how long each kind of hook survives. An id or a name attribute is
 * authored and stable. A data- attribute is usually the theme's own hook and
 * nearly as good. A class combination is stable until the theme is rebuilt. An
 * nth-child path is last because a hydrating React tree rewrites it on every
 * render — useful only as a final fallback, and marked as such.
 */
const DURABLE_SELECTOR = function (el) {
  const esc = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : String(s).replace(/[^\w-]/g, '\\$&'));
  const unique = (sel) => { try { return document.querySelectorAll(sel).length === 1 ? sel : null; } catch { return null; } };

  if (el.id) { const s = unique('#' + esc(el.id)); if (s) return { sel: s, tier: 'id' }; }
  for (const attr of ['name', 'data-testid', 'data-test', 'data-action', 'data-cart-action', 'data-checkout', 'aria-label']) {
    const v = el.getAttribute(attr);
    if (!v) continue;
    const s = unique(`${el.tagName.toLowerCase()}[${attr}="${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"]`);
    if (s) return { sel: s, tier: 'attribute' };
  }
  const cls = (el.className && el.className.baseVal !== undefined ? el.className.baseVal : el.className) || '';
  const parts = String(cls).trim().split(/\s+/).filter((c) => c && !/^(is-|js-)?(active|open|selected|hidden|visible)$/.test(c)).slice(0, 3);
  if (parts.length) {
    const s = unique(el.tagName.toLowerCase() + '.' + parts.map(esc).join('.'));
    if (s) return { sel: s, tier: 'class' };
  }
  // Positional last resort. Recorded with its tier so a consumer can weight it
  // lower — on a hydrating storefront this is worth very little.
  const path = [];
  let n = el;
  while (n && n.nodeType === 1 && path.length < 6 && n !== document.body) {
    const i = [...n.parentNode.children].indexOf(n) + 1;
    path.unshift(`${n.tagName.toLowerCase()}:nth-child(${i})`);
    n = n.parentNode;
  }
  return { sel: 'body > ' + path.join(' > '), tier: 'position' };
};

const WANTED = new Set(['checkout', 'cart', 'addcart', 'search', 'menu', 'filter', 'carousel']);

const browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();

async function routes() {
  if (routesArg > 0) return process.argv[routesArg + 1].split(',');
  await page.goto(BASE + '/', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(2500);
  const found = await page.evaluate(() => {
    const h = [...document.querySelectorAll('a[href]')].map((a) => a.getAttribute('href') || '').filter((x) => !/^https?:/.test(x));
    return [h.find((x) => /\/products\//.test(x)), h.find((x) => /\/collections\//.test(x))].filter(Boolean);
  });
  return ['/', ...found.map((h) => (h.startsWith('/') ? h : '/' + h))];
}

const byKind = {};
const names = {};

for (const route of await routes()) {
  await page.goto(BASE + route, { waitUntil: 'load', timeout: 60000 }).catch(() => {});
  await page.waitForTimeout(3000);

  // Open every disclosure first. A theme's checkout button usually lives
  // inside a cart drawer that does not exist in the DOM until the drawer has
  // been opened once — so a pass that only reads the initial markup finds the
  // cart control and misses the checkout control it leads to, which is exactly
  // the one that matters.
  await page.evaluate(() => {
    for (const d of document.querySelectorAll('details')) d.open = true;
    for (const el of document.querySelectorAll('[aria-expanded="false"]')) {
      try { el.click(); } catch (_) {}
    }
  }).catch(() => {});
  await page.waitForTimeout(1200);

  const snap = await page.evaluate(SNAPSHOT).catch(() => null);
  if (!snap) continue;

  for (const c of snap.controls) {
    if (!WANTED.has(c.kind)) continue;
    const sel = await page.evaluate(([id, fn]) => {
      const el = document.querySelector(`[data-fl-ctl="${id}"]`);
      return el ? new Function('el', 'return (' + fn + ')(el)')(el) : null;
    }, [c.ctl, DURABLE_SELECTOR.toString()]).catch(() => null);
    if (!sel) continue;
    (byKind[c.kind] ||= []).push({ name: c.name, role: c.role, ...sel, href: c.href, route });
    (names[c.kind] ||= new Set()).add(c.name.toLowerCase());
  }
}

await browser.close();

// De-duplicate by selector, and drop positional selectors when a better hook
// for the same kind already exists — a fragile selector alongside a durable
// one adds risk and no reach.
const out = { generatedAt: Date.now(), base: BASE, kinds: {}, names: {} };
for (const [kind, list] of Object.entries(byKind)) {
  const seen = new Set();
  const durable = list.filter((x) => x.tier !== 'position');
  const keep = (durable.length ? durable : list).filter((x) => !seen.has(x.sel) && seen.add(x.sel));
  out.kinds[kind] = keep.slice(0, 24);
}
for (const [kind, set] of Object.entries(names)) out.names[kind] = [...set].slice(0, 24);

const dest = join(SITE, '_fl-controls.json');
writeFileSync(dest, JSON.stringify(out, null, 1));

const summary = Object.entries(out.kinds).map(([k, v]) => `${k}×${v.length}`).join(' ') || 'nothing';
console.log(`discovered ${summary} -> ${dest}`);
// A capture with no checkout control found is worth saying out loud. It is not
// necessarily wrong (a store can put checkout behind a drawer this pass could
// not open) but it means the bridge is running on name matching alone there.
if (!out.kinds.checkout) {
  console.log('  note: no checkout control identified by selector — the runtime');
  console.log('        name matcher is the only thing intercepting checkout on this capture.');
}
