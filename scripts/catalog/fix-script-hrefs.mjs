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
let files = 0, fixed = 0, reverted = 0;
for (const f of htmlFiles(SITE)) {
  files++;
  const html = readFileSync(f, 'utf8');
  let n = 0;
  const out = html.split(/(<script\b[^>]*>[\s\S]*?<\/script>)/i)
    .map((seg, i) => (i % 2 ? seg.replace(BROKEN, (m, p) => { n++; return `href="${p}"`; }) : seg))
    .join('');
  if (n) { writeFileSync(f, out); fixed++; reverted += n; }
}
console.log(`fix-script-hrefs: ${files} page(s) scanned, ${fixed} repaired, ${reverted} rewrite(s) reverted inside scripts`);
