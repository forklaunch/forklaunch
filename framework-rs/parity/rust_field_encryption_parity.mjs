import assert from 'node:assert/strict';
import { readFileSync, realpathSync } from 'node:fs';
import { createRequire, Module } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const tsc = process.env.TSC_BIN || join(root, 'framework/node_modules/.bin/tsc');
const typescript = require(require.resolve('typescript', { paths: [dirname(realpathSync(tsc)), join(root, 'framework')] }));
// Execute the actual TypeScript implementation, rather than a JS reimplementation
// of its wire format. Its sole runtime dependency is Node's crypto module.
const source = join(root, 'framework/core/src/persistence/fieldEncryptor.ts');
const compiled = typescript.transpileModule(readFileSync(source, 'utf8'), {
  compilerOptions: { module: typescript.ModuleKind.CommonJS, target: typescript.ScriptTarget.ES2022, esModuleInterop: true }
}).outputText;
const loaded = new Module(source);
loaded.filename = source;
loaded._compile(compiled, source);
const { FieldEncryptor } = loaded.exports;
const master = 'current-key-for-cross-language-tests';
const previous = ['previous-key-for-cross-language-tests'];
const fixtures = [];
for (const format of ['v2', 'v3']) {
  for (const [plaintext, tenant, anon] of [['private@example.com', 'tenant-a', 'p***@example.com'], ['', '', ''], ['秘密\u0000é🙂', 'tenant:水', 'anonymous:水']]) {
    const old = new FieldEncryptor(previous[0]);
    const current = new FieldEncryptor(master, { previousKeys: previous, format });
    // Generate legacy random-nonce v1 data with the same tenant-derived key.
    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv('aes-256-gcm', old.deriveKey(tenant), iv);
    const data = Buffer.concat([cipher.update(plaintext, 'utf8'), cipher.final()]);
    const legacy = `v1:${iv.toString('base64')}:${cipher.getAuthTag().toString('base64')}:${data.toString('base64')}`;
    const ciphertexts = [legacy, old.encrypt(plaintext, tenant), old.withFormat('v3').encrypt(plaintext, tenant), old.seal(plaintext, tenant, anon), current.encrypt(plaintext, tenant), current.seal(plaintext, tenant, anon), current.encrypt(plaintext, `${tenant}-wrong`), 'v2:bad:bad:bad'];
    fixtures.push({ master, previous, format, plaintext, tenant, anon, ciphertexts });
  }
}
const run = spawnSync('cargo', ['run', '--quiet', '--manifest-path', join(root, 'framework-rs/Cargo.toml'), '-p', 'forklaunch-core', '--features', 'encryption', '--example', 'field_encryption_parity'], {
  input: JSON.stringify(fixtures), encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 300000
});
assert.equal(run.status, 0, `${run.error || ''}\n${run.stderr}`);
const outputs = JSON.parse(run.stdout);
for (const [index, fixture] of fixtures.entries()) {
  const actual = outputs[index];
  const expected = new FieldEncryptor(master, { previousKeys: previous, format: fixture.format });
  assert.deepEqual(actual.keyIds, [expected.keyIds.current, ...expected.keyIds.previous]);
  assert.equal(actual.encrypted, expected.encrypt(fixture.plaintext, fixture.tenant));
  assert.equal(actual.blindIndex, expected.blindIndex(fixture.plaintext, fixture.tenant));
  assert.deepEqual(actual.blindIndexes, expected.blindIndexes(fixture.plaintext, fixture.tenant));
  assert.equal(expected.decrypt(actual.encrypted, fixture.tenant), fixture.plaintext);
  assert.equal(expected.decrypt(actual.sealed, fixture.tenant), fixture.plaintext);
  assert.equal(expected.open(actual.sealed, fixture.tenant).anon, fixture.anon);
  for (const [position, ciphertext] of fixture.ciphertexts.entries()) {
    let opened;
    try { opened = expected.open(ciphertext, fixture.tenant); } catch { opened = null; }
    const value = actual.opened[position];
    if (!opened) { assert.equal(value, null); continue; }
    const { rotated, ...metadata } = value;
    assert.deepEqual(metadata, { ...opened, anon: opened.anon ?? null });
    const rotation = expected.open(rotated, fixture.tenant);
    assert.equal(rotation.plaintext, fixture.plaintext);
    assert.equal(rotation.current, true);
    assert.equal(rotation.anon, opened.anon);
    if (opened.version !== 'v4') assert.equal(rotated, expected.rotate(ciphertext, fixture.tenant));
  }
}
console.log(`PASS TypeScript/Rust encryption conformance: ${fixtures.length} fixtures, v1-v4 reads, v2/v3 equality, v4 writes, tenant isolation, rotation, and blind indexes`);
