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
 */
import { chromium } from 'playwright';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync } from 'node:fs';
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

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    statSync(p).isDirectory() ? walk(p, out) : out.push(p);
  }
  return out;
}

/** Representative pages: home, a product, a collection. Enough to exercise
 *  the theme's shared chrome plus the two templates that differ most. */
async function choosePages(page) {
  if (pagesArg > 0) return process.argv[pagesArg + 1].split(',');
  await page.goto(BASE + '/', { waitUntil: 'load', timeout: 60000 });
  await page.waitForTimeout(3000);
  const found = await page.evaluate(() => {
    const hrefs = [...document.querySelectorAll('a[href]')].map((a) => a.getAttribute('href') || '');
    const pick = (re) => hrefs.find((h) => re.test(h) && !/^https?:/.test(h));
    return [pick(/products\//), pick(/collections\//)].filter(Boolean);
  });
  return ['/', ...found.map((h) => (h.startsWith('/') ? h : '/' + h))];
}

const browser = await chromium.launch();
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

const EXT_FOR = { script: '.js', stylesheet: '.css', font: '.woff2', image: '.img', media: '.bin' };
const localFor = new Map();
let got = 0, failed = 0;

for (const [u, type] of external) {
  try {
    const res = await fetch(u, { redirect: 'follow' });
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const bytes = Buffer.from(await res.arrayBuffer());
    const hash = createHash('sha1').update(bytes).digest('hex').slice(0, 10);
    const clean = basename(new URL(u).pathname).replace(/[^A-Za-z0-9._-]/g, '') || 'asset';
    const ext = extname(clean) || EXT_FOR[type] || '.bin';
    const name = `${clean.replace(new RegExp(ext.replace('.', '\\.') + '$'), '')}.${hash}${ext}`;
    writeFileSync(join(outDir, name), bytes);
    localFor.set(u, `/_a/ext/${name}`);
    got++;
  } catch (e) {
    // Left pointing at its origin: a working remote reference beats a local 404.
    failed++;
  }
}

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

console.log(`fetched ${got}, failed ${failed}, rewrote ${edits} file(s) -> /_a/ext/`);
