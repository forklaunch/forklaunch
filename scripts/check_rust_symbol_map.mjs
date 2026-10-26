// Check evidence pointers; only executing the linked fixtures proves behavior.
import assert from 'node:assert/strict';
import {readFileSync, realpathSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = realpathSync(fileURLToPath(new URL('..', import.meta.url)));
const read = relative => {
  assert.equal(typeof relative, 'string');
  assert.ok(!path.isAbsolute(relative), `absolute source path: ${relative}`);
  const resolved = realpathSync(path.resolve(root, relative));
  assert.ok(resolved.startsWith(root + path.sep), `source escapes repository: ${relative}`);
  return readFileSync(resolved, 'utf8');
};
const inventory = JSON.parse(read('plan/rust-typescript-public-api.json'));
const mapping = JSON.parse(read('plan/rust-typescript-symbol-map.json'));
assert.equal(mapping.format, 1);
assert.ok(mapping.mappings.length > 0);
const seen = new Set();
const packages = new Set();
for (const row of mapping.mappings) {
  const key = JSON.stringify([row.package, row.source, row.export]);
  assert.ok(!seen.has(key), `duplicate declaration: ${key}`);
  seen.add(key);
  packages.add(row.package);
  const sourcePackage = inventory.packages.find(pkg => pkg.name === row.package);
  assert.ok(sourcePackage, `unknown package: ${row.package}`);
  const file = sourcePackage.files.find(file => file.source === row.source);
  assert.ok(file?.exports.some(item => item.kind !== 'reexport' && item.name === row.export),
    `missing source declaration: ${key}`);
  read(row.source);
  assert.equal(row.status, 'covered-domain');
  assert.ok(typeof row.domain === 'string' && row.domain.trim());
  assert.ok(Array.isArray(row.boundaries) && row.boundaries.length > 0);
  assert.ok(row.boundaries.every(boundary => typeof boundary === 'string' && boundary.trim()));
  assert.match(row.rustSymbol, /^[a-z][a-z0-9_]*$/);
  const native = read(row.rustSource);
  assert.ok(new RegExp(`\\bpub\\s+fn\\s+${row.rustSymbol}\\s*\\(`).test(native),
    `missing native public function: ${row.rustSource}:${row.rustSymbol}`);
  const fixture = read(row.fixture);
  assert.ok(fixture.includes(path.basename(row.source)), `fixture lacks actual source: ${key}`);
  const consumer = read(row.nativeFixture);
  assert.ok(consumer.includes(`"${row.export}"`) && consumer.includes(row.rustSymbol),
    `native fixture lacks comparison output: ${key}`);
}
console.log(`PASS symbol evidence pointers: ${seen.size} declarations across ${packages.size} package(s); unlisted exports remain open, behavior requires linked fixture execution`);
