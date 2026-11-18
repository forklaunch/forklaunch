#!/usr/bin/env node
/**
 * finish — turn a raw capture into a verified storefront, without supervision.
 *
 *   node bin/finish.mjs <site-dir> [--live https://www.store.com] [--port 4180]
 *                       [--module <url>] [--stripe-pk <key>]
 *                       (module secret: HMAC_SECRET_KEY in the environment)
 *                       [--rounds 5] [--budget-min 30] [--offline]
 *                       [--out <dir>] [--serve-after]
 *
 * The capture step produces a directory of files. Everything between that and
 * "a storefront you can put in front of someone" used to be a checklist a
 * person ran by hand: shrink the video, localise the fonts, pull whatever the
 * page fetches at runtime, serve it, run the gates, read the failures, apply
 * the matching fix, run the gates again.
 *
 * That checklist is the reason migrations could not be one-shot. Every step
 * existed and worked; nothing chained them, and the failures they catch are
 * silent — a broken capture serves 200s and looks fine, so skipping a step
 * costs you nothing until a client is looking at the screen.
 *
 * WHAT "DONE" MEANS HERE
 *
 * Not a round count and not a percentage. Done is: the feature gate's list of
 * missing or non-functional features is EMPTY. check-features.mjs builds that
 * list by enumerating what the live storefront has and requiring the clone to
 * have it too, so the target moves with the merchant's site rather than with
 * a number somebody picked.
 *
 * --rounds and --budget-min are crash guards, not goals. They exist so a
 * pathological store cannot spin here forever. Hitting either is reported as
 * "ran out of budget with N features still missing" — never as success.
 *
 * NEVER ACCEPT A ROUND THAT MADE THINGS WORSE
 *
 * Repairs rewrite the capture in place. A repair that fixes two things and
 * breaks three is a net loss, and without a check it compounds over rounds.
 * Every round snapshots the capture's text assets first (which is where all
 * the destructive edits happen — the binary repairs only ever shrink, and are
 * height-guarded so they cannot run twice), and a round whose score got worse
 * is rolled back and reported rather than built upon.
 *
 * A GATE THAT CANNOT RUN IS NOT A GATE THAT PASSED
 *
 * This is the failure that motivated the rewrite. Playwright's browser was not
 * installed, the gates produced no assertions at all, and the loop read "no
 * assertions" as "nothing matched a repair" and declared the storefront needed
 * a person. It was one npx command from green. Every gate now exits 2 and
 * prints a line starting HARNESS-FAIL when it could not run, and this loop
 * exits 2 immediately on either signal — a completely different exit code from
 * a real fidelity failure, so nothing downstream can conflate them.
 *
 * Exit codes:
 *   0  every live feature present and responding
 *   1  features still missing after the budget ran out
 *   2  harness failure — the gates could not run; nothing was proved
 */
import { spawn, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
const { pageFileFor } = createRequire(import.meta.url)('../urlmap.js');
import { closeSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, readSync,
         rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkPrereqs, refuse, resolveBun } from '../check-prereqs.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPTS = resolve(HERE, '..');

const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const has = (n) => process.argv.includes(n);

const SITE = resolve(process.argv[2] || '');
const PORT = arg('--port', '4180');
const MODULE_URL = arg('--module', '');
if (process.argv.includes('--hmac')) { console.error('--hmac is not accepted: export HMAC_SECRET_KEY instead, so the secret never appears in a process listing.'); process.exit(2); }
const HMAC = process.env.HMAC_SECRET_KEY || '';
const STRIPE_PK = arg('--stripe-pk', '');
const PAYPAL_ID = arg('--paypal-id', '');
const ROUNDS = Number(arg('--rounds', '5'));
const BUDGET_MS = Number(arg('--budget-min', '30')) * 60_000;
const OFFLINE = has('--offline');
const SERVE_AFTER = has('--serve-after');
const OUT = resolve(arg('--out', dirname(SITE)));
const BASE = `http://localhost:${PORT}`;
let DEADLINE = Date.now() + BUDGET_MS;
// The budget must scale with the work the gate names. A store with a 105-link
// menu asked for a 31-page recapture inside a 30-minute window and the loop
// killed it, then reported round 1 as the verdict. When a recapture is chosen,
// the deadline grows to fit it (about 50s a page plus a gate), under a hard
// cap so a pathological store still ends.
const HARD_CAP = Date.now() + Math.max(BUDGET_MS, 150 * 60_000);

if (!SITE || !existsSync(SITE)) {
  console.error('usage: finish.mjs <site-dir> [--live <url>] [--port 4180] [--module <url>] [--offline]');
  process.exit(2);
}

const log = (s) => console.log(s);
const step = (s) => console.log(`\n\x1b[1m── ${s}\x1b[0m`);
const bail = (msg, hint) => {
  log(`\n\x1b[31mHARNESS-FAIL\x1b[0m — the gates could not run, so NOTHING was proved about this capture:`);
  log(`  ${msg}`);
  if (hint) log(`  fix: ${hint}`);
  stopServer();
  process.exit(2);
};

/**
 * The live storefront this capture came from — the feature gate's oracle.
 * manifest.json records it, so a normal run needs no flag; --live overrides,
 * and --offline runs the structural half of the gate without it.
 */
function resolveLive() {
  const explicit = arg('--live', '');
  if (explicit) return explicit.replace(/\/$/, '');
  for (const cand of [join(OUT, 'manifest.json'), join(dirname(SITE), 'manifest.json')]) {
    try {
      const m = JSON.parse(readFileSync(cand, 'utf8'));
      const u = m?.source?.url || (m?.source?.domain ? `https://${m.source.domain}` : null);
      if (u) return u.replace(/\/$/, '');
    } catch (_) {}
  }
  return null;
}
const LIVE = resolveLive();
if (!LIVE && !OFFLINE) {
  bail('no live storefront URL: none passed with --live and none in manifest.json',
       'pass --live https://the-store.com, or --offline to check the clone structurally');
}

function run(cmd, args, opts = {}) {
  const { timeoutMs, ...spawnOpts } = opts;
  return new Promise((res) => {
    // A timed-out child is killed by PROCESS GROUP, not pid: crawl.js owns a
    // headless Chromium, and killing only node would orphan it (the stray
    // browser processes that have pinned this laptop before).
    const p = spawn(cmd, args, { cwd: SCRIPTS, stdio: ['ignore', 'pipe', 'pipe'],
                                 detached: !!timeoutMs, ...spawnOpts });
    let out = '';
    let timer = null;
    if (timeoutMs) {
      timer = setTimeout(() => {
        out += `\n[finish] killed after ${Math.round(timeoutMs / 1000)}s — the wall-clock budget ran out mid-repair\n`;
        try { process.kill(-p.pid, 'SIGKILL'); } catch { try { p.kill('SIGKILL'); } catch {} }
      }, timeoutMs);
    }
    p.stdout.on('data', (d) => { out += d; });
    p.stderr.on('data', (d) => { out += d; });
    p.on('close', (code) => { if (timer) clearTimeout(timer); res({ code, out }); });
    p.on('error', (e) => { if (timer) clearTimeout(timer); res({ code: -1, out: String(e) }); });
  });
}

async function waitForServer(timeoutMs = 45000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(BASE + '/', { signal: AbortSignal.timeout(4000) });
      if (r.ok) return true;
    } catch {}
    await new Promise((s) => setTimeout(s, 1000));
  }
  return false;
}

// ---- serving --------------------------------------------------------------
// heroserve-fl, not the plain static server: the gates measure the storefront
// as it will actually be shown, and the bridge shim, the content-addressed
// sibling resolution and the media lightening are all part of that. Gating a
// different server than the one that ships proves nothing about the one that
// does.
let server = null;
function startServer() {
  const bun = resolveBun() || `${process.env.HOME}/.bun/bin/bun`;
  const args = [join(SCRIPTS, 'catalog/heroserve-fl.ts'), SITE, PORT];
  // The server takes everything but the site and port from its environment:
  // the HMAC secret must never be in an argv, and the two public keys follow
  // it there for consistency.
  if (MODULE_URL) args.push(MODULE_URL);
  server = spawn(bun, args, { cwd: SCRIPTS, stdio: ['ignore', 'pipe', 'pipe'], detached: false,
                              env: { ...process.env,
                                     ...(HMAC ? { HMAC_SECRET_KEY: HMAC } : {}),
                                     ...(STRIPE_PK ? { STRIPE_PUBLISHABLE_KEY: STRIPE_PK } : {}),
                                     ...(PAYPAL_ID ? { PAYPAL_CLIENT_ID: PAYPAL_ID } : {}) } });
  let boot = '';
  server.stdout.on('data', (d) => { boot += d; });
  server.stderr.on('data', (d) => { boot += d; });
  server.on('exit', (code) => {
    // heroserve refuses to start if the injected shim does not parse. That is
    // a harness failure of the loudest kind and must not be read as "the
    // storefront is broken" — every page would be inert and every gate would
    // fail for the same wrong reason.
    if (code && code !== 0 && !stopping) {
      console.error(`\n\x1b[31mHARNESS-FAIL\x1b[0m — the storefront server exited ${code}:`);
      console.error(boot.trim().split('\n').slice(-8).join('\n'));
      process.exit(2);
    }
  });
}
let stopping = false;
function stopServer() { stopping = true; try { server?.kill(); } catch {} stopping = false; }
async function restartServer() {
  stopping = true;
  try { server?.kill(); } catch {}
  await new Promise((s) => setTimeout(s, 1200));
  stopping = false;
  startServer();
  return waitForServer();
}
process.on('exit', () => { stopping = true; try { server?.kill(); } catch {} });
process.on('SIGINT', () => { stopping = true; try { server?.kill(); } catch {} process.exit(130); });

// ---- rollback -------------------------------------------------------------
// Only the text assets are snapshotted, and that is not a shortcut. Every
// destructive repair — the reference rewriting in localize-runtime and
// localize-fonts — edits text. The binary repair only ever transcodes
// downward and now refuses to touch a file already at or below the target
// height, so it cannot degrade something twice. Snapshotting 90MB of video on
// every round to guard a monotone operation would cost more disk than the
// capture itself, on a machine that does not have it.
const SNAP = join(OUT, '.fl-rollback.tgz');
const SNAP_LIST = join(OUT, '.fl-rollback.list');

/**
 * Which files a repair could destroy: every TEXT file in the capture.
 *
 * Extension is not the test, and that has bitten before. The crawl writes
 * unclassified assets as `.bin`, so a theme's main stylesheet and its webpack
 * bundle both live under `_a/other/*.bin` — and those are precisely the files
 * localize-fonts and localize-runtime rewrite references inside. A snapshot
 * filtered by `*.css,*.js` would look complete and restore none of them.
 * Decoding the first few KB and looking for a NUL is the same test the
 * repairs themselves use, so the two can never disagree about what is text.
 */
function textFiles(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    let st; try { st = statSync(p); } catch { continue; }
    if (st.isDirectory()) { textFiles(p, out); continue; }
    try {
      const b = Buffer.alloc(4096);
      const fd = openSync(p, 'r');
      let n = 0;
      try { n = readSync(fd, b, 0, 4096, 0); } finally { closeSync(fd); }
      let bin = false;
      for (let i = 0; i < n; i++) if (b[i] === 0) { bin = true; break; }
      if (!bin) out.push(p.slice(SITE.length + 1));
    } catch (_) {}
  }
  return out;
}

function snapshot() {
  try {
    writeFileSync(SNAP_LIST, textFiles(SITE).join('\n') + '\n');
    // -T because bsdtar's --include does not filter at creation time; a tar
    // built with it comes out empty, which is a silent way to have no backup.
    const r = spawnSync('tar', ['-czf', SNAP, '-C', SITE, '-T', SNAP_LIST], { encoding: 'utf8' });
    return r.status === 0 && existsSync(SNAP) && statSync(SNAP).size > 1024;
  } catch (_) { return false; }
}
function rollback() {
  if (!existsSync(SNAP)) return false;
  return spawnSync('tar', ['-xzf', SNAP, '-C', SITE], { encoding: 'utf8' }).status === 0;
}

// ---- gates ----------------------------------------------------------------
const FEATURES_JSON = join(OUT, 'features.json');
const LIVE_CACHE = join(OUT, 'feature-inventory.json');

/**
 * One route per template the capture actually holds.
 *
 * Taken from pages.json rather than by following links off the homepage. The
 * crawl writes relative hrefs, headless themes build some of their links in
 * JavaScript, and a homepage that happens to link to no collection would
 * silently narrow the gate to the homepage alone — which still reports a clean
 * pass having checked a third of the storefront. The capture's own page list
 * cannot lie about what it captured.
 */
function routesFromCapture() {
  for (const cand of [join(OUT, 'pages.json'), join(dirname(SITE), 'pages.json')]) {
    try {
      const pages = JSON.parse(readFileSync(cand, 'utf8'));
      const pick = (t) => pages.find((p) => p.type === t)?.route;
      const routes = ['/', pick('collection'), pick('product'), pick('page')].filter(Boolean);
      if (routes.length > 1) return [...new Set(routes)];
    } catch (_) {}
  }
  return null;   // let the gate discover them from the served homepage
}
const ROUTES = routesFromCapture();

async function runGates() {
  const featArgs = [join(SCRIPTS, 'check-features.mjs'), '--clone', BASE,
    // Routes re-captured this run: a template mismatch that survives one is
    // the store assigning templates per visitor, which the gate then names.
    ...(recapturedThisRun.size ? ['--recaptured', [...recapturedThisRun].join(',')] : []),
    '--json', FEATURES_JSON, '--cache', LIVE_CACHE];
  if (ROUTES) featArgs.push('--routes', ROUTES.join(','));
  if (OFFLINE) featArgs.push('--offline'); else featArgs.push('--live', LIVE);
  const features = await run('node', featArgs);
  const wired = await run('node', [join(SCRIPTS, 'check-wired.mjs'), BASE]);
  const budget = await run('node', [join(SCRIPTS, 'check-budget.mjs'), '--store', BASE, '--site', SITE]);
  return { features, wired, budget };
}

/**
 * Turn three gate runs into one verdict.
 *
 * `harness` is checked before anything else and short-circuits everything.
 * Exit code 2 and the HARNESS-FAIL marker are both accepted, so a gate that
 * dies before it can print its marker is still caught by its code.
 */
function verdict({ features, wired, budget }) {
  const all = [features, wired, budget];
  const text = all.map((g) => g.out).join('\n');
  const harness = all.filter((g) => g.code === 2 || /HARNESS-FAIL/.test(g.out));
  if (harness.length) return { harness: true, text };

  const lines = text.split('\n').filter((l) => /^\s+(PASS|FAIL|SKIP)\s/.test(l));
  // No assertions at all is the shape that fooled the previous loop. Even with
  // clean exit codes, a gate run that proved nothing is a harness failure.
  if (!lines.length) return { harness: true, text, why: 'the gates produced no assertions' };

  let missing = [], policy = [];
  try {
    const j = JSON.parse(readFileSync(FEATURES_JSON, 'utf8'));
    missing = j.missing || [];
    policy = j.policy || [];
  } catch (_) {}

  const failLines = lines.filter((l) => /^\s+FAIL/.test(l));
  return {
    harness: false, text, lines, failLines, missing, policy,
    // The score the rollback comparison uses. Missing features are weighted
    // above raw assertion failures because one FAIL line can stand for twenty
    // missing sections, and a repair that trades one for the other is not an
    // improvement.
    score: missing.length * 10 + failLines.length,
    green: failLines.length === 0 && missing.length === 0
  };
}

// ---- repairs --------------------------------------------------------------
// Two ways in. The feature gate names its own repair per defect, which is the
// precise path. The pattern rules below catch what the older gates report,
// which the feature gate has no vocabulary for (page weight, decode budget).
const REPAIR_CMD = {
  'localize-runtime': () => ['node', [join(SCRIPTS, 'catalog/localize-runtime.mjs'), SITE, BASE]],
  'localize-fonts': () => ['node', [join(SCRIPTS, 'catalog/localize-fonts.mjs'), SITE]],
  'fix-script-hrefs': () => ['node', [join(SCRIPTS, 'catalog/fix-script-hrefs.mjs'), SITE]],
  'shrink-media': () => ['node', [join(SCRIPTS, 'catalog/shrink-media.mjs'), SITE, '--height', '720']],
  'shrink-media-hard': () => ['node', [join(SCRIPTS, 'catalog/shrink-media.mjs'), SITE, '--height', '540']],
  'rediscover-controls': () => ['node', [join(SCRIPTS, 'discover-controls.mjs'), SITE, BASE]],
  /**
   * A 404 on OUR OWN origin: the crawl rewrote a reference and never fetched
   * the file, which happens with lazily-imported chunks the crawl's own visit
   * never triggered. localize-runtime cannot answer it — that script only
   * looks at requests to other hosts, and this one goes to localhost — so
   * until this existed the defect was reported every round against a repair
   * that could not possibly fix it.
   */
  'refetch-missing': () => ['node', [join(SCRIPTS, 'catalog/refetch-missing.mjs'), SITE, BASE, LIVE || '']],
  /**
   * The answer to a dead internal link: the page was never captured.
   *
   * TARGETED: the gate names the dead paths, so the re-crawl fetches exactly
   * those pages (`--only`) and adds them to the capture — four footer links
   * cost four page loads, about a minute each. The whole-sitemap walk
   * (`--complete`) runs only when a recapture is wanted without any path to
   * name, and it is bounded by the wall-clock budget like every repair. That
   * walk used to be the ONLY answer, and on graza.co it meant 693 blog posts
   * (six hours, and a sustained load on the merchant's origin) to fix four
   * links — then the loop noticed the budget had passed and exited 1 anyway.
   *
   * Still opt-in (--allow-recapture): `migrate.mjs` passes it because it is
   * already crawling that store with the operator's intent; `finish.mjs` by
   * hand on someone else's capture does not get to re-crawl a merchant unasked.
   *
   * Reported either way. A dead link is never silently accepted.
   */
  recapture: () => pendingRecapture.length
    ? ['node', [join(SCRIPTS, 'crawl.js'), DOMAIN, dirname(SITE), '--only', pendingRecapture.join(','), '--clean']]
    : ['node', [join(SCRIPTS, 'crawl.js'), DOMAIN, dirname(SITE), '--complete', '--clean']]
};
// The dead internal paths the last gate named — set by chooseRepairs, read by
// REPAIR_CMD.recapture above.
let pendingRecapture = [];
// A page re-captured once this run cannot gain anything from a second
// identical fetch; the loop re-captured olipop's homepage four times for
// review quotes a recapture can never produce.
const recapturedThisRun = new Set();
const ALLOW_RECAPTURE = has('--allow-recapture');
const DOMAIN = (LIVE || '').replace(/^https?:\/\//, '').replace(/\/.*$/, '').replace(/^www\./, '');

// Repairs that exist only as a description. Naming them is the point: a defect
// with no automated answer must be reported as such, never silently dropped
// into "no repair matched" where it looks like the loop simply gave up.
const NO_AUTO_REPAIR = {
  recapture: 'the page or control was never captured — needs a re-crawl (node crawl.js <domain> <out> --only </path,...>), ' +
             'or assisted capture if the store is bot-walled',
  null: 'no automated repair — see the assertion text'
};

const PATTERN_REPAIRS = [
  { name: 'shrink-media-hard', when: (t) => /FAIL.*(under \d+MB|video elements within budget)/i.test(t) },
  { name: 'localize-runtime', when: (t) => /FAIL.*(broken local assets|phoned home|dead asset)/i.test(t) },
  { name: 'rediscover-controls', when: (t) => /FAIL.*(checkout|cart drawer|cart responds)/i.test(t) }
];

function chooseRepairs(v) {
  const wanted = new Set();
  for (const m of v.missing) if (m.repair && REPAIR_CMD[m.repair]) wanted.add(m.repair);
  // Every path a recapture should fetch: a dead destination names the path
  // itself; a section, landmark, control, variant picker or blank page names
  // the ROUTE it was missing from, and re-capturing that page is the repair.
  // Without the second half, a badly-captured product page produced an empty
  // list and fell through to the whole-sitemap walk.
  pendingRecapture = [...new Set(v.missing
    .filter((m) => m.repair === 'recapture')
    .map((m) => (m.kind === 'nav' ? String(m.what).split(' -> ')[0] : String(m.where || '')).trim())
    .filter((p) => p.startsWith('/'))
    // /cart, /account, /search… are never captured (urlmap SKIP_PATH): the
    // server answers them itself. Naming them here burned two rounds on graza.
    .filter((p) => p === '/' || !!pageFileFor(p))
    .filter((p) => !recapturedThisRun.has(p)))];
  if (wanted.has('recapture') && !pendingRecapture.length && v.missing.some((m) => m.repair === 'recapture')) {
    // Everything a recapture could reach has been re-captured already.
    wanted.delete('recapture');
    log('  · the pages a re-crawl could fix were already re-captured this run — not repeating it');
  }
  for (const r of PATTERN_REPAIRS) if (r.when(v.text)) wanted.add(r.name);
  if (wanted.has('recapture') && (!ALLOW_RECAPTURE || !DOMAIN)) {
    wanted.delete('recapture');
    const n = v.missing.filter((m) => m.repair === 'recapture').length;
    log(`  · ${n} defect(s) need a re-crawl; not running one unasked` +
        (DOMAIN ? ` — pass --allow-recapture, or: node crawl.js ${DOMAIN} ${dirname(SITE)} --only ${pendingRecapture.join(',') || '</path,...>'} --clean` : ''));
  }
  return [...wanted];
}

// ---------------------------------------------------------------------------
(async () => {
  log(`\x1b[1mfinish\x1b[0m — ${SITE}`);
  log(`  live oracle: ${LIVE || '(offline — structural checks only)'}`);
  log(`  budget: ${ROUNDS} round(s), ${Math.round(BUDGET_MS / 60000)} min`);
  log(`  routes: ${ROUTES ? ROUTES.join(', ') : '(discovered from the served homepage)'}`);
  mkdirSync(OUT, { recursive: true });

  // Before anything else, and before the first gate can be misread. All three
  // of these fail silently rather than loudly, and this loop's entire job is
  // to distinguish "did not run" from "found nothing wrong".
  {
    const missing = await checkPrereqs(['playwright', 'bun', 'ffmpeg']);
    if (missing.length) refuse(missing);
  }

  // These two are always right and always an improvement, so they run before
  // the first gate rather than in response to a failure — which also means the
  // first gate result describes the storefront as it will actually ship, not
  // an intermediate state. Both are idempotent now, so re-running them in a
  // later round costs seconds and degrades nothing.
  step('repairs that are always right');
  for (const name of ['fix-script-hrefs', 'shrink-media', 'localize-fonts']) {
    const [cmd, args] = REPAIR_CMD[name]();
    const { code, out } = await run(cmd, args);
    log(`  ${code === 0 ? '✓' : '!'} ${name}  ${(out.trim().split('\n').filter(Boolean).pop() || '').slice(0, 100)}`);
  }

  step('serving');
  startServer();
  if (!(await waitForServer())) bail('the storefront never came up', `check bun is installed and ${SITE} is a capture`);
  log(`  ✓ up at ${BASE}`);

  // Per-capture control discovery, before the first gate. The bridge's
  // checkout interception reads what this writes; running it after the gates
  // would mean gating a bridge that is not the one that ships.
  step('discovering this theme\'s controls');
  {
    const { code, out } = await run('node', [join(SCRIPTS, 'discover-controls.mjs'), SITE, BASE]);
    for (const l of out.trim().split('\n').filter(Boolean).slice(-3)) log('  ' + l.trim());
    if (code !== 0) log('  ! discovery failed — the bridge falls back to name matching alone');
    // Restart so heroserve picks up _fl-controls.json; it is read once at boot.
    if (!(await restartServer())) bail('the storefront did not come back after control discovery');
  }

  let prev = null;
  let last = null;
  let lastChosen = null;

  for (let round = 1; round <= ROUNDS; round++) {
    if (Date.now() > DEADLINE) {
      log(`\n  wall-clock budget spent after ${round - 1} round(s)`);
      break;
    }
    step(`verify — round ${round} of ${ROUNDS}  (${Math.round((DEADLINE - Date.now()) / 60000)} min left)`);

    const v = verdict(await runGates());
    if (v.harness) {
      const first = v.text.split('\n').find((l) => /HARNESS-FAIL/.test(l)) || v.why || 'a gate exited 2';
      let hint = null;
      if (/playwright|Executable doesn't exist|browserType.launch/i.test(v.text)) hint = 'npx playwright install chromium';
      if (/rate limit|429|403|bot/i.test(v.text)) hint = 'the live store is throttling — retry later, or run with --offline';
      bail(first.replace(/^.*HARNESS-FAIL:?\s*/, ''), hint);
    }
    last = v;

    for (const l of v.lines) log('  ' + l.trim());
    if (v.missing.length) {
      log(`\n  ${v.missing.length} missing or non-functional feature(s):`);
      for (const m of v.missing.slice(0, 12)) log(`    · [${m.kind}] ${String(m.what).slice(0, 96)}`);
      if (v.missing.length > 12) log(`    · … and ${v.missing.length - 12} more`);
    }

    if (v.green) {
      log(`\n\x1b[32m✓ complete feature fidelity in ${round} round(s)\x1b[0m — ${BASE}`);
      if (v.policy.length) {
        const vendors = [...new Set(v.policy.map((m) => m.policy))];
        log(`  (${v.policy.length} third-party feature(s) deliberately not migrated: ${vendors.join(', ')} —`);
        log(`   their data lives in those vendors' databases and their scripts are blocked by policy)`);
      }
      rmSync(SNAP, { force: true });
      rmSync(SNAP_LIST, { force: true });
      if (SERVE_AFTER) { log('  (leaving the storefront serving — Ctrl-C to stop)'); await new Promise(() => {}); }
      stopServer();
      process.exit(0);
    }

    // Never build on a round that made things worse.
    if (prev !== null && v.score > prev) {
      log(`\n  \x1b[31mthat round made it worse\x1b[0m (score ${prev} -> ${v.score}) — rolling the capture back`);
      const ok = rollback();
      log(`  ${ok ? '✓ rolled back to the previous state' : '! rollback failed — the capture is as the last repair left it'}`);
      await restartServer();
      break;
    }
    const chosen = chooseRepairs(v);
    if (!chosen.length) {
      log('\n  no automated repair answers what is left — stopping rather than looping');
      break;
    }

    // Converged: the same repairs, and the score did not move. Every repair
    // here is idempotent by design, so running the identical set again cannot
    // produce a different answer — it would just spend the remaining rounds
    // and the wall-clock budget proving that. Stopping now leaves both for a
    // storefront that can still use them, and reports the truth: these
    // defects are past what the automated repairs reach.
    if (prev !== null && v.score === prev && lastChosen === chosen.join(',')) {
      log(`\n  the repairs have converged — ${chosen.join(', ')} ran again and changed nothing`);
      break;
    }
    lastChosen = chosen.join(',');
    prev = v.score;

    if (chosen.includes('recapture') && pendingRecapture.length) {
      const need = Date.now() + pendingRecapture.length * 50_000 + 8 * 60_000;
      if (need > DEADLINE) {
        const to = Math.min(need, HARD_CAP);
        log(`  budget extended by ${Math.round((to - DEADLINE) / 60000)} min for a ${pendingRecapture.length}-page recapture`);
        DEADLINE = to;
      }
    }
    step(`repair — round ${round}: ${chosen.join(', ')}`);
    if (chosen.includes('recapture')) {
      log(pendingRecapture.length
        ? `  (re-capturing ${pendingRecapture.length} page(s) the gate named: ${pendingRecapture.slice(0, 4).join(', ')}${pendingRecapture.length > 4 ? ', …' : ''})`
        : '  (no path to name — walking the whole sitemap, bounded by the budget; this is the slow one)');
    }
    if (!snapshot()) log('  ! could not snapshot for rollback; this round is not reversible');
    for (const name of chosen) {
      const [cmd, args] = REPAIR_CMD[name]();
      if (name === 'recapture') for (const p of pendingRecapture) recapturedThisRun.add(p);
      // No repair outlives the budget. Without this bound a recapture ran to
      // completion and only THEN did the loop notice the deadline had passed.
      const timeoutMs = Math.max(60_000, DEADLINE - Date.now());
      const { code, out } = await run(cmd, args, { timeoutMs });
      const tail = out.trim().split('\n').filter(Boolean).slice(-2).join(' · ');
      // A repair that exits 0 but reports an item it could not fix (✗) is not
      // a success; a ✓ beside a ✗ misled a blank agent on taylorstitch.com.
      const failed = code !== 0 || /✗/.test(tail);
      log(`  ${failed ? '!' : '✓'} ${name}  ${tail.slice(0, 110)}`);
      if (failed) {
        // Two lines of a stack trace are not an error message. Keep the whole
        // output where a person can read it and say where it went.
        const lf = join(OUT, `repair-${name}-round${round}.log`);
        try { writeFileSync(lf, out); log(`    full output: ${lf}`); } catch {}
      }
    }
    // A recapture writes pages the always-right repairs have never seen. Both
    // are idempotent, so this costs seconds and brings the new pages to the
    // same state the first gate measured everything else in.
    if (chosen.includes('recapture')) {
      for (const name of ['fix-script-hrefs', 'shrink-media', 'localize-fonts']) {
        const [cmd, args] = REPAIR_CMD[name]();
        const { code, out } = await run(cmd, args);
        log(`  ${code === 0 ? '✓' : '!'} ${name}  ${(out.trim().split('\n').filter(Boolean).pop() || '').slice(0, 100)}`);
      }
    }
    if (!(await restartServer())) bail('the storefront did not come back after repair');
  }

  // ---- what is left, and whose problem it is -------------------------------
  log('\n\x1b[31m✗ did not reach complete feature fidelity.\x1b[0m Remaining:');
  if (last?.policy?.length) {
    // Say what was deliberately NOT migrated before listing what is missing,
    // so a reader can tell the two apart — the manual promises this bucket.
    const vendors = [...new Set(last.policy.map((m) => m.policy))];
    log(`  (${last.policy.length} third-party feature(s) not migrated by policy — ${vendors.join(', ')} — these are not failures)`);
  }
  for (const l of (last?.failLines || [])) log('  ' + l.trim());
  const byRepair = {};
  for (const m of last?.missing || []) (byRepair[m.repair || 'null'] ||= []).push(m);
  for (const [repair, items] of Object.entries(byRepair)) {
    log(`\n  ${items.length} × ${repair === 'null' ? 'no automated repair' : repair}` +
        (NO_AUTO_REPAIR[repair] ? ` — ${NO_AUTO_REPAIR[repair]}` : ''));
    for (const m of items.slice(0, 8)) log(`    · [${m.kind}] ${String(m.what).slice(0, 96)}  ${m.where || ''}`);
    if (items.length > 8) log(`    · … and ${items.length - 8} more`);
  }
  log(`\n  full report: ${FEATURES_JSON}`);
  log('  This needs a person. The list above names the exact features, not a score.');
  if (SERVE_AFTER) { log(`\n  (still serving at ${BASE} — Ctrl-C to stop)`); await new Promise(() => {}); }
  stopServer();
  process.exit(1);
})();
