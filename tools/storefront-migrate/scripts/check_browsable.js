#!/usr/bin/env node
/**
 * Verify a crawled clone is actually BROWSABLE.
 *
 * Deliberately does not compare against live — that comparison is undefined for
 * stores that change between loads. This asks a self-contained question instead:
 * does the clone we produced stand on its own?
 *
 *   node check_browsable.js <domain> <outdir>
 */
const { chromium } = require('playwright');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const http = require('http');

// Serve over HTTP, not file://. That is how the clone is actually used, and
// file:// breaks module scripts / CORS in ways that have nothing to do with
// whether the capture is good.
function serve(root) {
  const MIME = { '.html':'text/html', '.css':'text/css', '.js':'text/javascript',
    '.json':'application/json', '.png':'image/png', '.jpg':'image/jpeg',
    '.jpeg':'image/jpeg', '.gif':'image/gif', '.svg':'image/svg+xml',
    '.webp':'image/webp', '.avif':'image/avif', '.woff':'font/woff',
    '.woff2':'font/woff2', '.ttf':'font/ttf', '.otf':'font/otf', '.ico':'image/x-icon' };
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
  const [domain, outdir] = process.argv.slice(2);
  const res = { domain, BROWSABLE: false, why: null, pages: 0, pagesChecked: 0,
                renderedOk: 0, localLinks: 0, deadLinks: 0, brokenImgs: 0 };
  try {
    const site = path.join(outdir, 'site');
    if (!fs.existsSync(path.join(site, 'index.html'))) {
      res.why = 'no_index'; throw new Error('no index');
    }
    const walk = (d, acc = []) => {
      for (const f of fs.readdirSync(d, { withFileTypes: true })) {
        if (f.name === '_a') continue;
        const p = path.join(d, f.name);
        if (f.isDirectory()) walk(p, acc);
        else if (f.name.endsWith('.html')) acc.push(p);
      }
      return acc;
    };
    const pages = walk(site);
    res.pages = pages.length;
    if (pages.length < 2) { res.why = 'single_page_only'; throw new Error('one page'); }

    const { srv, port } = await serve(path.resolve(site));
    const b = await chromium.launch({ headless: true });
    const pg = await b.newPage({ viewport: { width: 1280, height: 800 } });

    // check up to 4 pages: index + a sample
    const sample = [path.join(site, 'index.html'),
                    ...pages.filter(p => !p.endsWith('index.html')).slice(0, 3)];
    for (const f of sample) {
      res.pagesChecked++;
      try {
        const rel = path.relative(path.resolve(site), f).split(path.sep).join('/');
        await pg.goto(`http://127.0.0.1:${port}/${rel}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
        await pg.waitForTimeout(2500);
        const r = await pg.evaluate(() => {
          const imgs = [...document.images];
          return {
            h: document.documentElement.scrollHeight,
            w: document.documentElement.scrollWidth,
            broken: imgs.filter(i => i.complete && i.naturalWidth === 0).length,
            font: (getComputedStyle(document.body).fontFamily || '').split(',')[0]
                    .replace(/["']/g, '').trim(),
            localHrefs: [...document.querySelectorAll('a[href$=".html"]')].length,
            text: document.body.innerText.length,
          };
        });
        // a page "rendered" if it has real height, real text, and isn't blown out
        const ok = r.h > 600 && r.text > 400 && r.w <= 1400;
        if (ok) res.renderedOk++;
        res.brokenImgs += r.broken;
        res.localLinks += r.localHrefs;
        if (f.endsWith('index.html')) { res.homeFont = r.font; res.homeH = r.h; res.homeW = r.w; }
      } catch (_) { /* page failed to load; counts against renderedOk */ }
    }
    await b.close();
    srv.close();

    // resolve a sample of local links against disk
    const idx = fs.readFileSync(path.join(site, 'index.html'), 'utf8');
    const hrefs = [...idx.matchAll(/href="([^"]+\.html)"/g)].map(m => m[1])
      // off-site URLs that merely END in .html (news articles, PDFs elsewhere)
      // are not our links to resolve.
      .filter(h => !/^(https?:)?\/\//i.test(h))
      .slice(0, 25);
    for (const h of hrefs) {
      const t = path.join(site, h.replace(/^\.\//, ''));
      if (!fs.existsSync(t)) { res.deadLinks++; (res.deadSample ||= []).push(h); }
    }

    res.BROWSABLE = res.pages >= 2 &&
                    res.renderedOk === res.pagesChecked &&
                    res.localLinks > 0 &&
                    res.deadLinks === 0;
    if (!res.BROWSABLE && !res.why) {
      res.why = res.renderedOk < res.pagesChecked ? 'page_render_fail'
              : res.localLinks === 0 ? 'no_local_links'
              : res.deadLinks ? 'dead_links' : 'unknown';
    }
  } catch (e) {
    if (!res.why) res.why = String(e.message || e).slice(0, 80);
  } finally {
    console.log(JSON.stringify(res));
  }
})();
