#!/usr/bin/env node
/**
 * forklaunch storefront migration — single entrypoint.
 *
 *   node bin/migrate.mjs <store-url> [--out DIR] [--pages N] [--single] [--api URL] [--server URL] [--secret KEY] [--no-catalog] [--no-verify] [--no-serve] [--measure] [--clean]
 *
 * Captures a live storefront's homepage at high visual fidelity and stands it
 * up locally. Everything runs on 127.0.0.1; no data leaves the machine and no
 * checkout/payment path is wired up.
 *
 * Steps:
 *   1. capture   — headless browser loads the store, runs its JS, saves the
 *                  post-render DOM + every asset, rewrites URLs to local paths
 *   2. catalog   — pull the real product data and normalise it
 *   3. finish    — verify against the LIVE site and repair until the clone has
 *                  every feature the live storefront has, then serve
 *
 * Step 3 is why this is one command rather than three. Capture, verify and
 * repair all existed and all worked; nothing chained them, so every migration
 * needed a person who remembered the right order — and the failures they catch
 * are silent, because a broken capture serves 200s and looks perfectly fine
 * right up until a client is looking at the screen. bin/finish.mjs runs the
 * feature gate, applies the repair each reported defect maps to, and re-gates,
 * until the missing-feature list is empty or the budget runs out. It stops on
 * a LIST, never on a percentage or a round count.
 *
 * This reproduces what a visitor SEES. It does not recover the store's backend,
 * inventory counts, or third-party app data (reviews, loyalty) — those require
 * merchant credentials. See SKILL.md "What this does and does not do".
 */
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { ensureDeps } from './bootstrap.mjs';
import { checkPrereqs, refuse } from '../check-prereqs.mjs';
import { existsSync as _exists } from 'node:fs';
import { execSync } from 'node:child_process';

// The catalog pipeline is TypeScript run by bun. bun is usually installed at
// ~/.bun/bin/bun, which is NOT on a spawned process's PATH — resolve it
// explicitly, and install it if the machine doesn't have it yet.
function resolveBun() {
  const candidates = [
    process.env.BUN_INSTALL ? `${process.env.BUN_INSTALL}/bin/bun` : null,
    `${process.env.HOME}/.bun/bin/bun`,
    '/opt/homebrew/bin/bun', '/usr/local/bin/bun',
  ].filter(Boolean);
  for (const c of candidates) if (_exists(c)) return c;
  try { return execSync('command -v bun', { encoding: 'utf8' }).trim() || null; } catch (_) {}
  return null;
}

function ensureBun() {
  let bun = resolveBun();
  if (bun) return bun;
  console.log('   · installing bun (one time)…');
  try {
    execSync('curl -fsSL https://bun.sh/install | bash', { stdio: 'inherit', shell: '/bin/bash' });
  } catch (_) {}
  return resolveBun();
}

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');

function parseArgs(argv) {
  const a = { serve: true, measure: false, out: null, url: null, clean: false, pages: 20, noPreflight: false, noCatalog: false, server: null, secret: null, api: null, noVerify: false, rounds: 5, budgetMin: 30, port: '4173', stripePk: null, paypalId: null, allowRecapture: true };
  for (let i = 0; i < argv.length; i++) {
    const t = argv[i];
    if (t === '--no-serve') a.serve = false;
    else if (t === '--measure') a.measure = true;
    else if (t === '--clean') a.clean = true;
    else if (t === '--single') a.pages = 1;
    else if (t === '--no-preflight') a.noPreflight = true;
    else if (t === '--no-catalog') a.noCatalog = true;
    else if (t === '--no-verify') a.noVerify = true;
    else if (t === '--rounds') a.rounds = parseInt(argv[++i], 10) || 5;
    else if (t === '--budget-min') a.budgetMin = parseInt(argv[++i], 10) || 30;
    else if (t === '--port') a.port = argv[++i];
    else if (t === '--stripe-pk') a.stripePk = argv[++i];
    else if (t === '--paypal-id') a.paypalId = argv[++i];
    // A re-crawl is hours of work and sustained load on the merchant's origin.
    // On by default here because this command is already crawling that store
    // with the operator's intent — but it has to stay refusable.
    else if (t === '--no-recapture') a.allowRecapture = false;
    else if (t === '--server') a.server = argv[++i];
    else if (t === '--api') a.api = argv[++i];
    else if (t === '--secret') a.secret = argv[++i];
    else if (t === '--pages') a.pages = parseInt(argv[++i], 10) || 20;
    else if (t === '--out') a.out = argv[++i];
    else if (!t.startsWith('--')) a.url = t;
  }
  return a;
}

function domainOf(url) {
  let u = url.trim().replace(/^https?:\/\//, '').replace(/\/.*$/, '');
  return u.replace(/^www\./, '');
}

function run(cmd, args, opts = {}) {
  return new Promise((res, rej) => {
    const p = spawn(cmd, args, { stdio: 'inherit', ...opts });
    p.on('exit', code => code === 0 ? res() : rej(new Error(`${cmd} exited ${code}`)));
    p.on('error', rej);
  });
}

// Same output as `run('node', [preflight.js, domain])` (streamed straight to
// the terminal, exactly as before) but also buffered so the stack/theme line
// can be lifted out for manifest.json — preflight.js is the only place that
// detects it, and re-running it a second time with --json would double the
// page loads for no reason.
function runPreflight(domain) {
  return new Promise((res) => {
    let out = '';
    const p = spawn('node', [join(ROOT, 'preflight.js'), domain]);
    p.stdout.on('data', d => { out += d; process.stdout.write(d); });
    p.stderr.on('data', d => process.stderr.write(d));
    p.on('exit', () => res(out));
    p.on('error', () => res(out));
  });
}

function runCapture(domain, outdir, clean, pages, apiBase) {
  return new Promise((res, rej) => {
    let out = '';
    // crawl.js walks homepage + collections + PDPs and rewrites inter-page
    // links, so the clone is browsable. --single falls back to capture.js
    // (homepage only, links point at the live site).
    const argv = pages === 1
      ? [join(ROOT, 'capture.js'), domain, outdir]
      : [join(ROOT, 'crawl.js'), domain, outdir, '--pages', String(pages)];
    if (apiBase) { argv.push('--api', apiBase); }
    if (clean) argv.push('--clean');
    const p = spawn('node', argv);
    p.stdout.on('data', d => { out += d; });
    p.stderr.on('data', d => process.stderr.write(d));
    p.on('exit', () => {
      try { res(JSON.parse(out.trim().split('\n').pop())); }
      catch (e) { rej(new Error('capture produced no result')); }
    });
    p.on('error', rej);
  });
}

const args = parseArgs(process.argv.slice(2));
if (!args.url) {
  console.error('usage: node bin/migrate.mjs <store-url> [--out DIR] [--pages N] [--single] [--clean]\n' +
                '                              [--port 4173] [--rounds 5] [--budget-min 30] [--no-recapture]\n' +
                '                              [--api URL] [--server URL] [--secret KEY] [--stripe-pk pk_…] [--paypal-id …]\n' +
                '                              [--no-catalog] [--no-verify] [--no-serve] [--measure]');
  process.exit(2);
}

const domain = domainOf(args.url);
const outdir = args.out || join(ROOT, 'output', domain);
mkdirSync(outdir, { recursive: true });

console.log(`\n▶ migrating ${domain}`);
console.log(`  output: ${outdir}\n`);

// self-install on first run so the receiver has no setup step
ensureDeps();

// Refuse to start without the tools that do NOT self-install. Each of them
// fails silently: no chromium and the gates emit nothing (which reads as "no
// failures"), no ffmpeg and check-budget's height probe returns 0 and PASSES,
// no bun and the storefront never serves. Only what this particular run will
// actually use is required — a capture-only run genuinely does not need bun.
{
  const need = ['playwright'];
  if (!args.noCatalog || !args.noVerify) need.push('bun');
  if (!args.noVerify) need.push('ffmpeg');
  if (args.noVerify && args.serve) need.push('python');
  const missing = await checkPrereqs(need);
  if (missing.length) refuse(missing);
  console.log(`   ✓ prerequisites: ${need.join(', ')}`);
}

// Pre-flight first: say what to expect BEFORE doing the work, so a hard store
// is announced up front rather than explained away afterwards.
let preflightStack = null;
if (!args.noPreflight) {
  console.log('0/4  assessing storefront…');
  const preflightOut = await runPreflight(domain);
  const stackMatch = /^\s*stack:\s*(.+)$/m.exec(preflightOut);
  if (stackMatch) preflightStack = stackMatch[1].trim();
}

console.log(args.pages === 1
  ? '1/4  capturing homepage (headless render + assets)…'
  : `1/4  crawling storefront (up to ${args.pages} pages, headless render + assets)…`);
const cap = await runCapture(domain, outdir, args.clean, args.pages, args.api);
if (!cap.ok) {
  console.error(`\n✗ capture failed: ${cap.reason}`);
  console.error('  If the store blocks automated browsers, see SKILL.md ' +
                '"Stores that need a real browser".');
  process.exit(1);
}
console.log(`   ✓ ${cap.pages ? cap.pages + ' pages, ' : ''}` +
            `${cap.assetsSaved ?? cap.assets} assets, ` +
            `${(cap.bytes/1048576).toFixed(1)}MB, ${(cap.elapsedMs/1000).toFixed(0)}s`);

const indexPath = join(outdir, 'site', 'index.html');
if (!existsSync(indexPath)) { console.error('✗ no index.html produced'); process.exit(1); }

if (args.measure) {
  console.log('\n2/4  measuring fidelity vs live…');
  try {
    await run('node', [join(ROOT, 'measure.js'), domain, indexPath,
                        join(outdir, 'measure.json')]);
  } catch (e) { console.error(`   (measure skipped: ${e.message})`); }
}

// ---- catalog: pull -> normalize -> (optional) import into ecommerce-stripe --
// The visual clone is only half a migration. This pulls the real catalog as
// structured records and, when a server + HMAC secret are supplied, loads it
// into the actual ForkLaunch ecommerce module via POST /catalog-import.
// Tracks whether normalize actually produced FRESH output this run — not
// whether data/<slug>/normalized.json merely exists on disk. That directory
// is keyed only by domain, not by this run's --out, so a stale file from an
// earlier migration of the same domain can sit there through a --no-catalog
// run or a failed pull. manifest.js must not report that leftover file as
// this run's catalog.
let catalogReady = false;
if (!args.noCatalog) {
  console.log('\n2/4  migrating catalog…');
  const cat = join(ROOT, 'catalog');
  const BUN = ensureBun();
  if (!BUN) {
    console.error('   catalog step skipped: bun could not be installed. ' +
                  'Install from https://bun.sh then re-run.');
  } else {
  try {
    await run(BUN, [join(cat, 'cli.ts'), 'pull', `https://${domain}`], { cwd: cat });
    const raw = join(cat, 'data', domain.replace(/\./g, '-'), 'raw.json');
    await run(BUN, [join(cat, 'cli.ts'), 'normalize', raw, `https://${domain}`], { cwd: cat });
    catalogReady = true;
    if (args.server) {
      const norm = join(cat, 'data', domain.replace(/\./g, '-'), 'normalized.json');
      const importArgs = [join(cat, 'cli.ts'), 'import', norm, args.server];
      if (args.secret) importArgs.push(args.secret);
      await run(BUN, importArgs, { cwd: cat });
    } else {
      console.log('   (no --server given — catalog normalized but not imported)');
    }
  } catch (e) {
    console.error(`   catalog step failed: ${e.message}`);
    console.error('   The store published no readable public catalog (/products.json). Headless ' +
                  'storefronts and stores that disabled the feed do this. The clone still ' +
                  'captures and browses; product-data endpoints, the cart mapping and the ' +
                  'catalog import have nothing to draw from until a catalog is pulled with ' +
                  "merchant access: bun scripts/catalog/cli.ts pull-admin <shop-url> --token <admin-token>");
  }
  }
}

// ---- manifest: the structured summary a scaffolding tool reads instead of
// re-deriving everything above from site/ and crawl.json itself. Runs last so
// crawl.json, pages.json and (if the catalog step ran) normalized.json /
// gap-report.md all already exist to read from. Written even when the
// catalog step was skipped or failed — manifest.json just carries catalog:
// null in that case, same as --no-catalog.
console.log('\n   writing manifest…');
try {
  const manifestArgs = [join(ROOT, 'manifest.js'), domain, outdir, '--url', `https://${domain}`];
  if (preflightStack) manifestArgs.push('--stack', preflightStack);
  if (!catalogReady) manifestArgs.push('--no-catalog');
  await run('node', manifestArgs);
} catch (e) {
  console.error(`   manifest step failed: ${e.message}`);
}

// ---- verify and repair, then serve ---------------------------------------
// This is the half that used to be a checklist somebody ran by hand, and the
// reason a migration could not be one-shot. finish.mjs discovers this theme's
// controls, serves the capture through the same bridge that will ship, runs
// the feature gate against the LIVE storefront, applies the repair each
// reported defect maps to, and re-gates — stopping when the missing-feature
// list is EMPTY, not at a round count and not at a score.
//
// It also serves, so there is exactly one server in the picture: the gates
// measure the same process the viewer will open, rather than one that merely
// resembles it. loop_test.js used to run here; the feature gate subsumes it
// (it walks the same journey and asks harder questions of it) and loop_test
// remains as a standalone tool.
if (args.noVerify) {
  if (!args.serve) {
    console.log(`\n✓ done. Serve later with:\n  python3 ${join(ROOT, 'serve.py')} ${args.port} ${join(outdir, 'site')}\n`);
    process.exit(0);
  }
  console.log(`\n3/4  serving locally (UNVERIFIED — --no-verify was passed)…`);
  console.log(`   open  http://127.0.0.1:${args.port}\n   (Ctrl-C to stop)\n`);
  await run('python3', [join(ROOT, 'serve.py'), args.port, join(outdir, 'site')]);
} else {
  console.log(`\n3/4  verifying against the live storefront, and repairing…`);
  const finishArgs = [join(ROOT, 'bin', 'finish.mjs'), join(outdir, 'site'),
    '--live', `https://${domain}`, '--port', String(args.port),
    '--out', outdir, '--rounds', String(args.rounds), '--budget-min', String(args.budgetMin)];
  if (args.allowRecapture) finishArgs.push('--allow-recapture');
  if (args.server) finishArgs.push('--module', args.server);
  if (args.secret) finishArgs.push('--hmac', args.secret);
  if (args.stripePk) finishArgs.push('--stripe-pk', args.stripePk);
  if (args.paypalId) finishArgs.push('--paypal-id', args.paypalId);
  if (args.serve) finishArgs.push('--serve-after');

  // finish.mjs's exit code IS the migration's exit code, and its three values
  // mean genuinely different things: 0 verified, 1 features still missing (the
  // report names them), 2 the gates could not run so nothing was proved.
  // Collapsing 2 into either of the others is the exact failure this rewrite
  // exists to prevent.
  const code = await new Promise((res) => {
    const p = spawn('node', finishArgs, { stdio: 'inherit' });
    p.on('exit', (c) => res(c ?? 1));
    p.on('error', () => res(2));
  });
  if (code === 0 && !args.serve) {
    console.log(`\n✓ verified. Serve later with:\n  bun ${join(ROOT, 'catalog', 'heroserve-fl.ts')} ${join(outdir, 'site')} ${args.port}\n`);
  }
  console.log(`\n  exit code ${code} — ${code === 0 ? 'every live feature present' : code === 1 ? 'features still missing (named above)' : 'the check could not run; nothing proved'}`);
  process.exit(code);
}
