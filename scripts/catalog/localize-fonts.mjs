#!/usr/bin/env node
/**
 * localize-fonts — pull a capture's remaining webfonts onto the local host.
 *
 *   node localize-fonts.mjs <site-dir>
 *
 * A capture rewrites images, scripts and stylesheets to local paths, but
 * fonts hide one level deeper: they are named inside @font-face blocks in the
 * CSS, not in any HTML attribute, so an href-based rewrite walks straight past
 * them. The pages then look right only while the original CDN is reachable.
 *
 * That is not a cosmetic gap. Fall back from a brand's own typeface and you do
 * not merely lose the look — you change every text metric on the page, and
 * headers that fit at the real font's width start colliding. It reads as a
 * broken site rather than a restyled one.
 *
 * So: find every absolute font URL the capture still points at, fetch it,
 * store it content-addressed beside the other assets, and rewrite the
 * references. After this the storefront renders identically with no external
 * host reachable at all.
 */
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { join, extname, basename } from 'node:path';

const SITE = process.argv[2];
if (!SITE) {
  console.error('usage: localize-fonts.mjs <site-dir>');
  process.exit(2);
}

const FONT_URL = /https:\/\/[A-Za-z0-9.-]+\/[^"')\s]*?\.(?:woff2|woff|ttf|otf)(?:\?[^"')\s]*)?/g;
// Only rewrite files that are actually text. A capture stores CSS and JS under
// .bin, so extension is not a reliable signal — decode and look.
const isText = (buf) => {
  const head = buf.subarray(0, 4096);
  for (const b of head) if (b === 0) return false;
  return true;
};

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

const files = walk(SITE);
const urls = new Set();
const texts = new Map();

for (const f of files) {
  const buf = readFileSync(f);
  if (!isText(buf)) continue;
  const s = buf.toString('utf8');
  const found = s.match(FONT_URL);
  if (!found) continue;
  texts.set(f, s);
  for (const u of found) urls.add(u);
}

console.log(`${urls.size} external font URL(s) in ${texts.size} file(s)`);
if (!urls.size) process.exit(0);

const fontDir = join(SITE, '_a', 'font');
mkdirSync(fontDir, { recursive: true });

const localFor = new Map();
let got = 0;
let failed = 0;

for (const u of urls) {
  try {
    const res = await fetch(u, { redirect: 'follow' });
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const bytes = Buffer.from(await res.arrayBuffer());
    // Content-addressed, matching how the rest of the capture is named: the
    // URL can then be cached forever because it can never change meaning.
    const hash = createHash('sha1').update(bytes).digest('hex').slice(0, 10);
    const clean = basename(new URL(u).pathname).replace(/[^A-Za-z0-9._-]/g, '');
    const ext = extname(clean) || '.woff2';
    const name = `${clean.replace(ext, '')}.${hash}${ext}`;
    writeFileSync(join(fontDir, name), bytes);
    localFor.set(u, `/_a/font/${name}`);
    got++;
  } catch (e) {
    // A font that cannot be fetched is left pointing at its origin rather than
    // rewritten to a 404: a working remote reference beats a broken local one.
    failed++;
    console.warn(`  could not fetch ${u.slice(0, 80)} — ${e.message}`);
  }
}

let edits = 0;
for (const [f, s] of texts) {
  let next = s;
  for (const [u, local] of localFor) next = next.split(u).join(local);
  if (next !== s) {
    writeFileSync(f, next);
    edits++;
  }
}

console.log(`fetched ${got}, failed ${failed}, rewrote ${edits} file(s) -> /_a/font/`);
