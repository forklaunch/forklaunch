#!/usr/bin/env node
// Release helper for the TypeScript pnpm workspaces (framework/, blueprint/).
// Used by .github/workflows/ts_release.yml and runnable locally. It never edits
// a package.json, a lockfile or a version constant: versions come only from
// `pnpm up:packages` and `npx changeset version` (see
// .claude/skills/release-train/SKILL.md).
//
//   node ts_release.mjs plan <workspace> [--out plan.json]
//       List the publishable packages (not private, with a publish:package
//       script) in dependency order, and which versions npm does not have yet.
//       Fails if a local version is lower than npm's latest.
//   node ts_release.mjs publish <workspace> [--dry-run] [--provenance]
//       Run each pending package's own publish:package script
//       (`pnpm run publish:package --no-git-checks ...`), retrying npm's E409
//       "previously staged version" until the script reports it as published.
//   node ts_release.mjs wait <workspace> [--timeout 1200] [--interval 20] [--confirmations 2]
//       Poll `npm view name@version` until every publishable package's local
//       version has been seen on npm <confirmations> times in a row.
//   node ts_release.mjs check-pickup <framework-dir> <blueprint-dir> [--warn-only]
//       Every blueprint dependency on a framework package must point at the
//       framework's current version (the skill's `grep "@forklaunch/core"` check).
//   node ts_release.mjs check-mikro-orm <dir>...
//       Every manifest across the given workspaces must resolve one
//       @mikro-orm/* version per package (core pins it exactly).

import { execFile, spawn } from 'node:child_process';
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const DEP_FIELDS = ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies'];

function parseArgs(argv) {
  const positional = [];
  const flags = {};
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg.startsWith('--')) {
      const key = arg.slice(2);
      const next = argv[i + 1];
      if (next !== undefined && !next.startsWith('--')) {
        flags[key] = next;
        i++;
      } else {
        flags[key] = true;
      }
    } else {
      positional.push(arg);
    }
  }
  return { positional, flags };
}

function summary(markdown) {
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown + '\n');
}

function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

async function workspacePackages(dir) {
  const { stdout } = await execFileAsync('pnpm', ['ls', '-r', '--depth', '-1', '--json'], {
    cwd: dir,
    maxBuffer: 64 * 1024 * 1024
  });
  const root = resolve(dir);
  return JSON.parse(stdout)
    .filter((p) => resolve(p.path) !== root)
    .map((p) => {
      const manifest = readJson(join(p.path, 'package.json'));
      return {
        name: manifest.name,
        version: manifest.version,
        path: p.path,
        rel: relative(root, p.path),
        private: Boolean(manifest.private),
        publishScript: manifest.scripts?.['publish:package'] ?? null,
        manifest
      };
    });
}

// Kahn's algorithm over in-workspace dependency edges, ties broken by path so
// the order is stable between runs.
function topoSort(pkgs) {
  const byName = new Map(pkgs.map((p) => [p.name, p]));
  const deps = new Map(
    pkgs.map((p) => [
      p.name,
      new Set(
        DEP_FIELDS.flatMap((f) => Object.keys(p.manifest[f] ?? {})).filter(
          (d) => byName.has(d) && d !== p.name
        )
      )
    ])
  );
  const ordered = [];
  const done = new Set();
  while (ordered.length < pkgs.length) {
    const ready = pkgs
      .filter((p) => !done.has(p.name) && [...deps.get(p.name)].every((d) => done.has(d)))
      .sort((a, b) => a.rel.localeCompare(b.rel));
    if (ready.length === 0) {
      // A cycle: fall back to path order for the rest rather than hanging.
      ordered.push(...pkgs.filter((p) => !done.has(p.name)).sort((a, b) => a.rel.localeCompare(b.rel)));
      break;
    }
    for (const p of ready) {
      ordered.push(p);
      done.add(p.name);
    }
  }
  return ordered;
}

function parseSemver(v) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?/.exec(String(v).trim());
  if (!m) return null;
  return { nums: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] ?? null };
}

function compareSemver(a, b) {
  const x = parseSemver(a);
  const y = parseSemver(b);
  if (!x || !y) return 0;
  for (let i = 0; i < 3; i++) if (x.nums[i] !== y.nums[i]) return x.nums[i] - y.nums[i];
  if (x.pre === y.pre) return 0;
  if (x.pre === null) return 1;
  if (y.pre === null) return -1;
  return x.pre < y.pre ? -1 : 1;
}

// npm view lags 1-3 minutes after a publish, so always revalidate the cache.
async function npmVersionExists(name, version) {
  try {
    const { stdout } = await execFileAsync(
      'npm',
      ['view', `${name}@${version}`, 'version', '--json', '--prefer-online'],
      { maxBuffer: 16 * 1024 * 1024 }
    );
    const out = stdout.trim();
    if (!out) return false;
    const parsed = JSON.parse(out);
    return Array.isArray(parsed) ? parsed.includes(version) : parsed === version;
  } catch (err) {
    if (/E404|404 Not Found/.test(`${err.stdout ?? ''}${err.stderr ?? ''}`)) return false;
    throw err;
  }
}

async function npmLatest(name) {
  try {
    const { stdout } = await execFileAsync('npm', ['view', name, 'dist-tags.latest', '--prefer-online']);
    return stdout.trim() || null;
  } catch (err) {
    if (/E404|404 Not Found/.test(`${err.stdout ?? ''}${err.stderr ?? ''}`)) return null;
    throw err;
  }
}

async function publishable(dir) {
  return topoSort(await workspacePackages(dir)).filter((p) => !p.private && p.publishScript);
}

async function plan(dir, flags) {
  const pkgs = await publishable(dir);
  const rows = await Promise.all(
    pkgs.map(async (p) => {
      const [exists, latest] = await Promise.all([npmVersionExists(p.name, p.version), npmLatest(p.name)]);
      let status = exists ? 'published' : 'pending';
      if (!exists && latest && compareSemver(p.version, latest) < 0) status = 'lower-than-latest';
      return { name: p.name, version: p.version, path: p.rel, npmLatest: latest, status };
    })
  );
  const pending = rows.filter((r) => r.status === 'pending');
  const lower = rows.filter((r) => r.status === 'lower-than-latest');
  const result = { workspace: dir, packages: rows, pending: pending.map((r) => `${r.name}@${r.version}`) };

  for (const r of rows) console.log(`${r.status.padEnd(18)} ${r.name}@${r.version} (npm latest: ${r.npmLatest ?? 'none'})`);
  summary(
    [
      `### ${dir}: ${pending.length} package(s) to publish`,
      '',
      '| package | local | npm latest | status |',
      '| --- | --- | --- | --- |',
      ...rows.map((r) => `| ${r.name} | ${r.version} | ${r.npmLatest ?? '-'} | ${r.status} |`)
    ].join('\n')
  );
  if (flags.out) writeFileSync(flags.out, JSON.stringify(result, null, 2) + '\n');
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `pending_count=${pending.length}\n`);
  if (lower.length) {
    console.error(
      `::error::local version is lower than npm latest for ${lower.map((r) => `${r.name}@${r.version} < ${r.npmLatest}`).join(', ')}. Publishing it would move the latest tag backwards; merge the release PR that has the newer versions first.`
    );
    process.exit(1);
  }
  return result;
}

function run(cmd, args, cwd) {
  return new Promise((resolvePromise) => {
    const child = spawn(cmd, args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    child.stdout.on('data', (d) => {
      output += d;
      process.stdout.write(d);
    });
    child.stderr.on('data', (d) => {
      output += d;
      process.stderr.write(d);
    });
    child.on('close', (code) => resolvePromise({ code, output }));
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function publish(dir, flags) {
  const dryRun = Boolean(flags['dry-run']);
  const attempts = Number(flags.attempts ?? 6);
  const pkgs = await publishable(dir);
  const pending = [];
  for (const p of pkgs) if (!(await npmVersionExists(p.name, p.version))) pending.push(p);
  if (pending.length === 0) {
    console.log(`Nothing to publish in ${dir}: every version is already on npm.`);
    return;
  }
  // Same flags as `pnpm publish:packages --no-git-checks` (never `-- --no-git-checks`),
  // forwarded through each package's publish:package script to `pnpm publish`.
  const extra = ['--no-git-checks'];
  if (flags.provenance) extra.push('--provenance');
  if (dryRun) extra.push('--dry-run');
  const failed = [];
  for (const p of pending) {
    console.log(`\n=== ${dryRun ? 'dry run: ' : ''}${p.name}@${p.version} (${p.rel})`);
    let ok = false;
    for (let attempt = 1; attempt <= attempts; attempt++) {
      const { code, output } = await run('pnpm', ['run', 'publish:package', ...extra], p.path);
      if (code === 0) {
        ok = true;
        break;
      }
      if (!dryRun && /E409|previously staged/i.test(output) && attempt < attempts) {
        console.log(`npm answered E409 while it catches up; retrying in 30s (${attempt}/${attempts})`);
        await sleep(30_000);
        continue;
      }
      break;
    }
    if (!ok) failed.push(`${p.name}@${p.version}`);
  }
  summary(`### ${dir}: ${dryRun ? 'dry-run' : 'publish'} of ${pending.length} package(s)${failed.length ? `, failed: ${failed.join(', ')}` : ''}`);
  if (failed.length) {
    console.error(`::error::${dryRun ? 'dry-run ' : ''}publish failed for ${failed.join(', ')}`);
    process.exit(1);
  }
}

async function wait(dir, flags) {
  const timeout = Number(flags.timeout ?? 1200) * 1000;
  const interval = Number(flags.interval ?? 20) * 1000;
  const confirmations = Number(flags.confirmations ?? 2);
  const pkgs = await publishable(dir);
  const seen = new Map(pkgs.map((p) => [p.name, 0]));
  const deadline = Date.now() + timeout;
  for (;;) {
    await Promise.all(
      pkgs.map(async (p) => {
        if (seen.get(p.name) >= confirmations) return;
        seen.set(p.name, (await npmVersionExists(p.name, p.version)) ? seen.get(p.name) + 1 : 0);
      })
    );
    const missing = pkgs.filter((p) => seen.get(p.name) < confirmations);
    if (missing.length === 0) {
      console.log(`All ${pkgs.length} ${dir} versions are visible on npm.`);
      return;
    }
    if (Date.now() > deadline) {
      console.error(`::error::still not visible on npm after ${timeout / 1000}s: ${missing.map((p) => `${p.name}@${p.version}`).join(', ')}`);
      process.exit(1);
    }
    console.log(`waiting for ${missing.map((p) => `${p.name}@${p.version}`).join(', ')}`);
    await sleep(interval);
  }
}

function minVersion(spec) {
  const m = /^(?:npm:[^@]+@|[\^~=]|>=)?\s*v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(String(spec).trim());
  return m ? m[1] : null;
}

async function checkPickup(frameworkDir, blueprintDir, flags) {
  const framework = new Map((await workspacePackages(frameworkDir)).filter((p) => !p.private).map((p) => [p.name, p.version]));
  const bad = [];
  let checked = 0;
  for (const p of await workspacePackages(blueprintDir)) {
    for (const field of DEP_FIELDS) {
      for (const [dep, spec] of Object.entries(p.manifest[field] ?? {})) {
        if (!framework.has(dep)) continue;
        checked++;
        const v = minVersion(spec);
        if (v !== framework.get(dep)) bad.push(`${p.rel || '.'} ${field}.${dep} = ${spec} (framework has ${framework.get(dep)})`);
      }
    }
  }
  const root = readJson(join(blueprintDir, 'package.json'));
  for (const field of DEP_FIELDS) {
    for (const [dep, spec] of Object.entries(root[field] ?? {})) {
      if (!framework.has(dep)) continue;
      checked++;
      if (minVersion(spec) !== framework.get(dep)) bad.push(`. ${field}.${dep} = ${spec} (framework has ${framework.get(dep)})`);
    }
  }
  if (bad.length === 0) {
    console.log(`All ${checked} blueprint references to framework packages point at the current framework versions.`);
    return;
  }
  const level = flags['warn-only'] ? 'warning' : 'error';
  console.error(`::${level}::${bad.length} blueprint reference(s) do not point at the current framework version:`);
  for (const b of bad) console.error(`  ${b}`);
  summary(`### Blueprint framework pickup: ${bad.length} stale reference(s)\n\n${bad.map((b) => `- ${b}`).join('\n')}`);
  if (!flags['warn-only']) process.exit(1);
}

async function checkMikroOrm(dirs) {
  const seen = new Map();
  for (const dir of dirs) {
    const pkgs = await workspacePackages(dir);
    const manifests = [{ rel: '.', manifest: readJson(join(dir, 'package.json')) }, ...pkgs];
    for (const p of manifests) {
      for (const field of DEP_FIELDS) {
        for (const [dep, spec] of Object.entries(p.manifest[field] ?? {})) {
          if (!dep.startsWith('@mikro-orm/')) continue;
          const v = minVersion(spec) ?? spec;
          if (!seen.has(dep)) seen.set(dep, new Map());
          const where = seen.get(dep);
          if (!where.has(v)) where.set(v, []);
          where.get(v).push(`${dir}/${p.rel}`);
        }
      }
    }
  }
  const split = [...seen].filter(([, versions]) => versions.size > 1);
  if (split.length === 0) {
    console.log(`@mikro-orm/* resolves to one version per package across ${dirs.join(', ')}.`);
    return;
  }
  for (const [dep, versions] of split) {
    console.error(`::error::${dep} has ${versions.size} versions: ${[...versions].map(([v, w]) => `${v} (${w.length} manifests, e.g. ${w[0]})`).join('; ')}`);
  }
  process.exit(1);
}

const [command, ...rest] = process.argv.slice(2);
const { positional, flags } = parseArgs(rest);
const commands = {
  plan: () => plan(positional[0], flags),
  publish: () => publish(positional[0], flags),
  wait: () => wait(positional[0], flags),
  'check-pickup': () => checkPickup(positional[0], positional[1], flags),
  'check-mikro-orm': () => checkMikroOrm(positional)
};
if (!commands[command] || (command !== 'check-mikro-orm' && !positional[0]) || (command === 'check-pickup' && !positional[1])) {
  console.error('usage: ts_release.mjs plan|publish|wait <workspace> | check-pickup <framework> <blueprint> | check-mikro-orm <dir>...');
  process.exit(2);
}
await commands[command]();
