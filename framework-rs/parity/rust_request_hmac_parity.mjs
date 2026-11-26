import assert from 'node:assert/strict';
import { readFileSync, realpathSync } from 'node:fs';
import { createRequire, Module } from 'node:module';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const tsc = process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc');
const ts = require(require.resolve('typescript', { paths: [path.dirname(realpathSync(tsc)), path.join(root, 'framework')] }));
function load(relative, dependencies = {}) {
  const source = path.join(root, relative);
  const module = new Module(source);
  module.filename = source;
  module.require = specifier => dependencies[specifier] ?? require(specifier);
  module._compile(ts.transpileModule(readFileSync(source, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, source);
  return module.exports;
}
const common = load('framework/common/src/safeStringify.ts');
const { createHmacToken } = load('framework/core/src/http/createHmacToken.ts', { '@forklaunch/common': common });
const { generateHmacAuthHeaders } = load('framework/core/src/http/generateHmacAuthHeaders.ts', { './createHmacToken': { createHmacToken } });
const fixtures = [];
for (const timestamp of ['2026-10-10T10:00:00.000Z', '2026-10-10T11:30:00.123456+01:30']) {
  for (const [kind, value] of [['absent', undefined], ['text', ''], ['text', '{ "message": "秘密🙂" }'], ['text', 'hello\nworld'], ['bytes', Buffer.alloc(0)], ['bytes', Buffer.from([255, 0, 195, 169, 240, 159, 153, 130])], ['bytes', Buffer.from(JSON.stringify({b: 2, a: 1}))]]) {
    const fixture = { secret: 'cross-language-request-hmac-secret', method: kind === 'absent' ? 'GET' : 'POST', path: '/codeSet/PROC-001', kind,
      body: [...Buffer.from(value ?? '')], timestamp, nonce: `nonce-${fixtures.length}` };
    fixture.signature = createHmacToken({ ...fixture, body: value, timestamp: new Date(timestamp), secretKey: fixture.secret });
    fixtures.push(fixture);
  }
}
// Exercise the actual header generator as well as the canonical signer.
for (const body of [undefined, { message: 'wire JSON', nested: { b: 2, a: 1 } }]) {
  const secret = 'cross-language-request-hmac-secret';
  const method = body === undefined ? 'GET' : 'POST';
  const requestPath = '/codeSet';
  const { authorization } = generateHmacAuthHeaders({ secretKey: secret, method, path: requestPath, body });
  const match = authorization.match(/^HMAC keyId=default ts=(\S+) nonce=(\S+) signature=(\S+)$/);
  assert.ok(match);
  fixtures.push({ secret, method, path: requestPath, kind: body === undefined ? 'absent' : 'bytes',
    body: [...Buffer.from(body === undefined ? '' : common.safeStringify(body))], timestamp: match[1], nonce: match[2], signature: match[3] });
}
const result = spawnSync('cargo', ['run', '--quiet', '--manifest-path', 'framework-rs/Cargo.toml', '-p', 'forklaunch-core','--features','auth', '--example', 'request_hmac_parity'],
  { cwd: root, encoding: 'utf8', input: JSON.stringify(fixtures), timeout: 120000 });
assert.equal(result.status, 0, result.stderr);
const outputs = JSON.parse(result.stdout);
assert.equal(outputs.length, fixtures.length);
for (const [index, actual] of outputs.entries()) {
  const fixture = fixtures[index];
  assert.equal(actual.signature, fixture.signature);
  assert.equal(actual.verified, true);
  assert.equal(actual.header, `HMAC keyId=default ts=${new Date(fixture.timestamp).toISOString()} nonce=${fixture.nonce} signature=${fixture.signature}`);
}
console.log(`PASS actual TypeScript/Rust HMAC request signatures and authorization headers: ${fixtures.length} absent/text/binary/JSON/Unicode/timezone fixtures`);
