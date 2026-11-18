#!/usr/bin/env node
/**
 * Pre-flight assessment — run BEFORE crawling.
 *
 * Loads the storefront once and reports what to expect: how faithful the clone
 * is likely to be, roughly how long it will take, and which limits apply to
 * THIS store specifically. Point is to set expectations before the work starts
 * rather than explain a disappointing result afterwards.
 *
 *   node preflight.js <domain> [--json]
 *
 * Verdicts:
 *   GREEN  — expect a faithful, browsable clone
 *   AMBER  — expect it to work, with caveats named
 *   RED    — expect failure or a severely degraded clone, with the reason
 */
const { chromium } = require('playwright');

const UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 ' +
           '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';

(async () => {
  const domain = (process.argv[2] || '').replace(/^https?:\/\//, '').replace(/\/.*$/, '');
  const asJson = process.argv.includes('--json');
  const r = { domain, verdict: 'RED', confidence: 'low', reasons: [], notes: [],
              expect: {}, blockers: [] };

  let browser;
  try {
    browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 },
                                           userAgent: UA, ignoreHTTPSErrors: true });
    const page = await ctx.newPage();

    let status = 0, failed = false;
    try {
      const resp = await page.goto(`https://${domain}/`,
        { waitUntil: 'domcontentloaded', timeout: 35000 });
      status = resp ? resp.status() : 0;
    } catch (_) { failed = true; }

    if (failed || !status || status >= 400) {
      r.verdict = 'RED';
      r.blockers.push(status === 403 || failed
        ? 'The store refused an automated browser. This is anti-bot protection ' +
          '(Cloudflare / DataDome / similar). We cannot capture it without the ' +
          "merchant's cooperation — and we don't build ways around it."
        : `The store returned HTTP ${status}.`);
      r.reasons.push(failed ? 'no_response' : `http_${status}`);
      throw new Error('unreachable');
    }

    try { await page.waitForLoadState('networkidle', { timeout: 12000 }); } catch (_) {}
    await page.waitForTimeout(2000);

    const probe = await page.evaluate(() => {
      const txt = document.body ? document.body.innerText : '';
      return {
        title: document.title || '',
        textLen: txt.length,
        imgs: document.images.length,
        anchors: document.querySelectorAll('a[href]').length,
        productLinks: [...document.querySelectorAll('a[href*="/products/"]')].length,
        collectionLinks: [...document.querySelectorAll('a[href*="/collections/"]')].length,
        // Distinct in-nav destinations. The crawl guarantees these on top of
        // the page budget, so the time estimate has to count them or it
        // under-promises badly on stores with a large menu.
        navLinks: (() => {
          try {
            const sel = 'nav a[href], header a[href], [role="navigation"] a[href]';
            const seen = new Set();
            for (const a of document.querySelectorAll(sel)) {
              const h = a.getAttribute('href') || '';
              if (!h || h.startsWith('#') || /^(mailto|tel|javascript):/i.test(h)) continue;
              seen.add(h.split('?')[0].replace(/\/$/, ''));
            }
            return seen.size;
          } catch (_) { return 0; }
        })(),
        priceHits: (txt.match(/[$£€]\s?\d/g) || []).length,
        shopify: !!(window.Shopify || document.querySelector('[href*="cdn.shopify"]')),
        theme: (() => { try { return window.Shopify && window.Shopify.theme
                 ? (window.Shopify.theme.schema_name || 'custom') : null; } catch (_) { return null; } })(),
        nextjs: !!document.querySelector('script[src*="/_next/"], #__NEXT_DATA__'),
        nuxt: !!(document.querySelector('#__nuxt, script[src*="/_nuxt/"]') || window.__NUXT__),
        // Shopify app-proxy routes (/a/..., /apps/...) are a third-party app's
        // own web app mounted under the store's domain. When the shop itself
        // lives there (kettleandfire.com: every product link goes to
        // /a/collections/products, a Nuxt app), the theme pages capture and
        // the app does not.
        appProxyLinks: (() => {
          const seen = new Set();
          for (const a of document.querySelectorAll('a[href]')) {
            const h = (a.getAttribute('href') || '').replace(/^https?:\/\/[^/]+/, '');
            if (/^\/(a|apps)\//.test(h)) seen.add(h.split('?')[0]);
          }
          return seen.size;
        })(),
        // ...and how many of those look like the shop itself rather than a
        // help centre or rewards page.
        appProxyShopLinks: (() => {
          const seen = new Set();
          for (const a of document.querySelectorAll('a[href]')) {
            const h = (a.getAttribute('href') || '').replace(/^https?:\/\/[^/]+/, '');
            if (/^\/(a|apps)\/.*(shop|collection|product|store|bundle|catalog|build)/i.test(h)) seen.add(h.split('?')[0]);
          }
          return seen.size;
        })(),
        hydrogen: /hydrogen|oxygen/i.test(document.documentElement.innerHTML.slice(0, 60000)),
        challenge: /just a moment|checking your browser|verify you are human|attention required/i
                     .test(txt.slice(0, 3000)),
        loginWall: /sign in to (view|shop)|wholesale login|trade account required/i
                     .test(txt.slice(0, 3000)),
        h: document.documentElement.scrollHeight,
      };
    });

    if (probe.challenge) {
      r.verdict = 'RED';
      r.reasons.push('bot_challenge');
      r.blockers.push('The store is serving a human-verification challenge. ' +
        'It will not let an automated browser through. Needs merchant cooperation.');
      throw new Error('challenge');
    }
    if (probe.loginWall) {
      r.verdict = 'RED';
      r.reasons.push('login_wall');
      r.blockers.push('The catalog is behind a login (wholesale / trade / private ' +
        'store). Nothing to capture without an account.');
      throw new Error('login');
    }

    // second load: how much does the page move between visits?
    let churn = null;
    try {
      const p2 = await ctx.newPage();
      await p2.goto(`https://${domain}/`, { waitUntil: 'domcontentloaded', timeout: 30000 });
      try { await p2.waitForLoadState('networkidle', { timeout: 8000 }); } catch (_) {}
      await p2.waitForTimeout(1500);
      const h2 = await p2.evaluate(() => document.documentElement.scrollHeight);
      churn = probe.h ? Math.abs(h2 - probe.h) / probe.h : null;
      await p2.close();
    } catch (_) {}

    // ---- scoring -----------------------------------------------------------
    const stack = probe.hydrogen ? 'Hydrogen (React)'
                : probe.nextjs ? 'Next.js (React)'
                : probe.theme ? `Shopify theme: ${probe.theme}`
                : probe.shopify ? 'Shopify (custom)' : 'non-Shopify / unknown';
    r.stack = stack;
    r.pagesAvailable = { products: probe.productLinks, collections: probe.collectionLinks };

    const rendered = probe.textLen > 800 && probe.h > 900;
    const hasNav = (probe.productLinks + probe.collectionLinks) >= 3;

    // Non-Shopify stores are out of scope for the migration pipeline. Say so
    // explicitly — otherwise a thin result looks like a tool failure when the
    // real answer is "this isn't a Shopify store".
    if (!probe.shopify && !probe.theme) {
      r.notes.push('This does not look like a Shopify storefront. The visual ' +
        'capture may still work, but catalog structure (collections, products) ' +
        'will not be discoverable the usual way, so expect fewer pages and no ' +
        'catalog mapping. ForkLaunch migration targets Shopify today.');
      r.reasons.push('non_shopify');
    }

    if (!rendered) {
      r.verdict = 'AMBER';
      r.reasons.push('thin_render');
      r.notes.push('The homepage returned little content on first load. It may be ' +
        'heavily script-driven; the crawl runs a real browser and usually still ' +
        'gets it, but expect this one to be slower and less certain.');
    } else if (!hasNav) {
      r.verdict = 'AMBER';
      r.reasons.push('few_internal_links');
      r.notes.push('Few product/collection links on the homepage, so the clone may ' +
        'be mostly the landing page with limited click-through.');
    } else {
      r.verdict = 'GREEN';
      r.confidence = 'high';
    }

    // Headless storefronts (Hydrogen, Next.js) render product grids and
    // product pages in the browser from an API the offline clone cannot
    // reach, and usually publish no public catalog feed. The landing pages
    // capture; the shop does not. Say so before anyone spends an hour on it.
    if (probe.hydrogen || probe.nextjs) {
      if (r.verdict === 'GREEN') { r.verdict = 'AMBER'; r.confidence = 'medium'; }
      r.reasons.push('headless');
      r.notes.push('This is a headless (React) storefront. Its product grids and product ' +
        'pages are rendered in the browser from an API the clone cannot reach offline, ' +
        'and the public catalog feed is usually absent. Expect the homepage and content ' +
        'pages to capture well and collection/product pages to come through thin or empty. ' +
        'A full migration of a headless store needs the merchant\'s Storefront API access.');
    }
    // The shop behind an app proxy: same warning shape as headless, because
    // it is the same failure (a browser-rendered app with no offline data).
    if (probe.appProxyShopLinks >= 1 || probe.nuxt || (probe.appProxyLinks >= 3 && probe.productLinks === 0)) {
      if (r.verdict === 'GREEN') { r.verdict = 'AMBER'; r.confidence = 'medium'; }
      r.reasons.push('app_proxy');
      r.notes.push(`Some shop pages are served by a Shopify app under /a/ or /apps/ ` +
        `(${probe.appProxyLinks} link(s) on the homepage${probe.productLinks === 0 ? ', and no direct /products/ links' : ''}). ` +
        'That is a separate web app rendered in the browser, not the theme. Expect the ' +
        'homepage and content pages to be faithful and the app\'s product grid or shop ' +
        'page to come through thin, with console errors from the app. Those show up ' +
        'as named items in the report, not as a tool failure.');
    }
    if (churn !== null && churn > 0.15) {
      r.notes.push(`This store renders differently on each visit (~${Math.round(churn * 100)}% ` +
        'height difference between two loads) — A/B testing or personalization. ' +
        'We capture one coherent version, which is exactly what a migration ships. ' +
        'Not a problem, just expect the clone to match one variant rather than every one.');
      r.reasons.push('non_deterministic');
    }
    if (probe.imgs > 150) {
      r.notes.push(`Image-heavy homepage (${probe.imgs} images) — expect a larger ` +
        'download and a slower capture.');
    }

    // Pages: the crawl guarantees every nav destination ON TOP of the page
    // budget (crawl.js HARD_CAP = MAX_PAGES + up to 12 nav links), so the
    // real count is well above the budget. Estimating only the budgeted
    // pages is what made this promise ~12 and then capture 31.
    // Mirrors crawl.js: a 20-page budget plus every menu destination up to
    // NAV_MUST_CAP (40). The old cap of 12 here promised 24 pages for a store
    // the crawl then captured 52 of.
    const navExtra = Math.min(40, probe.navLinks || 0);
    // On any real store the 20-page budget fills (collection pages harvest
    // products into it), so the estimate is the budget plus the menu.
    const pages = (hasNav ? 20 : Math.min(20, 1 + Math.min(5, probe.collectionLinks) +
      Math.min(14, probe.productLinks))) + navExtra;

    // Seconds: measured throughput is 20-35s/page (image-heavy stores at the
    // top of that range), not a flat constant. The old estimate ignored page
    // count entirely, so it read ~110s for a run that genuinely takes 10x
    // that — the single most misleading number the tool printed.
    // Measured, not guessed: graza.co (image-heavy, 44 trackers, never goes
    // network-idle) ran ~37s/page sequential. The old 25/35 constants printed
    // ~620s for a run that took 18 minutes — and that is the number a person
    // watching the terminal remembers. Pages now capture in a worker pool
    // (crawl.js CONC, default 3 in normal mode) which changes no per-page wait,
    // so wall time is ceil(pages / workers) * perPage, not pages * perPage.
    const perPage = probe.imgs > 120 ? 40 : 30;
    const workers = Math.max(1, Number(process.env.FL_CONCURRENCY) || 3);

    r.expect = {
      seconds: Math.round(20 + Math.ceil(pages / workers) * perPage),
      pages,
      browsable: r.verdict !== 'RED' && hasNav,
    };
  } catch (_) {
    // verdict/blockers already set
  } finally {
    if (browser) await browser.close().catch(() => {});
  }

  // always-true limits, stated up front so nobody discovers them mid-demo
  r.alwaysApplies = [
    'Cart, search, variant switching and checkout are visual only — they need a ' +
      'backend behind them (that is the ForkLaunch ecommerce module).',
    'Inventory counts, SKUs, weights and tax flags are never shown on a page, so ' +
      'they cannot be captured. They come from the Admin API once a merchant grants access.',
    'Reviews, loyalty balances and subscriptions live in third-party systems ' +
      '(Bazaarvoice, Yotpo, Klaviyo) and do not transfer.',
  ];

  if (asJson) { console.log(JSON.stringify(r, null, 1)); return; }

  const bar = { GREEN: '● GREEN', AMBER: '● AMBER', RED: '● RED' }[r.verdict];
  console.log(`\n  ${bar}  ${r.domain}`);
  if (r.stack) console.log(`  stack: ${r.stack}`);
  if (r.verdict === 'GREEN')
    console.log(`  Expect a faithful, browsable clone (~${r.expect.seconds}s, ~${r.expect.pages} pages).`);
  if (r.verdict === 'AMBER')
    console.log('  Should work, with caveats below.');
  for (const b of r.blockers) console.log(`\n  ✗ ${b}`);
  for (const n of r.notes) console.log(`\n  · ${n}`);
  console.log('\n  Always true, on every store:');
  for (const a of r.alwaysApplies) console.log(`    – ${a}`);
  console.log('');
})();
