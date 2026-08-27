#!/usr/bin/env node
/**
 * check-store — end-to-end gate for the ForkLaunch Supply storefront and the
 * ecommerce module behind it.
 *
 *   node check-store.mjs [--store http://localhost:4310]
 *                        [--db forklaunch-supply-ecommerce]
 *                        [--admin-token demo-admin-token]
 *
 * This automates the pass a person would otherwise do by hand: browse, open a
 * product, add it, review the bag, check out, pay, and confirm the money and
 * the stock both moved. Assertions that matter are checked against the
 * database rather than the page, because the page will happily say "thank
 * you" for an order that never reached `paid`.
 *
 * Every case here is a bug that actually shipped and was caught by clicking:
 *
 *   product page renders     tiles linked to /products/<handle> before that
 *                            route existed; every product 404'd
 *   variant drives stock     the size selector has to repoint price AND stock,
 *                            or a sold-out size looks buyable
 *   bag arithmetic           quantity controls have to reprice the line and
 *                            the subtotal, not just the number on screen
 *   cart survives checkout   the basket is consumed only once it is paid for,
 *                            so a shopper can go back and add what they forgot
 *   card payment             order pending -> paid, stock down by the quantity
 *                            ordered, not by one
 *   declined card            order stays pending and stock does NOT move; the
 *                            failure path is where silent inventory loss hides
 *   orders view is gated     it exposes every order and address in the shop,
 *                            so no token must mean no data
 *
 * Exits non-zero if any case fails, or if it crashes — partial results are
 * still printed.
 */
import { chromium } from 'playwright';
import { until, recorder, sqlFor, money, cardFrame, fillCard, pay } from './lib-gate.mjs';

const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const STORE = arg('--store', 'http://localhost:4310').replace(/\/$/, '');
const DB = arg('--db', 'forklaunch-supply-ecommerce');
const TOKEN = arg('--admin-token', process.env.ADMIN_TOKEN || 'demo-admin-token');
const sql = sqlFor(`postgresql://postgresql@localhost:5434/${DB}`);

const { check, report } = recorder(`check-store — ${STORE} (db ${DB})`);

/**
 * Restock before running.
 *
 * This gate buys things. Left alone it drains the catalog it depends on, and
 * then starts failing for reasons that are not defects: the quantity control
 * refuses to go past available stock, so "increasing quantity reprices the
 * bag" fails once a variant is down to one. A test whose pass rate decays with
 * each run teaches you to ignore it.
 *
 * Topping up is a fixture, not a fix — it belongs to the test, not the module,
 * so it goes straight to the table rather than through the API.
 */
function restock(min = 25) {
  const n = sql(`update inventory set stock = ${min} where stock < ${min};`);
  return n;
}

const cartId = (page) => page.evaluate(() => localStorage.getItem('fl_cart'));
const lines = (id) =>
  Number(sql(`select coalesce(jsonb_array_length(items),0) from cart where id='${id}';`) || 0);

/** Home, waited on properly rather than slept through. */
async function home(page) {
  await page.goto(STORE + '/', { waitUntil: 'load', timeout: 45000 });
  await until(() => page.evaluate(() => document.querySelectorAll('[data-add]').length > 0),
    { label: 'product tiles' });
}

/** Add one unit and wait until the MODULE agrees, not just the button. */
async function addFromHome(page, index = 0) {
  await home(page);
  const ok = await page.evaluate((i) => {
    const b = [...document.querySelectorAll('[data-add]')].filter((x) => !x.disabled);
    if (!b[i]) return false;
    b[i].click();
    return true;
  }, index);
  if (!ok) return null;
  const id = await until(() => cartId(page), { label: 'a cart id' });
  if (!id) return null;
  await until(() => lines(id) > 0, { label: 'the cart to reach the module' });
  return id;
}

async function toPayment(page) {
  await page.goto(STORE + '/checkout', { waitUntil: 'load', timeout: 45000 });
  // Wait for the page to finish deciding, not merely for the button to look
  // clickable. #toPayment ships enabled in the markup and is disabled a moment
  // later if the cart is empty or Stripe is unconfigured — so polling the
  // disabled flag alone catches the initial state and sails past a checkout
  // that was never going to work. The order summary only renders once the
  // config and cart have both come back, which is the real ready signal.
  await until(() => page.evaluate(() =>
    document.querySelectorAll('#lines .line').length > 0 ||
    /not configured|empty/i.test(document.getElementById('lines')?.textContent || '')
  ), { label: 'checkout to finish loading' });
  const usable = await page.evaluate(() => {
    const b = document.getElementById('toPayment');
    return !!b && !b.disabled;
  });
  if (!usable) {
    throw new Error('checkout never became usable: ' +
      (await page.evaluate(() => document.getElementById('lines')?.textContent.trim().slice(0, 80))));
  }
  await page.evaluate(() => {
    const set = (id, v) => { const e = document.getElementById(id); if (e) e.value = v; };
    set('name', 'Test Shopper'); set('line1', '1 Market St');
    set('city', 'San Francisco'); set('state', 'CA'); set('postalCode', '94105');
  });
  await page.click('#toPayment');
}

async function main() {
  const browser = await chromium.launch({ headless: true });
  const page = await (await browser.newContext()).newPage();
  let crashed = null;

  try {
    restock();

    // ---- the catalog is the module's ------------------------------------
    await home(page);
    const grid = await page.evaluate(() => ({
      cards: document.querySelectorAll('[data-add]').length,
      links: document.querySelectorAll('a[href^="/products/"]').length,
      priced: [...document.querySelectorAll('*')].some((e) => /^\$\d+\.\d{2}$/.test((e.textContent || '').trim()))
    }));
    const dbProducts = Number(sql('select count(*) from product;') || 0);
    check('catalog renders from the module', grid.cards > 0 && dbProducts > 0,
      `${grid.cards} buyable tiles, ${dbProducts} products in the database`);
    check('prices rendered', grid.priced);
    check('tiles link to product pages', grid.links > 0, `${grid.links} links`);

    // ---- the product page -----------------------------------------------
    const handle = await page.evaluate(() => {
      const a = document.querySelector('a[href^="/products/"]');
      return a ? a.getAttribute('href') : null;
    });
    await page.goto(STORE + handle, { waitUntil: 'load', timeout: 45000 });
    const pdp = await until(() => page.evaluate(() => {
      const h1 = document.querySelector('h1');
      const add = document.getElementById('add');
      if (!h1 || !add) return null;
      return {
        title: h1.textContent.trim(),
        opts: document.querySelectorAll('.opt').length,
        stock: (document.querySelector('.stock') || {}).textContent || '',
        addLabel: add.textContent.trim()
      };
    }), { label: 'the product page to render' });
    check('product page renders', !!pdp && !!pdp.title, pdp ? pdp.title : 'never rendered');
    check('product page shows live stock', !!pdp && /in stock|Only \d+ left|Out of stock/i.test(pdp.stock),
      pdp ? pdp.stock.trim() : '');

    // A size selector that does not repoint stock is how a sold-out variant
    // stays buyable. Prove the second option changes what the page claims.
    if (pdp && pdp.opts > 1) {
      // Assert the page agrees with the module about the SELECTED variant,
      // not merely that the numbers changed when clicked. "It differs from
      // the previous size" is not a property of correct code: sizes routinely
      // share a stock count, and this gate's own purchases keep changing which
      // ones do. That made the assertion pass or fail depending on inventory,
      // which is the definition of a flaky test — it failed once in a full
      // suite run and passed on retry, having found no bug either time.
      await page.evaluate(() => document.querySelectorAll('.opt')[1].click());
      const truth = await until(async () => {
        const view = await page.evaluate(async () => {
          const sel = [...document.querySelectorAll('.opt')].find(
            (o) => o.getAttribute('aria-pressed') === 'true');
          if (!sel) return null;
          const vid = sel.dataset.vid;
          const products = await fetch('/shop/products').then((r) => r.json());
          let variant = null;
          for (const p of products) {
            const v = (p.variants ?? []).find((x) => x.id === vid);
            if (v) { variant = v; break; }
          }
          return {
            vid,
            shownPrice: document.querySelector('.price').textContent.trim(),
            shownStock: document.querySelector('.stock').textContent.trim(),
            realStock: variant ? (variant.stock ?? 0) : null,
            realPriceCents: variant ? variant.priceCents : null
          };
        });
        return view && view.realStock !== null ? view : null;
      }, { label: 'the variant to switch' });

      const priceMatches = truth && truth.shownPrice.includes(money(truth.realPriceCents));
      const stockMatches = truth && (
        truth.realStock === 0
          ? /out of stock/i.test(truth.shownStock)
          : new RegExp(`\\b${truth.realStock}\\b`).test(truth.shownStock));
      check('variant selection shows that variant\'s real price and stock',
        !!truth && priceMatches && stockMatches,
        truth
          ? `variant ${truth.vid.slice(0, 8)}: page says "${truth.shownPrice} / ${truth.shownStock}", module says ${money(truth.realPriceCents)} / ${truth.realStock}`
          : 'selector never engaged');
    } else {
      check('variant selection shows that variant\'s real price and stock', true,
        'single-variant product, nothing to switch');
    }

    // ---- add from the product page --------------------------------------
    await page.evaluate(() => document.getElementById('add').click());
    const cart = await until(() => cartId(page), { label: 'a cart id' });
    const served = cart ? await until(() => lines(cart) > 0, { label: 'the module cart' }) : false;
    check('add to bag reaches the module', !!served,
      cart ? `cart ${cart.slice(0, 8)} holds ${lines(cart)} line(s) server-side` : 'no cart created');

    // ---- the bag ---------------------------------------------------------
    await page.goto(STORE + '/bag', { waitUntil: 'load', timeout: 45000 });
    const bag = await until(() => page.evaluate(() => {
      const t = document.querySelector('.row.total span:last-child');
      return t ? { total: t.textContent.trim(), rows: document.querySelectorAll('.line').length } : null;
    }), { label: 'the bag to render' });
    check('bag lists what was added', !!bag && bag.rows > 0, bag ? `${bag.rows} line(s), total ${bag.total}` : 'never rendered');

    // Quantity has to reprice the line and the subtotal, not just the digit.
    if (bag && bag.rows > 0) {
      const before = bag.total;
      await page.evaluate(() => document.querySelector('[data-inc]')?.click());
      const after = await until(async () => {
        const t = await page.evaluate(() => document.querySelector('.row.total span:last-child')?.textContent.trim());
        return t && t !== before ? t : null;
      }, { label: 'the subtotal to change' });
      check('increasing quantity reprices the bag', !!after, `${before} -> ${after ?? 'unchanged'}`);
    }

    // ---- checkout creates a priced order ---------------------------------
    await toPayment(page);
    const orderId = await until(() =>
      sql(`select id from "order" where cart_id='${cart}' and status='pending' order by created_at desc limit 1;`) || null,
      { label: 'a pending order' });
    check('checkout created a pending order', !!orderId, (orderId || '').slice(0, 8));

    const totalCents = Number(sql(`select total_cents from "order" where id='${orderId}';`) || 0);
    const qty = Number(sql(`select (it->>'quantity') from "order" o cross join lateral jsonb_array_elements(o.items) it where o.id='${orderId}' limit 1;`) || 1);
    const variantId = sql(`select (it->>'variantId') from "order" o cross join lateral jsonb_array_elements(o.items) it where o.id='${orderId}' limit 1;`);
    const stockBefore = Number(sql(`select stock from inventory where variant_id='${variantId}';`) || 0);
    check('the order is priced', totalCents > 0, money(totalCents));

    // Regression guard: the cart used to be emptied the moment a payment
    // intent existed, stranding anyone who did not finish paying.
    check('cart survives an unpaid checkout', lines(cart) > 0, `${lines(cart)} line(s) still held`);

    // ---- pay, and watch the whole chain ----------------------------------
    await fillCard(page, await cardFrame(page), '4242424242424242');
    await pay(page);
    const paid = await until(() => sql(`select status from "order" where id='${orderId}';`) === 'paid',
      { timeout: 40000, label: 'the webhook to mark it paid' });
    check('order pending -> paid via webhook', !!paid, sql(`select status from "order" where id='${orderId}';`));

    // Wrapped in an object because until() waits on truthiness, and stock
    // landing on exactly 0 is both a correct result and a falsy one. Returning
    // the bare number reported a successful decrement to zero as a timeout.
    const landed = await until(() => {
      const s = Number(sql(`select stock from inventory where variant_id='${variantId}';`) || 0);
      return s === stockBefore - qty ? { stock: s } : null;
    }, { timeout: 40000, label: 'the worker to adjust inventory' });
    check('worker decremented stock by the quantity ordered', !!landed,
      `${stockBefore} -> ${sql(`select stock from inventory where variant_id='${variantId}';`)} (ordered ${qty})`);
    check('cart cleared once paid', lines(cart) === 0, `${lines(cart)} line(s) left`);

    // ---- a declined card must not move inventory --------------------------
    await page.evaluate(() => localStorage.removeItem('fl_cart'));
    const cart2 = await addFromHome(page, 1);
    await toPayment(page);
    const order2 = await until(() =>
      sql(`select id from "order" where cart_id='${cart2}' and status='pending' order by created_at desc limit 1;`) || null,
      { label: 'a second pending order' });
    const variant2 = sql(`select (it->>'variantId') from "order" o cross join lateral jsonb_array_elements(o.items) it where o.id='${order2}' limit 1;`);
    const stock2Before = Number(sql(`select stock from inventory where variant_id='${variant2}';`) || 0);
    await fillCard(page, await cardFrame(page), '4000000000009995');
    await pay(page);
    // Wait for the decline to actually land rather than assuming it is instant.
    await until(() => sql(`select status from payment where order_id='${order2}' order by created_at desc limit 1;`) === 'failed',
      { timeout: 40000, label: 'the decline' });
    check('declined card leaves the order pending',
      sql(`select status from "order" where id='${order2}';`) === 'pending',
      sql(`select status from "order" where id='${order2}';`));
    check('declined card does not move stock',
      Number(sql(`select stock from inventory where variant_id='${variant2}';`) || 0) === stock2Before,
      `${stock2Before} -> ${sql(`select stock from inventory where variant_id='${variant2}';`)}`);

    // ---- the orders view is gated ----------------------------------------
    // It lists every order, total and shipping address in the shop. Reachable
    // without a token, it is a customer-data leak wearing a dashboard.
    const anon = await page.evaluate(async (s) => (await fetch(s + '/shop/orders')).status, STORE);
    check('orders view refuses an unauthenticated request', anon === 401 || anon === 503, `HTTP ${anon}`);

    const withToken = await page.evaluate(async ([s, t]) => {
      const r = await fetch(s + '/shop/orders', { headers: { authorization: 'Bearer ' + t } });
      return { status: r.status, count: r.ok ? (await r.json()).length : 0 };
    }, [STORE, TOKEN]);
    check('orders view returns orders with a token',
      withToken.status === 200 && withToken.count > 0,
      `HTTP ${withToken.status}, ${withToken.count} order(s)`);

    const bogus = await page.evaluate(async (s) =>
      (await fetch(s + '/shop/orders', { headers: { authorization: 'Bearer not-the-token' } })).status, STORE);
    check('orders view rejects a wrong token', bogus === 401, `HTTP ${bogus}`);
  } catch (e) {
    crashed = e;
  } finally {
    await browser.close();
  }

  process.exit(report(crashed));
}

main();
