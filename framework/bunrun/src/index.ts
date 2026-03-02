/**
 * bunrun.ts — Bun-native TypeScript (no deps)
 *
 * Runs a workspace script in topological order:
 *  - Parallel-by-tier (default): run each tier concurrently, wait between tiers
 *  - Sequential: run one package at a time in overall topo order
 *
 * Ordering edges come from:
 *  - dependencies + optionalDependencies
 *  - (opt-in) devDependencies + peerDependencies
 *
 * Filters:
 *  - --filter <glob>  (match by package name or path; can repeat, alias for --only)
 *  - --only <glob>    (match by package name or path; can repeat)
 *  - --exclude <glob> (same; can repeat)
 *
 * Usage:
 *  bunrun [script] [options]
 *
 * Arguments:
 *  script                  Script to run (default: build)
 *
 * Flags:
 *  - -j, --jobs <n>        concurrency per tier (default: CPU count)
 *  - --no-dev              ignore devDependencies for ordering
 *  - --no-peer             ignore peerDependencies for ordering
 *  - --sequential          run strictly one-by-one in topo order
 *  - --print-only          print the plan (and commands) but do not run
 *  - --debug               show diagnostic info, plan and command preview
 *
 * Examples:
 *   bun run bunrun
 *   bun run bunrun build
 *   bun run bunrun typecheck -j 8
 *   bun run bunrun build --no-dev --no-peer
 *   bun run bunrun test --filter "@frontera/*" --exclude "./packages/legacy-*"
 *   bun run bunrun lint --only "@frontera/*" --exclude "./packages/legacy-*"
 *   bun run bunrun build --sequential
 *   bun run bunrun build --debug --print-only
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { availableParallelism, cpus } from 'node:os';
import { join, relative, resolve, sep } from 'node:path';

// Bun types (avoid using global Bun to be more explicit)
declare const Bun: {
  spawn(
    cmd: string[],
    options?: {
      cwd?: string;
      stdout?: 'inherit' | 'pipe';
      stderr?: 'inherit' | 'pipe';
    }
  ): {
    exited: Promise<number>;
  };
};

interface PackageJson {
  name: string;
  version?: string;
  private?: boolean;
  workspaces?: string[] | { packages?: string[] };
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  scripts?: Record<string, string>;
  [key: string]: unknown;
}

interface Pkg {
  readonly name: string;
  readonly dir: string;
  readonly scripts: Readonly<Record<string, string>>;
  readonly json: PackageJson;
}

interface Opts {
  readonly script: string;
  readonly jobs: number;
  readonly includeDev: boolean;
  readonly includePeer: boolean;
  readonly only: readonly string[];
  readonly exclude: readonly string[];
  readonly printOnly: boolean;
  readonly sequential: boolean;
  readonly debug: boolean;
}

const cwd = process.cwd();

function detectCPUCount(): number {
  try {
    return Math.max(1, availableParallelism());
  } catch {
    try {
      return Math.max(1, cpus()?.length ?? 4);
    } catch {
      return 4;
    }
  }
}

function parseArgs(): Opts {
  const args = process.argv.slice(2);
  const opts: {
    script: string;
    jobs: number;
    includeDev: boolean;
    includePeer: boolean;
    only: string[];
    exclude: string[];
    printOnly: boolean;
    sequential: boolean;
    debug: boolean;
  } = {
    script: 'build',
    jobs: detectCPUCount(),
    includeDev: true,
    includePeer: true,
    only: [],
    exclude: [],
    printOnly: false,
    sequential: false,
    debug: false
  };

  // Check if first argument is a script name (doesn't start with -)
  let startIndex = 0;
  if (args.length > 0 && args[0] && !args[0].startsWith('-')) {
    opts.script = args[0];
    startIndex = 1;
  }

  for (let i = startIndex; i < args.length; i++) {
    const a = args[i];
    if (!a) continue;

    if (a === '-j' || a === '--jobs') {
      const nextArg = args[++i];
      if (!nextArg) {
        console.error(`Missing value for ${a}`);
        process.exit(1);
      }
      const jobs = Number(nextArg);
      if (isNaN(jobs) || jobs < 1) {
        console.error(`Invalid jobs value: ${nextArg}`);
        process.exit(1);
      }
      opts.jobs = jobs;
    } else if (a === '--no-dev') {
      opts.includeDev = false;
    } else if (a === '--no-peer') {
      opts.includePeer = false;
    } else if (a === '--filter' || a === '--only') {
      const nextArg = args[++i];
      if (!nextArg) {
        console.error(`Missing value for ${a}`);
        process.exit(1);
      }
      opts.only.push(nextArg);
    } else if (a === '--exclude') {
      const nextArg = args[++i];
      if (!nextArg) {
        console.error(`Missing value for ${a}`);
        process.exit(1);
      }
      opts.exclude.push(nextArg);
    } else if (a === '--print-only') {
      opts.printOnly = true;
    } else if (a === '--sequential') {
      opts.sequential = true;
    } else if (a === '--debug') {
      opts.debug = true;
    } else {
      console.error(`Unknown arg: ${a}`);
      process.exit(1);
    }
  }
  return opts as Opts;
}

function readJSON(p: string): PackageJson {
  try {
    const content = readFileSync(p, 'utf8');
    const parsed = JSON.parse(content) as unknown;

    if (typeof parsed !== 'object' || parsed === null) {
      throw new Error(`Invalid JSON structure in ${p}`);
    }

    const pkg = parsed as Record<string, unknown>;

    if (typeof pkg.name !== 'string') {
      throw new Error(`Missing or invalid name field in ${p}`);
    }

    return pkg as PackageJson;
  } catch (error) {
    if (error instanceof Error) {
      // eslint-disable-next-line preserve-caught-error
      throw new Error(`Failed to read package.json at ${p}: ${error.message}`);
    }
    throw error;
  }
}

function isDir(p: string): boolean {
  try {
    return statSync(p).isDirectory();
  } catch {
    return false;
  }
}

function safeReadDir(d: string): string[] {
  try {
    return readdirSync(d);
  } catch {
    return [];
  }
}

/** Tiny glob matcher: supports * and ** (slash-aware) */
function matchGlob(s: string, glob: string): boolean {
  const esc = glob
    .replace(/[-/\\^$+?.()|[\]{}]/g, '\\$&')
    .replace(/\*\*/g, '.*')
    .replace(/\*/g, '[^/]*');
  const re = new RegExp('^' + esc + '$');
  return re.test(s);
}

/** Expand a glob pattern into directories (supports * and ** segments) */
function expandGlob(root: string, pattern: string): string[] {
  const abs = resolve(root, pattern);
  const parts = relative(root, abs).split(sep);
  const out = new Set<string>();

  function walk(dir: string, i: number) {
    if (i === parts.length) {
      out.add(dir);
      return;
    }
    const seg = parts[i];
    if (seg === '**') {
      walk(dir, i + 1);
      for (const e of safeReadDir(dir)) {
        const p = join(dir, e);
        if (isDir(p)) walk(p, i);
      }
    } else if (seg === '*') {
      for (const e of safeReadDir(dir)) {
        const p = join(dir, e);
        if (isDir(p)) walk(p, i + 1);
      }
    } else {
      const next = join(dir, seg);
      if (isDir(next)) walk(next, i + 1);
    }
  }
  walk(root, 0);
  return [...out];
}

function getWorkspaceDirs(rootPkg: PackageJson): string[] {
  const ws = Array.isArray(rootPkg.workspaces)
    ? rootPkg.workspaces
    : rootPkg.workspaces?.packages || [];
  const dirs: string[] = [];

  for (const patt of ws) {
    if (typeof patt !== 'string') {
      console.warn(`Skipping invalid workspace pattern: ${patt}`);
      continue;
    }

    for (const d of expandGlob(cwd, patt)) {
      try {
        readJSON(join(d, 'package.json'));
        dirs.push(d);
      } catch {
        // ignore
      }
    }
  }
  return [...new Set(dirs)];
}

function collectDeps(
  pkgJson: PackageJson,
  includeDev: boolean,
  includePeer: boolean
): Record<string, string> {
  return {
    ...(pkgJson.dependencies || {}),
    ...(pkgJson.optionalDependencies || {}),
    ...(includeDev ? pkgJson.devDependencies || {} : {}),
    ...(includePeer ? pkgJson.peerDependencies || {} : {})
  };
}

/** Build topo tiers: edges map is dep -> Set(dependant) */
function topoTiers(
  nodes: readonly string[],
  edges: ReadonlyMap<string, ReadonlySet<string>>
): string[][] {
  const indeg = new Map<string, number>(nodes.map((n) => [n, 0]));
  for (const [, outs] of edges) {
    for (const m of outs) {
      indeg.set(m, (indeg.get(m) || 0) + 1);
    }
  }

  const remain = new Set(nodes);
  const tiers: string[][] = [];

  while (remain.size > 0) {
    const zero: string[] = [];
    for (const n of remain) {
      if ((indeg.get(n) || 0) === 0) {
        zero.push(n);
      }
    }

    if (zero.length === 0) {
      // cycle present → put remaining in the last tier
      console.warn(
        'Dependency cycle detected, placing remaining packages in final tier'
      );
      tiers.push([...remain]);
      break;
    }

    tiers.push(zero);
    for (const z of zero) {
      remain.delete(z);
      const outs = edges.get(z);
      if (!outs) continue;
      for (const m of outs) {
        indeg.set(m, Math.max(0, (indeg.get(m) || 0) - 1));
      }
    }
  }
  return tiers;
}

async function runSequential(
  dirsInOrder: readonly string[],
  script: string
): Promise<void> {
  for (const dir of dirsInOrder) {
    console.log(`▶ ${dir}: bun run ${script}`);
    const p = Bun.spawn(['bun', 'run', script], {
      cwd: dir,
      stdout: 'inherit',
      stderr: 'inherit'
    });
    const code = await p.exited;
    if (code !== 0) {
      throw new Error(`Failed in ${dir} with exit code ${code}`);
    }
  }
}

async function runTierParallel(
  dirs: readonly string[],
  script: string,
  concurrency: number
): Promise<void> {
  if (dirs.length === 0) return;

  let i = 0;
  const errs: Error[] = [];

  async function worker(): Promise<void> {
    while (true) {
      const idx = i++;
      if (idx >= dirs.length) break;
      const dir = dirs[idx];
      if (!dir) continue;

      console.log(`▶ ${dir}: bun run ${script}`);
      const p = Bun.spawn(['bun', 'run', script], {
        cwd: dir,
        stdout: 'inherit',
        stderr: 'inherit'
      });
      const code = await p.exited;
      if (code !== 0) {
        errs.push(new Error(`Failed in ${dir} with exit code ${code}`));
      }
    }
  }

  const n = Math.max(1, Math.min(concurrency, dirs.length));
  await Promise.all(Array.from({ length: n }, () => worker()));
  if (errs.length > 0) {
    throw errs[0];
  }
}

async function main(): Promise<void> {
  const opts = parseArgs();

  const rootPkg = readJSON(join(cwd, 'package.json'));
  const wsDirs = getWorkspaceDirs(rootPkg);
  if (wsDirs.length === 0) {
    console.error('No workspaces found in package.json');
    process.exit(1);
  }

  const pkgs: Pkg[] = wsDirs
    .map((dir): Pkg | null => {
      try {
        const json = readJSON(join(dir, 'package.json'));
        return {
          name: json.name,
          dir,
          scripts: json.scripts ?? {},
          json
        };
      } catch (error) {
        console.warn(
          `Failed to load package.json in ${dir}:`,
          error instanceof Error ? error.message : error
        );
        return null;
      }
    })
    .filter((p): p is Pkg => p !== null && p.name.length > 0);

  const selected = pkgs.filter((p) => {
    const matchOnly =
      opts.only.length > 0
        ? opts.only.some((g) => matchGlob(p.name, g) || matchGlob(p.dir, g))
        : true;
    const matchExclude =
      opts.exclude.length > 0
        ? opts.exclude.some((g) => matchGlob(p.name, g) || matchGlob(p.dir, g))
        : false;
    return matchOnly && !matchExclude;
  });

  if (selected.length === 0) {
    console.error('No packages matched selection.');
    process.exit(1);
  }

  // Graph: dep -> dependants (only for internal deps that are also selected)
  const names = new Set(selected.map((p) => p.name));
  const edges = new Map<string, Set<string>>();
  const ensure = (k: string): Set<string> => {
    const existing = edges.get(k);
    if (existing) return existing;
    const newSet = new Set<string>();
    edges.set(k, newSet);
    return newSet;
  };

  for (const p of selected) {
    const all = collectDeps(p.json, opts.includeDev, opts.includePeer);
    for (const depName of Object.keys(all)) {
      if (!names.has(depName)) continue;
      ensure(depName).add(p.name); // dep -> p
    }
  }

  const nodes = selected.map((p) => p.name);
  const tiers = topoTiers(nodes, edges);

  const hasScript = (name: string): boolean => {
    const pkg = selected.find((p) => p.name === name);
    if (!pkg) {
      console.warn(`Package not found: ${name}`);
      return false;
    }
    return Boolean(pkg.scripts[opts.script]);
  };

  if (opts.debug) {
    console.log(`Script: ${opts.script}`);
    console.log(
      `Mode: ${opts.sequential ? 'sequential' : `parallel by tier (jobs=${opts.jobs})`}`
    );
    console.log(
      `Edges: deps + optional${opts.includeDev ? ' + dev' : ''}${opts.includePeer ? ' + peer' : ''}`
    );
    console.log('');
  }

  const tiersDirs: string[][] = tiers.map((tier) =>
    tier.filter(hasScript).map((name) => {
      const pkg = selected.find((p) => p.name === name);
      if (!pkg) {
        throw new Error(`Package not found: ${name}`);
      }
      return pkg.dir;
    })
  );

  const seqDirs = tiers
    .flat()
    .filter(hasScript)
    .map((name) => {
      const pkg = selected.find((p) => p.name === name);
      if (!pkg) {
        throw new Error(`Package not found: ${name}`);
      }
      return pkg.dir;
    });

  if (opts.debug) {
    if (opts.sequential) {
      console.log('Plan (sequential order):');
      seqDirs.forEach((d, i) => console.log(`  ${i + 1}. ${d}`));
      console.log('');
      console.log('Command preview:');
      console.log(
        seqDirs
          .map((d) => `(cd ${JSON.stringify(d)} && bun run ${opts.script})`)
          .join(' && ')
      );
    } else {
      console.log('Plan (tiers):');
      tiersDirs.forEach((dirs, i) => {
        if (!dirs.length) return;
        console.log(`  Tier ${i + 1}:`);
        dirs.forEach((d) => console.log(`    - ${d}`));
      });
      console.log('');
      console.log('Command preview (conceptual):');
      console.log(
        tiersDirs
          .filter((dirs) => dirs.length)
          .map(
            (dirs) =>
              dirs
                .map(
                  (d) => `(cd ${JSON.stringify(d)} && bun run ${opts.script})`
                )
                .join(' & ') + ' && wait'
          )
          .join(' && ')
      );
    }
  }

  if (opts.printOnly) return;

  if (opts.sequential) {
    await runSequential(seqDirs, opts.script);
  } else {
    for (const dirs of tiersDirs) {
      if (dirs.length === 0) continue;
      await runTierParallel(dirs, opts.script, opts.jobs);
    }
  }
}

main().catch((err: unknown) => {
  throw err;
});
