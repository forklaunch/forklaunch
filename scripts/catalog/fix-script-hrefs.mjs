#!/usr/bin/env node
/**
 * fix-script-hrefs — revert the demo-mode link rewrite where it landed INSIDE
 * a <script> body.
 *
 *   node catalog/fix-script-hrefs.mjs <site-dir>
 *
 * crawl.js's uncaptured-link rewrite turns `href="/checkout"` into
 * `href="#" data-mirror-uncaptured="/checkout"` so a demo never ejects the
 * viewer into the live store. Run over JavaScript that is a syntax error:
 * `location.href="#" data-mirror-uncaptured="/checkout"` (a vendor's inline
 * snippet on graza.co /pages/subscribe) took the whole inline script down.
 * crawl.js now skips script bodies; this repairs captures made before it did.
 * Idempotent: a page with nothing to revert is left byte-identical.
 */
import { readdirSync, readFileSync, writeFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const SITE = process.argv[2];
if (!SITE) { console.error('usage: fix-script-hrefs.mjs <site-dir>'); process.exit(2); }

function* htmlFiles(d) {
  for (const name of readdirSync(d)) {
    if (name === '_a' || name.startsWith('.')) continue;
    const p = join(d, name);
    let st; try { st = statSync(p); } catch { continue; }
    if (st.isDirectory()) yield* htmlFiles(p);
    else if (name.endsWith('.html')) yield p;
  }
}

const BROKEN = /href="#" data-mirror-uncaptured="([^"]*)"/g;
let files = 0, fixed = 0, reverted = 0, logos = 0;
for (const f of htmlFiles(SITE)) {
  files++;
  const html = readFileSync(f, 'utf8');
  let n = 0;
  let out = html.split(/(<script\b[^>]*>[\s\S]*?<\/script>)/i)
    .map((seg, i) => (i % 2 ? seg.replace(BROKEN, (m, p) => { n++; return `href="${p}"`; }) : seg))
    .join('');
  // Older captures rewrote the logo's href="/" to the homepage's own HTML
  // recorded as an asset (_a/other/index.<hash>.bin). Point it at index.html.
  let l = 0;
  out = out.replace(/href="((?:\.\.\/)*)_a\/other\/index\.[0-9a-f]{10}\.bin"/g, (m, up) => { l++; return `href="${up}index.html"`; });
  if (n || l) { writeFileSync(f, out); fixed++; reverted += n; logos += l; }
}
console.log(`fix-script-hrefs: ${files} page(s) scanned, ${fixed} repaired, ${reverted} rewrite(s) reverted inside scripts, ${logos} home link(s) pointed back at index.html`);
