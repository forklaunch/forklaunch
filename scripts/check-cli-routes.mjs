#!/usr/bin/env node
/**
 * Does every URL the CLI builds correspond to a route the platform mounts?
 *
 * `forklaunch github connect|disconnect|status` shipped for months calling
 * `/applications/:id/github/...` while platform-management mounts those
 * handlers under `/github-app/applications/:id/github/...`. Every run 404'd,
 * and nothing caught it: the CLI compiles, its tests pass (they assert on
 * argument parsing, not on URLs), and the platform is a separate repository.
 *
 * This is the cheap half of a parity lint — it does not check that every route
 * HAS a caller, only that every caller has a route. That is the half that
 * catches a 404 before a customer does.
 *
 *   node scripts/check-cli-routes.mjs --platform ../forklaunch-platform
 *
 * Exits 1 when a CLI path matches no mounted route.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const args = process.argv.slice(2);
const platformArg = args.indexOf('--platform');
const PLATFORM =
  platformArg !== -1
    ? args[platformArg + 1]
    : process.env.FORKLAUNCH_PLATFORM_PATH || '../forklaunch-platform';
const CLI_SRC = join(process.cwd(), 'cli/src');

const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  'templates',
  'dist',
  '.git'
]);

function walk(dir, ext, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (SKIP_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    let st;
    try {
      st = statSync(full);
    } catch {
      continue;
    }
    if (st.isDirectory()) walk(full, ext, out);
    else if (entry.endsWith(ext)) out.push(full);
  }
  return out;
}

// ── 1. Every route the platform mounts ───────────────────────────────────────
// A router declares its base (`forklaunchRouter('/applications', ...)`) and
// mounts handlers onto it (`applicationRouter.get('/:id', getApplication)`).
function mountedRoutes() {
  const routes = [];
  for (const file of walk(join(PLATFORM, 'src/modules'), '.ts')) {
    if (!file.includes('/api/routes/')) continue;
    const src = readFileSync(file, 'utf8');
    const bases = new Map();
    for (const m of src.matchAll(
      /(?:export\s+)?const\s+(\w+)\s*=\s*forklaunchRouter\(\s*(['"`])([^'"`]*)\2/g
    )) {
      bases.set(m[1], m[3]);
    }
    for (const m of src.matchAll(
      /(\w+)\.(get|post|put|patch|delete)\(\s*(['"`])([^'"`]*)\3/g
    )) {
      const base = bases.get(m[1]);
      if (base === undefined) continue;
      const path = `${base.replace(/\/$/, '')}/${m[4].replace(/^\//, '')}`
        .replace(/\/$/, '');
      routes.push({ method: m[2].toUpperCase(), path: path || '/' });
    }
  }
  return routes;
}

// ── 2. Every URL the CLI builds ──────────────────────────────────────────────
// `format!("{}/applications/{}/...", get_x_api_url(), id)` and the
// `managed_url("/templates")` helper, which prefixes /managed-mode.
function cliPaths() {
  const found = [];
  for (const file of walk(CLI_SRC, '.rs')) {
    if (file.includes('/tests/')) continue;
    const rel = relative(CLI_SRC, file);
    const flat = readFileSync(file, 'utf8').replace(/\s+/g, ' ');
    // Only files that actually build a platform URL. core/docker.rs has volume
    // mounts, core/openapi_export.rs has $PATH entries; neither is an API call.
    if (!/_api_url\(\)|managed_url\(/.test(flat)) continue;

    const add = (raw) => {
      const path = raw.split('?')[0].replace(/\{[^}]*\}/g, ':p');
      const segs = path.split('/').filter(Boolean);
      // A path that is only wildcards tells us nothing.
      if (segs.every((s) => s === ':p')) return;
      // `"{}/x{}"` concatenates a variable onto a segment ("readiness:p",
      // "explorer:p"). The real path is decided at runtime, so it cannot be
      // resolved statically — checking it would only produce false alarms.
      if (segs.some((s) => s !== ':p' && s.includes(':p'))) return;
      if (path.startsWith('/dashboard') || path.startsWith('/api/auth')) return;
      // `"{:>5.1}/100"` is a score, not a route. A real API path's first
      // segment is a word.
      if (!/^[a-z][a-z0-9-]*$/i.test(segs[0])) return;
      found.push({ path, file: rel });
    };
    for (const m of flat.matchAll(/"\{[a-z_]*\}(\/[^"]{1,200})"/g)) add(m[1]);
    if (rel.startsWith('managed/')) {
      for (const m of flat.matchAll(
        /"(\/(?:templates|instances|rollouts|summary)[^"]{0,200})"/g
      )) {
        if (m[1].includes('/abc/') || m[1].endsWith('/clinic')) continue; // fixtures
        add(`/managed-mode${m[1]}`);
      }
    }
  }
  return found;
}

const seg = (p) =>
  p
    .split('/')
    .filter(Boolean)
    .map((s) => (s.startsWith(':') ? ':x' : s));

function matches(routePath, cliPath) {
  const a = seg(routePath);
  const b = seg(cliPath);
  if (a.length !== b.length) return false;
  return a.every((x, i) => x === b[i] || x === ':x' || b[i] === ':x');
}

const routes = mountedRoutes();
if (routes.length === 0) {
  console.error(
    `No routes found under ${PLATFORM}/src/modules — is --platform correct?`
  );
  process.exit(2);
}

const paths = cliPaths();
const unmatched = [];
const seen = new Set();
for (const { path, file } of paths) {
  const key = `${path}|${file}`;
  if (seen.has(key)) continue;
  seen.add(key);
  if (!routes.some((r) => matches(r.path, path))) unmatched.push({ path, file });
}

console.log(
  `${routes.length} mounted routes · ${seen.size} distinct CLI paths · ${unmatched.length} unmatched`
);

if (unmatched.length > 0) {
  console.error('\nCLI paths with no matching mounted route:\n');
  for (const { path, file } of unmatched.sort((x, y) =>
    x.path.localeCompare(y.path)
  )) {
    console.error(`  ${path.padEnd(64)} ${file}`);
  }
  console.error(
    '\nEither the route moved on the platform, or the CLI is building the wrong URL.'
  );
  process.exit(1);
}

console.log('Every CLI path resolves to a mounted route.');
