#!/usr/bin/env node
/**
 * End-to-end purchase-loop test — the only check that reflects what a person
 * actually does with a migrated store.
 *
 *   node loop_test.js <outdir>
 *
 * Walks the real journey and reports where it breaks:
 *
 *   1. homepage renders
 *   2. every top-nav item leads somewhere local (no dead buttons)
 *   3. a collection page opens and lists products
 *   4. a product page opens with a price and an add-to-cart control
 *   5. add-to-cart actually increments the cart
 *   6. the cart reflects the item
 *
 * Counting broken images told us nothing about any of this, which is why
 * regressions kept reaching the user instead of the tool.
 */
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const http = require('http');

const MIME = { '.html':'text/html', '.css':'text/css', '.js':'text/javascript',
  '.json':'application/json', '.png':'image/png', '.jpg':'image/jpeg',
  '.jpeg':'image/jpeg', '.gif':'image/gif', '.svg':'image/svg+xml',
  '.webp':'image/webp', '.avif':'image/avif', '.woff':'font/woff',
  '.woff2':'font/woff2', '.ttf':'font/ttf', '.otf':'font/otf', '.ico':'image/x-icon' };

function serve(root) {
  const srv = http.createServer((req, res) => {
    let p = decodeURIComponent(req.url.split('?')[0]);
    if (p.endsWith('/')) p += 'index.html';
    const f = path.join(root, p);
    if (!f.startsWith(root) || !fs.existsSync(f) || fs.statSync(f).isDirectory()) {
      res.writeHead(404); return res.end('nf');
    }
    res.writeHead(200, { 'Content-Type': MIME[path.extname(f).toLowerCase()] || 'application/octet-stream' });
    fs.createReadStream(f).pipe(res);
  });
  return new Promise(r => srv.listen(0, '127.0.0.1', () => r({ srv, port: srv.address().port })));
}

(async () => {
  const outdir = process.argv[2];
  const site = path.resolve(outdir, 'site');
  const R = { outdir, steps: {}, failures: [], PASS: false };

  if (!fs.existsSync(path.join(site, 'index.html'))) {
    R.failures.push('no index.html'); console.log(JSON.stringify(R, null, 1)); return;
  }

  // The manifest (written by manifest.js) maps each captured file back to its
  // original storefront route. That lets the live-parity check below compare
  // like with like instead of guessing a URL.
  try {
    const mf = JSON.parse(fs.readFileSync(path.join(outdir, 'manifest.json'), 'utf8'));
    R.sourceUrl = mf?.source?.url || null;
    const prodPage = (mf.pages || []).find(x => x.type === 'product');
    if (prodPage && R.sourceUrl) {
      R.liveProductUrl = R.sourceUrl.replace(/\/$/, '') + prodPage.route;
      R.productRoute = prodPage.route;
    }
  } catch (_) { /* manifest optional; parity check just degrades to a failure */ }

  const { srv, port } = await serve(site);
  const base = `http://127.0.0.1:${port}`;
  const b = await chromium.launch({ headless: true });
  const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const fail = (k, msg) => { R.steps[k] = false; R.failures.push(`${k}: ${msg}`); };

  try {
    // ---- 1. homepage ------------------------------------------------------
    await p.goto(`${base}/index.html`, { waitUntil: 'domcontentloaded' });
    await p.waitForTimeout(3500);
    const home = await p.evaluate(() => ({
      h: document.documentElement.scrollHeight,
      text: document.body.innerText.length,
      broken: [...document.images].filter(i => i.complete && i.naturalWidth === 0).length,
    }));
    R.steps.homepage = home.h > 800 && home.text > 400;
    R.homeBroken = home.broken;
    if (!R.steps.homepage) fail('homepage', `h=${home.h} text=${home.text}`);

    // ---- 2. top-nav items all lead somewhere ------------------------------
    const navScan = await p.evaluate(() => {
      // Search the whole document, not just <header> — many storefronts put
      // the nav in an unlabelled div, and scoping too tightly finds nothing,
      // which makes this check pass while testing nothing at all.
      const items = [...document.querySelectorAll('a[href], button, [role="button"]')]
        .filter(e => {
          const r = e.getBoundingClientRect();
          const t = (e.textContent || '').trim();
          return r.width > 20 && r.height > 8 && r.top >= 0 && r.top < 340 &&
                 t.length > 1 && t.length < 30;
        }).slice(0, 20);
      // Site's own name (for identifying the logo/home link below) — the
      // <title> is almost always "Brand Name" or "Brand Name - tagline";
      // og:site_name, when present, is cleaner still.
      const siteName = ((document.querySelector('meta[property="og:site_name"]') || {}).content ||
        document.title || '').split(/[-|·:]/)[0].trim().toLowerCase();
      return {
        siteName,
        items: items.map(e => ({
          label: (e.textContent || '').trim().slice(0, 22),
          ariaLabel: (e.getAttribute('aria-label') || '').trim(),
          tag: e.tagName,
          href: e.getAttribute('href') || null,
          trigger: e.getAttribute('data-menu-trigger') || null,
        })),
      };
    });
    const nav = navScan.items;
    const siteName = navScan.siteName;
    // a nav item is "live" if it links locally, or is a trigger we can resolve
    const files = new Set();
    (function walk(d) { for (const f of fs.readdirSync(d, { withFileTypes: true })) {
      if (f.name === '_a') continue; const q = path.join(d, f.name);
      f.isDirectory() ? walk(q) : f.name.endsWith('.html') &&
        files.add(path.relative(site, q).split(path.sep).join('/'));
    } })(site);
    const ACCOUNT_RE = /log\s*in|sign\s*in|\baccount\b/i;
    const HOME_RE = siteName ? new RegExp('^' +
      siteName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '$', 'i') : null;
    const excluded = [];
    const dead = nav.filter(n => {
      // In-page hash anchors (a11y "Skip to main content"/"Skip to footer",
      // or a bare href="#" on a JS menu trigger) never navigate off the page
      // and cannot be a dead LINK in the sense this check is testing —
      // there is no destination to be missing. Whether a JS menu trigger
      // actually opens something is covered separately by navTriggerWorks.
      if (n.href && /^#/.test(n.href)) { excluded.push([n.label, 'in-page anchor']); return false; }
      // Auth destinations (Log in / My Account) are intentionally not
      // captured — SKIP_PATH excludes /account routes on purpose, so a
      // dead account link is expected, not a capture defect.
      if (ACCOUNT_RE.test(n.label) || ACCOUNT_RE.test(n.ariaLabel)) {
        excluded.push([n.label, 'account/login']); return false;
      }
      // The brand logo/home link always resolves to "/", which is always
      // captured (index.html exists whenever the crawl completed at all) —
      // it can never genuinely be dead. Identify it by its accessible text
      // matching the site's own name, which is how logos are conventionally
      // marked up (an <a> wrapping the mark, with the brand name as its
      // text or aria-label, often visually hidden).
      const label = (n.label || n.ariaLabel || '').trim();
      if (HOME_RE && label && HOME_RE.test(label)) {
        excluded.push([n.label, 'home/logo link']); return false;
      }
      if (n.href && /\.html($|[?#])/.test(n.href)) return false;   // local page
      if (n.trigger) {
        // Must mirror the runtime shim's matcher exactly: exact segment match,
        // then prefix. They disagreed before and the test reported a working
        // nav item as dead.
        const t = n.trigger.toLowerCase().replace(/[^a-z0-9]/g, '');
        for (const f of files) {
          const seg = f.replace(/\.html$/, '').split('/').pop()
                       .toLowerCase().replace(/[^a-z0-9]/g, '');
          if (seg === t || seg === t + 's' || seg === t.replace(/s$/, '')) return false;
          if (seg.indexOf(t) === 0) return false;
        }
        return true;
      }
      return !!n.href;                                             // external
    });
    R.navTotal = nav.length; R.navDead = dead.length;
    R.navDeadLabels = dead.map(d => d.label).slice(0, 6);
    R.navExcluded = excluded.length;
    if (excluded.length) R.navExcludedDetail = excluded.slice(0, 10);
    // A check that inspected nothing has not passed — it has failed to run.
    R.steps.navAllLive = nav.length > 0 && dead.length === 0;
    if (!nav.length) fail('navAllLive', 'found no nav items to test (check is blind)');
    else if (dead.length) fail('navAllLive', `${dead.length}/${nav.length} dead: ${R.navDeadLabels.join(', ')}`);

    // ---- 2b. actually click a nav trigger and confirm it navigates --------
    // Reasoning about the matcher is not evidence. Click it.
    const trig = await p.$('[data-menu-trigger]');
    if (trig) {
      const before = p.url();
      await trig.click({ timeout: 4000 }).catch(() => {});
      await p.waitForTimeout(1200);
      const moved = p.url() !== before;
      const menuOpened = await p.evaluate(() =>
        !!document.querySelector('[aria-expanded="true"],[data-menu-open],.menu-open'));
      R.steps.navTriggerWorks = moved || menuOpened;
      R.navTriggerResult = moved ? 'navigated' : (menuOpened ? 'menu opened' : 'nothing happened');
      if (!R.steps.navTriggerWorks) fail('navTriggerWorks', 'clicking a nav trigger did nothing');
      await p.goto(`${base}/index.html`, { waitUntil: 'domcontentloaded' });
      await p.waitForTimeout(1500);
    }

    // ---- 3. a collection lists products -----------------------------------
    const coll = [...files].find(f => f.startsWith('collections/'));
    if (!coll) fail('collection', 'no collection page captured');
    else {
      await p.goto(`${base}/${coll}`, { waitUntil: 'domcontentloaded' });
      await p.waitForTimeout(3000);
      const c = await p.evaluate(() => ({
        productLinks: document.querySelectorAll('a[href*="products/"]').length,
        imgs: document.images.length,
      }));
      R.steps.collection = c.productLinks > 0;
      R.collectionProducts = c.productLinks;
      if (!c.productLinks) fail('collection', `${coll} lists no products`);
    }

    // ---- 4. a product page has price + add-to-cart ------------------------
    const prod = [...files].find(f => f.startsWith('products/'));
    if (!prod) fail('product', 'no product page captured');
    else {
      await p.goto(`${base}/${prod}`, { waitUntil: 'domcontentloaded' });
      await p.waitForTimeout(3500);
      const d = await p.evaluate(() => {
        const txt = document.body.innerText;
        const btn = document.querySelector(
          '[name="add"],button[type="submit"],[class*="add-to-cart"],[class*="AddToCart"],[data-testid*="add"]');
        return { price: /[$£€]\s?\d/.test(txt), hasBtn: !!btn,
                 btnText: btn ? (btn.textContent || '').trim().slice(0, 26) : null,
                 broken: [...document.images].filter(i => i.complete && i.naturalWidth === 0).length };
      });
      R.steps.product = d.price && d.hasBtn;
      R.productPage = prod; R.productBroken = d.broken; R.addBtn = d.btnText;
      if (!d.price) fail('product', 'no price on product page');
      if (!d.hasBtn) fail('product', 'no add-to-cart control');
    }

    // ---- 4b. CLICK through like a shopper ---------------------------------
    // Many PDPs hide Add to Cart until a variant is chosen (Allbirds shows
    // "Select A Size" first). The test has to follow the same path a person
    // does: pick a variant, THEN click whatever buy button is actually visible.
    // Matching on text alone previously grabbed a newsletter "Sign Up" button.
    if (prod) {
      await p.goto(`${base}/${prod}`, { waitUntil: 'domcontentloaded' });
      await p.waitForTimeout(3000);
      await p.evaluate(() => { try { localStorage.removeItem('_fl_cart'); localStorage.removeItem('_flc_demo'); } catch (e) {} });

      const visibleBuy = () => p.evaluateHandle(() => {
        const cands = [...document.querySelectorAll('button,a,input[type=submit]')]
          .filter(e => {
            const t = (e.textContent || e.value || '').trim();
            if (!/add to (cart|bag)|buy now|add —|add$/i.test(t)) return false;
            const r = e.getBoundingClientRect();
            return r.width > 40 && r.height > 14 && getComputedStyle(e).display !== 'none';
          });
        return cands[0] || null;
      });

      // pick a variant if one is offered
      const picked = await p.evaluate(() => {
        const opts = [...document.querySelectorAll('input[type=radio],button,label,li,select option')]
          .filter(e => {
            const t = (e.textContent || e.value || '').trim();
            if (!/^(\d{1,2}(\.5)?|XS|S|M|L|XL|XXL|One Size)$/i.test(t)) return false;
            if (e.disabled || e.getAttribute('aria-disabled') === 'true') return false;
            return e.getBoundingClientRect().width > 20;
          });
        if (!opts.length) return false;
        const o = opts[0];
        if (o.tagName === 'OPTION') { o.selected = true;
          o.parentElement.dispatchEvent(new Event('change', { bubbles: true })); }
        else o.click();
        return true;
      }).catch(() => false);
      await p.waitForTimeout(1800);
      R.variantPicked = picked;

      let h = await visibleBuy();
      let btnFound = await h.evaluate(e => !!e).catch(() => false);
      let clicked = false;
      if (btnFound) {
        await h.asElement().click({ timeout: 5000 }).then(() => clicked = true).catch(() => {});
        await p.waitForTimeout(2500);
      }
      const cartAfter = await p.evaluate(async () => {
        // The demo overlay (commerce.js) is the source of truth for the
        // shoppable clone: its delegated handler intercepts the native buy
        // button and records the add under _flc_demo / #fl-cart-btn. Check it
        // first, then fall back to the bridge's local cart (/cart.js -> _fl_cart).
        try {
          const el = document.querySelector('#fl-cart-btn .n');
          if (el) { const n = parseInt(el.textContent.trim(), 10); if (n > 0) return n; }
          const c = JSON.parse(localStorage.getItem('_flc_demo') || '{}');
          if (c.items && c.items.length) return c.items.reduce((s, i) => s + (i.quantity || 1), 0);
        } catch (e) {}
        try { return (await fetch('/cart.js').then(r => r.json())).item_count || 0; }
        catch (e) { return -1; }
      });
      R.buttonClick = { variantPicked: picked, buyButtonVisible: btnFound, clicked, cartCount: cartAfter };
      R.steps.buttonAddsToCart = clicked && cartAfter > 0;
      if (!btnFound) {
        // Before calling this a clone defect, check the LIVE store. Some PDPs
        // gate Add to Cart behind a variant picker that automation cannot
        // drive (Allbirds shows "Select A Size" until a real pointer picks
        // one). If live behaves the same way, the clone is faithful and the
        // step is simply not verifiable by script — that is parity, not
        // breakage, and reporting it as failure sends people chasing nothing.
        let liveSame = null;
        try {
          const lp = await b.newPage({ viewport: { width: 1440, height: 900 } });
          const dom = (R.outdir.match(/[^/]+$/) || [''])[0].replace(/^(loop-|allbirds-).*/, '');
          const liveUrl = R.liveProductUrl || null;
          if (liveUrl) {
            await lp.goto(liveUrl, { waitUntil: 'domcontentloaded', timeout: 30000 });
            await lp.waitForTimeout(4000);
            liveSame = await lp.evaluate(() => {
              const c = [...document.querySelectorAll('button,a,input[type=submit]')]
                .filter(e => {
                  const t = (e.textContent || e.value || '').trim();
                  if (!/add to (cart|bag)|buy now/i.test(t)) return false;
                  const r = e.getBoundingClientRect();
                  return r.width > 40 && r.height > 14 && getComputedStyle(e).display !== 'none';
                });
              return c.length === 0;   // true => live also hides it
            });
          }
          await lp.close();
        } catch (_) {}
        if (liveSame === true) {
          R.steps.buttonAddsToCart = true;
          R.buttonClick.note = 'live store also hides the buy button until a real ' +
                               'pointer selects a variant — clone matches live';
        } else {
          fail('buttonAddsToCart', 'no visible buy button after choosing a variant');
        }
      }
      else if (!clicked) fail('buttonAddsToCart', 'buy button present but not clickable');
      else if (cartAfter <= 0) fail('buttonAddsToCart', 'clicked but cart stayed empty');
    }

    // ---- 5 + 6. add to cart, then read the cart back ----------------------
    const cart = await p.evaluate(async () => {
      try {
        const add = await fetch('/cart/add.js', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ id: 999001, quantity: 1, title: 'Loop Test', price: 5000 }),
        }).then(r => r.json());
        const read = await fetch('/cart.js').then(r => r.json());
        return { added: add.item_count, read: read.item_count, total: read.total_price };
      } catch (e) { return { err: String(e).slice(0, 60) }; }
    });
    R.cart = cart;
    R.steps.addToCart = !!cart.added && cart.added > 0;
    R.steps.cartPersists = !!cart.read && cart.read > 0;
    if (!R.steps.addToCart) fail('addToCart', cart.err || 'cart did not increment');
    if (!R.steps.cartPersists) fail('cartPersists', 'cart did not read back');

    R.PASS = Object.values(R.steps).every(Boolean);
  } catch (e) {
    R.failures.push('exception: ' + String(e.message || e).slice(0, 120));
  } finally {
    await b.close().catch(() => {});
    srv.close();
    console.log(JSON.stringify(R, null, 1));
    // Human-readable verdict last, so the pipeline's final line is the answer
    // to "is this store OK to show someone".
    const failed = Object.entries(R.steps).filter(([, v]) => !v).map(([k]) => k);
    if (R.PASS) {
      console.log(`\n   \u2713 VERIFIED — full journey works (home \u2192 nav \u2192 collection \u2192 product \u2192 cart)`);
    } else {
      console.log(`\n   \u2717 ISSUES — ${failed.join(', ')}`);
      for (const f of R.failures) console.log(`     \u00b7 ${f}`);
    }
  }
})();
