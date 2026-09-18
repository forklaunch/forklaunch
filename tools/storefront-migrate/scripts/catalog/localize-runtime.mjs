#!/usr/bin/env node
/**
 * localize-runtime — pull every subresource a captured storefront still fetches
 * from the internet onto the local host, so the replica renders identically
 * with no external host reachable.
 *
 *   node localize-runtime.mjs <site-dir> <base-url> [--pages /,/products/x.html]
 *
 * Why this exists, and why it is not a grep:
 *
 * A crawl rewrites what it can see in the markup. It cannot see what the page
 * asks for once it is running — a stylesheet that names a font, a theme script
 * that lazy-loads another script, an image URL assembled in JavaScript. Those
 * stay pointed at the original CDN, and the replica then looks right only for
 * as long as that CDN answers. Two things go wrong the moment it does not:
 *
 *   - the brand's typeface falls back, which changes every text metric on the
 *     page. Headers that fit at the real width begin to collide. It reads as
 *     a broken site rather than a restyled one.
 *   - the theme's own behaviour stops. Shopify serves a theme's JavaScript
 *     from the same CDN as its trackers, so anything that blocks the trackers
 *     by host also kills the code that paints selected states, opens drawers
 *     and swaps variants. The page looks finished and does nothing.
 *
 * So rather than guess from the markup, drive the real pages in a browser and
 * record what they actually request. Whatever comes back and is not a tracker
 * gets stored beside the rest of the capture and rewritten to a local path.
 *
 * Trackers are the one thing deliberately left behind: a demo must not beacon
 * to the original merchant's analytics from a client's machine. They are
 * excluded here rather than blocked later, so the replica needs no policy at
 * runtime and cannot be broken by one.
 *
 * IDEMPOTENT. Inside a repair loop the browser pass has to run every time —
 * the point is to see what the page requests NOW, after the last repair — but
 * the network half does not. A marker records every URL already stored and
 * every URL already found dead, so the second pass re-drives the pages, finds
 * that everything it discovered is accounted for, rewrites nothing, and exits
 * in seconds instead of re-downloading a theme's whole bundle. It also stops a
 * dead CDN reference from costing a fresh round of timeouts on every round.
 */
import { chromium } from 'playwright';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, extname, basename } from 'node:path';

const SITE = process.argv[2];
const BASE = (process.argv[3] || 'http://localhost:4180').replace(/\/$/, '');
const pagesArg = process.argv.indexOf('--pages');
if (!SITE) {
  console.error('usage: localize-runtime.mjs <site-dir> <base-url> [--pages a,b,c]');
  process.exit(2);
}

/**
 * Analytics, advertising and session-replay. Everything here is left pointing
 * at its origin and will simply fail to load, which is the intent — none of it
 * paints a pixel. Matched against the hostname, suffix-wise.
 */
const TRACKERS = [
  'google-analytics.com', 'googletagmanager.com', 'doubleclick.net', 'googleadservices.com',
  'googlesyndication.com', 'google.com/pagead', 'facebook.net', 'facebook.com', 'connect.facebook.net',
  'sentry.io', 'klaviyo.com', 'hotjar.com', 'fullstory.com', 'segment.io', 'segment.com',
  'adsrvr.org', 'adnxs.com', 'pubmatic.com', 'applovin.com', 'dstillery.com', 'bidr.io',
  'criteo.com', 'taboola.com', 'outbrain.com', 'tiktok.com', 'snapchat.com', 'pinterest.com',
  'bing.com', 'clarity.ms', 'amplitude.com', 'mixpanel.com', 'heap.io', 'postscript.io',
  'attentivemobile.com', 'shopifysvc.com', 'monorail-edge.shopifysvc.com', 'roeye.com',
  'roeyecdn.com', 'awin.com', 'awin1.com', 'superfiliate.com', 'superfiliate-cdn.com',
  'config-security.com', 'vaultdcr.com', 'visually-io.com', 'alia-prod.com', 'consentmo.com',
  'digismoothie.app', 'media6degrees.com', 'oloiyb.net', 'shop.app', 'instagram.com',
  'twitter.com', 'x.com', 'youtube.com', 'vimeo.com'
];
const isTracker = (host) =>
  TRACKERS.some((t) => host === t || host.endsWith('.' + t));

// Only subresources. A document or a beacon is not something to store.
const WANTED = new Set(['script', 'stylesheet', 'font', 'image', 'media']);

const isText = (buf) => {
  const head = buf.subarray(0, 4096);
  for (const b of head) if (b === 0) return false;
  return true;
};

/**
 * Walk the capture's files.
 *
 * Skips this tool's own marker files, and that is not tidiness — it is a bug
 * fix. The rewrite pass below replaces every external URL it finds in every
 * text file under the capture. The marker is a text file under the capture,
 * and its KEYS are external URLs. So the first run happily rewrote its own
 * memory into `{"/_a/ext/foo.js": "/_a/ext/foo.js"}` — after which no lookup
 * could ever match, every later round re-downloaded all 80 subresources, and
 * the summary said "reused 0 already local" on a capture where everything was
 * already local. An idempotence marker that destroys itself is worse than none.
 */
function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name.startsWith('.fl-')) continue;
    const p = join(dir, name);
    statSync(p).isDirectory() ? walk(p, out) : out.push(p);
  }
  return out;
}

/** Representative pages: home, a product, a collection. Enough to exercise
 *  the theme's shared chrome plus the two templates that differ most. */
async function choosePages(page) {
  if (pagesArg > 0) {
    const list = String(process.argv[pagesArg + 1] || '').split(',').map((s) => s.trim()).filter(Boolean);
    if (!list.length) { console.error('--pages needs a comma-separated list of paths'); process.exit(2); }
    return list;
  }
  await page.goto(BASE + '/', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(3000);
  const found = await page.evaluate(() => {
    const hrefs = [...document.querySelectorAll('a[href]')].map((a) => a.getAttribute('href') || '');
    const pick = (re) => hrefs.find((h) => re.test(h) && !/^https?:/.test(h));
    return [pick(/products\//), pick(/collections\//)].filter(Boolean);
  });
  return ['/', ...found.map((h) => (h.startsWith('/') ? h : '/' + h))];
}

const browser = await chromium.launch({ headless: true, args: ['--disable-dev-shm-usage', '--js-flags=--max-old-space-size=1536'] });
// bypassCSP so this run sees what the page wants, not what a policy allows.
const ctx = await browser.newContext({ bypassCSP: true, viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();

const external = new Map();   // url -> resourceType
ctx.on('requestfinished', (r) => {
  const u = r.url();
  let host;
  try { host = new URL(u).host; } catch { return; }
  if (/^localhost|^127\.0\.0\.1/.test(host)) return;
  if (!/^https?:/.test(u)) return;
  if (!WANTED.has(r.resourceType())) return;
  if (isTracker(host)) return;
  if (!external.has(u)) external.set(u, r.resourceType());
});

const pages = await choosePages(page);
console.log('exercising:', pages.join(', '));
for (const path of pages) {
  await page.goto(BASE + path, { waitUntil: 'load', timeout: 60000 }).catch(() => {});
  await page.waitForTimeout(5000);
  // Scroll the whole page so lazy-loaded assets actually get requested.
  await page.evaluate(async () => {
    for (let y = 0; y < document.body.scrollHeight; y += window.innerHeight) {
      window.scrollTo(0, y);
      await new Promise((r) => setTimeout(r, 250));
    }
    window.scrollTo(0, 0);
  }).catch(() => {});
  await page.waitForTimeout(3000);
}
await browser.close();

console.log(`${external.size} external subresource(s) to localize`);
if (!external.size) process.exit(0);

const outDir = join(SITE, '_a', 'ext');
mkdirSync(outDir, { recursive: true });

const MARKER = join(SITE, '_a', '.fl-runtime.json');
let marker = { fetched: {}, dead: {} };
try { if (existsSync(MARKER)) marker = { fetched: {}, dead: {}, ...JSON.parse(readFileSync(MARKER, 'utf8')) }; } catch (_) {}
/** Same reasoning as localize-fonts: remembered, not blacklisted forever. */
const DEAD_TTL_MS = 6 * 60 * 60 * 1000;

const EXT_FOR = { script: '.js', stylesheet: '.css', font: '.woff2', image: '.img', media: '.bin' };
const localFor = new Map();
let got = 0, failed = 0;

let reused = 0, knownDead = 0;
for (const [u, type] of external) {
  // Already stored, and the file is still there. Re-fetching it would produce
  // byte-identical content under an identical content-addressed name.
  const prior = marker.fetched[u];
  if (prior && existsSync(join(SITE, prior.replace(/^\//, '')))) {
    localFor.set(u, prior);
    reused++;
    continue;
  }
  if (marker.dead[u] && Date.now() - marker.dead[u].at < DEAD_TTL_MS) { knownDead++; continue; }
  try {
    const res = await fetch(u, { redirect: 'follow', signal: AbortSignal.timeout(15000) });
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const bytes = Buffer.from(await res.arrayBuffer());
    const hash = createHash('sha1').update(bytes).digest('hex').slice(0, 10);
    const clean = basename(new URL(u).pathname).replace(/[^A-Za-z0-9._-]/g, '') || 'asset';
    const ext = extname(clean) || EXT_FOR[type] || '.bin';
    const name = `${clean.replace(new RegExp(ext.replace('.', '\\.') + '$'), '')}.${hash}${ext}`;
    writeFileSync(join(outDir, name), bytes);
    localFor.set(u, `/_a/ext/${name}`);
    marker.fetched[u] = `/_a/ext/${name}`;
    delete marker.dead[u];
    got++;
  } catch (e) {
    // Left pointing at its origin: a working remote reference beats a local 404.
    failed++;
    marker.dead[u] = { at: Date.now(), why: String(e.message).slice(0, 80) };
  }
}

try { writeFileSync(MARKER, JSON.stringify(marker, null, 1)); } catch (_) {}

// Rewrite references. Both the absolute URL and its protocol-relative form
// appear in captured CSS and JS.
let edits = 0;
for (const f of walk(SITE)) {
  let buf;
  try { buf = readFileSync(f); } catch { continue; }
  if (!isText(buf)) continue;
  let s = buf.toString('utf8');
  const before = s;
  for (const [u, local] of localFor) {
    if (!s.includes(u) && !s.includes(u.replace(/^https:/, ''))) continue;
    s = s.split(u).join(local).split(u.replace(/^https:/, '')).join(local);
  }
  if (s !== before) { writeFileSync(f, s); edits++; }
}

// Reporting `reused` separately matters in a loop: on the second round the
// right answer is "fetched 0" and a summary that cannot say why looks like a
// repair that stopped working.
console.log(`fetched ${got}, reused ${reused} already local, ` +
  `${knownDead} already known dead, failed ${failed}, rewrote ${edits} file(s) -> /_a/ext/`);
// Nothing new and nothing rewritten is the steady state. Say so, so the loop's
// operator can see the repair has converged rather than stalled.
if (!got && !edits) console.log('  converged — every subresource this page requests is already local');
