#!/usr/bin/env node
/**
 * import-dom — feed an externally captured page into a --complete capture.
 *
 *   node import-dom.mjs <capture-dir> <page-url-or-path> < page.html
 *
 * The assisted-capture half of the skill: when a store rate-limits or
 * bot-walls the headless crawler, the STORE OWNER opens their own site in
 * their real browser and Claude (via the Claude-in-Chrome extension) reads
 * each rendered page's DOM and pipes it here. The page lands in the same
 * .raw/<file> checkpoint the crawler itself writes, so a re-run of
 * `crawl.js <domain> <dir> --complete` resumes, treats it as captured, and
 * runs the normal rewrite/serve pipeline over it. check-complete.mjs then
 * verifies coverage exactly as if the crawler had fetched it.
 *
 * Consent boundary (do not remove): this exists for owner-consented
 * migrations — the owner browsing their own storefront — and for pages the
 * owner has deliberately opened (login-gated areas). It is not a tool for
 * defeating the anti-bot measures of stores we have no relationship with;
 * the crawler's polite stop-when-walled behavior remains the rule there.
 *
 * Known limitation (v1): pages imported this way keep absolute asset URLs
 * (images/css load from the live CDN) — the crawler's asset-interception
 * pass only sees pages it fetched itself. The clone stays fully browsable;
 * full asset localization for assisted pages is a follow-up.
 */
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
const { pageFileFor } = createRequire(import.meta.url)('./urlmap.js');

const [dir, urlArg] = process.argv.slice(2);
if (!dir || !urlArg) {
  console.error('usage: node import-dom.mjs <capture-dir> <page-url-or-path> < page.html');
  process.exit(1);
}

let pagePath;
try { pagePath = new URL(urlArg).pathname; } catch { pagePath = urlArg; }
if (!pagePath.startsWith('/')) pagePath = '/' + pagePath;

const mapped = pageFileFor(pagePath);
if (!mapped) {
  console.error(`refusing: ${pagePath} maps to no capture file (cart/checkout/account and file-ish paths are excluded by design)`);
  process.exit(2);
}

const html = fs.readFileSync(0, 'utf8');
if (!html || html.length < 500 || !/<html[\s>]/i.test(html)) {
  console.error(`refusing: stdin does not look like a rendered HTML document (${html.length} bytes)`);
  process.exit(3);
}

const rawFile = path.join(dir, '.raw', mapped.file);
fs.mkdirSync(path.dirname(rawFile), { recursive: true });
fs.writeFileSync(rawFile, html);
console.log(`imported ${pagePath} -> .raw/${mapped.file} (${html.length} bytes)`);
