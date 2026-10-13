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

const deent = (u) => u.replace(/&(?:amp|#38);/g, '&').trim();

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

  // A sitemap URL with no captured file is only a NOT a gap when it redirects
  // to a page we DID capture — the shopper bounces to real content. Verifying
  // the 3xx alone is not enough and silently inflates coverage: an anti-bot
  // layer answers with a 302 to a challenge page, so a fully bot-walled store
  // scores 100% while every one of those routes is dead in the clone. (Seen
  // for real: a store returned 3xx under rate limiting, then 429 once the
  // limiter engaged — 38% of its sitemap was being counted as covered.)
  // So: follow the redirect, map where it LANDS, and require that file.
  const origin = `https://${domain.replace(/^https?:\/\//, '').replace(/\/.*/, '')}`;
  const missing = [];
  const redirected = [];
  const blocked = [];
  // A burst of one HEAD per absent URL is exactly what makes a store answer
  // 429, and a 429 here is scored as unverifiable — so the gate would fail
  // itself. A small pool keeps the probe polite.
  const CONC = Math.max(1, Number(process.env.FIDELITY_PROBE_CONCURRENCY ?? 4));
  const probeOne = async (p) => {
    let status = 0, location = null;
    try {
      const r = await fetch(`${origin}${p}`,
        { method: 'HEAD', redirect: 'manual', headers: { 'User-Agent': UA }, signal: AbortSignal.timeout(15000) });
      status = r.status;
      location = r.headers.get('location');
    } catch { status = 0; }

    // Rate limiting / bot walls are neither "captured" nor "a missing page" —
    // they mean the probe could not determine anything. Surfacing them
    // separately keeps them from being silently scored either way.
    if (status === 429 || status === 403) { blocked.push(p); return; }

    if (status >= 300 && status < 400 && location) {
      let targetPath;
      try { targetPath = new URL(location, origin).pathname; } catch { targetPath = null; }
      const targetFile = targetPath ? pageFileFor(targetPath) : null;
      if (targetFile && fs.existsSync(path.join(siteDir, targetFile.file))) {
        redirected.push(`${p} -> ${targetPath}`);
        return;
      }
    }
    missing.push(p);
  };
  const queue = [...absent];
  await Promise.all(Array.from({ length: Math.min(CONC, queue.length) }, async () => {
    while (queue.length) await probeOne(queue.shift());
  }));

  // Blocked probes count against coverage, not for it: an unverifiable page is
  // not a captured page, and treating it as one is how a bot-walled store
  // reports 100%.
  const total = wanted.size;
  const present = total - missing.length - blocked.length;
  const pct = total ? (present / total * 100) : 100;
  console.log(`fidelity: ${present}/${total} public pages verified captured (${pct.toFixed(2)}%)`);
  if (redirected.length) {
    console.log(`  ${redirected.length} sitemap URL(s) redirect to a page we captured — reachable, not gaps:`);
    for (const p of redirected.slice(0, 10)) console.log(`    ↪ ${p}`);
  }
  if (blocked.length) {
    console.log(`  ${blocked.length} URL(s) returned 429/403 — the store is rate limiting or bot walling this probe,`);
    console.log(`    so coverage for them is UNKNOWN, not verified. Re-run later or capture them assisted:`);
    for (const p of blocked.slice(0, 10)) console.log(`    ⏳ ${p}`);
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
