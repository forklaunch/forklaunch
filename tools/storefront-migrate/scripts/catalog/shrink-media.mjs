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
 *
 * IDEMPOTENT, and that is not a nicety. This runs inside a repair loop that
 * may call it several times in one migration. h264 is lossy: a file that has
 * already been scaled to 720p and re-encoded at 1400k loses quality on every
 * further pass while saving almost nothing, so a loop that ran this four times
 * shipped visibly mushy video and called it a success. Two independent guards
 * stop that:
 *
 *   1. the file's real height, straight from the container — the ground truth,
 *      and correct even if every marker on disk is deleted;
 *   2. a marker beside the assets recording what was already shrunk, keyed by
 *      byte size so a file replaced by a later capture is not mistaken for one
 *      that has already been processed.
 *
 * The marker is the fast path (ffprobe on 24 files is not free); the height
 * check is the one that cannot be wrong.
 */
import { execFileSync } from 'node:child_process';
import { readdirSync, statSync, renameSync, unlinkSync, openSync, readSync, closeSync,
         existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
if (!dir) { console.error('usage: shrink-media.mjs <site-dir> [--height 720] [--dry]'); process.exit(2); }
const heightFlag = (process.argv.find((a) => a.startsWith('--height=')) || '').split('=')[1] ||
  (process.argv.includes('--height') ? process.argv[process.argv.indexOf('--height') + 1] : undefined);
const HEIGHT = Number(heightFlag) > 0 ? Number(heightFlag) : 720;
const DRY = process.argv.includes('--dry');

// The H.264 encoder depends on the ffmpeg build in front of us: videotoolbox
// is hardware-backed on Apple silicon; libx264 is the portable one; neither
// is guaranteed. Probed once, on the first file that needs transcoding, and
// a build with no H.264 encoder fails the run loudly instead of per-file.
let ENCODER = null;
function pickEncoder() {
  if (ENCODER) return ENCODER;
  let listed;
  try { listed = execFileSync('ffmpeg', ['-hide_banner', '-encoders'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }); }
  catch { throw new Error('ffmpeg cannot list its encoders'); }
  const names = [...listed.matchAll(/^\s*V\S*\s+(\S+)/gm)].map((m) => m[1]);
  const preferred = process.platform === 'darwin' ? ['h264_videotoolbox', 'libx264'] : ['libx264', 'h264_videotoolbox'];
  ENCODER = preferred.find((n) => names.includes(n)) || names.find((n) => /264/.test(n)) || null;
  if (!ENCODER) throw new Error('this ffmpeg build has no H.264 encoder (libx264 or h264_videotoolbox)');
  return ENCODER;
}

/** The capture writes unclassified assets as .bin, so extension proves nothing. */
function isMp4(fp) {
  try {
    const b = Buffer.alloc(12);
    const fd = openSync(fp, 'r');
    try { readSync(fd, b, 0, 12, 0); } finally { closeSync(fd); }
    return b.slice(4, 8).toString('latin1') === 'ftyp';
  } catch { return false; }
}

/** Ground truth from the container. The page cannot be asked, and the marker
 *  can be stale — this can be neither. */
function probeHeight(fp) {
  try {
    const out = execFileSync('ffprobe', ['-v', 'error', '-select_streams', 'v:0',
      '-show_entries', 'stream=height', '-of', 'csv=p=0', fp], { encoding: 'utf8' });
    return Number(out.trim().split('\n')[0]) || 0;
  } catch { return 0; }
}

const assets = join(dir, '_a');
const MARKER = join(assets, '.fl-shrunk.json');
let marker = {};
try { if (existsSync(MARKER)) marker = JSON.parse(readFileSync(MARKER, 'utf8')); } catch (_) {}
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
let before = 0, after = 0, done = 0, skipped = 0, failed = 0, alreadyDone = 0;

console.log(`shrink-media: ${targets.length} video file(s) under ${assets}\n`);

for (const fp of targets) {
  const size = statSync(fp).size;
  before += size;

  // Anything already small is left alone; re-encoding it would only lose
  // quality for no meaningful saving.
  if (size < 1_500_000) { after += size; skipped++; continue; }

  // Already done. Size-keyed: a recapture that replaced this content-addressed
  // name with different bytes must NOT inherit the old file's marker.
  const key = fp.slice(assets.length + 1);
  const mark = marker[key];
  if (mark && mark.size === size && mark.height <= HEIGHT) { after += size; alreadyDone++; continue; }

  // The check that cannot be wrong. A 720p file re-encoded at 720p is pure
  // quality loss, so height alone is enough to refuse the work — and it holds
  // even on a capture whose marker was never written or was deleted.
  const h = probeHeight(fp);
  if (h && h <= HEIGHT) {
    marker[key] = { size, height: h, at: Date.now() };
    after += size; alreadyDone++; continue;
  }
  if (DRY) { console.log(`  would shrink  ${mb(size)}MB  ${fp.split('/').pop()}`); after += size; continue; }

  const tmp = fp + '.shrunk.mp4';
  const encoder = pickEncoder();
  try {
    execFileSync('ffmpeg', [
      '-y', '-loglevel', 'error', '-i', fp,
      // Only ever scale down: -2 keeps the width even, which h264 requires.
      '-vf', `scale=-2:'min(${HEIGHT},ih)'`,
      '-c:v', encoder, '-b:v', '1400k', '-maxrate', '1800k', '-bufsize', '3000k',
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
    marker[fp.slice(assets.length + 1)] = { size: newSize, height: HEIGHT, at: Date.now() };
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

if (!DRY) { try { writeFileSync(MARKER, JSON.stringify(marker, null, 1)); } catch (_) {} }

// `already at or below <h>p` is reported separately from `left alone`. In a
// loop, "0 shrunk" on the second pass is the CORRECT outcome, and a summary
// that cannot distinguish it from "nothing matched" makes a working repair
// look like a broken one.
console.log(`\n  ${mb(before)}MB -> ${mb(after)}MB   (${done} shrunk, ${alreadyDone} already at or below ${HEIGHT}p, ${skipped} left alone, ${failed} failed)`);
