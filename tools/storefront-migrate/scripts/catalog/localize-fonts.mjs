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
 *
 * IDEMPOTENT. This runs inside a repair loop, and a font URL that cannot be
 * fetched is left in place on purpose (a working remote reference beats a
 * local 404) — which means the very next pass finds it again, tries again,
 * fails again, and reports "fetched 0, failed 8". Three rounds of that is
 * ninety seconds of timeouts and a summary that reads like a broken repair
 * when nothing is broken at all: those eight URLs are simply gone from the
 * internet. A marker records what was already fetched and what is already
 * known unreachable, so a second pass costs nothing and says so plainly.
 *
 * Unreachable is remembered with a timestamp rather than forever. A CDN having
 * a bad minute is not the same as a font that no longer exists, and a
 * permanent blacklist would turn the first into the second.
 */
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, extname, basename } from 'node:path';

/** How long an unreachable font stays remembered as unreachable. Long enough
 *  to cover a whole migration, short enough that a CDN blip is not permanent. */
const UNREACHABLE_TTL_MS = 6 * 60 * 60 * 1000;

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

const fontDir = join(SITE, '_a', 'font');
const MARKER = join(SITE, '_a', '.fl-fonts.json');
let marker = { fetched: {}, unreachable: {} };
try { if (existsSync(MARKER)) marker = { fetched: {}, unreachable: {}, ...JSON.parse(readFileSync(MARKER, 'utf8')) }; } catch (_) {}

// Anything already tried and found gone is not retried this round. This is the
// whole difference between "fetched 0, failed 8" on every pass and one honest
// line saying eight faces are unavailable at source.
const now = Date.now();
const known = [...urls].filter((u) => marker.unreachable[u] && now - marker.unreachable[u].at < UNREACHABLE_TTL_MS);
// Already stored, and the file is still on disk. A font URL can survive a
// successful fetch — it appears in a file the rewrite pass did not reach, or
// in one written after it — so the reference still needs rewriting even though
// the bytes are already here. Re-downloading them to learn that would be pure
// waste; reusing the recorded path gets the rewrite without the network.
const reused = new Map();
for (const u of urls) {
  const prior = marker.fetched[u];
  if (prior && !known.includes(u) && existsSync(join(SITE, prior.replace(/^\//, '')))) reused.set(u, prior);
}
const todo = [...urls].filter((u) => !known.includes(u) && !reused.has(u));

console.log(`${urls.size} external font URL(s) in ${texts.size} file(s)` +
  (known.length ? ` — ${known.length} already known unreachable, skipping` : ''));
if (!todo.length && !reused.size) {
  if (known.length) console.log(`  ${known.length} face(s) are gone at source; the capture keeps the remote reference so they load if the CDN returns.`);
  else console.log('  nothing to do — every font is already local');
  process.exit(0);
}

mkdirSync(fontDir, { recursive: true });

const localFor = new Map(reused);
let got = 0;
let failed = 0;

for (const u of todo) {
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
    marker.fetched[u] = `/_a/font/${name}`;
    delete marker.unreachable[u];
    got++;
  } catch (e) {
    // A font that cannot be fetched is left pointing at its origin rather than
    // rewritten to a 404: a working remote reference beats a broken local one.
    // Remembered so the next pass in the loop does not pay for the timeout
    // again.
    failed++;
    marker.unreachable[u] = { at: Date.now(), why: e.message.slice(0, 80) };
    console.warn(`  could not fetch ${u.slice(0, 80)} — ${e.message}`);
  }
}

try { writeFileSync(MARKER, JSON.stringify(marker, null, 1)); } catch (_) {}

let edits = 0;
for (const [f, s] of texts) {
  let next = s;
  for (const [u, local] of localFor) next = next.split(u).join(local);
  if (next !== s) {
    writeFileSync(f, next);
    edits++;
  }
}

console.log(`fetched ${got}, reused ${reused.size} already local, unavailable at source ${failed}, ` +
  `${known.length} already known unreachable, rewrote ${edits} file(s) -> /_a/font/`);
