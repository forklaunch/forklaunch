import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

execFileSync(process.execPath, [fileURLToPath(new URL('./rust_api_inventory.mjs', import.meta.url)), '--check'], {stdio: 'inherit'});
const {packages} = JSON.parse(readFileSync(new URL('../plan/rust-typescript-public-api.json', import.meta.url), 'utf8'));
const pkg = name => {
  const found = packages.find(value => value.name === '@forklaunch/' + name);
  assert.ok(found, name);
  return found;
};
// Published current counts must describe the checked artifacts, not an older receipt.
const inventoryText = readFileSync(new URL('../plan/RUST-TYPESCRIPT-PARITY-INVENTORY.md', import.meta.url), 'utf8');
const entries = packages.reduce((count, value) => count + value.entries.length, 0);
const declarations = packages.reduce((count, value) => count + value.files.reduce((sum, file) => sum + file.exports.length, 0), 0);
assert.ok(inventoryText.includes(`It records ${entries.toLocaleString('en-US')} resolved declaration entry points and ${declarations.toLocaleString('en-US')} declaration/re-export`),
  'Current inventory introduction has stale entry/declaration counts');
const plan = readFileSync(new URL('../plan/RUST-FRAMEWORK-BLUEPRINT-IMPLEMENTATION.md', import.meta.url), 'utf8');
const completed = [...plan.matchAll(/^- \[x\]/gm)].length;
const open = [...plan.matchAll(/^- \[ \]/gm)].length;
const total = completed + open;
assert.ok(total > 0, 'Checklist is empty');
assert.ok(plan.includes(`**${completed}/${total} checklist items (${(100 * completed / total).toFixed(1)}%), ${open} open**`),
  'Current checklist introduction has stale progress counts');
assert.equal(packages.length, 44);
assert.equal(packages.filter(value => value.private).length, 13);
for (const value of packages) {
  for (const entry of value.entries) assert.ok(!entry.unresolved, `${value.name}:${entry.subpath}`);
  for (const file of value.files) for (const entry of file.exports) {
    assert.ok(!entry.unresolved, `${file.source}:${entry.specifier}`);
    assert.ok(entry.line > 0);
    for (const member of entry.members ?? []) assert.ok(!/^(private|protected)\s/.test(member.signature));
  }
}
// RootDir=src must map wildcard exports to src, not a nonexistent root file.
assert.equal(pkg('common').entries.find(value => value.subpath === '.').source, 'framework/common/src/index.ts');
assert.ok(pkg('common').entries.some(value => value.subpath === './types/dto.types'));
const dto = pkg('common').files.find(value => value.source.endsWith('/dto.types.ts'));
assert.ok(dto.exports.some(value => value.name === 'IdDto'));
// Explicitly published test helpers are API, even though ordinary tests are not.
assert.equal(pkg('validator').entries.find(value => value.subpath === './tests/mockSchemaValidator').source,
  'framework/validator/__test__/utils/mockSchemaValidator.ts');
// Packages without a root entry retain their named subpaths and DTO/service graph.
assert.ok(!pkg('implementation-billing-base').entries.some(value => value.subpath === '.'));
assert.ok(pkg('implementation-billing-base').entries.some(value => value.subpath === './services'));
assert.ok(pkg('implementation-billing-base').files.some(file => file.exports.some(value => value.members?.length)));
assert.ok(pkg('blueprint-iam-base').moduleSources.some(value => value.includes('/controllers/')));
assert.equal(pkg('blueprint-iam-base').entries.length, 0);
assert.equal(pkg('bunrun').bins.bunrun, 'dist/cli.js');
assert.ok(packages.some(value => value.files.some(file => file.exports.some(entry => entry.external))));
console.log('PASS public API inventory: manifest coverage, wildcard/root mappings, public DTO/service members, external boundaries, private modules and CLI bins');
