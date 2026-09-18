#!/usr/bin/env node
/**
 * check-prereqs — refuse to start without the tools the pipeline silently
 * needs.
 *
 *   node check-prereqs.mjs [--need playwright,bun,ffmpeg,python]
 *
 * Three of this pipeline's dependencies do not self-install, and every one of
 * them fails SILENTLY rather than loudly. That combination is what makes them
 * worth a dedicated gate:
 *
 *   playwright's chromium   the gates are Playwright scripts. With no browser
 *                           they emit NOTHING — and a loop reading their output
 *                           sees no failures, which is indistinguishable from
 *                           no problems. This one cost an afternoon: the
 *                           storefront was declared to need a human when it
 *                           was one `npx` command from green. The npm package
 *                           being installed proves nothing; the ~150MB browser
 *                           build is a separate download, so this checks for
 *                           the executable on disk.
 *
 *   bun                     runs the catalog pipeline and heroserve-fl. Without
 *                           it manifest.json comes back `catalog: null` and the
 *                           storefront never serves — with no error that names
 *                           bun as the cause.
 *
 *   ffmpeg / ffprobe        shrink-media transcodes with ffmpeg, and
 *                           check-budget reads video height with ffprobe.
 *                           Missing, shrink-media reports every file as
 *                           "FAILED (left as-is)" and check-budget's
 *                           videoHeight() returns 0 — which compares as
 *                           "0 <= 720" and PASSES. A budget gate that passes
 *                           because it cannot measure is worse than no gate,
 *                           because it is believed.
 *
 * The pattern in all three: the absence of a tool reads as the absence of a
 * problem. Checking up front, by name, with the command that fixes it, is the
 * cheapest possible answer to that.
 *
 * Exit 0 if everything needed is present, 2 (a HARNESS failure, matching the
 * gates' own convention) if not.
 */
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';

/**
 * Each probe returns null when satisfied, or a string saying what is wrong.
 * `fix` is a command that can be pasted, not a description of one.
 */
export const PREREQS = {
  playwright: {
    why: 'every gate is a Playwright script; without a browser they produce no output at all',
    fix: 'cd ' + new URL('.', import.meta.url).pathname + ' && npx playwright install chromium',
    async probe() {
      let chromium;
      try { ({ chromium } = await import('playwright')); }
      catch { return 'the playwright package is not installed (npm i inside scripts/)'; }
      // executablePath() answers from the registry without launching anything,
      // so this stays fast and cannot itself hang.
      let p;
      try { p = chromium.executablePath(); } catch (e) { return 'playwright cannot locate a chromium build: ' + e.message; }
      if (!p || !existsSync(p)) return `chromium is not downloaded (expected at ${p || 'the playwright cache'})`;
      return null;
    }
  },
  bun: {
    why: 'runs the catalog pipeline and heroserve-fl (the storefront server the gates measure)',
    fix: 'curl -fsSL https://bun.sh/install | bash',
    async probe() {
      return resolveBun() ? null : 'bun is not on PATH and not at ~/.bun/bin/bun';
    }
  },
  ffmpeg: {
    why: 'shrink-media transcodes with ffmpeg; check-budget reads video height with ffprobe, and returns 0 without it — which silently PASSES the height budget',
    fix: 'brew install ffmpeg',
    async probe() {
      const missing = ['ffmpeg', 'ffprobe'].filter((b) => !which(b));
      return missing.length ? `${missing.join(' and ')} not on PATH` : null;
    }
  },
  python: {
    why: 'serve.py is the fallback static server',
    fix: 'install Python 3',
    async probe() { return which('python3') ? null : 'python3 not on PATH'; }
  }
};

function which(bin) {
  // execFileSync without a shell: passing args through `shell: true` concatenates
  // them unescaped, and node warns about it for good reason.
  try { return execFileSync('/usr/bin/which', [bin], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim() || null; }
  catch { return null; }
}

/** bun is usually at ~/.bun/bin/bun, which is NOT on a spawned process's PATH. */
export function resolveBun() {
  const candidates = [
    process.env.BUN_PATH,
    process.env.BUN_INSTALL ? `${process.env.BUN_INSTALL}/bin/bun` : null,
    `${process.env.HOME}/.bun/bin/bun`,
    '/opt/homebrew/bin/bun', '/usr/local/bin/bun'
  ].filter(Boolean);
  for (const c of candidates) if (existsSync(c)) return c;
  return which('bun');
}

/**
 * Check the named prerequisites. Returns the list of what is missing rather
 * than exiting, so a caller can decide whether a given absence is fatal for
 * the work it is about to do — a capture-only run genuinely does not need bun.
 */
export async function checkPrereqs(need = Object.keys(PREREQS)) {
  const missing = [];
  for (const name of need) {
    const spec = PREREQS[name];
    if (!spec) continue;
    const problem = await spec.probe();
    if (problem) missing.push({ name, problem, why: spec.why, fix: spec.fix });
  }
  return missing;
}

/** Print and exit 2. Shared so every entrypoint refuses identically. */
export function refuse(missing) {
  console.error('\n\x1b[31mHARNESS-FAIL\x1b[0m — missing prerequisites. Nothing has run.\n');
  for (const m of missing) {
    console.error(`  \x1b[1m${m.name}\x1b[0m — ${m.problem}`);
    console.error(`     needed for: ${m.why}`);
    console.error(`     fix:        ${m.fix}\n`);
  }
  console.error('These do not self-install, and each one fails SILENTLY: the gates would');
  console.error('produce no output, which reads as "no failures" rather than "did not run".');
  console.error('Refusing to start is the only honest option.\n');
  process.exit(2);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const i = process.argv.indexOf('--need');
  const need = i >= 0 ? process.argv[i + 1].split(',') : Object.keys(PREREQS);
  const missing = await checkPrereqs(need);
  if (missing.length) refuse(missing);
  console.log(`✓ prerequisites present: ${need.join(', ')}`);
}
