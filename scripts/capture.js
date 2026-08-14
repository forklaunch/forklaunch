#!/usr/bin/env node
/**
 * Headless capture of a storefront homepage.
 *
 * Improvement over mirror.py: assets are collected by observing actual network
 * responses rather than regex-parsing HTML. That means lazily-loaded webpack
 * chunks, runtime-injected assets, and relative url() refs inside CSS are all
 * captured for free — those were the three things that broke the first pass.
 *
 * Usage: node capture.js <domain> <outdir>
 */
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const VIEWPORT = { width: 1280, height: 800 };
const UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 ' +
           '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
const NAV_TIMEOUT = 45000;
const MAX_BYTES = 260 * 1024 * 1024;   // per-store disk cap

const EXT_DIR = { css:'css', js:'js', woff:'fonts', woff2:'fonts', ttf:'fonts',
                  otf:'fonts', eot:'fonts', png:'img', jpg:'img', jpeg:'img',
                  gif:'img', webp:'img', svg:'img', avif:'img', ico:'img' };

function localFor(url) {
  const u = new URL(url);
  const clean = u.pathname.split('/').pop() || 'index';
  let ext = (clean.includes('.') ? clean.split('.').pop() : '').toLowerCase();
  if (!EXT_DIR[ext]) ext = 'bin';
  const sub = EXT_DIR[ext] || 'other';
  const h = crypto.createHash('md5').update(url).digest('hex').slice(0, 10);
  const base = clean.replace(/[^A-Za-z0-9._-]/g, '_').replace(/\.[^.]*$/, '').slice(0, 48) || 'a';
  return `_a/${sub}/${base}.${h}.${ext}`;
}

(async () => {
  const domain = process.argv[2];
  const outdir = process.argv[3];
  const result = { domain, ok: false, reason: null, assets: 0, assetsSaved: 0,
                   bytes: 0, htmlBytes: 0, consoleErrors: 0, startedAt: Date.now() };

  fs.mkdirSync(path.join(outdir, 'site'), { recursive: true });

  // Hard watchdog: no single store may hang the batch.
  const WATCHDOG_MS = 110000;
  const watchdog = setTimeout(() => {
    result.reason = result.reason || 'watchdog';
    result.elapsedMs = Date.now() - result.startedAt;
    try {
      fs.writeFileSync(path.join(outdir, 'capture.json'), JSON.stringify(result, null, 1));
      console.log(JSON.stringify(result));
    } catch (_) {}
    process.exit(0);
  }, WATCHDOG_MS);
  watchdog.unref?.();

  const browser = await chromium.launch({ headless: true });
  const ctx = await browser.newContext({
    viewport: VIEWPORT, userAgent: UA, ignoreHTTPSErrors: true,
    javaScriptEnabled: true,
  });
  const page = await ctx.newPage();

  const assetMap = new Map();      // absolute url -> local rel path
  const pending = [];
  let total = 0;

  page.on('console', m => { if (m.type() === 'error') result.consoleErrors++; });

  page.on('response', async (resp) => {
    try {
      const url = resp.url();
      if (!/^https?:/.test(url)) return;
      const ct = (resp.headers()['content-type'] || '').split(';')[0];
      if (ct.startsWith('text/html')) return;          // the document itself
      if (resp.status() >= 400) return;
      if (assetMap.has(url)) return;
      const rel = localFor(url);
      assetMap.set(url, rel);
      pending.push((async () => {
        try {
          // resp.body() can hang indefinitely on streamed/aborted responses
          const buf = await Promise.race([
            resp.body(),
            new Promise(r => setTimeout(() => r(null), 8000)),
          ]);
          if (!buf || !buf.length) return;
          if (total + buf.length > MAX_BYTES) return;
          total += buf.length;
          const dst = path.join(outdir, 'site', rel);
          fs.mkdirSync(path.dirname(dst), { recursive: true });
          fs.writeFileSync(dst, buf);
          result.assetsSaved++;
        } catch (_) { /* body unavailable (redirect/cached) */ }
      })());
    } catch (_) {}
  });

  try {
    const resp = await page.goto(`https://${domain}/`,
      { waitUntil: 'domcontentloaded', timeout: NAV_TIMEOUT });
    if (!resp) { result.reason = 'no_response'; throw new Error('no response'); }
    if (resp.status() >= 400) { result.reason = `http_${resp.status()}`; throw new Error('http'); }

    // let the JS settle
    try { await page.waitForLoadState('networkidle', { timeout: 20000 }); } catch (_) {}

    // trigger lazy-loaded images: walk the full page, then return to top
    // Bounded: scrollHeight grows while lazy content loads, so a naive
    // "scroll until you reach the bottom" loop never terminates.
    await page.evaluate(async () => {
      await new Promise(res => {
        let y = 0, iters = 0;
        const MAX_ITERS = 60, MAX_Y = 40000;
        const step = () => {
          y += 700; iters++;
          window.scrollTo(0, y);
          const done = iters >= MAX_ITERS || y >= MAX_Y ||
                       y >= document.body.scrollHeight + 1000;
          if (!done) setTimeout(step, 80);
          else { window.scrollTo(0, 0); setTimeout(res, 500); }
        };
        step();
      });
    }).catch(() => {});
    try { await page.waitForLoadState('networkidle', { timeout: 15000 }); } catch (_) {}
    await page.waitForTimeout(1200);

    // Materialize CSSOM rules into <style> tags before serializing.
    // styled-components (and emotion) in production inject rules via
    // CSSStyleSheet.insertRule(), which leaves the <style> element's
    // textContent EMPTY. page.content() serializes the DOM, so those styles
    // vanish — the clone loads with fallback fonts and an unconstrained
    // layout. bombas.com rendered 46,175px tall in Times for exactly this.
    await page.evaluate(() => {
      for (const sheet of Array.from(document.styleSheets)) {
        const node = sheet.ownerNode;
        if (!node || node.tagName !== 'STYLE') continue;
        if (node.textContent && node.textContent.trim().length) continue;
        let rules;
        try { rules = sheet.cssRules; } catch (_) { continue; }  // cross-origin
        if (!rules || !rules.length) continue;
        try {
          node.textContent = Array.from(rules).map(r => r.cssText).join('\n');
        } catch (_) {}
      }
    }).catch(() => {});

    // capture the POST-JAVASCRIPT dom, not the delivered html
    let html = await page.content();
    result.htmlBytes = Buffer.byteLength(html);

    await Promise.race([
      Promise.allSettled(pending),
      new Promise(r => setTimeout(r, 25000)),
    ]);
    result.assets = assetMap.size;
    result.bytes = total;

    // rewrite absolute asset urls -> local paths
    const sorted = [...assetMap.entries()].sort((a, b) => b[0].length - a[0].length);
    for (const [abs, rel] of sorted) {
      const noProto = abs.replace(/^https?:/, '');
      html = html.split(abs).join(rel).split(noProto).join(rel);
    }
    // Pathname fallback for responsive images: Shopify serves the same file at
    // many ?width= sizes. The DOM may reference a width variant we didn't fetch,
    // so exact-URL rewrite misses it. Map any surviving CDN ref to a captured
    // asset with the same pathname, ignoring the query string.
    const byPath = new Map();
    for (const [abs, rel] of assetMap) {
      try { const pth = new URL(abs).pathname; if (!byPath.has(pth)) byPath.set(pth, rel); }
      catch (_) {}
    }
    html = html.replace(/(https?:)?\/\/[a-z0-9.-]+\/(?:cdn|s\/files)\/[^\s"')]+/gi, (m) => {
      try {
        const pth = new URL(m.startsWith('//') ? 'https:' + m : m).pathname;
        return byPath.get(pth) || m;
      } catch (_) { return m; }
    });

    // Same-origin absolute-root asset refs: Next.js/Hydrogen stores reference
    // assets by root path (/_next/static/..., /cdn/...). Under file:// those
    // resolve to the filesystem root and 404 — no CSS, blown-out layout, Times
    // fallback. The assets ARE on disk; rewrite any ref whose pathname we
    // captured. Nav links (not in byPath) are left untouched.
    html = html.replace(/((?:src|href|data-src|poster)=")(\/[^"]+)"/gi, (m, pre, p) => {
      const local = byPath.get(p.split('?')[0].split('#')[0]);
      return local ? `${pre}${local}"` : m;
    });
    html = html.replace(/(srcset=")([^"]+)"/gi, (m, pre, val) => {
      const nv = val.split(',').map(part => {
        const seg = part.trim().split(/\s+/);
        if (seg[0] && seg[0].startsWith('/')) {
          const local = byPath.get(seg[0].split('?')[0].split('#')[0]);
          if (local) seg[0] = local;
        }
        return seg.join(' ');
      }).join(', ');
      return `${pre}${nv}"`;
    });

    // Protocol-relative URLs (//host/path). Under file:// these resolve to
    // file://host/path and fail — which kills the scripts that build the page.
    // Captured ones go local; the rest are made absolute https so they load
    // from the network instead of breaking outright.
    const fixProtoRel = (u) => {
      const clean = u.split('?')[0].split('#')[0];
      let pth = null;
      try { pth = new URL('https:' + u).pathname; } catch (_) {}
      const local = (pth && byPath.get(pth)) || null;
      return local || ('https:' + u);
    };
    html = html.replace(/((?:src|href|data-src|poster)=")(\/\/[^"]+)"/gi,
      (m, pre, u) => `${pre}${fixProtoRel(u)}"`);

    // Same-origin root paths we never captured: absolutise to the live origin
    // so they fall back to the network rather than 404ing against the filesystem.
    html = html.replace(/((?:src|poster)=")(\/[^\/"][^"]*)"/gi, (m, pre, p) => {
      const clean = p.split('?')[0].split('#')[0];
      if (byPath.has(clean)) return m;              // already rewritten to local
      if (/^_a\//.test(p)) return m;
      return `${pre}https://${domain}${p}"`;
    });

    // Stylesheet hrefs specifically. href is excluded from the rule above
    // because most hrefs are nav links, but an uncaptured stylesheet is fatal:
    // no CSS means fallback fonts and a completely unconstrained layout.
    html = html.replace(/<link\b[^>]*>/gi, (tag) => {
      if (!/rel\s*=\s*["']?stylesheet/i.test(tag)) return tag;
      return tag.replace(/href="(\/[^\/"][^"]*)"/i, (m, p) => {
        const clean = p.split('?')[0].split('#')[0];
        if (byPath.has(clean) || /^_a\//.test(p)) return m;
        return `href="https://${domain}${p}"`;
      });
    });

    // strip integrity/crossorigin — hashes no longer match rewritten local files
    html = html.replace(/\sintegrity="[^"]*"/g, '').replace(/\scrossorigin="[^"]*"/g, '');
    // neutralise base tags that would re-point relative urls at the origin
    html = html.replace(/<base[^>]*>/gi, '');

    // Cookie-consent dialogs are removed from the captured DOM. They are not
    // part of the storefront, they are geo/cookie-gated (so they appear
    // inconsistently), and their styling is managed by vendor JS that doesn't
    // run offline — leaving them expanded to full height. On hellotushy the
    // Cookiebot dialog rendered 49,492px tall, 87% of the whole document.
    // Done with CSS rather than by excising HTML: consent containers nest
    // arbitrarily, and a regex that eats balanced <div>s is a good way to
    // silently delete half a page.
    const consentCss = '<style id="_mirror-consent">' +
      '[id*="Cybot"],[class*="Cybot"],[id*="onetrust"],[class*="onetrust"],' +
      '[id*="ot-sdk"],[class*="ot-sdk"],[id*="truste"],[class*="truste"],' +
      '[id*="klaro"],[class*="klaro"],[id*="osano"],[class*="osano"],' +
      '[id*="didomi"],[class*="didomi"],[id*="usercentrics"],[class*="usercentrics"],' +
      '[class*="cookie-consent"],[id*="cookie-consent"],[class*="CookieConsent"]' +
      '{display:none !important}</style>';
    html = html.replace(/<\/head>/i, consentCss + '</head>');

    // Opt-in demo polish (--clean): suppress vendor marketing modals so the
    // clone is presentable. OFF by default — it changes layout, and fidelity
    // measurement must compare against an unmodified capture.
    if (process.argv.includes('--clean')) {
      const shim = `<script>(function(){
        var SEL = ['[class*="klaviyo"]','[id*="klaviyo"]','[class*="kl-private"]',
                   '[class*="attentive"]','[id*="attentive"]','#attentive_overlay',
                   '[class*="privy"]','[id*="privy"]','[class*="justuno"]',
                   '[class*="ju_"]','[class*="postscript"]','[class*="wisepops"]',
                   '[class*="OptIn"]','[data-testid*="modal"]'];
        function sweep(){
          SEL.forEach(function(s){
            document.querySelectorAll(s).forEach(function(el){
              var cs = getComputedStyle(el);
              if (cs.position === 'fixed' || cs.position === 'absolute') {
                if (el.getBoundingClientRect().width > 200) el.style.display='none';
              }
            });
          });
          document.documentElement.style.overflow='';
          document.body.style.overflow='';
        }
        sweep(); setTimeout(sweep,800); setTimeout(sweep,2500);
        new MutationObserver(sweep).observe(document.body,{childList:true,subtree:true});
      })();</` + `script>`;
      html = html.replace(/<\/body>/i, shim + '</body>');
    }

    fs.writeFileSync(path.join(outdir, 'site', 'index.html'), html);

    // rewrite url() refs inside captured CSS (css sits at _a/css/ -> ../)
    for (const [abs, rel] of assetMap) {
      if (!rel.endsWith('.css')) continue;
      const p = path.join(outdir, 'site', rel);
      if (!fs.existsSync(p)) continue;
      let css = fs.readFileSync(p, 'utf8');
      css = css.replace(/url\(\s*(['"]?)([^'")]+)\1\s*\)/g, (m, q, ref) => {
        if (/^(data:|#)/.test(ref)) return m;
        let target;
        try { target = new URL(ref, abs).toString(); } catch (_) { return m; }
        const local = assetMap.get(target);
        return local ? `url("../${local.slice(3)}")` : m;
      });
      fs.writeFileSync(p, css);
    }

    // Fallback: some responses never yield a body (streamed, cached, aborted).
    // Re-fetch those over plain HTTP so they don't show up as broken assets.
    const missed = [...assetMap.entries()]
      .filter(([, rel]) => !fs.existsSync(path.join(outdir, 'site', rel)));
    result.missedAfterCapture = missed.length;
    let recovered = 0;
    await Promise.allSettled(missed.slice(0, 400).map(async ([abs, rel]) => {
      try {
        const r = await fetch(abs, { headers: { 'User-Agent': UA }, redirect: 'follow',
                                     signal: AbortSignal.timeout(9000) });
        if (!r.ok) return;
        const buf = Buffer.from(await r.arrayBuffer());
        if (!buf.length || total + buf.length > MAX_BYTES) return;
        total += buf.length;
        const dst = path.join(outdir, 'site', rel);
        fs.mkdirSync(path.dirname(dst), { recursive: true });
        fs.writeFileSync(dst, buf);
        recovered++;
      } catch (_) {}
    }));
    result.recovered = recovered;
    result.assetsSaved += recovered;
    result.bytes = total;

    fs.writeFileSync(path.join(outdir, 'assetmap.json'),
      JSON.stringify(Object.fromEntries(assetMap), null, 1));
    result.ok = true;
  } catch (e) {
    if (!result.reason) {
      const msg = String(e.message || e);
      result.reason = /timeout/i.test(msg) ? 'timeout'
                    : /net::ERR/.test(msg) ? 'network'
                    : 'error';
    }
  } finally {
    clearTimeout(watchdog);
    result.elapsedMs = Date.now() - result.startedAt;
    await browser.close().catch(() => {});
    fs.writeFileSync(path.join(outdir, 'capture.json'), JSON.stringify(result, null, 1));
    console.log(JSON.stringify(result));
  }
})();
