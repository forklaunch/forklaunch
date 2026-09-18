#!/usr/bin/env node
/**
 * check-wired — behavioural gate for a storefront served by heroserve-fl.
 *
 *   node check-wired.mjs <storefront-url> [--product /products/foo.html]
 *
 * check-complete.mjs asks whether every page was captured. This asks the
 * harder question: does the served store actually *work* — is it driving the
 * ForkLaunch module, does it still look like the merchant's site, and is it
 * light enough to open on a modest device.
 *
 * Every assertion here corresponds to a bug that shipped and was found by
 * hand. That is the point of the file: each one was invisible to a string
 * match over the served HTML, because the markup was present and correct
 * while the page was inert.
 *
 *   shim executed        three separate injection bugs left the bridge in
 *                        the markup but never running — a regex that also
 *                        matched <header>, a lost backslash, and a backtick
 *                        inside a template literal that silently truncated
 *                        the script
 *   overlay neutralised  commerce.js owns the visible buttons and makes no
 *                        fetch call at all, so a shopper could complete a
 *                        whole purchase against localStorage while the
 *                        module never heard about it
 *   cart reaches module  the only assertion that proves the wiring, rather
 *                        than proving a button exists
 *   both providers       the checkout silently fell back to card-only when
 *                        a key was missing
 *   logo navigates       the asset localiser rewrote the logo's href to a
 *                        .bin file, so clicking it downloaded that file
 *   no dead nav          the catalog import pulls every product while the
 *                        crawl saves only what it walked to; graza.co had
 *                        79 products against 8 captured pages
 *   no demo copy         "test mode" and placeholder text reaching a page a
 *                        client is being shown
 *   media budget         five autoplay videos, all visibility:hidden,
 *                        pulled 49MB and ran five decoders forever
 *
 * Exits non-zero if any check fails, so it can gate a migration before
 * anyone sees it.
 */
import { chromium } from 'playwright';

const [urlArg, ...rest] = process.argv.slice(2);
if (!urlArg) {
  console.error('usage: node check-wired.mjs <storefront-url> [--product <path>]');
  process.exit(2);
}
const BASE = urlArg.replace(/\/$/, '');
const productFlag = rest.indexOf('--product');
const PRODUCT_PATH = productFlag >= 0 ? rest[productFlag + 1] : null;

/** Page weight a modest device can open without swapping. */
const MAX_PAGE_MB = 20;
/** Demo/placeholder copy that must never reach a client-facing page. */
const FORBIDDEN_COPY = [
  'test mode',
  'no real payment',
  'demo checkout',
  'replace-with-',
  'lorem ipsum'
];

const results = [];
const check = (name, pass, detail = '') =>
  results.push({ name, pass: !!pass, state: pass ? 'PASS' : 'FAIL', detail });

/**
 * NOT CONFIGURED IS NOT BROKEN.
 *
 * Without a ForkLaunch module behind the bridge, `/__fl/cart` answers
 * `{item_count: 0}` — heroserve swallows the connection error — so the
 * add-to-cart assertion saw 0 -> 0 and reported FAIL on every keyless visual
 * demo. That is a lie in the expensive direction: a red line nobody can act on
 * teaches everyone to skim past red lines, and this is the one that must never
 * be skimmed past when a real backend IS attached.
 *
 * A skipped check is not counted as passed and is not counted as failed. It is
 * counted as unproven, and says what would prove it.
 */
const skip = (name, why) =>
  results.push({ name, pass: true, state: 'SKIP', detail: why });

/** Whether the bridge was given a module at all (see heroserve's /__fl/health). */
async function moduleHealth(base) {
  try {
    const r = await fetch(base + '/__fl/health', { signal: AbortSignal.timeout(6000) });
    if (r.ok) return await r.json();
  } catch (_) { /* an older heroserve has no health route */ }
  return { configured: false, reachable: false, module: null };
}

let HEALTH = { configured: false, reachable: false, module: null };

async function main() {
  HEALTH = await moduleHealth(BASE);
  const browser = await chromium.launch({ headless: true });
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  // Track what the page actually pulls, which is the only honest measure of
  // whether it will open on a small machine.
  let bytes = 0;
  page.on('response', async (r) => {
    const len = Number(r.headers()['content-length'] || 0);
    bytes += len;
  });

  try {
    await page.goto(BASE + '/', { waitUntil: 'load', timeout: 45000 });
    await page.waitForTimeout(3000);

    // ---- the bridge is actually running -------------------------------
    const shim = await page.evaluate(() => window.__flShim || 0);
    check('shim executed', shim === 1, `window.__flShim = ${shim}`);

    const fl = await page.evaluate(() => ({
      exists: typeof window.FL === 'object' && window.FL !== null,
      bound: !!(window.FL && window.FL.__flBound)
    }));
    check(
      'offline overlay neutralised',
      !fl.exists || fl.bound,
      fl.exists ? `FL present, bound=${fl.bound}` : 'no overlay on this capture'
    );

    // ---- media budget --------------------------------------------------
    const media = await page.evaluate(() => {
      const vs = [...document.querySelectorAll('video')];
      const visible = (v) => {
        const b = v.getBoundingClientRect();
        const c = getComputedStyle(v);
        return b.width > 10 && b.height > 10 && c.visibility !== 'hidden' && c.display !== 'none';
      };
      return {
        total: vs.length,
        autoplay: vs.filter((v) => v.hasAttribute('autoplay')).length,
        hiddenPlaying: vs.filter((v) => !visible(v) && !v.paused).length
      };
    });
    check('no autoplay attributes left', media.autoplay === 0, `${media.autoplay} of ${media.total}`);
    check('no invisible video decoding', media.hiddenPlaying === 0, `${media.hiddenPlaying} playing while hidden`);

    // Snapshot here, before the gate navigates anywhere else — the counter
    // is cumulative across the whole run, and charging the homepage for
    // pages visited later would make this assertion meaningless.
    const mb = +(bytes / 1048576).toFixed(1);
    check(`homepage under ${MAX_PAGE_MB}MB`, mb <= MAX_PAGE_MB, `${mb}MB on first load`);

    // ---- the logo goes home, it does not download a file ----------------
    const logoHref = await page.evaluate(() => {
      const a = document.querySelector('a[href*="_a/"]');
      return a ? a.getAttribute('href') : null;
    });
    if (logoHref) {
      await page.evaluate(() => document.querySelector('a[href*="_a/"]').click());
      await page.waitForTimeout(1200);
      const landed = new URL(page.url()).pathname;
      check('asset-rewritten link goes home', landed === '/', `landed on ${landed}`);
      await page.goto(BASE + '/', { waitUntil: 'load' });
      await page.waitForTimeout(1500);
    } else {
      check('asset-rewritten link goes home', true, 'no mis-rewritten links on this capture');
    }

    // ---- no dead internal navigation -----------------------------------
    const links = await page.evaluate(() =>
      [...document.querySelectorAll('a[href]')]
        .map((a) => a.getAttribute('href'))
        .filter((h) => h && !/^(https?:|mailto:|tel:|#|javascript:)/i.test(h))
        // Served by heroserve or never captured by design (urlmap SKIP_PATH):
        // a 404 there is not a dead link in the clone.
        .filter((h) => !/^\/?(account|cart|checkouts?|search|policies|apps)(\/|$|\?)/i.test(h))
        .slice(0, 25)
    );
    const dead = [];
    for (const href of [...new Set(links)]) {
      const target = new URL(href, BASE + '/').toString();
      const res = await page.request.get(target).catch(() => null);
      if (!res || res.status() >= 400) dead.push(`${href} -> ${res ? res.status() : 'ERR'}`);
    }
    check('no dead internal links', dead.length === 0, dead.slice(0, 4).join(', '));

    // ---- add to cart reaches the module, not localStorage ---------------
    const productPath = PRODUCT_PATH || (await firstProductPath(page, BASE));
    let cartProved = false;
    let cartDetail = 'no product page found to test';
    if (productPath) {
      await page.goto(BASE + productPath, { waitUntil: 'load', timeout: 45000 });
      await page.waitForTimeout(2500);
      const before = await moduleCartCount(page, BASE);
      const clicked = await page.evaluate(() => {
        const b = document.querySelector('#fl-add, [data-add], button[name="add"], form[action*="/cart/add"] button, .sqs-add-to-cart-button');
        if (!b) return false;
        b.click();
        return true;
      });
      if (clicked) {
        await page.waitForTimeout(3000);
        const after = await moduleCartCount(page, BASE);
        cartProved = after > before;
        cartDetail = `module cart ${before} -> ${after}`;
      } else {
        cartDetail = 'no add-to-cart control found';
      }
    }
    if (!HEALTH.configured) {
      skip('add to cart reaches the module',
        'no ForkLaunch module configured — re-run with --module <url> to prove the wiring');
    } else if (!HEALTH.reachable) {
      check('add to cart reaches the module', false,
        `module ${HEALTH.module} configured but not answering`);
    } else {
      check('add to cart reaches the module', cartProved, cartDetail);
    }

    // ---- checkout offers what it was configured with --------------------
    await page.goto(BASE + '/__fl/checkout', { waitUntil: 'load', timeout: 45000 });
    await page.waitForTimeout(2000);
    const checkout = await page.evaluate(() => {
      const txt = document.body.innerText.toLowerCase();
      return {
        hasForm: !!document.querySelector('#go, #name, #line1'),
        providers: [...document.querySelectorAll('input[name=prov]')].map((i) => i.value),
        forbidden: txt
      };
    });
    if (!HEALTH.configured) {
      skip('checkout collects an address',
        'no ForkLaunch module configured — checkout is not wired in this run');
    } else {
      check('checkout collects an address', checkout.hasForm, checkout.hasForm ? '' : 'no shipping form rendered');
    }
    const badCopy = FORBIDDEN_COPY.filter((p) => checkout.forbidden.includes(p));
    check('no demo copy on checkout', badCopy.length === 0, badCopy.join(', '));
  } finally {
    await browser.close();
  }

  const failed = results.filter((r) => r.state === 'FAIL');
  const skipped = results.filter((r) => r.state === 'SKIP');
  console.log('\ncheck-wired — ' + BASE + '\n');
  for (const r of results) {
    console.log(`  ${r.state}  ${r.name}${r.detail ? '   (' + r.detail + ')' : ''}`);
  }
  console.log(`\n${results.length - failed.length - skipped.length}/${results.length - skipped.length} passed` +
    (skipped.length ? `, ${skipped.length} skipped (not configured)` : '') + '\n');
  process.exit(failed.length ? 1 : 0);
}

/** The module's own view of the cart — the only thing that proves wiring. */
async function moduleCartCount(page, base) {
  const res = await page.request.get(base + '/__fl/cart').catch(() => null);
  if (!res || res.status() !== 200) return -1;
  const body = await res.json().catch(() => null);
  return body && typeof body.item_count === 'number' ? body.item_count : -1;
}

/** First product link on the homepage, so the gate works on any store. */
// Shopify links products as /products/<h>; Squarespace as /<collection>/p/<h>
// (flattened to /<collection>/p-<h>.html in a capture). A Squarespace home
// page often carries no product link at all, so the shop page is tried next.
const PRODUCT_LINK = /(^|\/)products\/|\/p\/[^/]+|\/p-[^/]+\.html$/;
async function firstProductPath(page, base) {
  const find = () => page.evaluate((re) => {
    const a = [...document.querySelectorAll('a[href]')]
      .map((x) => x.getAttribute('href'))
      .find((h) => h && new RegExp(re).test(h) && !/^https?:/.test(h));
    return a || null;
  }, PRODUCT_LINK.source);
  let href = await find();
  if (!href) {
    const r = await page.goto(base + '/shop', { waitUntil: 'load', timeout: 45000 }).catch(() => null);
    if (r && r.status() === 200) { await page.waitForTimeout(1500); href = await find(); }
  }
  if (!href) return null;
  return new URL(href, base + '/').pathname;
}

// Exit 2 with the HARNESS-FAIL marker the repair loop keys off. A gate that
// could not run must never be mistaken for a gate that passed, and must not be
// mistaken for a capture defect either.
main().catch((e) => {
  console.error('\nHARNESS-FAIL: check-wired crashed — ' + e.message);
  process.exit(2);
});
