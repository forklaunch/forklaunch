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
 *   2. serve     — static server on 127.0.0.1 so you can view the clone
 *   3. measure   — (optional) scores local vs live fidelity for iteration
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
  const a = { serve: true, measure: false, out: null, url: null, clean: false, pages: 20, noPreflight: false, noCatalog: false, server: null, secret: null, api: null, noVerify: false };
  for (let i = 0; i < argv.length; i++) {
    const t = argv[i];
    if (t === '--no-serve') a.serve = false;
    else if (t === '--measure') a.measure = true;
    else if (t === '--clean') a.clean = true;
    else if (t === '--single') a.pages = 1;
    else if (t === '--no-preflight') a.noPreflight = true;
    else if (t === '--no-catalog') a.noCatalog = true;
    else if (t === '--no-verify') a.noVerify = true;
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
  console.error('usage: node bin/migrate.mjs <store-url> [--out DIR] [--pages N] [--single] [--api URL] [--server URL] [--secret KEY] [--no-catalog] [--no-verify] [--no-serve] [--measure] [--clean]');
  process.exit(2);
}

const domain = domainOf(args.url);
const outdir = args.out || join(ROOT, 'output', domain);
mkdirSync(outdir, { recursive: true });

console.log(`\n▶ migrating ${domain}`);
console.log(`  output: ${outdir}\n`);

// self-install on first run so the receiver has no setup step
ensureDeps();

// Pre-flight first: say what to expect BEFORE doing the work, so a hard store
// is announced up front rather than explained away afterwards.
let preflightStack = null;
if (!args.noPreflight) {
  console.log('0/‌3  assessing storefront…');
  const preflightOut = await runPreflight(domain);
  const stackMatch = /^\s*stack:\s*(.+)$/m.exec(preflightOut);
  if (stackMatch) preflightStack = stackMatch[1].trim();
}

console.log(args.pages === 1
  ? '1/‌3  capturing homepage (headless render + assets)…'
  : `1/‌3  crawling storefront (up to ${args.pages} pages, headless render + assets)…`);
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
  console.log('\n2/‌3  measuring fidelity vs live…');
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
  console.log('\n2/‌4  migrating catalog…');
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

// Self-verification. Every capture reports its own condition rather than
// relying on someone remembering to check. loop_test walks the actual purchase
// journey (home -> nav -> collection -> product -> add to cart); repeated
// regressions reached the user because earlier checks only counted images and
// links and never clicked anything.
if (!args.noVerify) {
  console.log('\n▸ verifying…');
  try {
    await run('node', [join(ROOT, 'loop_test.js'), outdir]);
  } catch (e) {
    console.error(`   (verification could not run: ${e.message})`);
  }
}

if (!args.serve) {
  console.log(`\n✓ done. Serve later with:\n  python3 ${join(ROOT, 'serve.py')} 4173 ${join(outdir, 'site')}\n`);
  process.exit(0);
}

console.log('\n3/‌3  serving locally…');
console.log(`   open  http://127.0.0.1:4173\n   (Ctrl-C to stop)\n`);
await run('python3', [join(ROOT, 'serve.py'), '4173', join(outdir, 'site')]);
