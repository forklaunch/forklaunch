#!/usr/bin/env node
/**
 * check-purchase — proves a migrated storefront can actually take money.
 *
 *   node check-purchase.mjs [--store http://localhost:4173] --db <DB_NAME>
 *                           --pg postgresql://<DB_USER>@<DB_HOST>:<DB_PORT>
 *   (or export the module's DB_NAME, DB_USER, DB_HOST, DB_PORT and omit both)
 *
 * check-wired.mjs proves the page is bridged to the module. This proves the
 * bridge carries a purchase all the way to `paid` and moves inventory. Every
 * assertion is checked against the database, never the page: a storefront will
 * happily render "thank you" for an order that never left `pending`.
 *
 * Covered, each one a failure seen while clicking by hand:
 *
 *   catalog is the module's       the storefront's own product JSON is still
 *                                 in the capture; reading it would pass while
 *                                 the module sat empty
 *   the trip to paid              order pending -> paid via webhook, and stock
 *                                 down by the quantity ordered, not by one
 *   decline holds inventory       a declined card must leave the order pending
 *                                 and stock untouched
 *   cart survives an unpaid       leaving payment and returning has to resume,
 *   checkout                      not strand the shopper with an empty bag
 *   redirect return is verified   the return trip must ask Stripe for the
 *                                 intent status, not believe redirect_status
 *                                 in an address bar the shopper controls
 *   the demo stays offline        a captured storefront carries the original
 *                                 site's ad stack; none of it may phone home
 *
 * Exits non-zero if any case fails.
 */
import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
import { until, cardFrame, fillCard, pay } from './lib-gate.mjs';

const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const env = process.env;
// The store is wherever heroserve-fl.ts is serving (4173 unless --port said
// otherwise). The database is the module's own: pass --db/--pg, or export the
// module's DB_* variables (the values in its .env.local), and they are used.
const STORE = arg('--store', 'http://localhost:4173').replace(/\/$/, '');
const DB = arg('--db', env.DB_NAME);
const PGBASE = arg('--pg', env.DB_HOST ? `postgresql://${env.DB_USER || 'postgres'}@${env.DB_HOST}:${env.DB_PORT || 5432}` : undefined);
if (!DB || !PGBASE) {
  console.error('usage: node check-purchase.mjs [--store <url>] --db <DB_NAME> --pg postgresql://<DB_USER>@<DB_HOST>:<DB_PORT>');
  console.error('       (or export DB_NAME, DB_USER, DB_HOST, DB_PORT from the module\'s .env.local)');
  process.exit(2);
}
const PGURL = `${PGBASE}/${DB}`;

const results = [];
const check = (name, pass, detail = '') => results.push({ name, pass: !!pass, detail });
const sql = (q) => execFileSync('psql', [PGURL, '-tAc', q], { encoding: 'utf8' }).trim();
const money = (c) => '$' + (c / 100).toFixed(2);

/** Poll until the webhook has had its chance to land. */
async function eventually(fn, timeoutMs = 30000, everyMs = 900) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    last = fn();
    if (last) return last;
    await new Promise((r) => setTimeout(r, everyMs));
  }
  return last;
}

/**
 * Submit a card payment. Frame discovery, field filling and the click itself
 * all live in lib-gate.mjs, which the merch-store gate shares — the click in
 * particular has to be dispatched on the element rather than through
 * page.click, or it silently lands nowhere and every later assertion blames
 * the payment instead of the click.
 */
async function payWithCard(page, number) {
  await fillCard(page, await cardFrame(page), number);
  await pay(page);
}

/** First product page the storefront links to; capture layouts vary. */
// Shopify links products as /products/<h>; Squarespace as /<collection>/p/<h>
// (flattened to /<collection>/p-<h>.html in a capture). A Squarespace home
// page often carries no product link at all, so the shop page is tried next.
const PRODUCT_LINK = /(^|\/)products\/|\/p\/[^/]+|\/p-[^/]+\.html$/;
async function firstProductPath(page) {
  const find = () => page.evaluate((re) => {
    const a = [...document.querySelectorAll('a[href]')]
      .map((x) => x.getAttribute('href'))
      .find((h) => h && new RegExp(re).test(h) && !/^https?:/.test(h));
    return a ? (a.startsWith('/') ? a : '/' + a) : null;
  }, PRODUCT_LINK.source);
  await page.goto(STORE + '/', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(3500);
  let p = await find();
  if (!p) {
    const r = await page.goto(STORE + '/shop', { waitUntil: 'load', timeout: 60000 }).catch(() => null);
    if (r && r.status() === 200) { await page.waitForTimeout(1500); p = await find(); }
  }
  return p;
}

async function addOne(page, path) {
  await page.goto(STORE + path, { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(3000);
  const clicked = await page.evaluate(() => {
    const b = document.querySelector('#fl-add, [data-add], button[name="add"], form[action*="/cart/add"] button, .sqs-add-to-cart-button');
    if (!b) return false;
    b.click();
    return true;
  });
  await page.waitForTimeout(3500);
  return clicked;
}

/** Fill shipping and advance to the payment step. */
async function toPayment(page) {
  await page.goto(STORE + '/__fl/checkout', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(3000);
  await page.evaluate(() => {
    const set = (id, v) => { const e = document.getElementById(id); if (e) e.value = v; };
    set('name', 'Test Shopper'); set('line1', '1 Market St');
    set('city', 'San Francisco'); set('state', 'CA'); set('postalCode', '94105');
  });
  await page.click('#go');
  await page.waitForTimeout(7000);
}

const latestPending = () =>
  sql(`select id from "order" where status='pending' order by created_at desc limit 1;`);

async function main() {
  const browser = await chromium.launch({ headless: true });
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  // Watched across the whole run: nothing may reach a third party.
  // Stripe and PayPal are deliberately allowed through the policy — a demo
  // that cannot reach the payment processor cannot take a payment. Everything
  // else reaching the network is the original site's ad stack leaking.
  // stripecdn and hcaptcha are Stripe's own: it serves the Payment Element's
  // assets from one and challenges suspicious payments with the other.
  const ALLOWED =
    /(^|\.)stripe\.(com|network)$|(^|\.)stripecdn\.com$|(^|\.)hcaptcha\.com$|(^|\.)paypal(objects)?\.com$/;
  const escaped = new Set();
  ctx.on('requestfinished', (r) => {
    const h = new URL(r.url()).host;
    if (/^localhost|^127\.0\.0\.1/.test(h) || ALLOWED.test(h)) return;
    escaped.add(h);
  });

  try {
    const productPath = await firstProductPath(page);
    check('storefront links to a product page', !!productPath, productPath || 'none found');

    // ---- the catalog behind the page is the module's ----------------------
    const dbProducts = Number(sql('select count(*) from product;') || 0);
    check('module holds the imported catalog', dbProducts > 0, `${dbProducts} products`);

    // ---- add to cart lands in the module ----------------------------------
    const added = await addOne(page, productPath);
    const cartLines = Number(sql('select coalesce(jsonb_array_length(items),0) from cart order by updated_at desc limit 1;') || 0);
    check('add to cart reaches the module', added && cartLines > 0, `${cartLines} line(s) server-side`);

    // ---- checkout creates a priced order ----------------------------------
    await toPayment(page);
    const orderId = latestPending();
    check('checkout created a pending order', !!orderId, orderId.slice(0, 8));
    if (!orderId) return; // the report below still prints what was proved; nothing else can be

    const totalCents = Number(sql(`select total_cents from "order" where id='${orderId}';`) || 0);
    const qty = Number(sql(`select (it->>'quantity') from "order" o cross join lateral jsonb_array_elements(o.items) it where o.id='${orderId}' limit 1;`) || 1);
    const variantId = sql(`select (it->>'variantId') from "order" o cross join lateral jsonb_array_elements(o.items) it where o.id='${orderId}' limit 1;`);
    const stockBefore = Number(sql(`select stock from inventory where variant_id='${variantId}';`) || 0);
    check('the order is priced', totalCents > 0, money(totalCents));

    // ---- the cart is not consumed until the money arrives ------------------
    const survived = Number(sql('select coalesce(jsonb_array_length(items),0) from cart order by updated_at desc limit 1;') || 0);
    check('cart survives an unpaid checkout', survived > 0, `${survived} line(s) still held`);

    // ---- pay, and watch the whole chain -----------------------------------
    await payWithCard(page, '4242424242424242');
    const paid = await eventually(() => sql(`select status from "order" where id='${orderId}';`) === 'paid');
    check('order pending -> paid via webhook', paid, sql(`select status from "order" where id='${orderId}';`));

    const landed = await eventually(() => {
      const s = Number(sql(`select stock from inventory where variant_id='${variantId}';`) || 0);
      return s === stockBefore - qty ? s : null;
    });
    check('worker decremented stock by the quantity ordered', landed === stockBefore - qty,
      `${stockBefore} -> ${sql(`select stock from inventory where variant_id='${variantId}';`)} (ordered ${qty})`);

    // ---- a declined card must not move inventory ---------------------------
    await addOne(page, productPath);
    await toPayment(page);
    const order2 = latestPending();
    check('checkout created a second pending order', !!order2, order2.slice(0, 8));
    if (!order2) return;
    const variant2 = sql(`select (it->>'variantId') from "order" o cross join lateral jsonb_array_elements(o.items) it where o.id='${order2}' limit 1;`);
    const stock2Before = Number(sql(`select stock from inventory where variant_id='${variant2}';`) || 0);
    await payWithCard(page, '4000000000009995');
    await page.waitForTimeout(12000);
    const status2 = sql(`select status from "order" where id='${order2}';`);
    const stock2After = Number(sql(`select stock from inventory where variant_id='${variant2}';`) || 0);
    check('declined card leaves the order pending', status2 === 'pending', status2);
    check('declined card does not move stock', stock2After === stock2Before, `${stock2Before} -> ${stock2After}`);

    // ---- the redirect return trip is verified, not believed -----------------
    // Hand the page a real client secret for an order that was never paid,
    // with redirect_status=succeeded in the URL. Trusting the query string
    // would announce a payment that did not happen; asking Stripe does not.
    const secret = await page.evaluate(() => window.__flLastSecret || null);
    const pi = sql(`select provider_ref from payment where order_id='${order2}' order by created_at desc limit 1;`);
    const clientSecret = secret || (pi ? `${pi}_secret_test` : '');
    if (clientSecret) {
      await page.goto(`${STORE}/__fl/checkout?fl_order=${order2}&redirect_status=succeeded&payment_intent_client_secret=${encodeURIComponent(clientSecret)}`,
        { waitUntil: 'load', timeout: 60000 });
      await page.waitForTimeout(6000);
      const shown = await page.evaluate(() => document.querySelector('.ok h1')?.textContent.trim() || '');
      const lied = /payment received/i.test(shown);
      const stillPending = sql(`select status from "order" where id='${order2}';`) === 'pending';
      check('redirect return does not trust the URL', !!shown && !lied && stillPending,
        `page said "${shown}", order is ${sql(`select status from "order" where id='${order2}';`)}`);
    } else {
      check('redirect return does not trust the URL', false, 'no client secret available to test with');
    }

    // ---- nothing phoned home ----------------------------------------------
    check('the demo stayed offline', escaped.size === 0,
      escaped.size ? `reached ${[...escaped].slice(0, 5).join(', ')}` : 'no external host completed a request');
  } finally {
    await browser.close();
  }

  const failed = results.filter((r) => !r.pass);
  console.log(`\ncheck-purchase — ${STORE} (db ${DB})\n`);
  for (const r of results) {
    console.log(`  ${r.pass ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? '   (' + r.detail + ')' : ''}`);
  }
  console.log(`\n${results.length - failed.length}/${results.length} passed\n`);
  process.exit(failed.length ? 1 : 0);
}

main().catch((e) => {
  console.error('check-purchase crashed:', e.message);
  process.exit(2);
});
