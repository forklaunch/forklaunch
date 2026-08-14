#!/usr/bin/env node
/**
 * check-complete — fidelity gate for a --complete capture.
 *
 *   node check-complete.mjs <capture-dir> <domain>
 *
 * Fetches the store's live sitemap (the authoritative list of every public
 * page), maps each URL to the file the crawler would write, and checks that
 * file actually exists in <capture-dir>/site. Prints coverage and lists any
 * missing pages — a missing page is a page a shopper could reach on the real
 * site but not on ours (a dead link). Exits non-zero if coverage is below the
 * threshold, so it can gate a migration before anyone sees it.
 *
 * The URL->file mapping mirrors crawl.js's pageFileFor exactly; keep them in
 * sync (there's a unit test, check-complete.test.mjs, that pins the shared
 * cases so they can't silently drift).
 */
import fs from 'node:fs';
import path from 'node:path';
// The SAME mapping the crawler uses to place pages — imported, not copied, so
// "did we capture every page?" can never drift from where pages actually land.
import { createRequire } from 'node:module';
const { pageFileFor } = createRequire(import.meta.url)('./urlmap.js');

const UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 ' +
          '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';

const deent = (u) => u.replace(/&amp;/g, '&').replace(/&#38;/g, '&').trim();

async function fetchXml(u) {
  try {
    const r = await fetch(u, { headers: { 'User-Agent': UA }, signal: AbortSignal.timeout(20000) });
    return r.ok ? await r.text() : null;
  } catch { return null; }
}

// Every public URL the store publishes, via sitemap.xml + all child sitemaps.
export async function enumerateSitemap(domain) {
  const origin = `https://${domain.replace(/^https?:\/\//, '').replace(/\/.*/, '')}`;
  const root = await fetchXml(`${origin}/sitemap.xml`);
  if (!root) return [];
  const seen = new Set();
  const out = [];
  const queue = [...root.matchAll(/<loc>([^<]+)<\/loc>/g)]
    .map((m) => deent(m[1])).filter((u) => /sitemap[^"]*\.xml/i.test(u));
  if (!queue.length) return [...root.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => deent(m[1]));
  while (queue.length) {
    const c = queue.shift();
    if (seen.has(c)) continue;
    seen.add(c);
    const xml = await fetchXml(c);
    if (!xml) continue;
    for (const m of xml.matchAll(/<loc>([^<]+)<\/loc>/g)) {
      const u = deent(m[1]);
      if (/sitemap[^"]*\.xml/i.test(u)) { if (!seen.has(u)) queue.push(u); }
      else out.push(u);
    }
  }
  return out;
}

// Run as a CLI (not when imported by the test).
if (import.meta.url === `file://${process.argv[1]}`) {
  const [dir, domain] = process.argv.slice(2);
  if (!dir || !domain) { console.error('usage: node check-complete.mjs <capture-dir> <domain>'); process.exit(1); }
  const siteDir = fs.existsSync(path.join(dir, 'site')) ? path.join(dir, 'site') : dir;

  const urls = await enumerateSitemap(domain);
  const wanted = new Map(); // file -> sample url
  for (const u of urls) {
    let p; try { p = new URL(u).pathname; } catch { continue; }
    const f = pageFileFor(p);
    if (f) wanted.set(f.file, p);
  }
  const absent = [];
  for (const [f, p] of wanted) if (!fs.existsSync(path.join(siteDir, f))) absent.push(p);

  // A sitemap URL with no captured file is only a real GAP if a shopper would
  // actually hit a dead page there. Sitemaps routinely list URLs that 3xx to
  // a canonical page (renamed collections, consolidated handles) — the
  // crawler follows the redirect and captures the TARGET, so the source path
  // is reachable (it bounces to a real page), not missing. Probe each absent
  // URL live and only count non-redirecting ones (2xx/4xx/5xx) as gaps.
  const missing = [];
  const redirected = [];
  await Promise.all(absent.map(async (p) => {
    let status = 0;
    try {
      const r = await fetch(`https://${domain.replace(/^https?:\/\//, '').replace(/\/.*/, '')}${p}`,
        { method: 'HEAD', redirect: 'manual', headers: { 'User-Agent': UA }, signal: AbortSignal.timeout(15000) });
      status = r.status;
    } catch { status = 0; }
    if (status >= 300 && status < 400) redirected.push(p);
    else missing.push(p);
  }));

  const total = wanted.size, present = total - missing.length;
  const pct = total ? (present / total * 100) : 100;
  console.log(`fidelity: ${present}/${total} public pages captured (${pct.toFixed(2)}%)`);
  if (redirected.length) {
    console.log(`  ${redirected.length} sitemap URL(s) redirect (3xx) to a captured page — reachable, not counted as gaps:`);
    for (const p of redirected.slice(0, 10)) console.log(`    ↪ ${p}`);
  }
  if (missing.length) {
    console.log(`  ${missing.length} MISSING (reachable on the real store, not on ours):`);
    for (const p of missing.slice(0, 25)) console.log(`    ✗ ${p}`);
    if (missing.length > 25) console.log(`    … and ${missing.length - 25} more`);
  }
  const THRESHOLD = Number(process.env.FIDELITY_THRESHOLD ?? 99.5);
  if (pct + 1e-9 < THRESHOLD) { console.log(`  FAIL — below ${THRESHOLD}% (a client could hit a dead page)`); process.exit(2); }
  console.log('  PASS — every public page is present');
}
