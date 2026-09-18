#!/usr/bin/env node
/**
 * refetch-missing — fetch the files the CLONE asks itself for and does not have.
 *
 *   node refetch-missing.mjs <site-dir> <clone-url> <live-origin> [--pages /,/x]
 *
 * There are three kinds of failed request on a served capture, and until now
 * only two of them had an answer:
 *
 *   external, blocked      the offline guarantee working. Left alone.
 *   external, wanted       localize-runtime pulls it local.
 *   OUR OWN ORIGIN, 404    nothing handled this at all.
 *
 * The third is the dangerous one. It means the page asks for
 * `/_a/js/chunk.reportAssetMetrics_BUu0CFbd.esm.js` — a path the crawl itself
 * wrote — and the file is not there. The crawl saw the reference and rewrote
 * it, but never fetched the file: lazily-imported chunks are requested only
 * after some interaction or viewport the crawl did not reproduce, so the
 * rewrite runs ahead of the download.
 *
 * localize-runtime cannot fix it, and that is not an oversight — it only looks
 * at requests going to OTHER hosts, and this one goes to localhost. So the
 * defect sat in the report every round with a repair named against it that
 * could never do anything, which is its own kind of lie.
 *
 * The fix is to ask the live site. Drive the same pages there, index every
 * subresource it loads by filename, and for each thing the clone is missing,
 * fetch the file with that name and write it where the clone was looking. The
 * live storefront is the only place the bytes exist, and it is the same source
 * the crawl would have used.
 *
 * Files are written under the EXACT name that 404'd, so the next request
 * resolves on the direct path with no sibling lookup needed.
 */
import { chromium } from 'playwright';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';

const SITE = process.argv[2];
const CLONE = (process.argv[3] || 'http://localhost:4180').replace(/\/$/, '');
const LIVE = (process.argv[4] || '').replace(/\/$/, '');
const pagesArg = process.argv.indexOf('--pages');

if (!SITE || !existsSync(SITE) || !LIVE) {
  console.error('usage: refetch-missing.mjs <site-dir> <clone-url> <live-origin> [--pages a,b]');
  process.exit(2);
}

// Same marker convention as the other repairs: remember what could not be
// found so a loop does not re-drive two browsers to rediscover it. Note the
// `.fl-` prefix — the localisers skip files named that way when they rewrite,
// after one of them rewrote its own marker into uselessness.
const MARKER = join(SITE, '_a', '.fl-refetch.json');
let marker = { fetched: {}, unavailable: {} };
try { if (existsSync(MARKER)) marker = { fetched: {}, unavailable: {}, ...JSON.parse(readFileSync(MARKER, 'utf8')) }; } catch (_) {}
const UNAVAILABLE_TTL_MS = 6 * 60 * 60 * 1000;

const browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
const ctx = await browser.newContext({ bypassCSP: true, viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();

async function choosePages() {
  if (pagesArg > 0) return process.argv[pagesArg + 1].split(',');
  await page.goto(CLONE + '/', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(3000);
  const found = await page.evaluate(() => {
    const h = [...document.querySelectorAll('a[href]')].map((a) => a.getAttribute('href') || '')
      .filter((x) => !/^(https?:|mailto:|tel:|#)/.test(x));
    return [h.find((x) => /(^|\/)collections\//.test(x)), h.find((x) => /(^|\/)products\//.test(x))].filter(Boolean);
  });
  return ['/', ...found.map((h) => (h.startsWith('/') ? h : '/' + h))];
}

/** Load a page and let it settle, scrolling so lazy work actually happens. */
async function exercise(base, path) {
  await page.goto(base + path, { waitUntil: 'load', timeout: 60000 }).catch(() => {});
  await page.waitForTimeout(4000);
  await page.evaluate(async () => {
    for (let y = 0; y < document.body.scrollHeight; y += window.innerHeight) {
      window.scrollTo(0, y);
      await new Promise((r) => setTimeout(r, 200));
    }
    window.scrollTo(0, 0);
  }).catch(() => {});
  await page.waitForTimeout(2500);
}

const pages = await choosePages();

// ---- pass 1: what does the clone ask itself for and not have? -------------
const missing = new Map();          // "/_a/js/name.js" -> filename
const onCloneResponse = (r) => {
  if (r.status() < 400) return;
  let u; try { u = new URL(r.url()); } catch { return; }
  if (!/^localhost|^127\.0\.0\.1/.test(u.host)) return;
  if (!/^\/_a\//.test(u.pathname)) return;   // only the capture's own buckets
  missing.set(u.pathname, basename(u.pathname));
};
page.on('response', onCloneResponse);
for (const p of pages) await exercise(CLONE, p);
page.off('response', onCloneResponse);

const now = Date.now();
const stale = [...missing.keys()].filter((k) => marker.unavailable[k] && now - marker.unavailable[k].at < UNAVAILABLE_TTL_MS);
for (const k of stale) missing.delete(k);

console.log(`${missing.size} file(s) the clone requests from itself and does not have` +
  (stale.length ? ` (+${stale.length} already known unavailable)` : ''));
if (!missing.size) {
  await browser.close();
  console.log('  converged — nothing on our own origin is 404ing');
  process.exit(0);
}

// ---- pass 2: where do those names live on the real site? ------------------
// filename -> live URL, or null once two different URLs share the name: a
// basename that is not unique cannot be placed with confidence, and a wrong
// file under the right name is worse than a 404 (it fails silently).
const liveByName = new Map();
const onLiveRequest = (r) => {
  try {
    const u = new URL(r.url());
    const n = basename(u.pathname);
    if (!n) return;
    if (!liveByName.has(n)) liveByName.set(n, r.url());
    else if (liveByName.get(n) !== r.url()) liveByName.set(n, null);
  } catch (_) {}
};
ctx.on('request', onLiveRequest);
for (const p of pages) await exercise(LIVE, p);
ctx.off('request', onLiveRequest);
await browser.close();

// ---- fetch and place ------------------------------------------------------
const MAX_ASSET_BYTES = 50 * 1024 * 1024; // a single storefront asset never legitimately exceeds this
let got = 0, gone = 0;
for (const [path, name] of missing) {
  // The crawl appends its own content hash: `foo.<10hex>.js`. A file that
  // 404'd may be asked for under either shape, so try the requested name first
  // and then the same name with any crawl hash stripped out.
  const bare = name.replace(/\.[0-9a-f]{10}(\.[a-z0-9]+)$/i, '$1');
  const src = liveByName.get(name) || liveByName.get(bare);
  if (!src) {
    gone++;
    const ambiguous = liveByName.get(name) === null || liveByName.get(bare) === null;
    marker.unavailable[path] = { at: Date.now(), why: ambiguous
      ? 'the live site serves more than one file by this name, so none can be placed with confidence'
      : 'the live site never requested a file by this name' };
    continue;
  }
  try {
    const res = await fetch(src, { redirect: 'follow', signal: AbortSignal.timeout(20000) });
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const len = Number(res.headers.get('content-length') || 0);
    if (len > MAX_ASSET_BYTES) throw new Error(`too large (${len} bytes)`);
    const bytes = Buffer.from(await res.arrayBuffer());
    if (bytes.length > MAX_ASSET_BYTES) throw new Error(`too large (${bytes.length} bytes)`);
    const dest = join(SITE, path.replace(/^\//, ''));
    try { mkdirSync(join(dest, '..'), { recursive: true }); } catch (e) { if (e && e.code === 'EEXIST') { console.log('  ✗ ' + String(dest || '').slice(-60) + ' — a path nested under a file name; cannot be served as a file'); continue; } throw e; }
    // Written under the EXACT name that 404'd, so the next request hits the
    // direct path and needs no sibling resolution to find it.
    writeFileSync(dest, bytes);
    marker.fetched[path] = src;
    delete marker.unavailable[path];
    got++;
    console.log(`  ✓ ${path}  (${(bytes.length / 1024).toFixed(0)}KB from ${new URL(src).host})`);
  } catch (e) {
    gone++;
    marker.unavailable[path] = { at: Date.now(), why: String(e.message).slice(0, 80) };
    console.log(`  ✗ ${path}  — ${e.message}`);
  }
}

try { writeFileSync(MARKER, JSON.stringify(marker, null, 1)); } catch (_) {}
console.log(`refetched ${got}, unavailable ${gone}`);
