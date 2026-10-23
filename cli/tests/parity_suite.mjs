#!/usr/bin/env node
// Table-driven runner for the TypeScript/Rust parity families.
//
// parity_suites.json lists, per family, the scripts rust_parity.yml runs one
// step at a time, each with the exact step environment. This runner replays
// them in one process: it first compiles every Cargo example or test target
// the family needs in a single cargo invocation per feature set (cargo
// parallelises the builds and links that the scripts otherwise did serially),
// then runs each script unchanged, so every assertion still runs exactly as in
// its own step. Scripts still build anything the prebuild missed themselves.
//
//   node cli/tests/parity_suite.mjs <family> [--shard I/N] [--only REGEX] [--list] [--no-prebuild] [--keep-going]
//   node cli/tests/parity_suite.mjs --families
//
// ${FL_RUST_TS} / ${FL_CANONICAL_HTTP} in the table resolve from
// FORKLAUNCH_PARITY_RUST_TS / FORKLAUNCH_PARITY_CANONICAL_HTTP, falling back to
// $RUNNER_TEMP/fl-rust-ts and $RUNNER_TEMP/fl-canonical-http as in CI.
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const table = JSON.parse(fs.readFileSync(path.join(here, 'parity_suites.json'), 'utf8')).families;
const args = process.argv.slice(2);
const flag = name => args.includes(name);
const option = name => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : undefined; };

if (flag('--families') || args.length === 0) {
  for (const [name, family] of Object.entries(table))
    console.log(`${name.padEnd(15)} ${String(family.entries.length).padStart(3)} scripts  node ${family.node.padEnd(7)} ${family.job} job: ${family.description}`);
  process.exit(args.length === 0 ? 2 : 0);
}
const familyName = args.find(a => !a.startsWith('--') && a !== option('--shard') && a !== option('--only'));
const family = table[familyName];
if (!family) { console.error(`unknown family ${familyName}; known: ${Object.keys(table).join(', ')}`); process.exit(2); }

let entries = family.entries.map((entry, index) => ({...entry, index}));
const shard = option('--shard');
if (shard) {
  const [i, n] = shard.split('/').map(Number);
  if (!(n >= 1 && i >= 1 && i <= n)) { console.error(`bad --shard ${shard}; expected I/N`); process.exit(2); }
  entries = entries.filter(entry => entry.index % n === i - 1);
}
const only = option('--only');
if (only) entries = entries.filter(entry => new RegExp(only).test(entry.script));
if (flag('--list')) {
  for (const entry of entries) console.log(`${entry.script}${entry.env ? ' ' + JSON.stringify(entry.env) : ''}`);
  process.exit(0);
}

const wanted = family.node;
const actual = process.versions.node;
if (!(actual === wanted || actual.split('.')[0] === wanted) && process.env.FORKLAUNCH_PARITY_ALLOW_NODE_MISMATCH !== '1') {
  console.error(`family ${familyName} runs on Node ${wanted} in CI, this is ${actual}; set FORKLAUNCH_PARITY_ALLOW_NODE_MISMATCH=1 to run anyway`);
  process.exit(2);
}

const placeholders = {
  FL_RUST_TS: process.env.FORKLAUNCH_PARITY_RUST_TS ?? (process.env.RUNNER_TEMP && path.join(process.env.RUNNER_TEMP, 'fl-rust-ts')),
  FL_CANONICAL_HTTP: process.env.FORKLAUNCH_PARITY_CANONICAL_HTTP ?? (process.env.RUNNER_TEMP && path.join(process.env.RUNNER_TEMP, 'fl-canonical-http')),
};
const resolve = value => value.replace(/\$\{(FL_[A-Z_]+)\}/g, (_, key) => {
  if (!placeholders[key]) { console.error(`\${${key}} is unresolved: set FORKLAUNCH_PARITY_${key.slice(3)} or RUNNER_TEMP`); process.exit(2); }
  return placeholders[key];
});
// As in a workflow step: the job's environment, overridden by the step's own.
// Run a family in the job it came from (`job` in the table) so that the
// job-level variables its steps rely on (some written to $GITHUB_ENV) are set.
const baseEnv = {...process.env};

const started = Date.now();
const seconds = since => ((Date.now() - since) / 1000).toFixed(1);
if (!flag('--no-prebuild')) {
  for (const build of family.prebuild ?? []) {
    // Test targets map to the script that runs them; build only the selected ones.
    const selected = new Set(entries.map(entry => entry.script));
    const tests = build.tests && Object.entries(build.tests).filter(([, script]) => selected.has(script)).map(([test]) => test);
    if (tests && tests.length === 0) continue;
    const targets = build.examples ? build.examples.flatMap(e => ['--example', e]) : tests.flatMap(t => ['--test', t]);
    const argv = [build.tests ? 'test' : 'build', '--manifest-path', path.join(root, build.manifest), '-p', build.package,
      ...(build.features ? ['--features', build.features] : []), ...targets, ...(build.tests ? ['--no-run'] : [])];
    const env = {...baseEnv, CARGO_INCREMENTAL: '0',
      CARGO_TARGET_DIR: path.resolve(root, process.env.CARGO_TARGET_DIR ?? build.default_target)};
    const at = Date.now();
    console.log(`[prebuild] cargo ${argv.join(' ')}`);
    const result = spawnSync('cargo', argv, {cwd: root, env, stdio: 'inherit', timeout: 3600000});
    // Not fatal: a script that needs a target the prebuild could not produce
    // builds it itself and reports the real error in its own context.
    console.log(`[prebuild] ${result.status === 0 ? 'ok' : `exit ${result.status}`} in ${seconds(at)}s`);
  }
}

const results = [];
for (const entry of entries) {
  const env = {...baseEnv, ...Object.fromEntries(Object.entries(entry.env ?? {}).map(([k, v]) => [k, resolve(v)]))};
  const label = `${entry.script}${entry.name ? ` (${entry.name})` : ''}`;
  console.log(`\n[parity ${familyName}] ${label}`);
  const at = Date.now();
  const result = spawnSync(process.execPath, [path.join(here, entry.script)], {cwd: root, env, stdio: 'inherit', timeout: 3600000});
  const ok = result.status === 0;
  results.push({label, ok, time: seconds(at)});
  console.log(`[parity ${familyName}] ${ok ? 'PASS' : 'FAIL'} ${label} in ${seconds(at)}s`);
  if (!ok && !flag('--keep-going')) break;
}
const failed = results.filter(r => !r.ok);
console.log(`\n[parity ${familyName}${shard ? ` ${shard}` : ''}] ${results.length - failed.length}/${entries.length} passed in ${seconds(started)}s`);
for (const r of failed) console.log(`  FAIL ${r.label}`);
process.exit(failed.length || results.length !== entries.length ? 1 : 0);
