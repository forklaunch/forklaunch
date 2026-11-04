#!/usr/bin/env node
/**
 * check-budget — the gate that would have caught the crash.
 *
 *   node check-budget.mjs [--store http://localhost:4180]
 *                         [--site output/graza.co/site]
 *                         [--paths /,/products/<handle>]
 *
 * A migrated storefront is somebody else's site, and it arrives with whatever
 * that site could afford to ship from a CDN. graza.co's capture held 220MB of
 * video across 24 files, every one of them 1080p at 4.8-7.2 Mbps.
 *
 * Twice, opening that page took down a laptop. Not from running out of memory
 * in the obvious way — macOS killed WindowServer, the compositor, because its
 * main thread had been unresponsive for 40 seconds. The relevant number is not
 * the file size but the decode surface: a 1080p frame is roughly 8MB of raw
 * pixels, the browser keeps a queue of them, and several videos at once will
 * saturate the GPU pipeline the window server shares.
 *
 * check-wired.mjs proves the storefront is bridged to the module.
 * check-purchase.mjs proves it takes money. Neither looks at what the page
 * costs to render, so both stayed green through every crash. This one asserts
 * the things that actually hurt:
 *
 *   video resolution      capped at the source, because no page-weight number
 *                         reveals decode cost
 *   page weight           what a shopper on a normal connection pays
 *   broken local assets   a 404 on our own origin means the capture and the
 *                         page disagree; 172 of them shipped silently once
 *   external hosts        a captured storefront carries the original site's
 *                         ad stack, which must not phone home from a demo
 *   concurrent video      how many elements could decode at once
 *
 * Exits non-zero if any budget is exceeded.
 */
import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
import { readdirSync, statSync, openSync, readSync, closeSync } from 'node:fs';
import { join } from 'node:path';

const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const STORE = arg('--store', 'http://localhost:4180').replace(/\/$/, '');
const SITE = arg('--site', '');
const PATHS = arg('--paths', '').split(',').filter(Boolean);

// Budgets. Deliberately generous — this is a crash guard, not a performance
// score. Anything over these is a page that has gone wrong, not one that is
// merely heavy.
// Overridable so the gate can be run against a deliberately failing control.
// A checker nobody has ever seen fail is not evidence of anything, and these
// assertions are all "count came back small", which is exactly the shape that
// silently passes when the measurement breaks.
const num = (flag, fallback) => {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? Number(process.argv[i + 1]) : fallback;
};
const MAX_VIDEO_HEIGHT = num('--max-video-height', 720);
const MAX_PAGE_MB = num('--max-page-mb', 12);
const MAX_BROKEN_ASSETS = num('--max-broken', 0);
const MAX_EXTERNAL_HOSTS = num('--max-external', 0);
const MAX_VIDEO_ELEMENTS = num('--max-video-elements', 12);

// Stripe and PayPal are deliberately reachable; a demo that cannot reach the
// payment processor cannot take a payment.
const PAYMENT_HOST = /(^|\.)stripe\.(com|network)$|(^|\.)paypal(objects)?\.com$/;

const results = [];
const check = (name, pass, detail = '') => results.push({ name, pass: !!pass, detail });

function isMp4(fp) {
  try {
    const b = Buffer.alloc(12);
    const fd = openSync(fp, 'r');
    try { readSync(fd, b, 0, 12, 0); } finally { closeSync(fd); }
    return b.slice(4, 8).toString('latin1') === 'ftyp';
  } catch { return false; }
}

/** Resolution straight from the container — the page cannot be asked. */
function videoHeight(fp) {
  try {
    const out = execFileSync('ffprobe', ['-v', 'error', '-select_streams', 'v:0',
      '-show_entries', 'stream=height', '-of', 'csv=p=0', fp], { encoding: 'utf8' });
    return Number(out.trim().split('\n')[0]) || 0;
  } catch { return 0; }
}

async function main() {
  // ---- the capture on disk ------------------------------------------------
  if (SITE) {
    const offenders = [];
    let count = 0, bytes = 0;
    for (const sub of ['other', 'video', 'img']) {
      let names = [];
      try { names = readdirSync(join(SITE, '_a', sub)); } catch { continue; }
      for (const n of names) {
        const fp = join(SITE, '_a', sub, n);
        let st;
        try { st = statSync(fp); } catch { continue; }
        if (!st.isFile() || !isMp4(fp)) continue;
        count++; bytes += st.size;
        const h = videoHeight(fp);
        // Encoders round to macroblocks: a 731p file IS the 720p rendition.
        if (h > MAX_VIDEO_HEIGHT * 1.03) offenders.push(`${n.slice(0, 40)} ${h}p`);
      }
    }
    check(`no captured video above ${MAX_VIDEO_HEIGHT}p`, offenders.length === 0,
      offenders.length
        ? `${offenders.length} of ${count} too tall: ${offenders.slice(0, 3).join(', ')}`
        : `${count} file(s), ${(bytes / 1048576).toFixed(1)}MB total`);
  }

  // ---- the pages ----------------------------------------------------------
  const browser = await chromium.launch({ headless: true });
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();

  // Discover a product page if none was named, so the check covers the two
  // page types a shopper actually loads.
  let paths = PATHS;
  if (!paths.length) {
    await page.goto(STORE + '/', { waitUntil: 'load', timeout: 60000 });
    await page.waitForTimeout(3000);
    const p = await page.evaluate(() => {
      const a = [...document.querySelectorAll('a[href]')]
        .map((x) => x.getAttribute('href'))
        .find((h) => h && /products\//.test(h) && !/^https?:/.test(h));
      return a ? (a.startsWith('/') ? a : '/' + a) : null;
    });
    paths = ['/', p].filter(Boolean);
  }

  for (const path of paths) {
    let bytes = 0;
    const broken = [];
    const external = new Set();

    const onResp = (r) => {
      bytes += Number(r.headers()['content-length'] || 0);
      const host = new URL(r.url()).host;
      const localish = /^localhost|^127\.0\.0\.1/.test(host);
      // Only our own origin's failures count as broken assets. A tracker
      // endpoint rewritten to a local path 404s harmlessly and is not a
      // missing file — filtering by the asset directory keeps this honest.
      if (localish && r.status() >= 400 && /\/_a\/(js|css|img|fonts?|ext)\//.test(new URL(r.url()).pathname)) {
        broken.push(new URL(r.url()).pathname);
      }
    };
    const onFinished = (r) => {
      const host = new URL(r.url()).host;
      if (/^localhost|^127\.0\.0\.1/.test(host) || PAYMENT_HOST.test(host)) return;
      external.add(host);
    };
    page.on('response', onResp);
    page.on('requestfinished', onFinished);

    await page.goto(STORE + path, { waitUntil: 'load', timeout: 60000 });

    // --self-test proves the two zero-valued detectors actually detect.
    // "Zero broken assets" and "zero external hosts" pass whether the page is
    // clean or the measurement is dead, and those are the assertions most
    // worth trusting. Injecting one of each makes the gate demonstrate it can
    // see them before anyone relies on a green run.
    if (process.argv.includes('--self-test')) {
      await page.evaluate(() => Promise.all([
        fetch('/_a/js/deliberately-missing-control.js').catch(() => {}),
        fetch('https://example.com/control-beacon').catch(() => {})
      ]));
    }

    await page.waitForTimeout(6000);

    const media = await page.evaluate(() => {
      const v = [...document.querySelectorAll('video')];
      return {
        elements: v.length,
        playing: v.filter((x) => !x.paused && !x.ended).length,
        // A video with a real src is one the browser may fetch and decode.
        sourced: v.filter((x) => x.currentSrc || x.src).length
      };
    });

    page.off('response', onResp);
    page.off('requestfinished', onFinished);

    const label = path === '/' ? 'home' : path;
    check(`${label}: under ${MAX_PAGE_MB}MB`, bytes / 1048576 <= MAX_PAGE_MB,
      `${(bytes / 1048576).toFixed(1)}MB`);
    check(`${label}: no broken local assets`, broken.length <= MAX_BROKEN_ASSETS,
      broken.length ? `${broken.length}: ${broken.slice(0, 2).join(', ')}` : 'none');
    check(`${label}: nothing phoned home`, external.size <= MAX_EXTERNAL_HOSTS,
      external.size ? `reached ${[...external].slice(0, 4).join(', ')}` : 'no external host completed a request');
    check(`${label}: video elements within budget`, media.elements <= MAX_VIDEO_ELEMENTS,
      `${media.elements} element(s), ${media.sourced} with a source, ${media.playing} playing`);
  }

  await browser.close();

  const failed = results.filter((r) => !r.pass);
  console.log(`\ncheck-budget — ${STORE}\n`);
  for (const r of results) {
    console.log(`  ${r.pass ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? '   (' + r.detail + ')' : ''}`);
  }
  console.log(`\n${results.length - failed.length}/${results.length} passed\n`);
  process.exit(failed.length ? 1 : 0);
}

// HARNESS-FAIL, not FAIL: the repair loop must be able to tell "the gate broke"
// from "the capture is broken". Reading the first as the second sent a good
// storefront round a repair loop it did not need; reading it as a pass shipped
// a broken one.
main().catch((e) => { console.error('\nHARNESS-FAIL: check-budget crashed — ' + e.message); process.exit(2); });
