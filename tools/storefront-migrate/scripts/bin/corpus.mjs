#!/usr/bin/env node
/**
 * corpus — run the whole migration on every store in corpus.json and tabulate.
 *
 *   node bin/corpus.mjs [--parallel N] [--only domain,domain] [--out DIR]
 *
 * This is the regression suite behind the claim "it works on any Shopify
 * store": not one store tuned until green, but ten across the theme families
 * most stores run on, every one required to exit 0 or leave only NAMED policy
 * residue. Each store runs the real one command (migrate.mjs) with
 * --no-serve, one browser per store; --parallel bounds how many at once
 * (default 1 — one headless Chromium is ~450MB, size it to the machine).
 *
 * Output: <out>/corpus-report.json and a table on stdout with, per store:
 * exit code, pages, rounds, missing-by-kind, policy count, wall time.
 */
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS = dirname(dirname(fileURLToPath(import.meta.url)));
const argv = process.argv.slice(2);
const arg = (k, d) => { const i = argv.indexOf(k); return i > -1 ? argv[i + 1] : d; };
const PAR = Math.max(1, Number(arg('--parallel', '1')) || 1);
const OUT = arg('--out', join(SCRIPTS, 'output', 'corpus'));
const ONLY = (arg('--only', '') || '').split(',').map((s) => s.trim()).filter(Boolean);
const corpus = JSON.parse(readFileSync(join(SCRIPTS, 'corpus.json'), 'utf8')).stores
  .filter((s) => !ONLY.length || ONLY.includes(s.domain));
mkdirSync(OUT, { recursive: true });

let port = 4300;
function runOne(store) {
  return new Promise((res) => {
    const d = store.domain;
    const out = join(OUT, d);
    const log = join(OUT, `${d}.log`);
    const p = port++;
    const t0 = Date.now();
    const args = [join(SCRIPTS, 'bin', 'migrate.mjs'), `https://www.${d}`, '--clean', '--no-serve', '--out', out, '--port', String(p)];
    const child = spawn('node', args, { cwd: SCRIPTS, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, FL_CONCURRENCY: process.env.FL_CONCURRENCY || '1' } });
    let buf = '';
    const sink = (c) => { buf += c; };
    child.stdout.on('data', sink); child.stderr.on('data', sink);
    child.on('close', (code) => {
      writeFileSync(log, buf);
      // One store at a time means every headless Chromium alive now is a
      // stray from this store; clear them so the next store starts clean.
      if (PAR === 1) { try { spawn('pkill', ['-9', '-f', 'headless_shell']); } catch {} }
      const r = { domain: d, theme: store.theme, exit: code, seconds: Math.round((Date.now() - t0) / 1000), pages: null, rounds: null, missing: {}, policy: null, harness: null, verdict: null };
      const m = buf.match(/✓ (\d+) pages, (\d+) assets/); if (m) r.pages = Number(m[1]);
      const rounds = buf.match(/verify — round (\d+) of/g); r.rounds = rounds ? rounds.length : 0;
      const hf = buf.match(/HARNESS-FAIL[^\n]*\n\s*([^\n]+)/); if (hf) r.harness = hf[1].trim().slice(0, 140);
      const pre = buf.match(/● (GREEN|AMBER|RED)/); if (pre) r.preflight = pre[1];
      try {
        const f = JSON.parse(readFileSync(join(out, 'features.json'), 'utf8'));
        for (const x of f.missing || []) r.missing[x.kind] = (r.missing[x.kind] || 0) + 1;
        r.policy = (f.policy || []).length;
        r.missingTotal = (f.missing || []).length;
      } catch {}
      r.verdict = code === 0 ? 'CLEAN' : code === 2 ? 'HARNESS-FAIL' : (r.missingTotal === 0 ? 'CLEAN' : 'MISSING');
      console.log(`  ${r.verdict.padEnd(12)} ${d.padEnd(22)} exit ${code}  ${r.pages ?? '?'} pages  ${r.rounds} round(s)  ${r.seconds}s  missing: ${JSON.stringify(r.missing)}${r.harness ? '  ' + r.harness : ''}`);
      res(r);
    });
  });
}

console.log(`corpus: ${corpus.length} store(s), ${PAR} at a time, FL_CONCURRENCY=${process.env.FL_CONCURRENCY || '1'} → ${OUT}\n`);
const results = [];
const queue = [...corpus];
await Promise.all(Array.from({ length: PAR }, async () => {
  while (queue.length) results.push(await runOne(queue.shift()));
}));
results.sort((a, b) => corpus.findIndex((s) => s.domain === a.domain) - corpus.findIndex((s) => s.domain === b.domain));
writeFileSync(join(OUT, 'corpus-report.json'), JSON.stringify({ at: new Date().toISOString(), results }, null, 1));
const clean = results.filter((r) => r.verdict === 'CLEAN').length;
console.log(`\n${clean}/${results.length} clean. Report: ${join(OUT, 'corpus-report.json')}`);
process.exit(clean === results.length ? 0 : 1);
