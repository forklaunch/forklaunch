#!/usr/bin/env node
/**
 * check-features — the primary gate. Does the clone have every FEATURE the
 * live storefront has?
 *
 *   node check-features.mjs --live https://www.graza.co --clone http://localhost:4180
 *                           [--routes /,/collections/all,/products/x]
 *                           [--cache <dir>/feature-inventory.json] [--refresh-live]
 *                           [--json <dir>/features.json] [--offline]
 *
 * The gates that came before this one each encode a bug somebody already hit:
 * the shim is running, nothing 404s, the page is under 12MB. They are worth
 * keeping and they are all backward-looking. A clone can pass every one of
 * them while silently missing half the live site's sections, because nobody
 * has written the assertion for a section that has not gone missing yet.
 *
 * This gate is forward-looking. It enumerates what the LIVE page has and
 * requires the clone to have it too — so the specification updates itself
 * every time the merchant changes their site, and features nobody anticipated
 * are covered for free.
 *
 * WHAT IT ASSERTS, AND WHAT IT REFUSES TO ASSERT
 *
 * It asserts that a feature EXISTS and RESPONDS. It never asserts a value
 * matches. Live stock counts, rotating banners, "N people viewing", A/B
 * buckets and personalised recommendations legitimately differ between two
 * loads of the same live page, and treating any of that as a failure produces
 * a report so noisy nobody reads it. Two mechanisms enforce this:
 *
 *   - the live page is sampled TWICE and only what both samples contain
 *     becomes a requirement (see inventory.mjs `stable`);
 *   - "works" means the control responds at all — a DOM burst above the page's
 *     own idle rate, an aria state flip, an overlay, a request, a navigation.
 *     Not that it responds identically.
 *
 * A control that does nothing on the LIVE site is not a feature and is never
 * held against the clone. The live site is the oracle for both halves of the
 * question: what exists, and what "working" looks like.
 *
 * EXIT CODES — these matter, and the distinction is the whole point:
 *
 *   0  every feature present and responding (or legitimately skipped)
 *   1  real assertion failure: the clone is missing something
 *   2  HARNESS FAILURE: the gate could not run at all
 *
 * 2 is separate from 1 deliberately. A gate that cannot run must never be
 * mistaken for a gate that passed. This exact thing happened: Playwright's
 * browser was not installed, the gates produced no output, and the repair loop
 * read "no assertions" as "nothing to repair" and declared the storefront
 * finished. Every harness failure here prints a line beginning HARNESS-FAIL
 * and exits 2, and the loop keys off that, not off a guess.
 */
import { chromium } from 'playwright';
import { Script as VmScript } from 'node:vm';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { inventoryRoute, stable, actuate, SNAPSHOT } from './inventory.mjs';

const arg = (n, d = null) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const has = (n) => process.argv.includes(n);

const LIVE = (arg('--live') || '').replace(/\/$/, '');
const CLONE = (arg('--clone') || 'http://localhost:4180').replace(/\/$/, '');
const CACHE = arg('--cache');
const JSON_OUT = arg('--json');
const REFRESH = has('--refresh-live');
const OFFLINE = has('--offline');
const ROUTES_ARG = (arg('--routes') || '').split(',').filter(Boolean);
// Tolerance on COUNTS only. Counts are the one place a like-for-like number is
// unavoidable (you cannot name every background image), and lazy-loading makes
// them jitter on both sides. Set generously: this catches "the heroes are
// gone", not "one fewer thumbnail painted in the 6 seconds we waited".
const COUNT_FLOOR = Number(arg('--count-floor', '0.7'));

if (!LIVE && !OFFLINE) {
  console.error('HARNESS-FAIL: --live <url> is required (or --offline to check the clone alone)');
  process.exit(2);
}

/** Distinct from a failed assertion. See the exit-code note above. */
function harnessFail(msg, hint) {
  console.error(`\nHARNESS-FAIL: ${msg}`);
  if (hint) console.error(`  fix: ${hint}`);
  process.exit(2);
}

/**
 * Shape version of the cached live requirement. Bump on ANY change to what
 * `stable()` returns or what the oracle records — see buildLiveRequirement.
 */
// 3: live requirement gained contactedHosts (every third-party host the page
//    talked to, not just script hosts). A schema-2 cache has none, and the
//    vendor attribution that depends on it would silently never fire.
// 4: headingVendor gained the cookie-consent signature (inventory.mjs
//    VENDOR_SIG). A schema-3 cache attributes no consent banner to a vendor.
// 5: optionGroups carry inCart (a picker inside the cart drawer is a
//    cart-state feature; browse-only mode cannot prove it).
// 6: rejection reasons recorded by content (inventory.mjs __flRej), so
//    live/clone error texts changed shape.
// 7: link controls keyed by destination path; sale phrases stripped from
//    feature keys (inventory.mjs featureKey / hrefKey).
// 8: templateId / templateVaries in the live requirement.
// 9: headingDynamic / dynamic flags (recommendation & upsell content).
// 10: landmarks and background counts exclude vendor widgets.
// 11: headingSlide (carousel slide content).
// 12: pickers only inside add-to-cart forms; slide/dynamic signatures widened.
// 13: hrefKey canonical fold (collections/x/products/y → products/y; a/b/c → a/b-c).
// 14: store-locator vendor signature; policies pages now captured.
const REQUIREMENT_SCHEMA = 15; // 15: lazy images flipped eager before the media count

const results = [];
const check = (name, pass, detail = '') => results.push({ name, pass: !!pass, state: pass ? 'PASS' : 'FAIL', detail });
/**
 * Not configured is not broken. A cart gate with no ForkLaunch module behind
 * it is proving nothing either way, and reporting that as FAIL trains everyone
 * to ignore the report — which is how a real cart failure gets waved through.
 */
const skip = (name, why) => results.push({ name, pass: true, state: 'SKIP', detail: why });

/**
 * Concrete defects, each carrying the repair that answers it.
 *
 * `policy` marks a difference we CHOSE — a third-party widget whose data lives
 * in another vendor's database and whose script the offline guarantee blocks
 * on purpose. Those are reported, never hidden, but they do not fail the gate:
 * a bar that can only be cleared by abandoning the offline guarantee is not a
 * bar, it is a permanent red light, and a permanent red light gets ignored.
 * Anything we cannot positively attribute to a policy stays a real defect.
 */
const missing = [];
// The crawl's asset-origin record (site/_a/.fl-origins.json), fetched once
// through the clone's own server so the gate needs no filesystem path.
// Absent on captures made before it existed — then attribution falls back to
// the host and identifier evidence below, exactly as before.
let originsCache = null;
async function loadOrigins(page) {
  if (originsCache) return originsCache;
  try {
    const r = await page.request.get(CLONE + '/_a/.fl-origins.json', { timeout: 10000 });
    originsCache = r.ok() ? await r.json() : {};
  } catch { originsCache = {}; }
  return originsCache;
}

// "Identifier 'X' has already been declared" names no file — the second
// declaration is a parse error before any frame exists. Attribute it by
// finding every script on the clone page that declares X: if each one is
// vendor code (recorded origin off the store's host or a Shopify app
// extension; or vendor markers / a contacted third-party host named in its
// own text), the collision is between two vendors' snippets — GTM's inline
// tag and an app's handler on gorillamind.com — and is policy, named.
const cloneHtmlCache = new Map();
async function cloneHtml(page, route) {
  if (cloneHtmlCache.has(route)) return cloneHtmlCache.get(route);
  let html = '';
  try { const r = await page.request.get(cloneUrlFor(route), { timeout: 15000 }); html = r.ok() ? await r.text() : ''; } catch {}
  cloneHtmlCache.set(route, html);
  return html;
}
const VENDOR_TEXT = /\b(fbq|gtag|dataLayer|google_tag_manager|GTM-[A-Z0-9]{4,}|_learnq|klaviyo|ttq|snaptr|pintrk|webPixelsManager|clarity|amplitude|mixpanel|heap|attentive|yotpo|okendo|skio|rebuy|loomi|savedby|sezzle|afterpay|klarna|postscript|gorgias|hotjar|sentry)\b/i;
async function redeclaredByVendor(page, route, ident, origins, hosts, storeHost) {
  const html = await cloneHtml(page, route);
  if (!html) return null;
  const re = new RegExp('\\b(?:const|let|class|function)\\s+' + ident.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\b');
  const declarers = [];
  for (const m of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi)) {
    const src = /\bsrc="([^"]+)"/.exec(m[1]);
    if (src) {
      const rel = src[1].replace(/^(\.\.\/)+/, '').replace(/^\//, '');
      if (!/^_a\//.test(rel)) continue;
      let text = '';
      try { const r = await page.request.get(CLONE + '/' + rel, { timeout: 15000 }); text = r.ok() ? await r.text() : ''; } catch {}
      if (re.test(text)) declarers.push({ rel, origin: origins[rel] || '', text });
    } else if (re.test(m[2])) declarers.push({ rel: '', origin: '', text: m[2] });
  }
  if (declarers.length < 2) return null;
  const vendorOf = (d) => {
    if (d.origin) {
      try {
        const o = new URL(d.origin); const h = o.host.replace(/^www\./, '');
        if (h !== storeHost && (h !== 'cdn.shopify.com' || /\/(extensions|shopifycloud)\//.test(o.pathname))) return h;
      } catch {}
    }
    const t = d.text.slice(0, 6000);
    const vm = VENDOR_TEXT.exec(t);
    if (vm) return vm[1];
    for (const h of hosts) {
      const label = (h.split('.').slice(-2, -1)[0] || '').toLowerCase();
      if (label.length >= 4 && new RegExp('\\b' + label + '\\b', 'i').test(t)) return h;
    }
    return null;
  };
  const vendors = declarers.map(vendorOf);
  return vendors.every(Boolean) ? [...new Set(vendors)].join(' + ') : null;
}

// Links a theme renders only when a menu opens. On the live site the menu's
// panels are usually in the DOM at rest; a clone captured at rest can carry
// them in a portal the theme fills on hover (gorillamind.com's mega menu).
// The link is there either way — open the menus and count what appears.
async function menuRevealedKeys(page, got) {
  const keys = new Set();
  const menus = got.controls.filter((c) => c.kind === 'menu').slice(0, 3);
  if (!menus.length) return keys;
  for (const m of menus) { try { await actuate(page, m.ctl, { wait: 900, key: m.key }); } catch {} }
  try {
    const list = await page.evaluate(() => [...document.querySelectorAll('a')].map((a) => {
      try {
        const h = a.getAttribute('data-mirror-uncaptured') || a.getAttribute('data-fl-href') || a.getAttribute('href');
        if (!h || /^(#|mailto:|tel:|javascript:|data:)/i.test(h)) return null;
        const u = new URL(h, location.href);
        if (u.host && u.host.replace(/^www\./, '') !== location.host.replace(/^www\./, '')) return null;
        let p = u.pathname.replace(/\.html$/, '').replace(/\/+$/, '').replace(/^\/+/, '') || '/';
        if (p === 'index') p = '/';
        return 'link href:' + p.toLowerCase();
      } catch { return null; }
    }).filter(Boolean));
    for (const k of list) keys.add(k);
  } catch {}
  try { await page.keyboard.press('Escape'); } catch {}
  return keys;
}

async function inlineScriptVendor(page, route, line, hosts) {
  const html = await cloneHtml(page, route);
  if (!html) return null;
  const lines = html.split('\n');
  const idx = line - 1;
  if (idx < 0 || idx >= lines.length) return null;
  let start = idx; while (start >= 0 && !/<script\b/i.test(lines[start])) start--;
  if (start < 0) return null;
  let end = idx; while (end < lines.length && !/<\/script>/i.test(lines[end])) end++;
  const text = lines.slice(start, Math.min(end + 1, start + 400)).join('\n').slice(0, 8000);
  if (/\bsrc=/.test(lines[start].match(/<script\b[^>]*>/i)?.[0] || '')) return null;   // external: origin rules own it
  const vm = VENDOR_TEXT.exec(text);
  if (vm) return vm[1];
  for (const h of hosts) {
    const label = (h.split('.').slice(-2, -1)[0] || '').toLowerCase();
    if (label.length >= 4 && new RegExp('\\b' + label + '\\b', 'i').test(text)) return h;
  }
  return null;
}

// A SyntaxError in an inline script never carries a frame: the parser fails
// before there is one. Find the script by parsing the clone page's inline
// classic scripts ourselves; then the same vendor evidence applies, and when
// it is not vendor code the report shows the script's head so a person can
// see what broke (a rewrite of ours, once — see fix-script-hrefs).
async function syntaxErrorSource(page, route, hosts) {
  const html = await cloneHtml(page, route);
  if (!html) return null;
  for (const m of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi)) {
    const a = m[1];
    if (/\bsrc=/.test(a) || /type="(?!text\/javascript|application\/javascript|module)[^"]*"/i.test(a)) continue;
    const body = m[2];
    if (!body.trim()) continue;
    try { new VmScript(/type="module"/.test(a) ? '(async()=>{' + body + '\n})' : body); }
    catch (e) {
      const head = body.trim().slice(0, 160).replace(/\s+/g, ' ');
      const t = body.slice(0, 6000);
      const vm = VENDOR_TEXT.exec(t);
      let vendor = vm ? vm[1] : null;
      if (!vendor) for (const h of hosts) { const label = (h.split('.').slice(-2, -1)[0] || '').toLowerCase(); if (label.length >= 4 && new RegExp('\\b' + label + '\\b', 'i').test(t)) { vendor = h; break; } }
      return { message: String(e.message).slice(0, 80), head, vendor };
    }
  }
  return null;
}

const defect = (kind, what, repair = null, where = '', policy = null, url = '') =>
  missing.push({ kind, what, repair, where, ...(policy ? { policy } : {}), ...(url ? { url } : {}) });

// ---------------------------------------------------------------------------

/** Routes to compare: home plus one of each template that exists. */
async function chooseRoutes(page) {
  if (ROUTES_ARG.length) return ROUTES_ARG;
  await page.goto(CLONE + '/', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(4000);
  const found = await page.evaluate(() => {
    const hrefs = [...document.querySelectorAll('a[href]')]
      .map((a) => a.getAttribute('href') || '')
      .filter((h) => !/^(https?:|mailto:|tel:|#)/.test(h));
    // The crawl rewrites inter-page links RELATIVE — `products/x.html`, with no
    // leading slash — so a pattern anchored on `/products/` matches nothing and
    // the gate silently narrows to the homepage alone. It still reports a clean
    // pass, having checked a third of what it should have. Anchoring on a
    // segment boundary instead covers both forms.
    const pick = (re) => hrefs.find((h) => re.test(h));
    return [pick(/(^|\/)collections\//), pick(/(^|\/)products\//)].filter(Boolean);
  });
  return ['/', ...found.map((h) => (h.startsWith('/') ? h : '/' + h))]
    // The capture writes /products/x.html; the live site serves /products/x.
    // Both sides get the form they understand, resolved per side below.
    .map((r) => r.replace(/\.html$/, ''));
}

const liveUrlFor = (route) => LIVE + (route === '/' ? '/' : route);
const cloneUrlFor = (route) => CLONE + (route === '/' ? '/' : route);

/**
 * The live requirement set, sampled twice per route and intersected.
 *
 * Cached to disk and reused across repair rounds. That is not an optimisation
 * — it is the difference between one polite pass over the merchant's origin
 * and six of them. Re-crawling live after every repair would be both slower
 * and ruder, and the live site is not what the repairs change.
 */
async function buildLiveRequirement(browser, routes) {
  if (CACHE && existsSync(CACHE) && !REFRESH) {
    try {
      const c = JSON.parse(readFileSync(CACHE, 'utf8'));
      // The schema version is load-bearing, not bookkeeping. A cache written
      // before a new field existed silently DISABLES every check that reads
      // that field — the assertion still runs, finds the field undefined, and
      // quietly passes. That happened here: `scriptHosts` was added to power
      // vendor attribution, a stale cache had none, and every third-party
      // error went back to being reported as a defect with no sign anything
      // was wrong. Bump this whenever the requirement shape changes.
      if (c.schema === REQUIREMENT_SCHEMA && c.live === LIVE && routes.every((r) => c.routes[r])) {
        console.log(`  (live requirement from cache: ${CACHE})`);
        return c.routes;
      }
      if (c.schema !== REQUIREMENT_SCHEMA) {
        console.log(`  (live requirement cache is schema ${c.schema ?? 'pre-1'}, need ${REQUIREMENT_SCHEMA} — re-measuring live)`);
      }
    } catch (_) { /* a corrupt cache is just a cache miss */ }
  }
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  const out = {};
  for (const route of routes) {
    process.stdout.write(`  live ${route} …`);
    let a, b;
    // One polite retry after a pause. A single 60s timeout on an ad-heavy
    // product page is ordinary and should not void the whole run; a SECOND
    // failure after resting is the store throttling us, and that is reported
    // as a harness failure, never retried against.
    const sample = async () => [await inventoryRoute(page, liveUrlFor(route)),
                                await inventoryRoute(page, liveUrlFor(route))];
    try {
      try { [a, b] = await sample(); }
      catch (first) {
        process.stdout.write(` (${String(first.message).split('\n')[0].slice(0, 60)} — resting 20s, one retry)`);
        await new Promise((r) => setTimeout(r, 20000));
        [a, b] = await sample();
      }
    } catch (e) {
      await ctx.close();
      harnessFail(`could not load the live page ${liveUrlFor(route)} — ${e.message}`,
        'the store may be rate limiting or bot walling; retry later, or pass --offline to check the clone alone');
    }
    // Actuate the safe controls on live so we learn what "working" looks like
    // for THIS theme. Never add-to-cart and never checkout: those write to a
    // real merchant's session, and we have no business doing that.
    const oracle = await probeKinds(page, a, ['menu', 'search', 'carousel', 'filter', 'cart']);
    out[route] = { ...stable(a, b), oracle };
    console.log(` ${out[route].headings.length} sections, ${out[route].controls.length} controls`);
  }
  await ctx.close();
  if (CACHE) {
    mkdirSync(dirname(CACHE), { recursive: true });
    writeFileSync(CACHE, JSON.stringify({ schema: REQUIREMENT_SCHEMA, live: LIVE, at: Date.now(), routes: out }, null, 1));
  }
  return out;
}

/**
 * Actuate one representative control of each named kind and record whether it
 * responded. One per kind, not all of them: the question is "does this theme's
 * carousel work", and clicking twenty Next buttons answers it twenty times.
 */
async function probeKinds(page, snap, kinds, { wait } = {}) {
  const out = {};
  let live = snap;
  for (const kind of kinds) {
    // Re-read the page before each probe. The previous probe may have opened a
    // drawer, navigated, or triggered a React re-render, any of which
    // invalidates the stamps taken with `snap`. Re-snapshotting is cheap and
    // it is the difference between a probe that measures the control it named
    // and one that measures whatever happens to hold that id now.
    if (kind !== kinds[0]) live = (await page.evaluate(SNAPSHOT).catch(() => null)) || live;
    // VISIBLE only. A hamburger that is display:none at 1440px still exists in
    // the DOM on a desktop layout, and clicking it fires no handler — so
    // falling back to an invisible control produced a flapping "menu responds"
    // assertion that passed on the homepage and failed on the product page for
    // no reason but which template hid it. A gate that flaps is worse than one
    // that is silent: it makes the repair loop's score noisy and triggers false
    // regressions. If nothing visible answers to this kind, this page simply
    // does not offer it, and the live oracle will say the same.
    const c = live.controls.find((x) => x.kind === kind && x.visible);
    if (!c) continue;
    // The key is passed so actuate can re-find this control after an earlier
    // probe navigated the page and wiped the stamps. Without it every probe
    // after the first navigating one reported "control no longer in the DOM"
    // and scored as a broken feature — a harness bug reported as a fidelity
    // bug, which is the precise thing this tool exists to stop.
    const r = await actuate(page, c.ctl, { key: c.key, ...(wait ? { wait } : {}) }).catch((e) => ({ ok: false, responded: false, why: e.message }));
    out[kind] = { name: c.name, responded: r.responded, why: r.why, vendor: c.vendor || null };
    if (r.navigated) {
      await page.goBack({ waitUntil: 'load', timeout: 30000 }).catch(() => {});
      await page.waitForTimeout(2000);
      // Re-stamp: goBack rebuilds the document, so the ids in `snap` are stale
      // for every remaining kind.
      await page.evaluate(SNAPSHOT).catch(() => {});
    }
  }
  return out;
}

// ---------------------------------------------------------------------------

async function main() {
  let browser;
  try {
    browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
  } catch (e) {
    harnessFail(`Playwright could not launch a browser — ${e.message}`, 'npx playwright install chromium');
  }

  // The clone must be up before anything else is worth measuring. A connection
  // refused here is a harness problem (nobody started the server), not a
  // fidelity problem, and conflating them wastes a repair round.
  try {
    const r = await fetch(CLONE + '/', { signal: AbortSignal.timeout(8000) });
    if (!r.ok) throw new Error('HTTP ' + r.status);
  } catch (e) {
    await browser.close();
    harnessFail(`the clone at ${CLONE} did not answer — ${e.message}`,
      'start it first: bun catalog/heroserve-fl.ts <site> <port>');
  }

  let ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  let page = await ctx.newPage();

  const routes = await chooseRoutes(page);
  console.log(`\nroutes: ${routes.join(', ')}`);

  const want = OFFLINE ? null : await buildLiveRequirement(browser, routes);

  // Whether the commerce bridge has a backend at all. Without one the cart and
  // checkout features cannot be proved either way, and saying so is the honest
  // answer — see `skip` above.
  let health = { configured: false, reachable: false };
  try {
    const h = await fetch(CLONE + '/__fl/health', { signal: AbortSignal.timeout(6000) });
    if (h.ok) health = await h.json();
  } catch (_) { /* an older heroserve has no health endpoint; treat as unconfigured */ }

  for (const route of routes) {
    console.log(`\n  clone ${route} …`);
    let got;
    try {
      got = await inventoryRoute(page, cloneUrlFor(route));
    } catch (e) {
      // A crashed renderer (a page that ate the machine's memory — seen as a
      // 4GB headless process on deathwishcoffee.com) kills the tab, not the
      // storefront. Retry once in a fresh context; if it crashes again, that
      // PAGE is the defect and the other routes still get judged. Anything
      // that is not a crash means the server is gone: a harness failure.
      if (/crash/i.test(String(e.message))) {
        try { await ctx.close(); } catch {}
        ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
        page = await ctx.newPage();
        try {
          got = await inventoryRoute(page, cloneUrlFor(route));
        } catch (e2) {
          const L0 = route === '/' ? 'home' : route;
          check(`${L0}: page renders`, false, `the browser crashed loading this page twice (${String(e2.message).slice(0, 60)}) — runaway script or media`);
          defect('page', `${route} crashes the browser (out of memory)`, null, route);
          try { await ctx.close(); } catch {}
          ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
          page = await ctx.newPage();
          continue;
        }
      } else {
        await browser.close();
        harnessFail(`could not load the clone page ${cloneUrlFor(route)} — ${e.message}`,
          'is the storefront still serving?');
      }
    }
    const L = route === '/' ? 'home' : route;
    const req = want ? want[route] : null;

    // ---- did it render at all -------------------------------------------
    // First, because every assertion below is meaningless on an error page,
    // and a blank clone otherwise reports as "no missing controls".
    const rendered = got.textLength > 200 && got.domNodes > 80;
    check(`${L}: page renders`, rendered, `${got.domNodes} nodes, ${got.textLength} chars of text`);
    if (!rendered) {
      defect('page', `${route} renders blank or errored`, 'recapture', route);
      continue;
    }

    // ---- template -----------------------------------------------------------
    // Shopify stamps every section with the template that rendered the page.
    // When the live site serves a different template than the capture — an
    // A/B test, or a theme change since the crawl (gorillamind.com switched
    // its product template between 10:11 and 13:30 the same day) — comparing
    // sections, landmarks, controls and pickers one by one reports phantom
    // defects. Say the one true thing instead, and route it to a targeted
    // recapture, which picks up whichever template the store now serves.
    const templateChanged = !!(req && req.templateId && got.templateId && req.templateId !== got.templateId);
    // Recaptured this run and STILL a different template than live: the
    // store assigns templates per visitor (fromourplace.com's product page),
    // and a re-crawl cannot change which one we get. A live condition, named;
    // not a feature the clone is missing.
    const recapturedRoutes = new Set((arg('--recaptured') || '').split(',').filter(Boolean));
    const templateAB = templateChanged && recapturedRoutes.has(route);
    if (req && req.templateVaries) {
      skip(`${L}: structural comparison`, 'live is A/B testing templates — two samples rendered different templates');
    }
    if (templateAB) {
      skip(`${L}: same template as live`, `live serves ${req.templateId}, the capture is ${got.templateId} even after a re-crawl — the store assigns templates per visitor; the clone is one of its variants`);
      defect('page', `live serves a different template than the capture on ${route} (A/B test)`, null, route, 'live A/B test');
    } else if (templateChanged) {
      check(`${L}: same template as live`, false,
        `live serves ${req.templateId}, the capture is ${got.templateId} — A/B test or theme change since the crawl`);
      defect('page', `template changed on ${route}: live ${req.templateId}, captured ${got.templateId}`, 'recapture', route);
    }
    const structural = !!(req && !templateChanged && !req.templateVaries);

    // ---- sections ---------------------------------------------------------
    if (structural) {
      const hs = new Set(got.headings);
      const allLost = req.headings.filter((h) => !hs.has(h));
      const lost = allLost.filter((h) => !req.headingVendor?.[h] && !req.headingDynamic?.[h] && !req.headingSlide?.[h]);
      // Carousel slides: judged by the carousel having content on the clone.
      const slideLost = allLost.filter((h) => req.headingSlide?.[h] && !req.headingVendor?.[h]);
      if (slideLost.length) {
        const cloneSlides = Object.keys(got.headingSlide || {}).length;
        check(`${L}: carousel content present`, cloneSlides > 0,
          cloneSlides > 0 ? `${cloneSlides} slide heading(s) on the clone (live showed ${slideLost.length} other slide(s), as carousels do)` : 'no carousel slide content on the clone');
        if (!cloneSlides) defect('dynamic', `carousel content missing: ${slideLost.slice(0, 2).map((h) => JSON.stringify(h.slice(0, 30))).join(', ')}`, 'recapture', route);
      }
      const vendorLost = allLost.filter((h) => req.headingVendor?.[h]);
      // Headings inside a recommendations / upsell block are that block's
      // current picks — a value. Judged once, by whether the block is filled.
      const dynamicLost = allLost.filter((h) => req.headingDynamic?.[h] && !req.headingVendor?.[h]);
      check(`${L}: every live section present`, lost.length === 0,
        lost.length ? `${lost.length} of ${req.headings.length} missing: ${lost.slice(0, 3).map((x) => JSON.stringify(x.slice(0, 32))).join(', ')}`
                    : `${req.headings.length} section(s)` + (vendorLost.length ? `, ${vendorLost.length} third-party` : ''));
      for (const h of lost) defect('section', `heading "${h.slice(0, 60)}"`, 'recapture', route);
      for (const h of vendorLost) defect('section', `heading "${h.slice(0, 50)}"`, null, route, req.headingVendor[h]);
      if (dynamicLost.length) {
        const filled = Object.keys(got.headingDynamic || {}).length > 0 || got.controls.some((c) => c.dynamic);
        const blocks = [...new Set(dynamicLost.map((h) => req.headingDynamic[h]))];
        check(`${L}: recommendation blocks filled`, filled,
          filled ? `${blocks.length} block(s) filled with the clone's own picks (live's ${dynamicLost.length} picks differ, as picks do)`
                 : `${blocks.join(', ')} empty on the clone`);
        if (!filled) defect('dynamic', `recommendation / upsell block empty: ${blocks.join(', ')}`, null, route);
      }

      const lm = new Set(got.landmarks);
      const lostLm = req.landmarks.filter((x) => !lm.has(x));
      check(`${L}: page chrome intact`, lostLm.length === 0, lostLm.length ? `missing ${lostLm.join(', ')}` : req.landmarks.join(', '));
      for (const x of lostLm) defect('landmark', x, 'recapture', route);
    }

    // ---- fonts ------------------------------------------------------------
    // The clone is allowed to have loaded MORE faces than live (it serves them
    // all from one origin, so nothing is deferred). It is not allowed to be
    // painting with a face live never used — that is a fallback, and a
    // fallback changes every text metric on the page.
    if (structural) {
      const have = new Set([...got.fontsInUse, ...got.fontsLoaded]);
      const lostFonts = req.fontsInUse.filter((f) => !have.has(f) && !/^(-apple-system|system-ui|sans-serif|serif|monospace|arial|helvetica)/.test(f));
      check(`${L}: brand typefaces load`, lostFonts.length === 0,
        lostFonts.length ? `falling back for: ${lostFonts.slice(0, 4).join(', ')}` : `${req.fontsInUse.length} family(ies) in use`);
      for (const f of lostFonts) defect('font', f, 'localize-fonts', route);
    }

    // ---- media ------------------------------------------------------------
    check(`${L}: no broken images`, got.media.imagesBroken === 0,
      got.media.imagesBroken
        ? `${got.media.imagesBroken}: ${(got.media.brokenSamples || []).join(', ')}`
        : `none (${got.media.imagesBrokenExternal} blocked tracking pixel(s) ignored)`);
    if (got.media.imagesBroken > 0) defect('image', `${got.media.imagesBroken} broken <img> on ${route}: ${(got.media.brokenSamples || []).join(', ')}`, 'localize-runtime', route);

    if (structural) {
      const need = Math.floor(req.media.imagesRendered * COUNT_FLOOR);
      check(`${L}: imagery renders`, got.media.imagesRendered >= need,
        `${got.media.imagesRendered} rendered vs ${req.media.imagesRendered} live (floor ${need})`);
      if (got.media.imagesRendered < need) defect('image', `only ${got.media.imagesRendered}/${req.media.imagesRendered} images render on ${route}`, 'localize-runtime', route);

      const needBg = Math.floor(req.media.backgroundImages * COUNT_FLOOR);
      check(`${L}: background art renders`, got.media.backgroundImages >= needBg,
        `${got.media.backgroundImages} vs ${req.media.backgroundImages} live`);
      if (got.media.backgroundImages < needBg) defect('image', `background imagery thin on ${route}`, 'localize-runtime', route);
    }

    // ---- dead assets ------------------------------------------------------
    // A 404 on a file the capture believed it had saved is the real defect,
    // and it is completely invisible in a screenshot. Two filters separate it
    // from the two kinds of 404 that are the system working, and both are
    // load-bearing.
    //
    // `ours` drops every external failure: a request to the merchant's CDN
    // failing IS the offline guarantee, not a defect.
    //
    // The bucket test drops `_a/other`, which is the capture's unknown-type
    // bin. A tracker URL rewritten into it and then re-appended with query
    // junk by the page's own script ("...bin&cx=c&gtm=...") 404s harmlessly
    // and forever; it is not a file anybody lost. The typed buckets — js, css,
    // img, font, ext — are files the crawl believed it had saved, so a 404
    // there is a genuine disagreement between the capture and the page, and
    // that is the one that shows up as a page which looks fine and does
    // nothing.
    // A request under one of our localised VENDOR files (OneTrust asking
    // <otSDKStub.js>/consent/<id>.json) is that vendor's runtime, refused by
    // design — not an asset the capture failed to save.
    const originsDead = await loadOrigins(page);
    const vendorFileRequest = (p) => {
      const m = p.match(/^\/(_a\/[^?#]*?\.(?:m?js|css))\/.+/);
      if (!m) return false;
      const rel = m[1];
      let origin = originsDead[rel];
      if (!origin) { const base = rel.split('/').pop().replace(/\.[0-9a-f]{10}(\.[a-z0-9]+)$/i, '$1'); for (const k of Object.keys(originsDead)) if (k.split('/').pop().replace(/\.[0-9a-f]{10}(\.[a-z0-9]+)$/i, '$1') === base) { origin = originsDead[k]; break; } }
      if (!origin) return false;
      try { const o = new URL(origin); const oh = o.host.replace(/^www\./, ''); const sh = (() => { try { return new URL(LIVE).host.replace(/^www\./, ''); } catch { return ''; } })(); return oh !== sh && (oh !== 'cdn.shopify.com' || /\/(extensions|shopifycloud)\//.test(o.pathname)); } catch { return false; }
    };
    const ourDead = got.deadRequests.filter((d) => d.ours && /\/_a\/(js|css|img|fonts?|ext)\//.test(d.path) && !vendorFileRequest(d.path));
    const blocked = got.deadRequests.filter((d) => !d.ours).length;
    // A 404 on our origin OUTSIDE the asset buckets is a third-party endpoint
    // the crawl rewrote to a local path and we never saved — Shopify's own
    // analytics beacon, its hosted checkout bundle, an Elevar collector. Those
    // are the offline guarantee, not lost files, but they are counted and
    // shown rather than quietly dropped.
    const rewrittenThirdParty = got.deadRequests.filter((d) => d.ours && !/\/_a\//.test(d.path)).length;
    check(`${L}: no dead asset requests`, ourDead.length === 0,
      ourDead.length ? `${ourDead.length}: ${ourDead.slice(0, 3).map((d) => d.why + ' ' + d.path).join(', ')}`
                     : `none (${blocked} external request(s) refused, as intended)`);
    // 'refetch-missing', not 'localize-runtime'. These are OUR paths: the file
    // is absent from the capture, and the only place the bytes still exist is
    // the live storefront. localize-runtime only ever looks at other hosts, so
    // naming it here produced a defect with a repair attached that could not
    // move it — which reads as a repair loop that gave up rather than one
    // pointed at the wrong tool.
    for (const d of ourDead.slice(0, 10)) defect('asset', `${d.why} ${d.path}`, 'refetch-missing', route);

    // ---- scripts: the page's own opinion of whether it is healthy ---------
    // Trackers are deliberately not localised (a demo must not beacon to the
    // merchant's analytics from a client's machine), so the errors they throw
    // when blocked are a policy outcome, not a defect. Separating the two is
    // what keeps a "zero console errors" bar both reachable AND meaningful.
    //
    // Classified by ORIGIN and by evidence, not by keyword. Anything complaining about a URL
    // that is not ours is the offline guarantee working as designed: the
    // served CSP refuses the merchant's ad stack, and those scripts then throw.
    // Keyword lists go stale the moment a store adds a vendor nobody listed —
    // the origin test never does.
    // Third-party script hosts the LIVE page used, minus anything the clone
    // still reaches. Everything left is a vendor whose code we deliberately do
    // not run, and therefore a plausible author of an error on the clone.
    const vendorHosts = [...new Set([...(req?.scriptHosts || []), ...(req?.contactedHosts || [])])]
      .map((h) => h.toLowerCase());
    // Where each local asset came from, recorded by the crawl. Lets an error's
    // FRAME be attributed by origin — evidence — instead of by file name.
    const origins = await loadOrigins(page);
    const storeHost = (() => { try { return new URL(LIVE).host.replace(/^www\./, ''); } catch { return ''; } })();
    const isPolicy = (e) => {
      const u = e.url || '';
      if (u && !/^https?:\/\/(localhost|127\.0\.0\.1)/.test(u)) return true;
      // A frame under /_a/ whose recorded ORIGIN is a third-party host, or a
      // Shopify app extension (cdn.shopify.com/extensions/ is reserved for
      // apps — a contract, not a guess), is a vendor script failing without
      // its backend: the offline guarantee. The theme's own files come from
      // the store's host or from cdn.shopify.com/s/files/…/t/… and never
      // match here, so a genuine theme error is still reported.
      const fm = u.match(/^https?:\/\/(?:localhost|127\.0\.0\.1)(?::\d+)?\/(_a\/[^?#]+)/);
      // The sibling resolver serves a hashed asset under the unhashed name a
      // module asked for (_a/chunks/x.esm.js for _a/js/x.esm.<hash>.js); match
      // the origin by base name when the exact key is absent.
      const originOf = (rel) => {
        if (origins[rel]) return origins[rel];
        const base = rel.split('/').pop().replace(/\.[0-9a-f]{10}(\.[a-z0-9]+)$/i, '$1');
        for (const k of Object.keys(origins)) if (k.split('/').pop().replace(/\.[0-9a-f]{10}(\.[a-z0-9]+)$/i, '$1') === base) return origins[k];
        return null;
      };
      if (fm && originOf(fm[1])) {
        try {
          const o = new URL(originOf(fm[1]));
          const oh = o.host.replace(/^www\./, '');
          // cdn.shopify.com hosts the merchant's theme files under /s/files/…,
          // app scripts under /extensions/, and Shopify's own platform code
          // (web pixels, consent, checkout) under /shopifycloud/. The last
          // two are third parties to the capture — by contract of the path.
          if (oh !== storeHost && (oh !== 'cdn.shopify.com' || /\/(extensions|shopifycloud)\//.test(o.pathname))) return true;
        } catch {}
      }
      // On the clone, third-party scripts we chose to localise live under
      // /_a/ext/ (localize-runtime writes them there). An error whose frame is
      // one of those files is a vendor script failing without its backend —
      // the offline guarantee, not a capture defect. The theme's own bundles
      // live under /_a/js/, so this cannot swallow a genuine theme error.
      if (/^https?:\/\/(localhost|127\.0\.0\.1)(:\d+)?\/_a\/ext\//.test(u)) return true;
      // The message names one of OUR localised files (a vendor script that
      // parses its own src and chokes on the relative path: "Failed to parse
      // scriptSrc: ../_a/js/novel-storefront…"). Attribute by that file's
      // origin, exactly like a frame.
      const pm = e.text.match(/_a\/[^\s'"()]+/);
      if (pm && originOf(pm[0])) {
        try {
          const o = new URL(originOf(pm[0]));
          const oh = o.host.replace(/^www\./, '');
          if (oh !== storeHost && (oh !== 'cdn.shopify.com' || /\/(extensions|shopifycloud)\//.test(o.pathname))) return true;
        } catch {}
      }
      if (/refused|blocked|Content Security Policy|ERR_BLOCKED|violates the following/i.test(e.text)) return true;
      // The message TEXT naming an external host is the same evidence as the
      // location naming one — Chrome puts the URL in one or the other
      // depending on how the load failed, and truncation can cut the "was
      // blocked" clause off the end.
      const inText = e.text.match(/https?:\/\/([a-z0-9.-]+)/i);
      if (inText && !/^(localhost|127\.0\.0\.1)/.test(inText[1])) return true;
      // The second-order failures: a page whose analytics script we refused
      // then throws on the global that script was going to define. That is the
      // offline guarantee working, one frame later. The list mirrors the
      // TRACKERS list in localize-runtime.mjs — the vendors we deliberately do
      // not localise — so the two cannot drift into disagreeing.
      if (/\b(fbq|gtag|dataLayer|ga|hj|_hsq|uetq|twq|rdt|obApi|_learnq|klaviyo|ttq|snaptr|pintrk|webPixelsManager|createShopifyExtend|Shopify\.analytics|clarity|amplitude|mixpanel|heap|attentive|Yotpo|okendo|UserWay\w*|updatePageElementsList|usercentrics|OneTrust|Optanon|CookieConsent|cookieconsent)\b/.test(e.text) &&
          /is not defined|is not a function|undefined/.test(e.text)) return true;
      // Derived rather than listed: pull the identifiers out of the error and
      // check them against the third-party hosts the LIVE page loaded scripts
      // from. `window.loomi.conf` matches loomi.ai; `loomi_api is not defined`
      // matches it too. That is evidence the error belongs to a vendor script
      // we chose not to ship — and unlike the fixed list above it stays
      // correct for a store running a vendor nobody has heard of. Only fires
      // when the identifier is distinctive enough (4+ characters) to mean
      // something.
      for (const ident of e.text.match(/[A-Za-z][A-Za-z0-9_]{3,}/g) || []) {
        const needle = ident.toLowerCase().replace(/_.*$/, '');
        if (needle.length < 4) continue;
        if (vendorHosts.some((h) => h.includes(needle))) return true;
      }
      // "Failed to load resource" is Chrome restating a subresource failure it
      // already reported as a network event. The dead-asset assertion above
      // owns that case and owns it far more precisely, with the URL attached —
      // counting it here too would double every asset defect and add nothing.
      if (/^Failed to load resource/i.test(e.text)) return true;
      // A fetch that failed, on a page where NOTHING of ours 404'd, can only
      // have been a fetch to somewhere else — which is the served CSP refusing
      // it, which is the offline guarantee. Deduced from the network record
      // rather than from a list of vendor names, so it stays correct when a
      // store adds a vendor nobody has heard of. Country detection, currency
      // rates, an app-info endpoint and a launcher SDK all land here on
      // graza.co, and every one of them is a third party we chose not to ship.
      // "Loading chunk N failed" is a fetch failure wearing a bundler's
      // wording — on graza.co every one of them is a piece of Shopify's HOSTED
      // checkout bundle (/cdn/shopifycloud/checkout-web/…), which is closed,
      // hosted, and explicitly never migrated. Same deduction, same evidence.
      if (/failed to fetch|networkerror|load failed|err_failed|loading chunk \S+ failed|error loading (dynamically imported )?module/i.test(e.text) &&
          ourDead.length === 0) return true;
      return false;
    };

    // Errors the LIVE page throws too are not something the clone introduced.
    const liveErrs = new Set(req?.consoleErrors || []);
    const realErrors = [];
    for (const e of got.consoleErrors.filter((x) => !isPolicy(x) && !liveErrs.has(x.text))) {
      const rm = /Identifier '([A-Za-z_$][\w$]*)' has already been declared/.exec(e.text);
      if (rm) {
        const v = await redeclaredByVendor(page, route, rm[1], origins, vendorHosts, storeHost);
        if (v) { defect('script', e.text.slice(0, 120), null, route, v); continue; }
      }
      // Thrown from an inline script on the page itself: find that script by
      // line and ask whether its own text names a vendor (GTM tags, app
      // snippets injected at runtime and captured inline).
      if (e.line && (!e.url || e.url.replace(/[?#].*$/, '') === cloneUrlFor(route).replace(/[?#].*$/, ''))) {
        const v = await inlineScriptVendor(page, route, e.line, vendorHosts);
        if (v) { defect('script', e.text.slice(0, 120), null, route, v); continue; }
      }
      if (!e.url && /Invalid or unexpected token|Unexpected (token|identifier|end of input|string|number)|missing \) after/.test(e.text)) {
        const src = await syntaxErrorSource(page, route, vendorHosts);
        if (src && src.vendor) { defect('script', `${e.text.slice(0, 80)} — in a ${src.vendor} inline snippet`, null, route, src.vendor); continue; }
        if (src) e.text = `${e.text.slice(0, 80)} — inline script starting: ${src.head.slice(0, 90)}`;
      }
      realErrors.push(e);
    }
    check(`${L}: no console errors`, realErrors.length === 0,
      realErrors.length ? `${realErrors.length}: ${realErrors.slice(0, 2).map((e) => e.text).join(' | ').slice(0, 150)}`
                        : `${got.consoleErrors.length} message(s), all from deliberately-blocked third parties` +
                          (rewrittenThirdParty ? ` (${rewrittenThirdParty} rewritten third-party endpoint(s) 404 by design)` : ''));
    // No repair is named because none exists for these. An error the origin
    // and vendor tests could not attribute is almost always the theme's own
    // code failing because a vendor it expects is deliberately absent — the
    // offline guarantee working, not a file we can fetch. Labelling it
    // 'localize-runtime' sent the repair loop after something it cannot fix.
    // The frame URL is carried so a reader can see where it came from.
    for (const e of realErrors.slice(0, 8)) defect('script', e.text.slice(0, 120), null, route, null, e.url || '');

    // ---- navigation -------------------------------------------------------
    // Every internal destination the page offers must be served. This is the
    // assertion that catches a clone whose menu links all bounce to the live
    // store — which looks perfect until someone clicks.
    if (req) {
      const internal = [...new Set(req.navTargets
        .filter((h) => !/^https?:/.test(h) || h.startsWith(LIVE))
        .map((h) => { try { return new URL(h, LIVE + '/').pathname; } catch { return null; } })
        .filter(Boolean)
        // /search is Shopify's server-rendered results page. The clone has no
        // index to answer a bare GET with, and the search CONTROL is asserted
        // on its own below ("search responds"), so the destination sits on the
        // same policy line as /account and /checkouts — not a defect.
        .filter((p) => !/^\/(cdn|checkouts?|account|challenge|search)\b/.test(p))
        // javascript:void(0) and friends are not destinations (hiyahealth.com
        // reported "void(0) -> ERR" on every page).
        .filter((p) => !/^\/?(void|javascript)/i.test(p)))].slice(0, 40);
      const dead = [];
      for (const p of internal) {
        const r = await page.request.get(CLONE + p, { timeout: 15000 }).catch(() => null);
        // .html is how the capture stores pages; heroserve resolves the
        // extensionless form, but an older static server may not.
        const ok = r && r.status() < 400;
        if (!ok) {
          const r2 = await page.request.get(CLONE + p.replace(/\/$/, '') + '.html', { timeout: 15000 }).catch(() => null);
          if (!r2 || r2.status() >= 400) dead.push(`${p} -> ${r ? r.status() : 'ERR'}`);
        }
      }
      // `/apps/<name>` is Shopify's app-proxy prefix: the page at the other end
      // is rendered by a third-party app's server, not by the storefront, so
      // there is nothing on the merchant's side to capture. It is the same
      // policy line as the review widgets, drawn at a URL instead of a DOM
      // subtree — and it is a CONTRACT (the prefix is reserved), not a guess.
      // Shopify reserves four app-proxy prefixes, not one: /apps, /a,
      // /community and /tools (kettleandfire's shop is under /a/).
      const APP_PROXY = /^\/(apps|a|community|tools)\//;
      const deadApp = dead.filter((d) => APP_PROXY.test(d));
      // A destination that is dead on the LIVE site too (kettleandfire's menu
      // carries a literal "/[" that 404s upstream) is the merchant's bug, not
      // a capture gap. Ask live before blaming the clone.
      const deadLive = [];
      const deadReal = [];
      for (const d of dead.filter((d) => !APP_PROXY.test(d))) {
        const path = d.split(' -> ')[0];
        const lr = await page.request.get(LIVE + path, { timeout: 15000, maxRedirects: 5 }).catch(() => null);
        if (lr && lr.status() >= 400) deadLive.push(d); else deadReal.push(d);
      }
      check(`${L}: every live destination is served`, deadReal.length === 0,
        deadReal.length ? `${deadReal.length} of ${internal.length} dead: ${deadReal.slice(0, 3).join(', ')}`
                        : `${internal.length} destination(s)` + (deadApp.length ? `, ${deadApp.length} third-party app proxy` : '')
                          + (deadLive.length ? `, ${deadLive.length} dead on the live site too` : ''));
      for (const d of deadReal) defect('nav', d, 'recapture', route);
      for (const d of deadApp) defect('nav', d, null, route, 'shopify app proxy');
      for (const d of deadLive) defect('nav', d, null, route, 'dead on the live site too');
    }

    // ---- controls ---------------------------------------------------------
    if (structural) {
      const haveKeys = new Set(got.controls.map((c) => c.key));
      const lateKeys = await menuRevealedKeys(page, got);
      const allLostCtl = req.controls.filter((c) => !haveKeys.has(c.key) && !lateKeys.has(c.key));
      const lostCtl = allLostCtl.filter((c) => !c.vendor && !c.dynamic);
      const vendorCtl = allLostCtl.filter((c) => c.vendor);
      const dynamicCtl = allLostCtl.filter((c) => c.dynamic && !c.vendor);
      if (dynamicCtl.length && !got.controls.some((c) => c.dynamic) && !Object.keys(got.headingDynamic || {}).length) {
        defect('dynamic', `${dynamicCtl.length} control(s) inside an empty recommendation / upsell block`, null, route);
      }
      // Grouped by kind so the report says "the cart drawer is gone", not
      // "17 elements differ".
      const byKind = {};
      for (const c of lostCtl) (byKind[c.kind] ||= []).push(c.name);
      const byVendor = {};
      for (const c of vendorCtl) (byVendor[c.vendor] ||= []).push(c.name);
      check(`${L}: every live control present`, lostCtl.length === 0,
        lostCtl.length
          ? Object.entries(byKind).map(([k, v]) => `${k}×${v.length}`).join(', ') + ` of ${req.controls.length}`
          : `${req.controls.length} control(s)` +
            (vendorCtl.length ? `, ${vendorCtl.length} belonging to ${Object.keys(byVendor).join('/')}` : ''));
      for (const [k, names] of Object.entries(byKind)) {
        defect('control', `${k}: ${names.slice(0, 4).join(', ')}${names.length > 4 ? ` (+${names.length - 4})` : ''}`, 'recapture', route);
      }
      for (const [v, names] of Object.entries(byVendor)) {
        defect('control', `${names.length} control(s) inside the ${v} widget`, null, route, v);
      }

      // Variant / size pickers, discovered structurally rather than by class.
      if (req.optionGroups.length) {
        const haveG = new Set(got.optionGroups.map((g) => g.kind + ':' + g.name));
        const lostAll = req.optionGroups.filter((g) => !haveG.has(g.kind + ':' + g.name));
        // A picker inside the cart drawer (an upsell's variant select) exists
        // only once the cart holds an item. Without a module the clone's cart
        // cannot be exercised, so the feature is unprovable here — the same
        // line add-to-cart draws below — not missing.
        const cartOnly = lostAll.filter((g) => g.inCart && !health.configured);
        const dynOnly = lostAll.filter((g) => g.dynamic && !(g.inCart && !health.configured));
        const lostG = lostAll.filter((g) => !(g.inCart && !health.configured) && !g.dynamic);
        const provable = req.optionGroups.length - cartOnly.length - dynOnly.length;
        if (dynOnly.length) skip(`${L}: ${dynOnly.length} picker(s) inside recommendation blocks`, 'a recommender\'s picks are values, judged by the block being filled');
        if (cartOnly.length) skip(`${L}: ${cartOnly.length} cart-drawer picker(s)`, 'need an item in the cart — run migrate with --server <url> --secret <key> to prove this');
        if (provable) {
          check(`${L}: variant pickers present`, lostG.length === 0,
            lostG.length ? `${lostG.length} of ${provable} missing` : `${provable} group(s)`);
        }
        for (const g of lostG) defect('variant', `${g.kind} "${g.name.slice(0, 40)}"`, 'recapture', route);
      }
    }

    // ---- add to cart -------------------------------------------------------
    // Two different questions, and conflating them is how "not configured"
    // ends up reported as "broken": does the control fire at all (always
    // answerable), and does it reach the ForkLaunch module (only answerable
    // with a module behind the bridge).
    const addCtl = templateChanged ? null : got.controls.find((c) => c.kind === 'addcart' && c.visible) ||
                   got.controls.find((c) => c.kind === 'addcart' && /\/products\//.test(route));
    if (addCtl) {
      const before = await moduleCartCount();
      const r = await actuate(page, addCtl.ctl, { wait: 1500, key: addCtl.key }).catch((e) => ({ responded: false, why: e.message }));
      check(`${L}: add-to-cart responds`, r.responded, `"${addCtl.name}" — ${r.why}`);
      if (!r.responded) defect('behaviour', `add-to-cart inert on ${route}`, 'localize-runtime', route);
      if (!health.configured) {
        skip(`${L}: add-to-cart reaches the module`, 'no ForkLaunch module configured — run migrate with --server <url> --secret <key> to prove this');
      } else if (!health.reachable) {
        check(`${L}: add-to-cart reaches the module`, false, `module ${health.module} configured but unreachable`);
        defect('backend', `module ${health.module} unreachable`, null, route);
      } else {
        const after = await moduleCartCount();
        check(`${L}: add-to-cart reaches the module`, after > before, `module cart ${before} -> ${after}`);
        if (!(after > before)) defect('backend', 'add-to-cart did not reach the module', null, route);
      }
    } else if (/\/products\//.test(route) && !templateChanged) {
      check(`${L}: add-to-cart present`, false, 'no add-to-cart control on a product page');
      defect('control', 'addcart missing on a product page', 'recapture', route);
    }

    // ---- behaviour: does it RESPOND ---------------------------------------
    // The half a static diff can never answer. Every kind that responded on
    // live must respond here; a kind that did nothing on live is not a feature
    // and is not held against the clone.
    // 'cart' LAST, deliberately. The bridge wires a cart control to jump
    // straight to /__fl/checkout, so probing it ends the document every
    // remaining probe was going to run against — and each of those then
    // reported "control no longer in the DOM" and scored as a broken feature.
    // Ordering costs nothing and removes a whole class of phantom failure.
    const kinds = ['menu', 'search', 'carousel', 'filter', 'cart'];
    const oracle = req?.oracle || {};
    if (templateChanged) {
      skip(`${L}: behaviours`, 'live is serving a different template than the capture on this page — its controls are not this page\'s controls');
    }
    const cloneProbe = await probeKinds(page, got, kinds);
    for (const kind of kinds) {
      if (templateChanged) break;
      const liveWorks = req ? oracle[kind]?.responded : undefined;
      const mine = cloneProbe[kind];
      if (req && !oracle[kind]) continue;                 // live has no such control
      // With no live oracle there is no requirement to compare against, so an
      // absent control is simply absent — reporting it as a missing feature
      // would be inventing a specification.
      if (!req && !mine) continue;
      if (req && liveWorks === false) {
        skip(`${L}: ${kind} responds`, `does not respond on the live site either — not a feature`);
        continue;
      }
      if (!mine) {
        // A control the live site renders from a third-party widget cannot be
        // reproduced by capturing a page — its data is in that vendor's
        // database and its script is blocked on purpose. graza.co's only
        // "filter" is Okendo's review-star filter, and reporting that as a
        // broken collection filter is both wrong and unfixable.
        if (oracle[kind]?.vendor) {
          skip(`${L}: ${kind} responds`, `belongs to the ${oracle[kind].vendor} widget — third-party, deliberately not migrated`);
          defect('behaviour', `${kind} ("${oracle[kind].name}") is part of the ${oracle[kind].vendor} widget`, null, route, oracle[kind].vendor);
          continue;
        }
        check(`${L}: ${kind} responds`, false, 'no such control on the clone');
        defect('behaviour', `${kind} control absent on ${route}`, 'recapture', route);
        continue;
      }
      // Present but inert, and the live twin belongs to a third-party widget:
      // the markup came across with the page, the script that drives it did
      // not, and never will. Okendo's review-star filter is the case here — a
      // "Filters" button that renders and does nothing, because the reviews
      // behind it live in Okendo's database. Reporting it as a broken filter
      // sends a repair after something no repair can reach.
      if (!mine.responded && oracle[kind]?.vendor) {
        skip(`${L}: ${kind} responds`, `renders, but is driven by the ${oracle[kind].vendor} widget — third-party, deliberately not migrated`);
        defect('behaviour', `${kind} ("${mine.name}") renders but is inert: it is part of the ${oracle[kind].vendor} widget`, null, route, oracle[kind].vendor);
        continue;
      }
      // A slow machine (a 2-vCPU box running three crawls) can miss a drawer
      // that opens late. One retry with a longer wait before "does not respond"
      // counts — the same control, the same page, more patience.
      let verdictProbe = mine;
      if (!mine.responded) {
        const fresh = (await page.evaluate(SNAPSHOT).catch(() => null)) || got;
        const again = await probeKinds(page, fresh, [kind], { wait: 3000 }).catch(() => ({}));
        if (again[kind]) verdictProbe = again[kind];
      }
      check(`${L}: ${kind} responds`, verdictProbe.responded, verdictProbe.why);
      if (!verdictProbe.responded) defect('behaviour', `${kind} ("${mine.name}") does not respond on ${route}`, 'localize-runtime', route);
    }

  }

  // ---- checkout ------------------------------------------------------------
  if (!health.configured) {
    skip('checkout renders', 'no ForkLaunch module configured — checkout is not wired in this run');
  } else if (!health.reachable) {
    check('checkout renders', false, `module ${health.module} unreachable`);
    defect('backend', `module ${health.module} unreachable`, null, '/__fl/checkout');
  } else {
    try {
      await page.goto(CLONE + '/__fl/checkout', { waitUntil: 'load', timeout: 45000 });
      await page.waitForTimeout(1500);
      const ok = await page.evaluate(() => !!document.querySelector('#go,#name,#line1,.ok h1,.done h1'));
      check('checkout renders', ok, ok ? '' : 'no address form and no confirmation');
      if (!ok) defect('backend', 'checkout page rendered nothing usable', null, '/__fl/checkout');
    } catch (e) {
      check('checkout renders', false, e.message.slice(0, 80));
    }
  }

  await browser.close();

  // ---- report --------------------------------------------------------------
  const failed = results.filter((r) => r.state === 'FAIL');
  const skipped = results.filter((r) => r.state === 'SKIP');
  console.log(`\ncheck-features — ${CLONE}${LIVE ? ' vs ' + LIVE : ' (offline)'}\n`);
  for (const r of results) console.log(`  ${r.state}  ${r.name}${r.detail ? '   (' + r.detail + ')' : ''}`);
  console.log(`\n${results.length - failed.length - skipped.length}/${results.length - skipped.length} passed` +
              (skipped.length ? `, ${skipped.length} skipped (not configured)` : '') + '\n');

  // Two lists, never one. A defect we chose (a third-party widget whose data
  // is in someone else's database) and a defect we caused are different kinds
  // of thing, and merging them means either hiding the first or never being
  // able to reach zero on the second.
  const blocking = missing.filter((m) => !m.policy);
  const policy = missing.filter((m) => m.policy);

  if (blocking.length) {
    console.log('missing or non-functional features:');
    for (const m of blocking) console.log(`  · [${m.kind}] ${m.what}${m.repair ? `  -> ${m.repair}` : '  -> no automated repair'}`);
    console.log('');
  }
  if (policy.length) {
    const byVendor = {};
    for (const m of policy) (byVendor[m.policy] ||= []).push(m);
    console.log('deliberately not migrated (third-party apps — their data lives in the vendor\'s');
    console.log('database, not the storefront\'s, and their scripts are blocked so an offline');
    console.log('demo cannot beacon from a client machine):');
    for (const [v, items] of Object.entries(byVendor)) {
      console.log(`  · ${v}: ${items.length} feature(s) — ${items.slice(0, 2).map((m) => m.what).join('; ').slice(0, 110)}`);
    }
    console.log('');
  }

  if (JSON_OUT) {
    mkdirSync(dirname(JSON_OUT), { recursive: true });
    writeFileSync(JSON_OUT, JSON.stringify({
      at: Date.now(), live: LIVE, clone: CLONE, routes,
      results, missing: blocking, policy, health
    }, null, 1));
  }

  process.exit(failed.length ? 1 : 0);

  async function moduleCartCount() {
    try {
      const r = await fetch(CLONE + '/__fl/cart', { signal: AbortSignal.timeout(6000) });
      const b = await r.json();
      return typeof b.item_count === 'number' ? b.item_count : -1;
    } catch { return -1; }
  }
}

// A crash IS a harness failure. It is not "the clone is fine" and it is not
// "the clone is broken" — it is "we do not know", and the loop must be told
// that in a way it cannot misread as either.
main().catch((e) => {
  console.error('\nHARNESS-FAIL: check-features crashed — ' + e.message);
  console.error(e.stack?.split('\n').slice(1, 4).join('\n') || '');
  process.exit(2);
});
