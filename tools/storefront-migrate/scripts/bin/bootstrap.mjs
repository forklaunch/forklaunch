/**
 * Make the skill self-installing.
 *
 * Ensures playwright (the npm package) and its Chromium build are present
 * before anything tries to use them. Runs once; subsequent invocations are a
 * fast no-op. The point is that whoever receives this skill runs one command
 * and it works — no README step they can skip.
 */
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');

function has(mod) {
  try { createRequire(join(ROOT, 'noop.js')).resolve(mod); return true; }
  catch (_) { return false; }
}

export function ensureDeps({ quiet = false } = {}) {
  const say = m => { if (!quiet) console.log(m); };

  if (!has('playwright')) {
    say('   · installing playwright (one time)…');
    const r = spawnSync('npm', ['install', '--no-audit', '--no-fund', '--loglevel', 'error'],
                        { cwd: ROOT, stdio: quiet ? 'ignore' : 'inherit' });
    if (r.status !== 0) {
      console.error('\n✗ could not install dependencies.');
      console.error('  Run manually:  cd ' + ROOT + ' && npm install');
      process.exit(1);
    }
  }

  // Chromium is a separate download from the npm package.
  let needBrowser = true;
  try {
    const { chromium } = createRequire(join(ROOT, 'noop.js'))('playwright');
    needBrowser = !existsSync(chromium.executablePath());
  } catch (_) { needBrowser = true; }

  if (needBrowser) {
    say('   · downloading chromium (one time, ~150MB)…');
    const r = spawnSync('npx', ['playwright', 'install', 'chromium'],
                        { cwd: ROOT, stdio: quiet ? 'ignore' : 'inherit' });
    if (r.status !== 0) {
      console.error('\n✗ could not download chromium.');
      console.error('  Run manually:  cd ' + ROOT + ' && npx playwright install chromium');
      process.exit(1);
    }
  }
}
