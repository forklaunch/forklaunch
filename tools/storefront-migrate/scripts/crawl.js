#!/usr/bin/env node
/**
 * Multi-page storefront crawl — produces a BROWSABLE local clone.
 *
 * capture.js saves one page; its links still point at the live site, so the
 * result is a photograph. This crawls the homepage plus collection and product
 * pages, shares one asset pool across all of them, then rewrites inter-page
 * links to point at the local copies. The result can actually be clicked through.
 *
 *   node crawl.js <domain> <outdir> [--pages N] [--clean] [--complete] [--only /a,/b]
 *     [--api <url>] [--hmac-secret <secret>]
 *
 * --api points the commerce bridge (bridge.js) at a running ForkLaunch
 * ecommerce module instead of the in-browser local cart — this is also
 * what wires up filters/sort/search against real GET /product data.
 * --hmac-secret must match that module's HMAC_SECRET_KEY; the module's
 * product/variant list endpoints are HMAC-authenticated (access:
 * 'internal'), so filter/sort/search calls will 403 without it even
 * though --api alone is enough for the (unauthenticated) cart forwarding.
 *
 * Layout produced:
 *   site/index.html
 *   site/collections/<handle>.html
 *   site/products/<handle>.html
 *   site/_a/{css,js,fonts,img}/...
 */
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { buildBridge } = require('./bridge.js');
const { buildCommerceOverlay } = require('./commerce.js');
const { pageFileFor, pageTypeFor, safe, SKIP_PATH } = require('./urlmap.js');

const VIEWPORT = { width: 1280, height: 800 };
const UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 ' +
           '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
// 700MB for a normal representative capture; effectively unbounded (20GB) for
// --complete, where a full catalog's images can run to several gigabytes.
const MAX_BYTES = process.argv.includes('--complete')
  ? 20 * 1024 * 1024 * 1024
  : 700 * 1024 * 1024;

const EXT_DIR = { css:'css', js:'js', woff:'fonts', woff2:'fonts', ttf:'fonts',
                  otf:'fonts', eot:'fonts', png:'img', jpg:'img', jpeg:'img',
                  gif:'img', webp:'img', svg:'img', avif:'img', ico:'img' };

const argv = process.argv.slice(2);
const domain = argv[0];
const outdir = argv[1];
const CLEAN = argv.includes('--clean');
const apiIdx = argv.indexOf('--api');
const API_BASE = apiIdx > -1 ? argv[apiIdx + 1] : null;
const hmacIdx = argv.indexOf('--hmac-secret');
// The real module's GET /product and GET /variant are HMAC-authenticated
// (access: 'internal') — see bridge.js's filtersMain docstring for the
// exact scheme. Without a matching secret, backend-mode filter/sort/search
// calls will 403; add-to-cart (unauthenticated forwarding, pre-existing)
// is unaffected either way.
const HMAC_SECRET = hmacIdx > -1 ? argv[hmacIdx + 1] : null;
const pagesIdx = argv.indexOf('--pages');
// --complete: photographic mode — capture EVERY public page the store's
// sitemap lists (products, collections, pages, blogs), no budget, no cap.
// Slower and larger, but it means no page is a stand-in. In this mode the
// page/byte ceilings are effectively removed and enumeration comes from the
// full sitemap rather than a budgeted homepage/anchor discovery.
const COMPLETE = argv.includes('--complete');
// --only /a,/b: TARGETED mode — capture exactly the named paths and ADD them
// to an existing capture, touching nothing else. This is what the verify loop
// runs when the gate names dead internal links: the answer to "four footer
// pages are missing" is four page loads, not a walk of the whole sitemap
// (graza.co's blog sitemap alone lists 693 URLs — six hours to fix a footer).
// Assets already on disk are never rewritten (the existsSync guard at the
// asset writer), so repairs applied to shared CSS/JS survive; the homepage is
// still loaded (discovery keys off it) but NOT rewritten; links from the new
// pages resolve against every page already on disk, not just this run's; and
// crawl.json / pages.json are merged into, not replaced.
const onlyIdx = argv.indexOf('--only');
const ONLY = onlyIdx > -1
  ? new Set(String(argv[onlyIdx + 1] || '').split(',').map((s) => s.trim()).filter(Boolean))
  : null;
// Apply a replace only OUTSIDE <script>…</script> bodies. The demo-mode link
// rewrite once ran over a vendor's inline `location.href="/checkout"` and
// produced `location.href="#" data-mirror-uncaptured="/checkout"` — a syntax
// error that took the whole inline script down (graza.co /pages/subscribe).
// Asset-URL rewrites still run inside scripts on purpose; only this one
// injects an attribute, and an attribute has no meaning inside JavaScript.
function replaceOutsideScripts(html, re, fn) {
  return html.split(/(<script\b[^>]*>[\s\S]*?<\/script\s*>)/i)
             .map((seg, i) => (i % 2 ? seg : seg.replace(re, fn)))
             .join('');
}
// Output files already present from the earlier capture (targeted mode only).
const ONLY_ON_DISK = new Set();
if (ONLY) {
  const siteDir = path.join(outdir, 'site');
  const walk = (d, rel) => {
    let ents = []; try { ents = fs.readdirSync(d, { withFileTypes: true }); } catch (_) { return; }
    for (const e of ents) {
      if (e.name === '_a' || e.name.startsWith('.')) continue;
      const r = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) walk(path.join(d, e.name), r);
      else if (e.name.endsWith('.html')) ONLY_ON_DISK.add(r);
    }
  };
  walk(siteDir, '');
}
const MAX_PAGES = COMPLETE ? Number.MAX_SAFE_INTEGER
  : (pagesIdx > -1 ? parseInt(argv[pagesIdx + 1], 10) || 20 : 20);
// Guaranteed nav-destination allocation, outside the normal page budget (see
// HARD_CAP below). Raised from 12: mega-menus routinely carry more than a
// dozen real category destinations (e.g. Men's/Women's split doubles every
// top-level category), and every one of them is a highly-visible dead link
// if missed.
const NAV_MUST_CAP = 40;   // brooklinen.com's menu has 105 destinations; 20 left 27 dead links for the loop to chase

const assetMap = new Map();          // absolute asset url -> local rel path
const capturingRel = new Set();      // local rel paths already fetched/queued —
                                      // every width/version variant of one image
                                      // now shares a rel (see localFor below), so
                                      // this stops the same picture being
                                      // downloaded once per srcset candidate.
let totalBytes = 0;

// ---- responsive-image normalization ----------------------------------
// Shopify (and most storefront CDNs) serve one base image at many sizes by
// appending resize hints to the query string — the same
// MS_T_VAR6_HERO_....webp is requested as ?width=216, ?width=288, ?v=123,
// ?v=123&width=432, etc. Every one of those is byte-for-byte the same
// picture at a different scale, so they must collapse to ONE captured
// file. If they don't: (a) we capture/store the same image N times, and
// (b) far worse, an <img>'s src or a srcset candidate whose exact query
// was never itself network-captured falls through to a literal
// substring/prefix match against a DIFFERENT query variant that WAS
// captured, leaving the un-matched tail of the query (e.g. "&width=432")
// dangling on the rewritten local path — a path no file on disk has.
//
// Framework image PROXIES (/_next/image?url=…, /cdn-cgi/image/…) are the
// opposite case: one proxy pathname serves every image on the page, and
// the query (or an encoded path segment) is what tells them apart — for
// those the query must never be stripped. isProxyImageUrl distinguishes
// the two so only genuine direct-CDN resize hints get normalized away.
// Resize/format/version query params that identify a *variant* of one image
// rather than a different image. Stripped (image URLs only) so every variant
// dedupes to one captured file with no dangling query on the local path.
// Long names are Shopify's; short ones (w/h/q/fm/fit/auto/...) are what
// Contentful, imgix and Next/image-passthrough CDNs use — e.g. ruggable
// serves images.ctfassets.net/...jpg?fm=avif&w=384&q=75.
const IMG_RESIZE_PARAMS = [
  'width', 'height', 'crop', 'v', 'format', 'quality', 'dpr', 'pad_color',
  'w', 'h', 'q', 'fm', 'fit', 'auto', 'bg', 'ar', 'cs', 'blur', 'sharpen', 'usm', 'ixlib'
];

function isImageExt(pathname) {
  const clean = pathname.split('/').pop() || '';
  const ext = (clean.includes('.') ? clean.split('.').pop() : '').toLowerCase();
  return EXT_DIR[ext] === 'img';
}

// Proxy pathnames known to multiplex many distinct images behind one route,
// plus the generic tell (a `url=` query param wrapping the real image URL) —
// same signal the proxy-unwrap logic elsewhere in this file already relies on.
function isProxyImageUrl(u) {
  if (/(^|\/)(_next\/image|cdn-cgi\/image|_image)(\/|$)/.test(u.pathname)) return true;
  if (u.searchParams.has('url')) return true;
  return false;
}

// Strips known resize/version query params from a direct (non-proxy) CDN
// image URL so every size variant of one base image normalizes to the same
// string. Proxy URLs and non-image URLs pass through untouched.
function normalizeImageUrl(url) {
  let u;
  try { u = new URL(url); } catch (_) { return url; }
  if (!isImageExt(u.pathname) || isProxyImageUrl(u)) return url;
  let changed = false;
  for (const p of IMG_RESIZE_PARAMS) {
    if (u.searchParams.has(p)) { u.searchParams.delete(p); changed = true; }
  }
  return changed ? u.toString() : url;
}

function localFor(url) {
  const key = normalizeImageUrl(url);
  const u = new URL(key);
  const clean = u.pathname.split('/').pop() || 'index';
  let ext = (clean.includes('.') ? clean.split('.').pop() : '').toLowerCase();
  if (!EXT_DIR[ext]) ext = 'bin';
  const sub = EXT_DIR[ext] || 'other';
  const h = crypto.createHash('md5').update(key).digest('hex').slice(0, 10);
  const base = clean.replace(/[^A-Za-z0-9._-]/g, '_').replace(/\.[^.]*$/, '').slice(0, 48) || 'a';
  return `_a/${sub}/${base}.${h}.${ext}`;
}

// ---- page identity ---------------------------------------------------------
// pageFileFor / pageTypeFor / safe / SKIP_PATH live in urlmap.js — the SINGLE
// source of truth shared with check-complete.mjs's fidelity verifier, so the
// "did we capture every page?" check can never drift from where the crawler
// actually writes them. See urlmap.js and its test (check-complete.test.mjs).

function fetchWithTimeout(resp, ms) {
  return Promise.race([resp.body(), new Promise(r => setTimeout(() => r(null), ms))]);
}

// Insert `script` immediately before the first </body> (case-insensitive),
// WITHOUT going through String.replace's string-replacement form. That form
// treats $&, $`, $', $1-$99 in the replacement as special patterns; any
// injected script that happens to contain one of those sequences (a literal
// '$' price prefix, a regex source, JSON with a lone "$'" substring, etc.)
// gets silently corrupted into invalid JS — the bridge script then throws a
// SyntaxError before window.fetch is ever patched, and every /cart/add.js
// call falls through to the static 404 body instead. Slicing around the
// match index inserts the text verbatim, no special-character interpretation
// possible. If there's no </body> at all, append at the end rather than
// dropping the script silently.
function injectBeforeBody(html, script) {
  const m = /<\/body>/i.exec(html);
  if (!m) return html + script;
  return html.slice(0, m.index) + script + html.slice(m.index);
}

// Insert into <head> using slice/concat (never html.replace, whose $-substitution
// corrupts injected scripts containing literal '$' — see injectBeforeBody).
function injectIntoHead(html, script) {
  const m = /<\/head>/i.exec(html);
  if (!m) return injectBeforeBody(html, script);
  return html.slice(0, m.index) + script + html.slice(m.index);
}

// Insert immediately after the opening <html> tag — the ABSOLUTE first thing the
// parser sees. Used for the commerce overlay because some captured storefronts
// serialize an inline <script> whose body contains a premature "</script>"
// sequence (a JSON/string literal); the HTML parser then flips into raw-text
// mode and silently swallows every tag AFTER it — including a <head>- or
// <body>-injected overlay (observed on jonesroadbeauty.com: our <script> tag
// never entered the DOM even with all other JS blocked). Parsing our overlay
// FIRST, before that malformed script, guarantees window.FL is defined and its
// MutationObserver is installed no matter how the rest of the document parses.
// Uses slice/concat, never html.replace ($-safe). Falls back to a prepend.
function injectAtDocStart(html, script) {
  const m = /<html[^>]*>/i.exec(html);
  if (!m) return script + html;
  const at = m.index + m[0].length;
  return html.slice(0, at) + script + html.slice(at);
}

(async () => {
  const result = { domain, ok: false, pages: 0, assets: 0, bytes: 0, reason: null,
                   startedAt: Date.now(), captured: [] };
  // Per-page route/file/title, for manifest.js. Kept out of crawl.json/result
  // entirely (rather than added to `captured`) so crawl.json's existing shape
  // stays exactly as-is for anything already reading it.
  const pageIndex = [];
  fs.mkdirSync(path.join(outdir, 'site'), { recursive: true });

  let browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
  let ctx = await browser.newContext({ viewport: VIEWPORT, userAgent: UA,
                                       ignoreHTTPSErrors: true });

  // Heavy/script-bloated storefronts can crash the headless renderer mid-crawl
  // (observed on getmaude.com: the browser closed after ~6 pages and, because
  // pages are written only at the end, the whole capture was lost). Relaunch a
  // dead browser so the crawl continues — the captured pages live in the
  // `captured` Map, not in the browser, so nothing already grabbed is lost.
  async function ensureBrowser() {
    if (browser.isConnected()) return;
    console.error('[crawl] renderer crashed — relaunching browser to continue');
    try { await browser.close(); } catch (_) {}
    browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
    ctx = await browser.newContext({ viewport: VIEWPORT, userAgent: UA,
                                     ignoreHTTPSErrors: true });
  }

  const pending = [];
  const attach = (page) => {
    page.on('response', async (resp) => {
      try {
        const url = resp.url();
        if (!/^https?:/.test(url)) return;
        const ct = (resp.headers()['content-type'] || '').split(';')[0];
        if (ct.startsWith('text/html')) return;
        if (resp.status() >= 400 || assetMap.has(url)) return;
        const rel = localFor(url);
        assetMap.set(url, rel);
        // A same-image resize variant we've already captured (or queued) —
        // localFor now maps every width/version of one CDN image to the same
        // rel, so this keys the download itself, not just the eventual path.
        if (capturingRel.has(rel)) return;
        capturingRel.add(rel);
        // Targeted mode: a file already on disk under its content-addressed
        // name IS this asset — possibly the shrunk / localized version of it.
        // Re-writing the original undid shrink-media on the homepage's videos
        // every recapture (60MB re-downloaded and re-encoded per round).
        if (ONLY && fs.existsSync(path.join(outdir, 'site', rel))) return;
        pending.push((async () => {
          try {
            const buf = await fetchWithTimeout(resp, 8000);
            if (!buf || !buf.length || totalBytes + buf.length > MAX_BYTES) return;
            totalBytes += buf.length;
            const dst = path.join(outdir, 'site', rel);
            fs.mkdirSync(path.dirname(dst), { recursive: true });
            fs.writeFileSync(dst, buf);
          } catch (_) {}
        })());
      } catch (_) {}
    });
  };

  const SCROLL = async (page) => {
    await page.evaluate(async () => {
      await new Promise(res => {
        let y = 0, i = 0;
        const step = () => {
          y += 700; i++;
          window.scrollTo(0, y);
          const done = i >= 60 || y >= 40000 || y >= document.body.scrollHeight + 1000;
          if (!done) setTimeout(step, 80);
          else { window.scrollTo(0, 0); setTimeout(res, 500); }
        };
        step();
      });
    }).catch(() => {});
  };

  // Grab a page's post-JS DOM plus the storefront links it contains.
  async function grab(url) {
    // newPage() is inside the try: on a crashed browser it throws
    // "Target page, context or browser has been closed", which must degrade to
    // a null (a normal fetch failure the caller can retry / recover from) rather
    // than propagate up and abort the whole crawl, discarding everything.
    let page;
    try {
      page = await ctx.newPage();
      attach(page);
      const r = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 40000 });
      // 404 = this page simply doesn't exist (we guessed a URL). That is NOT a
      // sign of throttling and must not count toward the backoff, or a few
      // speculative misses will abort an otherwise healthy crawl.
      if (r && r.status() === 404) return { notFound: true };
      if (!r || r.status() >= 400) return null;
      try { await page.waitForLoadState('networkidle', { timeout: 15000 }); } catch (_) {}
      await SCROLL(page);
      try { await page.waitForLoadState('networkidle', { timeout: 8000 }); } catch (_) {}
      await page.waitForTimeout(1200);

      // materialize CSSOM (styled-components/emotion leave <style> empty)
      await page.evaluate(() => {
        for (const s of Array.from(document.styleSheets)) {
          const n = s.ownerNode;
          if (!n || n.tagName !== 'STYLE') continue;
          if (n.textContent && n.textContent.trim().length) continue;
          let rules; try { rules = s.cssRules; } catch (_) { continue; }
          if (!rules || !rules.length) continue;
          try { n.textContent = Array.from(rules).map(x => x.cssText).join('\n'); } catch (_) {}
        }
      }).catch(() => {});

      // Promote lazy-loaded images before serializing. Many themes defer image
      // URLs in data-src/data-srcset and swap them onto src via an
      // IntersectionObserver that does not reliably fire under headless
      // capture — so the image is never fetched (never captured) and never
      // shown offline. Copy the deferred URL onto the real attribute, then let
      // the browser fetch it: that both pulls the bytes into the asset map and
      // makes the served clone display the image without the lazy library.
      await page.evaluate(() => {
        const SRC = ['data-src', 'data-original', 'data-lazy-src', 'data-lazy',
                     'data-image', 'data-fallback-src', 'data-srcurl', 'data-echo'];
        const SET = ['data-srcset', 'data-lazy-srcset'];
        const placeholder = s => !s || /^data:image\/(gif|svg)|placeholder|blank|1x1|spacer|lazy|transparent/i.test(s);
        for (const el of document.querySelectorAll('img, source')) {
          for (const a of SRC) { const v = el.getAttribute(a); if (v && placeholder(el.getAttribute('src'))) { el.setAttribute('src', v); break; } }
          for (const a of SET) { const v = el.getAttribute(a); if (v && !el.getAttribute('srcset')) { el.setAttribute('srcset', v); break; } }
          el.removeAttribute('loading');
          if (el.classList) el.classList.remove('lazy', 'lazyload', 'lazyloading');
        }
      }).catch(() => {});
      try { await page.waitForLoadState('networkidle', { timeout: 6000 }); } catch (_) {}
      await page.waitForTimeout(500);

      const html = await page.content();
      const links = await page.evaluate(() =>
        [...document.querySelectorAll('a[href]')].map(a => a.getAttribute('href')));
      // Top-nav items are frequently <button data-menu-trigger="men"> rather
      // than links. They are the most-clicked things on the page, so their
      // destinations must be captured or the primary nav is dead.
      const navTriggers = await page.evaluate(() =>
        [...document.querySelectorAll('[data-menu-trigger],[aria-controls]')]
          .map(e => e.getAttribute('data-menu-trigger') ||
                    (e.textContent || '').trim().toLowerCase())
          .filter(x => x && x.length < 24));
      // Scoped to header/nav CONTAINERS, not viewport visibility. Mega-menu
      // dropdown items (category links nested under a hover/click trigger)
      // are display:none until interaction, so a bounding-box filter (top
      // strip, width>0) sees only the trigger itself and never the real
      // destinations underneath it — those then never get captured and the
      // nav link is dead. Structural scoping catches them regardless of
      // whether the submenu happens to be open right now.
      const navLinks = await page.evaluate(() => {
        const containers = [...document.querySelectorAll('header, nav, [role="navigation"]')];
        const scope = containers.length ? containers : [document];
        const seen = new Set(), out = [];
        for (const c of scope) {
          for (const a of c.querySelectorAll('a[href]')) {
            const h = a.getAttribute('href');
            if (h && !seen.has(h)) { seen.add(h); out.push(h); }
            if (out.length >= 80) break;
          }
          if (out.length >= 80) break;
        }
        return out;
      });
      return { html, links, navTriggers, navLinks, finalUrl: page.url() };
    } catch (_) { return null; }
    finally { if (page) await page.close().catch(() => {}); }
  }

  try {
    // ---- 1. homepage, then discover pages to visit -------------------------
    const home = await grab(`https://${domain}/`);
    if (!home) { result.reason = 'homepage_failed'; throw new Error('home'); }

    const wanted = new Map();        // pathname -> {file, depth}
    wanted.set('/', { file: 'index.html', depth: 0 });

    const norm = (href) => {
      try {
        const u = new URL(href, `https://${domain}/`);
        if (u.hostname.replace(/^www\./, '') !== domain.replace(/^www\./, '')) return null;
        return u.pathname.replace(/\/+$/, '') || '/';
      } catch (_) { return null; }
    };

    // Sitemap discovery. Scraping <a href> misses stores whose nav is built in
    // JavaScript without real hrefs — one store had a 1MB homepage and zero
    // internal links. Every Shopify store publishes /sitemap.xml, which lists
    // the true catalog regardless of how the nav is rendered.
    // Sitemap XML escapes ampersands; an undecoded &amp; corrupts the query
    // string and the child sitemap 404s.
    const deent = (u) => u.replace(/&(?:amp|#38);/g, '&').trim();

    // all=false: a cheap supplement (first 6 child sitemaps) for the budgeted
    // path. all=true (--complete): every child sitemap, including the paginated
    // sitemap_products_N.xml set, so we enumerate the ENTIRE catalog.
    async function sitemapLinks(all = false) {
      const found = [];
      const seenMaps = new Set();
      const fetchXml = async (u) => {
        try {
          const r = await fetch(u, { headers: { 'User-Agent': UA },
                                     signal: AbortSignal.timeout(20000) });
          return r.ok ? await r.text() : null;
        } catch (_) { return null; }
      };
      const root = await fetchXml(`https://${domain}/sitemap.xml`);
      if (!root) return found;
      const locs = [...root.matchAll(/<loc>([^<]+)<\/loc>/g)].map(m => deent(m[1]));
      // top level lists child sitemaps (products, collections, pages, blogs)
      const childMaps = locs.filter(u => /sitemap[^"]*\.xml/i.test(u));
      const targets = all ? childMaps : childMaps.slice(0, 6);
      for (const c of targets) {
        if (seenMaps.has(c)) continue;
        seenMaps.add(c);
        const xml = await fetchXml(c);
        if (!xml) continue;
        for (const m of xml.matchAll(/<loc>([^<]+)<\/loc>/g)) {
          const u = deent(m[1]);
          // a child sitemap can itself index further sitemaps (deep pagination)
          if (all && /sitemap[^"]*\.xml/i.test(u) && !seenMaps.has(u)) targets.push(u);
          else found.push(u);
        }
        await new Promise(r => setTimeout(r, all ? 150 : 300));
      }
      if (!targets.length) found.push(...locs);
      return found;
    }

    const cols = [], prods = [], infoPages = [];
    for (const h of home.links) {
      const p = norm(h);
      if (!p) continue;
      const pf = pageFileFor(p);
      if (!pf || wanted.has(p)) continue;
      if (/^\/collections\/[^/]+$/.test(p) && !cols.includes(p)) cols.push(p);
      else if (/\/products\//.test(p) && !prods.includes(p)) prods.push(p);
      // /pages/* are the header nav items (Delivery, Contact, About). Missing
      // them is very visible: the top nav is the first thing anyone clicks, and
      // an uncaptured link bounces them to the live store mid-demo.
      else if (/^\/pages\//.test(p) && !infoPages.includes(p)) infoPages.push(p);
    }
    // Prefer breadth of what the homepage actually links to. Every uncaptured
    // link falls back to the live site, which breaks the illusion the moment
    // someone clicks it — so capture as much of the visible nav as the budget
    // allows, collections first (they carry the most onward links).
    // Nav-trigger destinations first. For data-menu-trigger="men" try
    // /collections/men and /collections/mens — whichever exists is the page the
    // header button should reach.
    // Every top-nav destination must be captured — these are the most-clicked
    // elements on the page, and a dead one is immediately visible. They get a
    // GUARANTEED allocation outside the normal budget split; previously they
    // competed with collections/products and simply displaced them.
    const navMust = [];
    for (const t of (home.navTriggers || [])) {
      const base = String(t).toLowerCase().replace(/[^a-z0-9-]/g, '');
      if (!base || base.length < 2) continue;
      for (const cand of [`/collections/${base}`, `/collections/${base}s`,
                          `/collections/${base}-mens`, `/collections/${base}-womens`]) {
        if (!navMust.includes(cand)) navMust.push(cand);
      }
    }
    // header anchors that already point at real pages
    for (const h of (home.navLinks || [])) {
      const q = norm(h);
      if (q && pageFileFor(q) && !navMust.includes(q)) navMust.push(q);
    }
    if (navMust.length) console.error(`[crawl] nav destinations (guaranteed): ${navMust.length}`);

    // Supplement anchor discovery with the sitemap. Cheap (a few requests) and
    // it is the only reliable source when the nav is JS-driven.
    if (cols.length + prods.length + infoPages.length < 8) {
      const sm = await sitemapLinks();
      let added = 0;
      for (const u of sm) {
        const p = norm(u);
        if (!p) continue;
        const f = pageFileFor(p);
        if (!f) continue;
        if (/^\/collections\/[^/]+$/.test(p) && !cols.includes(p)) { cols.push(p); added++; }
        else if (/\/products\//.test(p) && !prods.includes(p)) { prods.push(p); added++; }
        else if (!infoPages.includes(p) && !cols.includes(p) && !prods.includes(p)) { infoPages.push(p); added++; }
      }
      // A sitemap is mostly editorial and account plumbing. Rank by how
      // shop-like a path looks so the page budget buys product and category
      // pages, not /author/abby-wilson and /campaign/catalog-unsubscribe.
      const NOISE = /(article|blog|news|journal|stories|author|campaign|tag|legal|policy|policies|terms|privacy|unsubscribe|careers|faq|help|support|gift-?card|sitemap)/i;
      const SHOPPY = /(product|collection|shop|store|catalog|category|all-)/i;
      const score = (u) => {
        const segs = u.split('/').filter(Boolean);
        let sc = 0;
        if (SHOPPY.test(u)) sc -= 10;          // lower is better
        if (NOISE.test(u)) sc += 10;
        sc += segs.length;                     // shallower paths are more central
        return sc;
      };
      infoPages.sort((a, b) => score(a) - score(b));
      if (added) console.error(`[crawl] sitemap added ${added} URLs ` +
                               `(homepage anchors were sparse)`);
    }

    // Header nav pages first — they are always visible and always clicked.
    // Every take must be clamped to >= 0. A negative end index in slice()
    // silently returns almost the WHOLE array — that queued 78 pages against a
    // budget of 4 on one store, which then rate-limited us and failed every
    // subsequent fetch.
    // Nav first, then split what's left. MAX_PAGES is raised by the nav count
    // so guaranteeing the nav never eats the product budget.
    const navTake = navMust.slice(0, NAV_MUST_CAP);
    for (const q of navTake) if (!wanted.has(q)) wanted.set(q, pageFileFor(q));
    const budget = Math.max(0, MAX_PAGES - 1);
    // Products first — they ARE the store. The old order (up to 5 info pages,
    // then a 2-collection floor, then "the rest") allocated ZERO product pages
    // at any budget below 10: a store migration with no product pages. Give
    // the catalog at least half the budget up front, then collections (the
    // browse structure), then info pages (about/press) with whatever remains.
    let nProd = Math.min(prods.length,
      Math.max(Math.ceil(budget * 0.5), Math.min(prods.length, Math.min(budget, 3))));
    let afterProd = Math.max(0, budget - nProd);
    let nCol = Math.min(cols.length, afterProd,
      Math.max(cols.length > 0 && afterProd > 0 ? 1 : 0, Math.round(afterProd * 0.6)));
    let afterCol = Math.max(0, afterProd - nCol);
    let nInfo = Math.min(infoPages.length, afterCol);
    // Top-up: never leave budget unused when a bucket can still fill it.
    let left = budget - (nProd + nCol + nInfo);
    if (left > 0) { const t = Math.min(left, prods.length - nProd); nProd += t; left -= t; }
    if (left > 0) { const t = Math.min(left, cols.length - nCol);  nCol  += t; left -= t; }
    if (left > 0) { const t = Math.min(left, infoPages.length - nInfo); nInfo += t; left -= t; }
    if (COMPLETE) {
      // Photographic mode: enumerate EVERY URL the sitemap lists and queue all
      // of them — no budget, no sampling. This is what makes the clone a true
      // reproduction rather than a representative slice.
      const all = await sitemapLinks(true);
      let added = 0;
      for (const u of all) {
        const p = norm(u);
        if (!p) continue;
        const f = pageFileFor(p);
        if (!f) continue;                       // real product/collection/page/blog only
        if (!wanted.has(p)) { wanted.set(p, f); added++; }
      }
      // fold in anything anchors/nav already surfaced that the sitemap missed
      for (const p of [...cols, ...prods, ...infoPages, ...navTake]) {
        const f = pageFileFor(p);
        if (f && !wanted.has(p)) wanted.set(p, f);
      }
      console.error(`[crawl] COMPLETE: sitemap enumerated ${added} URLs — ` +
                    `capturing ${wanted.size} pages (every public page)`);
    } else {
      const pick = [...prods.slice(0, nProd),
                    ...cols.slice(0, nCol),
                    ...infoPages.slice(0, nInfo)];
      for (const p of pick) wanted.set(p, pageFileFor(p));
      console.error(`[crawl] discovered ${cols.length} collections, ${prods.length} products; ` +
                    `capturing ${wanted.size} pages`);
    }
    if (ONLY) {
      // Targeted mode replaces the discovered set wholesale. '/' stays (the
      // rewrite pass keys off it) but is skipped at write-out.
      for (const k of [...wanted.keys()]) if (k !== '/') wanted.delete(k);
      const unmappable = [];
      for (const p of ONLY) {
        const f = pageFileFor(p);
        if (f) wanted.set(p, f); else unmappable.push(p);
      }
      console.error(`[crawl] ONLY: capturing ${wanted.size - 1} named page(s) on top of ` +
        `${ONLY_ON_DISK.size} already on disk` +
        (unmappable.length ? ` — ${unmappable.length} never captured by design (${unmappable.join(', ')})` : ''));
    }

    // ---- 2. capture each page, discovering as we go ------------------------
    // The homepage links only a handful of products. Collection pages list the
    // whole catalog, so harvest their links too — that's what makes a complete
    // clone of a mid-market store possible instead of a 20-page sample.
    const captured = new Map();      // pathname -> html
    captured.set('/', home.html);

    // Resumability (--complete only): a full-store crawl runs for hours, so we
    // checkpoint each page's RAW html under <out>/.raw/ as it's captured. On a
    // re-run of the same command, we reload those and skip re-fetching — an
    // interrupted crawl picks up where it left off instead of starting over.
    // Gated to complete mode so normal captures are byte-for-byte unchanged.
    const rawDir = path.join(outdir, '.raw');
    const rawPath = (p) => { const f = (wanted.get(p) || pageFileFor(p)); return f ? path.join(rawDir, f.file) : null; };
    if (COMPLETE) {
      fs.mkdirSync(rawDir, { recursive: true });
      let resumed = 0;
      for (const [p, f] of wanted) {
        if (p === '/' || !f) continue;
        const rf = path.join(rawDir, f.file);
        if (fs.existsSync(rf)) { try { captured.set(p, fs.readFileSync(rf, 'utf8')); resumed++; } catch (_) {} }
      }
      if (resumed) console.error(`[crawl] resume: ${resumed} pages already captured, ${wanted.size - resumed} to go`);
    }

    // Dedupe by OUTPUT FILE, not path: Shopify serves the same product at both
    // /products/x and /collections/y/products/x. Path-keyed dedup captures it
    // twice, burning half the page budget on duplicates.
    const queue = [...wanted.keys()].filter(p => p !== '/' && !captured.has(p));
    const seenFiles = new Set(['index.html']);
    for (const p of [...queue, ...captured.keys()]) { const f = pageFileFor(p); if (f) seenFiles.add(f.file); }
    let visited = 0;

    // Be a polite client. Back-to-back page loads look like an attack and get
    // you rate-limited — which is both rude and self-defeating, since a
    // throttled store then fails every remaining fetch. A short gap between
    // pages, and a hard stop once failures cluster, keeps us welcome.
    const PAGE_DELAY_MS = 900;
    let consecutiveFails = 0;
    const retries = new Map();   // path -> attempts so far
    // Patient cooldown schedule: most storefront rate-limiting is a SLIDING
    // WINDOW (N requests / minute) that resets after a quiet period, not a
    // permanent block. So when short exponential backoff (below) stops being
    // enough, we don't quit — we wait out the window with an escalating
    // cooldown and RESUME. This is politeness taken to its limit (wait longer),
    // never evasion (same UA, same IP, no header games). We stop only when a
    // full cooldown buys ZERO new pages (a hard block, not a window) or the
    // schedule is exhausted — so a genuinely dead store still terminates.
    const COOLDOWNS_MS = [60000, 150000, 300000, 600000];
    let longCooldowns = 0;
    let sizeAtLastCooldown = 0;

    const HARD_CAP = MAX_PAGES + navMust.slice(0, NAV_MUST_CAP).length;
    // Capture pages. NORMAL mode runs one page at a time (CONC=1 below — the
    // exact old sequential loop, so every prior validation still holds).
    // --complete mode runs a BOUNDED worker pool: almost all of a page's ~30s
    // is spent WAITING on networkidle timeouts at ~0% CPU (heavy SPAs never go
    // network-idle — analytics beacons keep the connection count above zero —
    // so those waits run to their full timeout even though the page rendered
    // seconds ago). Running several pages at once is therefore nearly free and
    // cuts a full-store capture severalfold. The bound is a hard SAFETY CEILING
    // (never the runaway-Chrome pileup that a fan-out of full processes causes),
    // NOT a throttle — it is strictly faster than sequential. Concurrency runs
    // whole pages in parallel and changes NO per-page capture wait, so fidelity
    // is identical to sequential (no under-capture risk). FL_CONCURRENCY
    // overrides the default for beefier machines / more tolerant stores.
    // Normal mode ran ONE page at a time purely so earlier validations stayed
    // comparable — the pool itself changes no per-page wait, so fidelity is
    // identical. Measured cost of sequential: ~38s/page, almost all of it idle
    // in networkidle ceilings that ad-heavy stores always run to the limit.
    // Three workers is the conservative default (a modest load on someone's
    // production store); --complete keeps five. FL_CONCURRENCY overrides both.
    const CONC = COMPLETE ? Math.max(1, Number(process.env.FL_CONCURRENCY) || 5)
                          : Math.max(1, Number(process.env.FL_CONCURRENCY) || 3);
    console.error(`[crawl] workers: ${CONC} (FL_CONCURRENCY)`);
    let coolingDown = null;   // a promise while a patient cooldown is in progress
    let stopAll = false;      // hard block / cooldowns exhausted: drain everyone

    async function handlePage(p) {
      if (visited > 0) {
        // Slow down further the more failures we're seeing in a row — a burst
        // is usually a short-lived rate-limit window, not a permanently dead
        // store, and hammering it every 900ms just extends the block. This is
        // politeness (waiting longer), not evasion (no UA/IP rotation, no
        // header spoofing) — capped so a truly dead store still gives up.
        const delay = consecutiveFails > 0
          ? Math.min(12000, PAGE_DELAY_MS * Math.pow(2, consecutiveFails))
          : PAGE_DELAY_MS;
        await new Promise(r => setTimeout(r, delay));
      }
      const g = await grab(`https://${domain}${p}`);
      visited++;
      if (g && g.notFound) return;                 // guessed URL, no such page
      if (!g) {
        // If the failure was the renderer crashing, relaunch before the next
        // attempt so the requeued page retries on a live browser instead of
        // failing every remaining fetch against a dead one.
        await ensureBrowser();
        const attempts = (retries.get(p) || 0) + 1;
        retries.set(p, attempts);
        // Requeue for a later attempt — a fetch failure is usually the rate
        // limit noted above, not a dead page. Allow several, since a cooldown
        // below may rescue the page after the window resets.
        if (attempts <= 4) queue.push(p);
        console.error(`[crawl]   ✗ ${p}${attempts > 1 ? ` (attempt ${attempts})` : ''}`);
        // A cluster of failures through the short exponential backoff means the
        // window is real. Instead of quitting, wait it out with an escalating
        // cooldown that ALL workers pause on, and resume — as long as we keep
        // making progress across cooldowns. Only one worker runs the cooldown
        // at a time (guarded by coolingDown). Give up only when a full cooldown
        // captured nothing new (hard block) or the schedule is spent.
        if (++consecutiveFails >= 6 && !coolingDown) {
          const madeProgress = captured.size > sizeAtLastCooldown;
          if (longCooldowns > 0 && !madeProgress) {
            console.error('[crawl] stopping: a full cooldown captured nothing new — ' +
                          'this is a hard block, not a rate-limit window. Keeping what we have.');
            result.throttled = true; stopAll = true; return;
          }
          if (longCooldowns >= COOLDOWNS_MS.length) {
            console.error('[crawl] stopping: exhausted patient cooldowns — the store keeps ' +
                          'rate-limiting through long waits. Keeping what we have.');
            result.throttled = true; stopAll = true; return;
          }
          const cd = COOLDOWNS_MS[longCooldowns++];
          sizeAtLastCooldown = captured.size;
          console.error(`[crawl] rate-limited at ${captured.size} pages — cooling down ` +
                        `${Math.round(cd / 1000)}s to outlast the window, then resuming...`);
          let release;
          coolingDown = new Promise(r => (release = r));
          await new Promise(r => setTimeout(r, cd));
          consecutiveFails = 0;
          coolingDown = null;
          release();
        }
        return;
      }
      consecutiveFails = 0;
      captured.set(p, g.html);
      if (!wanted.has(p)) wanted.set(p, pageFileFor(p));
      // Checkpoint the raw page immediately (--complete) so an interrupted
      // multi-hour crawl can resume without re-fetching what it already has.
      if (COMPLETE) { const rf = rawPath(p); if (rf) { try { fs.mkdirSync(path.dirname(rf), { recursive: true }); fs.writeFileSync(rf, g.html); } catch (_) {} } }
      console.error(`[crawl]   ✓ ${p}`);

      // harvest onward product links from collection pages
      if (/^\/collections\//.test(p) && captured.size < MAX_PAGES && !ONLY) {
        let added = 0;
        for (const h of g.links) {
          if (captured.size + queue.length >= HARD_CAP) break;
          const q = norm(h);
          if (!q) continue;
          if (!/\/products\//.test(q)) continue;
          const f = pageFileFor(q);
          if (!f || seenFiles.has(f.file)) continue;
          seenFiles.add(f.file); queue.push(q); added++;
        }
        if (added) console.error(`[crawl]     +${added} products from this collection`);
      }
    }

    // Bounded worker pool. CONC=1 (normal mode) is exactly the old sequential
    // loop. Workers share the queue; a worker exits only when the queue is
    // empty AND no other worker is still in-flight — an in-flight collection
    // page may still enqueue freshly-harvested product links, so a worker that
    // finds the queue momentarily empty must wait for peers before deciding
    // the crawl is done.
    let inFlight = 0;
    const worker = async () => {
      while (!stopAll && captured.size < HARD_CAP) {
        if (coolingDown) { await coolingDown; continue; }
        const p = queue.shift();
        if (p === undefined) {
          if (inFlight === 0) break;
          await new Promise(r => setTimeout(r, 200));
          continue;
        }
        inFlight++;
        try { await handlePage(p); }
        catch (e) { console.error(`[crawl]   ✗ ${p} (${(e && e.message) || e})`); }
        inFlight--;
      }
    };
    await Promise.all(Array.from({ length: CONC }, () => worker()));

    await Promise.race([Promise.allSettled(pending),
                        new Promise(r => setTimeout(r, 30000))]);
    result.assets = assetMap.size;
    result.bytes = totalBytes;

    // recover assets whose body never arrived. Dedupe by rel first — many
    // different abs urls (srcset width/version variants) now share one rel
    // (see localFor), so without this a single missing image could burn
    // several slots of the 600-item budget below retrying the same file.
    const missedByRel = new Map();
    for (const [abs, rel] of assetMap) {
      if (!fs.existsSync(path.join(outdir, 'site', rel)) && !missedByRel.has(rel)) {
        missedByRel.set(rel, abs);
      }
    }
    const missed = [...missedByRel].map(([rel, abs]) => [abs, rel]);
    await Promise.allSettled(missed.slice(0, 600).map(async ([abs, rel]) => {
      try {
        const r = await fetch(abs, { headers: { 'User-Agent': UA },
                                     signal: AbortSignal.timeout(9000) });
        if (!r.ok) return;
        const buf = Buffer.from(await r.arrayBuffer());
        if (!buf.length) return;
        const dst = path.join(outdir, 'site', rel);
        fs.mkdirSync(path.dirname(dst), { recursive: true });
        fs.writeFileSync(dst, buf);
      } catch (_) {}
    }));

    // ---- 3. rewrite each page ---------------------------------------------
    // Drop any asset whose body never made it to disk. Pointing at a local
    // path with no file behind it yields a guaranteed broken image; leaving the
    // URL alone lets it load from the origin CDN instead.
    for (const [abs, rel] of [...assetMap]) {
      if (!fs.existsSync(path.join(outdir, 'site', rel))) assetMap.delete(abs);
    }

    const byPath = new Map();
    const byFull = new Map();   // pathname+query -> local; distinguishes proxy URLs
    for (const [abs, rel] of assetMap) {
      try {
        const u = new URL(abs);
        if (!byPath.has(u.pathname)) byPath.set(u.pathname, rel);
        byFull.set(u.pathname + u.search, rel);
      } catch (_) {}
    }
    // Split captured assets into direct (non-proxy) images vs everything
    // else. Images get a query-AWARE rewrite below (imgRewrites): the base
    // url is matched with an OPTIONAL trailing query and the whole thing —
    // base plus whatever query text actually follows it in this page's
    // markup — is replaced with the clean local path. A literal
    // html.split(exactCapturedUrl).join(...) can't do this safely for
    // images: if the exact query string this particular abs was captured
    // with (e.g. "?v=123", no width) happens to be a PREFIX of a longer
    // query on some other occurrence of the same base image in the page
    // (e.g. "?v=123&width=432" in a srcset candidate), the literal replace
    // only consumes the matched prefix and leaves the rest — "&width=432"
    // — dangling on the rewritten local path, pointing at a file that
    // doesn't exist under that name. Non-image assets (css/js/fonts) and
    // proxy image urls (/_next/image?url=…, /cdn-cgi/image/…, where the
    // query IS the identity, not a resize hint) keep the exact literal
    // match — see localFor / isProxyImageUrl above for why those two cases
    // are handled differently.
    const escapeRe = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const imgBaseRel = new Map();     // normalized (query-stripped) image url -> rel
    const otherAssets = [];
    for (const [abs, rel] of assetMap) {
      let u;
      try { u = new URL(abs); } catch (_) { otherAssets.push([abs, rel]); continue; }
      if (isImageExt(u.pathname) && !isProxyImageUrl(u)) {
        const base = normalizeImageUrl(abs);
        if (!imgBaseRel.has(base)) imgBaseRel.set(base, rel);
      } else {
        otherAssets.push([abs, rel]);
      }
    }
    const sortedAssets = otherAssets.sort((a, b) => b[0].length - a[0].length);
    const imgRewrites = [...imgBaseRel.entries()]
      .sort((a, b) => b[0].length - a[0].length)
      .map(([base, rel]) => {
        const noProto = base.replace(/^https?:/, '');
        const pattern = noProto !== base
          ? `(?:${escapeRe(base)}|${escapeRe(noProto)})`
          : escapeRe(base);
        // trailing query, if this occurrence in the page's html has one —
        // matched permissively (it may still carry HTML-entity-encoded
        // "&amp;" from page.content()'s serialization) and discarded whole.
        // …but never through an HTML-escaped quote: inside a style attribute
        // the URL sits in url(&quot;…&quot;), and eating the closing &quot;
        // left url(&quot;_a/img/x.jpg); — an unterminated CSS string whose
        // request then carried ");" (seen on olipop and kotn).
        return { re: new RegExp(pattern + '(?:\\?(?:(?!&quot;|&#39;|&#34;|&apos;)[^\\s"\'()<>])*)?', 'g'), rel };
      });

    // Rewrite url() inside captured CSS. Refs there resolve relative to the
    // STYLESHEET, not the page, so one rewrite works from every page depth.
    // Without this, @font-face src="Foo.woff2" dangles and text silently falls
    // back to Times — no console error, just the wrong typeface everywhere.
    for (const [abs, rel] of assetMap) {
      if (!rel.endsWith('.css')) continue;
      const p = path.join(outdir, 'site', rel);
      if (!fs.existsSync(p)) continue;
      let css = fs.readFileSync(p, 'utf8');
      css = css.replace(/url\(\s*(['"]?)([^'")]+)\1\s*\)/g, (m, q, ref) => {
        if (/^(data:|#)/.test(ref)) return m;
        let target;
        try { target = new URL(ref, abs).toString(); } catch (_) { return m; }
        let loc = assetMap.get(target);
        if (!loc) { try { loc = byPath.get(new URL(target).pathname); } catch (_) {} }
        return loc ? `url("../${loc.slice(3)}")` : m;   // _a/x/y -> ../x/y
      });
      fs.writeFileSync(p, css);
    }

    // every page file we actually wrote, for link matching
    const capturedFiles = new Set();
    for (const pth of captured.keys()) {
      const m = wanted.get(pth) || pageFileFor(pth);
      if (m) capturedFiles.add(m.file);
    }
    // Targeted mode: the new pages link to products and collections captured
    // by the EARLIER run. Those are on disk, so they are real destinations —
    // without this every such link would be marked uncaptured and made inert.
    let onlyPrevIndex = [];
    if (ONLY) {
      for (const f of ONLY_ON_DISK) capturedFiles.add(f);
      try { onlyPrevIndex = JSON.parse(fs.readFileSync(path.join(outdir, 'pages.json'), 'utf8')); } catch (_) {}
    }

    // Nav-menu destinations (see navMust above) that the page budget still
    // didn't reach — a real category the crawl ran out of room for, or a
    // guessed candidate (/collections/men vs /collections/mens) that never
    // resolved. These are the most-clicked links on the page; never leave
    // one pointing at a missing local file or eject the viewer to the live
    // site. Land on the nearest captured collection instead — same "browse
    // the catalog" intent, just not the exact category — or the homepage
    // if nothing was captured at all.
    const navPathSet = new Set(navMust);
    const navFallback = null;   // see the note at the href rewrite: no silent redirects

    for (const [pth, rawHtml] of captured) {
      if (ONLY && !ONLY.has(pth)) continue;      // targeted: write only the named pages ('/' too, when named)
      const meta = wanted.get(pth) || { file: 'index.html', depth: 0 };
      const up = '../'.repeat(meta.depth);
      let html = rawHtml;


      // direct CDN images first (query-aware — see imgRewrites above),
      // THEN everything else: exact URL, then protocol-relative fallback.
      for (const { re, rel } of imgRewrites) {
        html = html.replace(re, up + rel);
      }
      for (const [abs, rel] of sortedAssets) {
        const noProto = abs.replace(/^https?:/, '');
        html = html.split(abs).join(up + rel).split(noProto).join(up + rel);
      }
      html = html.replace(/((?:src|href|data-src|poster)=")(\/\/[^"]+)"/gi, (m, pre, u) => {
        let p = null; try { p = new URL('https:' + u).pathname; } catch (_) {}
        const loc = p && byPath.get(p);
        return loc ? `${pre}${up}${loc}"` : `${pre}https:${u}"`;
      });

      // ---- inter-page links: THE thing that makes it browsable ----
      html = replaceOutsideScripts(html, /href="([^"]+)"/gi, (m, href) => {
        if (/^(#|mailto:|tel:|javascript:|data:)/i.test(href)) return m;
        if (href.startsWith(up + '_a/') || href.startsWith('_a/')) return m;
        const p = norm(href);
        if (!p) return m;                                   // off-site: leave alone
        // An href can point at an ASSET (stylesheet, preload) rather than a
        // page. Those must map to the local copy — absolutising them to the
        // live origin loses the CSS we already captured, and the clone renders
        // in Times at several thousand px wide.
        // Match on the OUTPUT FILE, not the URL path. Shopify exposes the same
        // product at both /products/x and /collections/y/products/x; those are
        // different paths that resolve to one captured file, so a path-keyed
        // lookup misses half the links and sends them to the live store.
        // A captured PAGE wins over an asset recorded at the same path: the
        // homepage's own HTML is also in the asset map under "/", and the
        // logo's href="/" was being rewritten to _a/other/index.<hash>.bin.
        const tgt = pageFileFor(p);
        if (tgt && capturedFiles.has(tgt.file)) {
          return `href="${up}${tgt.file}"`;                 // -> local copy
        }
        const asset = byPath.get(p);
        if (asset && !/\/_a\/other\/index\.[0-9a-f]{10}\.bin$/.test('/' + asset)) return `href="${up}${asset}"`;
        // A nav-menu destination the budget didn't reach used to be sent to
        // the nearest captured collection. That hid the gap from the gate (the
        // link exists, it just goes to the wrong page) and threw away the real
        // destination, so the targeted recapture could never repair it. It now
        // falls through to the uncaptured marker below: inert in demo mode,
        // honest, and the gate names it as a dead destination to re-crawl.
        // A storefront page we didn't capture. Outside demo mode, send it to
        // the live site so a click never 404s. In demo mode that is wrong —
        // clicking a link must never eject the viewer into the real store
        // mid-presentation — so mark it inert instead.
        return CLEAN
          ? `href="${p}" data-mirror-uncaptured="${p}"`
          : `href="https://${domain}${p}"`;
      });

      // Product-card click nav fix (demo mode only): many themes (Taylor
      // Stitch especially) rewrite their own product/collection card hrefs
      // to "#" at RUNTIME for client-side routing — offline, that router
      // can't resolve anything, so the click dead-ends even though the href
      // above was already rewritten to a perfectly good local .html file.
      // Stamp the (already-rewritten, so it's the LOCAL destination) href
      // onto a data-fl-href attribute the theme's JS has no reason to ever
      // touch, so a delegated click handler (see navFlHrefShim below) can
      // always recover the real destination regardless of what the theme
      // does to the href attribute itself. Scoped to anchors that resolved
      // to a captured .html file, i.e. exactly the links that are actually
      // clickable in the clone.
      if (CLEAN) {
        html = html.replace(/<a\b([^>]*?)\shref="([^"]*\.html(?:[?#][^"]*)?)"([^>]*)>/gi,
          (m, pre, href, post) => {
            if (/\bdata-fl-href=/.test(pre) || /\bdata-fl-href=/.test(post)) return m;
            return `<a${pre} href="${href}"${post} data-fl-href="${href}">`;
          });
      }

      // responsive-image pathname fallback
      html = html.replace(/(https?:)?\/\/[a-z0-9.-]+\/(?:cdn|s\/files)\/[^\s"')]+/gi, (m) => {
        try {
          const p = new URL(m.startsWith('//') ? 'https:' + m : m).pathname;
          const loc = byPath.get(p);
          return loc ? up + loc : m;
        } catch (_) { return m; }
      });

      // uncaptured same-origin assets -> live origin (never break)
      html = html.replace(/((?:src|poster)=")(\/[^\/"][^"]*)"/gi, (m, pre, p) => {
        // Query-aware first. Image proxies (/_next/image?url=…&w=640) share ONE
        // pathname across every image on the page, so a pathname-only lookup
        // maps them all to whichever was captured first and the rest 404.
        const full = byFull.get(p);
        if (full) return `${pre}${up}${full}"`;
        const c = p.split('?')[0];
        const bare = byPath.get(c);
        if (bare) {
          // A direct (non-proxy) asset with a query still attached — that
          // query is just a resize/version hint (see normalizeImageUrl
          // above), safe to drop once the base pathname resolves. Proxy
          // pathnames genuinely need the query as part of the asset's
          // identity (byFull, above, is what resolves those), so leave
          // this fallback query-blind ONLY for them.
          const proxyLike = p.includes('?') &&
            (/(^|\/)(_next\/image|cdn-cgi\/image|_image)(\/|$)/.test(c) || /[?&]url=/.test(p));
          if (!proxyLike) return `${pre}${up}${bare}"`;
        }
        // Uncaptured proxy URL: unwrap it to the origin image it wraps, which
        // is a real CDN asset that loads without the proxy behind it.
        const inner = /[?&]url=([^&]+)/.exec(p);
        if (inner) {
          try {
            const dec = decodeURIComponent(inner[1]);
            if (/^https?:\/\//.test(dec)) {
              const loc = byFull.get(new URL(dec).pathname + new URL(dec).search) ||
                          byPath.get(new URL(dec).pathname);
              if (loc) return `${pre}${up}${loc}"`;
              // Not captured. Send it to the LIVE origin's own image proxy
              // rather than the underlying CDN — some CDNs (Sanity, Contentful)
              // refuse direct loads, and the proxy is what the real site uses.
              return `${pre}https://${domain}${p}"`;
            }
          } catch (_) {}
        }
        return `${pre}https://${domain}${p}"`;
      });
      html = html.replace(/<link\b[^>]*>/gi, (tag) => {
        if (!/rel\s*=\s*["']?stylesheet/i.test(tag)) return tag;
        return tag.replace(/href="(\/[^\/"][^"]*)"/i, (m, p) =>
          byPath.has(p.split('?')[0]) ? m : `href="https://${domain}${p}"`);
      });

      html = html.replace(/\sintegrity="[^"]*"/g, '').replace(/\scrossorigin="[^"]*"/g, '');
      html = html.replace(/<base[^>]*>/gi, '');

      const consentCss = '<style id="_mirror-consent">' +
        '[id*="Cybot"],[class*="Cybot"],[id*="onetrust"],[class*="onetrust"],' +
        '[id*="ot-sdk"],[class*="ot-sdk"],[id*="truste"],[class*="truste"],' +
        '[id*="klaro"],[class*="klaro"],[id*="osano"],[class*="osano"],' +
        '[id*="didomi"],[class*="didomi"],[id*="usercentrics"],[class*="usercentrics"],' +
        '[class*="cookie-consent"],[id*="cookie-consent"],[class*="CookieConsent"]' +
        '{display:none !important}</style>';
      html = html.replace(/<\/head>/i, consentCss + '</head>');

      // Storefronts route clicks in JavaScript (SPA / Hydrogen / prefetch
      // handlers). Offline the router can't resolve anything, so it swallows
      // the click and nothing happens. A capture-phase listener on window runs
      // before those bubble-phase handlers, so local page links do a real
      // navigation. Scoped to .html targets only — off-site links behave normally.
      const navShim = '<script>(function(){window.addEventListener("click",function(e){' +
        'var a=e.target&&e.target.closest?e.target.closest("a[href]"):null;if(!a)return;' +
        'var h=a.getAttribute("href")||"";' +
        'if(!/\\.html($|[?#])/.test(h))return;' +
        'if(/^(#|mailto:|tel:|javascript:)/i.test(h))return;' +
        'e.stopImmediatePropagation();e.preventDefault();window.location.href=a.href;' +
        '},true);})();</' + 'script>';
      html = injectBeforeBody(html, navShim);

      // Product-card click nav fix (demo mode only, see the data-fl-href
      // stamping pass above). navShim (just above) already recovers a click
      // whose CURRENT href attribute still says "*.html" — but themes like
      // Taylor Stitch rewrite their own product/collection card hrefs to
      // "#" at page-JS-init time, well before any click, specifically so
      // their own client-side router owns the navigation. By the time a
      // click happens the href is already mangled and navShim's own check
      // (which reads the live href) never matches. data-fl-href is a
      // custom attribute the theme's routing JS has no reason to read or
      // write, so it still holds the real local destination — this
      // listener reads THAT instead of the (possibly mangled) href.
      // Capture-phase on window, same as navShim, so it runs before the
      // theme's own bubble-phase card handler ever fires. Real form
      // controls/variant swatches nested inside a card (radio/checkbox/
      // select/button/swatch elements) are excluded so picking a color or
      // size still behaves normally instead of navigating away.
      if (CLEAN) {
        const flHrefShim = '<script>(function(){window.addEventListener("click",function(e){' +
          'var t=e.target;if(!t||!t.closest)return;' +
          'var host=t.closest("[data-fl-href]");if(!host)return;' +
          'var ctrl=t.closest&&t.closest(\'input,select,textarea,button,[role="radio"],' +
          '[type="radio"],[type="checkbox"],[class*="swatch" i],[class*="variant" i]\');' +
          'if(ctrl&&host.contains(ctrl)&&ctrl!==host)return;' +   // let real controls behave normally
          'var href=host.getAttribute("data-fl-href");if(!href)return;' +
          'e.stopImmediatePropagation();e.preventDefault();window.location.href=href;' +
          '},true);})();</' + 'script>';
        html = injectBeforeBody(html, flHrefShim);
      }

      // React/Next hydration re-renders anchors from its own props and throws
      // away the hrefs we rewrote at build time — the page looks right but
      // every link points back at the live store. Ship the path->file map into
      // the page and re-apply it after hydration, then keep applying it as the
      // app re-renders.
      const linkMap = {};
      for (const [pth] of captured) {
        const t = wanted.get(pth) || pageFileFor(pth);
        if (t) linkMap[pth] = t.file;
      }
      for (const x of onlyPrevIndex) if (x.route && x.file && !linkMap[x.route]) linkMap[x.route] = x.file;
      const relinkShim = '<script>(function(){var M=' + JSON.stringify(linkMap) +
        ',UP=' + JSON.stringify(up) + ',H=' + JSON.stringify(domain) +
        ',NAVSET=' + JSON.stringify([...navPathSet]) +
        ',FALLBACK=' + JSON.stringify(navFallback) + ';' +
        'function norm(u){try{var a=new URL(u,location.href);' +
        'if(a.hostname&&a.hostname.replace(/^www\\./,"")!==H.replace(/^www\\./,"")' +
        '&&a.protocol!=="file:"&&a.hostname!==location.hostname)return null;' +
        'return a.pathname.replace(/\\/+$/,"")||"/";}catch(e){return null}}' +
        'function fix(){var a=document.getElementsByTagName("a");' +
        'for(var i=0;i<a.length;i++){var e=a[i],h=e.getAttribute("href");' +
        'if(!h||/^(#|mailto:|tel:|javascript:)/i.test(h))continue;' +
        'if(/\\.html($|[?#])/.test(h))continue;' +
        'var p=norm(h);if(!p)continue;var f=M[p];' +
        // Hydration re-renders nav-menu links too, same as any other anchor.
        // If it's a known nav destination we still don't have a page for,
        // land on the nearest captured collection rather than leaving the
        // framework's own (live, off-origin) href in place.
        'if(!f&&FALLBACK&&NAVSET.indexOf(p)>=0)f=FALLBACK;' +
        'if(f)e.setAttribute("href",UP+f);}}' +
        'fix();if(document.readyState!=="complete")window.addEventListener("load",fix);' +
        'setTimeout(fix,600);setTimeout(fix,1800);setTimeout(fix,4000);' +
        'var t=null;new MutationObserver(function(){clearTimeout(t);t=setTimeout(fix,150);})' +
        '.observe(document.documentElement,{childList:true,subtree:true});' +
        '})();</' + 'script>';
      html = injectBeforeBody(html, relinkShim);

      // Dead-link click rescue (demo mode). A click on a content link whose
      // destination we didn't capture must never be a silent no-op. The
      // relink shim rewrites the href, but many themes re-mangle their own
      // product/collection card hrefs back to "#" at runtime for client-side
      // routing — so rewriting the attribute isn't enough. Intercept at CLICK
      // time (robust to any href re-rendering) and land on the nearest
      // captured page of the same kind: an uncaptured product -> a captured
      // product, anything else -> the nearest captured collection. Never a
      // dead click, never an ejection off-site. (For a real credentialed
      // migration the whole catalog is captured and this never fires.)
      if (CLEAN) {
        const capFiles = Object.values(linkMap);
        const prodFB = capFiles.find(f => /^products\//.test(f)) || '';
        const collFB = capFiles.find(f => /^collections\//.test(f)) || prodFB || 'index.html';
        const rescueShim = '<script>(function(){' +
          'var PRODFB=' + JSON.stringify(prodFB) + ',COLLFB=' + JSON.stringify(collFB) + ';' +
          'if(!PRODFB&&!COLLFB)return;' +
          'function up(){var d=location.pathname.split("/").length-2;return d>0?Array(d+1).join("../"):"";}' +
          'document.addEventListener("click",function(ev){' +
          'var e=ev.target&&ev.target.closest?ev.target.closest("a"):null;if(!e)return;' +
          // Never touch our own overlay chrome (ancestor match is correct here).
          'if(e.closest("#fl-drawer,#fl-co,#fl-cart-btn,#fl-add"))return;' +
          // Skip genuine icon CONTROLS (search/cart/account/hamburger toggles) by
          // inspecting the ANCHOR ITSELF only — matching a distant ancestor is
          // wrong: many themes wrap whole page regions in cart-state classes
          // (e.g. jonesroadbeauty.com nests content under `js-ajax-cart-empty`),
          // and a [class*=cart] closest() there would wrongly exclude real
          // category links. A megamenu PARENT like "Skin"/"Get The Look" is a
          // text link whose hover-submenu is dead offline, so a click must still
          // land somewhere rather than no-op.
          'var cc=((e.className||"")+" "+(e.getAttribute("aria-label")||""));' +
          'if(/swatch|quick-?(add|view|shop)|(^|[^a-z])search|hamburger|nav-?toggle|header__icon|mini-?cart|cart-(link|toggle|icon|button)|(^|[^a-z])account|wishlist|(^|[^a-z])log ?in/i.test(cc))return;' +
          'if(e.getAttribute("data-fl-href"))return;' +
          // Only rescue links that carry visible TEXT — an icon-only toggle
          // (no text) is a control, not a destination, and must be left alone.
          'var txt=(e.textContent||"").replace(/\\s+/g," ").trim();if(txt.length<2)return;' +
          'var h=e.getAttribute("href")||"",un=e.getAttribute("data-mirror-uncaptured")||"";' +
          'var voidH=(h==="#"||h===""||h.charAt(0)==="#"||/^javascript:/i.test(h));' +
          'if(!voidH&&!un)return;' +
          'var isP=/\\/products\\//.test(un)||/product/i.test(e.className||"")||(e.querySelector&&e.querySelector("[class*=price],[class*=money]"));' +
          'var t=isP?(PRODFB||COLLFB):(COLLFB||PRODFB);if(!t)return;' +
          'ev.preventDefault();ev.stopImmediatePropagation();window.location.href=up()+t;' +
          '},true);})();</' + 'script>';
        // Inject at document start (like the commerce overlay): a delegated
        // capture-phase click listener that must survive even when a captured
        // page's malformed inline <script> flips the parser into raw-text mode
        // and swallows every later tag (see injectAtDocStart). A before-</body>
        // rescue shim is silently dropped on exactly the stores that need it.
        html = injectAtDocStart(html, rescueShim);
      }

      // Commerce bridge: answer the storefront's cart/search/filter/sort API
      // calls so add-to-cart, filtering, sorting, and search actually work.
      // Points at the ForkLaunch module when --api is given (HMAC-signed with
      // --hmac-secret), otherwise runs a local cart in the browser and leaves
      // filter/sort/search controls exactly as captured.
      //
      // Uses injectBeforeBody (see its docstring) — the bridge's own source
      // (e.g. the literal '$' price prefix) can contain $&/$`/$'/$<digit>
      // sequences that would corrupt a string-form replace.
      html = injectBeforeBody(html, buildBridge({ apiBase: API_BASE, hmacSecret: HMAC_SECRET }));

      // Storefronts often put top-level nav on <button data-menu-trigger="men">
      // rather than a link — the button opens a mega-menu in JS. Offline that
      // handler frequently does nothing, so the most prominent nav item in the
      // header appears dead. If we captured a page that matches the trigger,
      // fall back to navigating there so the nav is usable.
      const triggerShim = '<script>(function(){var M=' + JSON.stringify(linkMap) +
        ',UP=' + JSON.stringify(up) + ';' +
        'var files=Object.keys(M).map(function(k){return {p:k,f:M[k]};});' +
        'function match(name){name=(name||"").toLowerCase().replace(/[^a-z0-9]/g,"");' +
        'if(!name)return null;var exact=null,partial=null;' +
        'files.forEach(function(x){var seg=x.f.replace(/\\.html$/,"").split("/").pop()' +
        '.toLowerCase().replace(/[^a-z0-9]/g,"");' +
        'if(seg===name||seg===name+"s"||seg===name.replace(/s$/,"")){exact=exact||x;}' +
        // Fall back to a partial match: a "Sale" trigger on a store with no
        // /collections/sale should still reach sale-mens or sale-womens.
        'else if(!partial&&seg.indexOf(name)===0){partial=x;}});' +
        'return exact||partial;}' +
        'document.addEventListener("click",function(e){' +
        // Also catch plain <button> elements with no href. Storefronts render
        // calls-to-action like "Shop Mens" / "Shop Women\'s Sale" as buttons
        // wired up in JS; offline they do nothing, and an audit found dozens
        // dead across collection pages. If the label resolves to a page we
        // captured, navigate there.
        'var t=e.target&&e.target.closest?e.target.closest("[data-menu-trigger],[aria-haspopup],button"):null;' +
        'if(!t)return; if(t.closest("a[href]"))return;' +
        'setTimeout(function(){' +
        // only act if the click produced no visible menu
        'var opened=document.querySelector("[aria-expanded=\'true\'],[data-menu-open],.menu-open");' +
        'if(opened)return;' +
        'var n=t.getAttribute("data-menu-trigger")||t.getAttribute("aria-controls")||t.textContent;' +
        'var m=match(n);' +
        // "Shop Mens" -> "mens"; "Shop Women\'s Sale" -> "womenssale" -> "sale-womens"
        'if(!m&&n){var strip=String(n).replace(/^\\s*shop\\s+/i,"");m=match(strip);}' +
        'if(!m&&n){var w=String(n).toLowerCase().replace(/[^a-z0-9 ]/g,"").split(/\\s+/)' +
        '.filter(function(x){return x&&x!=="shop";});' +
        'for(var k=w.length-1;k>=0&&!m;k--){m=match(w[k]);}}' +
        'if(m) window.location.href=UP+m.f;},260);' +
        '},true);})();</' + 'script>';
      html = injectBeforeBody(html, triggerShim);

      // Buy button "present but not clickable": third-party app scripts
      // (Swym wishlist, loyalty widgets, chat, etc.) are blocked offline, so
      // the transient state class they set on <html>/<body> while loading
      // (swym-loading, no-js, ...) is never cleared, and some drive an
      // invisible full-viewport pseudo-overlay that never gets torn down
      // either. Either one makes the browser's own hit-test resolve a click
      // at the CTA's center to <body> (or the leftover overlay) instead of
      // the button underneath — proven with Playwright's own interception
      // error, not a test artifact; a real mouse user would be blocked too.
      // Clear the stuck state, neutralize any leftover full-viewport
      // interceptor (never one with real visible content — only empty
      // chrome left behind by a script that stopped running), and — because
      // the theme's OWN add-to-cart JS is blocked offline too, so even a
      // click that lands cleanly may still do nothing — add a delegated,
      // fetch-based handler on common buy-button selectors so the click
      // adds to cart regardless of whether the theme's script ever ran.
      const interactionShim =
        '<style id="_mirror-clickable">html,body{pointer-events:auto !important}</style>' +
        '<script>(function(){' +
        'var PAT=/(^|\\s)([\\w-]*-loading|is-loading|js-loading|loading|no-js|' +
        'overflow-hidden|no-scroll|drawer-open|menu-open|nav-open|modal-open)(\\s|$)/gi;' +
        'function declass(){[document.documentElement,document.body].forEach(function(el){' +
        'if(!el||typeof el.className!=="string")return;' +
        'var c=el.className.replace(PAT," ").replace(/\\s+/g," ").trim();' +
        'if(c!==el.className)el.className=c;});}' +
        'function neutralizeOverlays(){if(!document.body)return;var vw=innerWidth,vh=innerHeight;' +
        'var all=document.body.querySelectorAll("*");' +
        'for(var i=0;i<all.length;i++){var n=all[i];var s=getComputedStyle(n);' +
        'if(s.position!=="fixed"&&s.position!=="sticky")continue;' +
        'if(s.pointerEvents==="none")continue;' +
        'var r=n.getBoundingClientRect();' +
        'var coversMost=r.width>=vw*0.8&&r.height>=vh*0.5&&r.top<=vh*0.2;' +
        'if(!coversMost)continue;' +
        'var hasContent=(n.textContent||"").trim().length>0||' +
        'n.querySelector("img,svg,video,button,input,a[href]");' +
        'if(hasContent)continue;' +
        'n.style.setProperty("pointer-events","none","important");}}' +
        'function fix(){declass();neutralizeOverlays();}' +
        'fix();document.addEventListener("DOMContentLoaded",fix);' +
        '[100,400,900,1800,3000].forEach(function(t){setTimeout(fix,t);});' +
        'var CTA=\'[name="add"],button[type="submit"],[class*="add-to-cart"],\'+' +
        '\'[class*="AddToCart"],[data-testid*="add"],[data-action="addCart"],\'+' +
        '\'[data-action="add-to-cart"],.js-product-cta,.product-form__submit,\'+' +
        '\'[data-add-to-cart]\';' +
        'function resolveVariantId(btn){' +
        'var checked=document.querySelector("input[name=\\"id\\"]:checked,select[name=\\"id\\"]");' +
        'if(checked&&checked.value&&/^\\d+$/.test(checked.value))return checked.value;' +
        'var dv=btn.getAttribute("data-variant-id")||' +
        '(btn.closest("[data-variant-id]")&&btn.closest("[data-variant-id]").getAttribute("data-variant-id"));' +
        'if(dv&&/^\\d+$/.test(dv))return dv;' +
        'var sw=document.querySelector("[data-variant-id].is-selected,[data-variant-id][aria-checked=\\"true\\"],[data-variant-id].active");' +
        'if(sw)return sw.getAttribute("data-variant-id");' +
        'var m=document.documentElement.innerHTML.match(/"variants":\\s*\\[\\s*\\{[^}]*?"id":\\s*(\\d+)/);' +
        'if(m)return m[1];' +
        'var any=document.querySelector("[data-variant-id]");' +
        'return any?any.getAttribute("data-variant-id"):null;}' +
        'document.addEventListener("click",function(e){' +
        'var btn=e.target&&e.target.closest?e.target.closest(CTA):null;if(!btn)return;' +
        'if(btn.closest("a[href]"))return;' +
        'var id=resolveVariantId(btn);if(!id)return;' +
        'fetch("/cart/add.js",{method:"POST",headers:{"Content-Type":"application/json"},' +
        'body:JSON.stringify({id:id,quantity:1})}).catch(function(){});' +
        '},true);' +
        '})();</' + 'script>';
      html = injectBeforeBody(html, interactionShim);

      // React sets image srcs at runtime, so static rewriting never sees them.
      // Framework image proxies (/_next/image?url=…, /cdn-cgi/image/…) have no
      // server behind them offline, so those images 404. Unwrap each proxy URL
      // to the origin CDN asset it wraps, which loads on its own.
      const imgShim = '<script>(function(){var H=' + JSON.stringify(domain) + ';' +
        'function unwrap(u){try{' +
        'var m=/[?&]url=([^&]+)/.exec(u);if(!m)return null;' +
        'var d=decodeURIComponent(m[1]);' +
        'return /^https?:\\/\\//.test(d)?d:null;}catch(e){return null}}' +
        'function fix(){var im=document.getElementsByTagName("img");' +
        'for(var i=0;i<im.length;i++){var e=im[i],s=e.getAttribute("src")||"";' +
        'if(!/(_next\\/image|\\/cdn-cgi\\/image|\\/_image)/.test(s))continue;' +
        // Prefer the LIVE origin's own proxy: it is exactly what the real site
        // serves, so it always works. Unwrapping to the underlying CDN is the
        // fallback, and some CDNs (Sanity) refuse direct loads.
        'if(/^https?:/.test(s)&&s.indexOf(location.origin)!==0){}' +
        'else{var rel=s.replace(location.origin,"");' +
        'if(rel.charAt(0)!=="/")rel="/"+rel.replace(/^\\.\\//,"");' +
        'e.setAttribute("src","https://"+H+rel);continue;}' +
        'var t=unwrap(s);if(t&&t!==s)e.setAttribute("src",t);' +
        'var ss=e.getAttribute("srcset");' +
        'if(ss&&/(_next\\/image|\\/cdn-cgi\\/image)/.test(ss)){' +
        'e.setAttribute("srcset",ss.split(",").map(function(p){' +
        'var b=p.trim().split(/\\s+/);var v=unwrap(b[0]);if(v)b[0]=v;' +
        'return b.join(" ");}).join(", "));}}}' +
        'fix();if(document.readyState!=="complete")window.addEventListener("load",fix);' +
        'setTimeout(fix,700);setTimeout(fix,2000);setTimeout(fix,4500);' +
        'var t=null;new MutationObserver(function(){clearTimeout(t);t=setTimeout(fix,200);})' +
        '.observe(document.documentElement,{childList:true,subtree:true,attributes:true,' +
        'attributeFilter:["src","srcset"]});' +
        '})();</' + 'script>';
      html = injectBeforeBody(html, imgShim);

      // Some storefronts re-render sliders/grids from JS on load. Offline that
      // fetch fails and the component empties itself, leaving a tall blank box
      // where products were. The captured markup was fine; the store's own
      // script cleared it. Collapse containers that ended up genuinely empty —
      // no children, no text, no background — so the page reads clean instead
      // of showing dead space. Nothing with content is ever touched.
      const collapseShim = '<script>(function(){' +
        'function sweep(){' +
        'var n=document.querySelectorAll("div,section,ul,slider-component");' +
        'for(var i=0;i<n.length;i++){var e=n[i];' +
        'if(e.children.length)continue;' +
        'if((e.textContent||"").trim())continue;' +
        'var r=e.getBoundingClientRect();if(r.height<160||r.width<160)continue;' +
        'var c=getComputedStyle(e);' +
        'if(c.backgroundImage&&c.backgroundImage!=="none")continue;' +
        'if(e.querySelector("img,svg,video,canvas,iframe"))continue;' +
        'e.style.display="none";}}' +
        'if(document.readyState==="complete")sweep();' +
        'else window.addEventListener("load",sweep);' +
        'setTimeout(sweep,1500);setTimeout(sweep,3500);' +
        '})();</' + 'script>';
      html = injectBeforeBody(html, collapseShim);

      // In demo mode, don't let account/cart/checkout links jump to the real
      // store mid-presentation. They are non-functional in the clone either
      // way; a dead click is better than silently leaving the demo.
      // Some storefronts build their nav in JavaScript with no <a href> at all,
      // so there is nothing to rewrite and the clone has no clickable route to
      // the pages we captured. In demo mode, add a small index of captured
      // pages so every clone is navigable regardless of how its nav is built.
      if (CLEAN) {
        const items = [...captured.keys()]
          .map(k => ({ p: k, f: (wanted.get(k) || pageFileFor(k) || {}).file }))
          .filter(x => x.f && x.f !== 'index.html')
          .slice(0, 40)
          .map(x => `<a href="${up}${x.f}">${x.p.replace(/^\//, '').slice(0, 46)}</a>`)
          .join('');
        if (items) {
          const idx = '<div id="_mirror-index" style="position:fixed;left:12px;bottom:12px;' +
            'z-index:2147483000;font:12px/1.5 system-ui,sans-serif;max-width:300px">' +
            '<details style="background:#111;color:#fff;border-radius:8px;padding:8px 10px;' +
            'box-shadow:0 4px 14px rgba(0,0,0,.35)">' +
            '<summary style="cursor:pointer;outline:none">Captured pages</summary>' +
            '<div style="max-height:320px;overflow:auto;margin-top:6px;display:flex;' +
            'flex-direction:column;gap:3px">' +
            items.replace(/<a /g, '<a style="color:#8ab4ff;text-decoration:none" ') +
            '</div></details></div>';
          html = injectBeforeBody(html, idx);
        }
      }

      // Demo mode: hard navigation guard. Rewriting every link correctly is
      // not achievable — country-variant domains, JS-driven navigation, form
      // actions and third-party widgets all provide escape routes. Instead,
      // block ANY navigation that would leave the local origin. Nothing can
      // eject the viewer back into the real storefront mid-presentation.
      if (CLEAN) {
        const navGuard = '<script>(function(){' +
          'function local(u){try{var a=new URL(u,location.href);' +
          'return a.origin===location.origin||a.protocol==="javascript:"||a.hash;}catch(e){return true}}' +
          'function toast(msg){var t=document.getElementById("_mg");if(!t){' +
          't=document.createElement("div");t.id="_mg";' +
          't.style.cssText="position:fixed;left:50%;bottom:24px;transform:translateX(-50%);' +
          'background:#111;color:#fff;padding:9px 15px;border-radius:8px;z-index:2147483600;' +
          'font:13px system-ui,sans-serif;opacity:0;transition:opacity .18s";' +
          'document.body.appendChild(t);}' +
          't.textContent=msg||"External link disabled in this demo";t.style.opacity="1";' +
          'clearTimeout(t._h);t._h=setTimeout(function(){t.style.opacity="0"},1600);}' +
          'document.addEventListener("click",function(e){' +
          'var a=e.target&&e.target.closest?e.target.closest("a[href]"):null;if(!a)return;' +
          'var h=a.getAttribute("href")||"";' +
          'if(/^(mailto:|tel:)/i.test(h))return;' +
          // A page the capture does not have keeps its REAL href (a theme that
          // rebuilds its menus drops "#" links — gorillamind.com lost 41 of 62)
          // and is made inert here, by the marker, with a toast that says why.
          'if(a.hasAttribute("data-mirror-uncaptured")||a.hasAttribute("data-mirror-inert")){' +
          'e.preventDefault();e.stopImmediatePropagation();toast("Not captured in this demo");return;}' +
          'if(!local(h)){e.preventDefault();e.stopImmediatePropagation();toast();}' +
          '},true);' +
          'document.addEventListener("submit",function(e){' +
          'var f=e.target;if(f&&f.action&&!local(f.action)){' +
          'e.preventDefault();e.stopImmediatePropagation();toast();}},true);' +
          // block programmatic escapes too
          'try{var _a=window.open;window.open=function(u){if(u&&!local(u)){toast();return null;}' +
          'return _a.apply(window,arguments);};}catch(e){}' +
          '})();</' + 'script>';
        html = injectBeforeBody(html, navGuard);
      }

      if (CLEAN) {
        html = html.replace(
          new RegExp(`href="https://${domain.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(\\/(?:account|cart|checkout)[^"]*)?"`, 'gi'),
          (m, path) => `href="${path || '/'}" data-mirror-inert="1"`);
      }

      if (CLEAN) {
        const shim = `<script>(function(){var S=['[class*="klaviyo"]','[id*="klaviyo"]',
          '[class*="kl-private"]','[class*="attentive"]','[id*="attentive"]',
          '[class*="privy"]','[id*="privy"]','[class*="justuno"]','[class*="wisepops"]',
          '[class*="postscript"]'];
          function f(){S.forEach(function(s){document.querySelectorAll(s).forEach(function(e){
            var c=getComputedStyle(e);
            if((c.position==='fixed'||c.position==='absolute')&&e.getBoundingClientRect().width>200)
              e.style.display='none';});});
            document.documentElement.style.overflow='';document.body.style.overflow='';}
          f();setTimeout(f,800);setTimeout(f,2500);
          new MutationObserver(f).observe(document.body,{childList:true,subtree:true});})();</` + `script>`;
        html = injectBeforeBody(html, shim);
      }

      // "Store paused" neutralizer (demo mode only). Some Shopify stores
      // (merchant-paused, e.g. shopseeti.com at the time this was written)
      // render a system announcement bar reading "The Store is Paused For
      // Now" and mark every product Sold Out. The Sold Out state doesn't
      // need fixing — the commerce overlay injected below always ships its
      // own guaranteed Add to Cart button on product pages regardless of
      // what the theme's own (offline-broken) buy button says — but the
      // announcement banner is a jarring, obviously-wrong artifact in a
      // demo and is worth stripping. Matched generically on the message
      // text (/store is paused/i), not on any store-specific selector, so
      // it applies to any storefront that shows the same Shopify system
      // banner rather than just the one this was diagnosed against.
      if (CLEAN) {
        const pausedShim = '<script>(function(){' +
          'function sweep(){' +
          'var all=document.querySelectorAll("body *");' +
          'for(var i=0;i<all.length;i++){var e=all[i];' +
          'var t=(e.textContent||"").trim();' +
          'if(!t||t.length>300||!/store is paused/i.test(t))continue;' +
          // Walk up from the matching (often innermost) element to the
          // largest ancestor that still looks banner-sized — a handful of
          // descendants and a short combined text — so the whole bar gets
          // hidden rather than just the text node's immediate wrapper.
          'var node=e;' +
          'while(node.parentElement&&node.parentElement!==document.body&&' +
          'node.parentElement.querySelectorAll("*").length<25&&' +
          '(node.parentElement.textContent||"").trim().length<400){' +
          'node=node.parentElement;}' +
          'node.style.display="none";}}' +
          'sweep();document.addEventListener("DOMContentLoaded",sweep);' +
          'setTimeout(sweep,500);setTimeout(sweep,1500);' +
          'new MutationObserver(sweep).observe(document.documentElement,{childList:true,subtree:true});' +
          '})();</' + 'script>';
        html = injectBeforeBody(html, pausedShim);
      }

      // ForkLaunch commerce overlay (demo mode only) — the piece that turns
      // the clone into something actually shoppable end-to-end: a cart
      // button + drawer, a full checkout page, and an order-confirmation
      // screen, plus a guaranteed Add to Cart button on product pages (see
      // commerce.js's own docstring). Injected at DOCUMENT START via
      // injectAtDocStart — before every one of the captured page's own
      // scripts. This is what makes it hydration-proof AND parser-robust: the
      // overlay defines window.FL and installs its self-healing
      // MutationObserver at the very first moment of parse, so neither a
      // React/Hydrogen/Remix client re-render (which drops body-level inline
      // scripts) nor a malformed captured inline <script> that flips the
      // parser into raw-text mode (which swallows every later tag — see
      // injectAtDocStart's docstring, observed on jonesroadbeauty.com) can
      // stop it. The overlay builds its own DOM into <body> at
      // DOMContentLoaded and re-asserts it on any wipe.
      if (CLEAN) {
        html = injectAtDocStart(html, buildCommerceOverlay());
      }

      const dst = path.join(outdir, 'site', meta.file);
      fs.mkdirSync(path.dirname(dst), { recursive: true });
      fs.writeFileSync(dst, html);
      result.captured.push(meta.file);

      // <title> survives every rewrite above untouched, so it's safe to pull
      // from the final html rather than re-reading the file back off disk.
      const titleMatch = /<title[^>]*>([^<]*)<\/title>/i.exec(html);
      pageIndex.push({
        route: pth,
        file: meta.file,
        type: pageTypeFor(pth),
        title: titleMatch ? (titleMatch[1].replace(/\s+/g, ' ').trim() || null) : null,
      });
    }

    result.pages = captured.size;
    result.ok = true;
  } catch (e) {
    if (!result.reason) result.reason = String(e.message || e).slice(0, 120);
  } finally {
    result.elapsedMs = Date.now() - result.startedAt;
    await browser.close().catch(() => {});
    // Where every local asset came from. check-features reads this to tell a
    // vendor script failing without its backend (policy) from a theme script
    // failing because the capture broke it (defect) — by ORIGIN, which is
    // evidence, rather than by guessing from a file name. Always merged, so a
    // targeted recapture adds to the record instead of replacing it.
    try {
      const of = path.join(outdir, 'site', '_a', '.fl-origins.json');
      let origins = {};
      try { origins = JSON.parse(fs.readFileSync(of, 'utf8')); } catch (_) {}
      for (const [abs, rel] of assetMap) {
        if (rel && fs.existsSync(path.join(outdir, 'site', rel))) origins[rel] = abs;
      }
      fs.mkdirSync(path.dirname(of), { recursive: true });
      fs.writeFileSync(of, JSON.stringify(origins));
    } catch (_) {}
    if (ONLY) {
      // Targeted mode ADDS to an existing capture: merge into its records
      // rather than replacing a 33-page crawl.json with a 5-page one.
      try {
        const prev = JSON.parse(fs.readFileSync(path.join(outdir, 'crawl.json'), 'utf8'));
        const files = new Set([...(prev.captured || []), ...result.captured]);
        result.captured = [...files];
        result.pages = files.size;
        result.assets = (prev.assets || 0) + result.assets;
        result.bytes = (prev.bytes || 0) + result.bytes;
      } catch (_) {}
      try {
        const prevIdx = JSON.parse(fs.readFileSync(path.join(outdir, 'pages.json'), 'utf8'));
        const seen = new Set(pageIndex.map((x) => x.route));
        for (const x of prevIdx) if (!seen.has(x.route)) pageIndex.push(x);
      } catch (_) {}
    }
    fs.writeFileSync(path.join(outdir, 'crawl.json'), JSON.stringify(result, null, 1));
    // Sidecar for manifest.js — see pageIndex declaration above.
    fs.writeFileSync(path.join(outdir, 'pages.json'), JSON.stringify(pageIndex, null, 1));
    console.log(JSON.stringify(result));
  }
})();
