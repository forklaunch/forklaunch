#!/usr/bin/env node
/**
 * shrink-media — transcode a capture's video down to something a normal
 * machine can actually play.
 *
 *   node shrink-media.mjs <site-dir> [--height 720] [--dry]
 *
 * Storefronts ship hero video at the resolution their CDN can afford, which
 * is 1080p at 5-7 Mbps. graza.co's capture is 24 files and 220MB of it. The
 * file size is the smaller half of the problem: a 1080p stream decodes to
 * roughly 8MB per frame of raw surface, and the browser holds a queue of
 * them, so two or three of these playing at once costs hundreds of megabytes
 * of RAM that never shows up in a page-weight number. On an 8GB laptop with
 * other work open, opening such a page is not slow — it is fatal.
 *
 * Downscaling to 720p at a capped bitrate keeps background b-roll looking
 * the same at the size it is actually displayed (none of these fill a 1080p
 * viewport) while cutting both the bytes and the decode surface by roughly an
 * order of magnitude.
 *
 * Files are transcoded in place, keeping the capture's content-addressed
 * names so nothing that references them has to change. Anything that fails to
 * transcode is left exactly as it was: a slightly heavy video is a bad demo,
 * a corrupted one is a broken page.
 */
import { execFileSync } from 'node:child_process';
import { readdirSync, statSync, renameSync, unlinkSync, openSync, readSync, closeSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
if (!dir) { console.error('usage: shrink-media.mjs <site-dir> [--height 720] [--dry]'); process.exit(2); }
const HEIGHT = Number((process.argv.find((a) => a.startsWith('--height=')) || '').split('=')[1]) ||
  (process.argv.includes('--height') ? Number(process.argv[process.argv.indexOf('--height') + 1]) : 720);
const DRY = process.argv.includes('--dry');

/** The capture writes unclassified assets as .bin, so extension proves nothing. */
function isMp4(fp) {
  try {
    const b = Buffer.alloc(12);
    const fd = openSync(fp, 'r');
    try { readSync(fd, b, 0, 12, 0); } finally { closeSync(fd); }
    return b.slice(4, 8).toString('latin1') === 'ftyp';
  } catch { return false; }
}

const assets = join(dir, '_a');
const targets = [];
for (const sub of ['other', 'video', 'img']) {
  let names = [];
  try { names = readdirSync(join(assets, sub)); } catch { continue; }
  for (const n of names) {
    const fp = join(assets, sub, n);
    try { if (statSync(fp).isFile() && isMp4(fp)) targets.push(fp); } catch {}
  }
}

const mb = (b) => (b / 1048576).toFixed(1);
let before = 0, after = 0, done = 0, skipped = 0, failed = 0;

console.log(`shrink-media: ${targets.length} video file(s) under ${assets}\n`);

for (const fp of targets) {
  const size = statSync(fp).size;
  before += size;

  // Anything already small is left alone; re-encoding it would only lose
  // quality for no meaningful saving.
  if (size < 1_500_000) { after += size; skipped++; continue; }
  if (DRY) { console.log(`  would shrink  ${mb(size)}MB  ${fp.split('/').pop()}`); after += size; continue; }

  const tmp = fp + '.shrunk.mp4';
  try {
    execFileSync('ffmpeg', [
      '-y', '-loglevel', 'error', '-i', fp,
      // Only ever scale down: -2 keeps the width even, which h264 requires.
      '-vf', `scale=-2:'min(${HEIGHT},ih)'`,
      // videotoolbox is hardware-backed on Apple silicon and keeps this from
      // pinning every core, which matters when the machine is already loaded.
      '-c:v', 'h264_videotoolbox', '-b:v', '1400k', '-maxrate', '1800k', '-bufsize', '3000k',
      '-c:a', 'aac', '-b:a', '96k',
      // Metadata at the front so the browser can start playing without
      // fetching the whole file first.
      '-movflags', '+faststart',
      tmp
    ], { stdio: ['ignore', 'ignore', 'pipe'] });

    const newSize = statSync(tmp).size;
    // Refuse a "shrink" that grew, which happens on already-efficient files.
    if (newSize >= size) { unlinkSync(tmp); after += size; skipped++; continue; }
    renameSync(tmp, fp);          // keep the content-addressed name
    after += newSize;
    done++;
    console.log(`  ${mb(size).padStart(6)}MB -> ${mb(newSize).padStart(6)}MB  ${fp.split('/').pop().slice(0, 52)}`);
  } catch (e) {
    try { unlinkSync(tmp); } catch {}
    after += size;
    failed++;
    console.log(`  FAILED (left as-is)  ${fp.split('/').pop().slice(0, 52)}`);
  }
}

console.log(`\n  ${mb(before)}MB -> ${mb(after)}MB   (${done} shrunk, ${skipped} left alone, ${failed} failed)`);
