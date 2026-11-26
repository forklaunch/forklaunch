// Shared deterministic JSON baselines for actual TypeScript/native comparisons.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

// Sort only object keys; preserve array order, literal keys and schema values.
function normalized(value) {
  if (Array.isArray(value)) return value.map(normalized);
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map(key => [key, normalized(value[key])]));
  }
  return value;
}

export async function assertJsonSnapshot(root, name, value) {
  assert.match(name, /^[a-z0-9_]+\.snap\.json$/);
  const file = path.join(root, 'framework-rs/parity/fixtures', name);
  const encoded = JSON.stringify(normalized(value), null, 2) + '\n';
  if (process.env.FORKLAUNCH_UPDATE_SNAPSHOTS === '1') {
    assert.equal(process.env.CI, undefined, 'CI must never rewrite snapshots');
    await writeFile(file, encoded);
  }
  assert.equal(encoded, await readFile(file, 'utf8'),
    `${name} changed; review the diff using FORKLAUNCH_UPDATE_SNAPSHOTS=1`);
}
