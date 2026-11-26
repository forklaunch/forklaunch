import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const parserPath = require.resolve(process.env.ASYNCAPI_PARSER_PATH || '@asyncapi/parser');
const { Parser } = require(parserPath);
const Ajv = createRequire(parserPath)('ajv');
const temporary = mkdtempSync(join(tmpdir(), 'forklaunch-asyncapi-parity-'));
try {
  const output = join(temporary, 'asyncapi.json');
  const run = spawnSync('cargo', ['test', '--quiet', '--manifest-path', join(root, 'framework-rs/Cargo.toml'), '-p', 'forklaunch-core','--features','build,asyncapi-import', '--all-features'], {
    cwd: root, env: { ...process.env, FORKLAUNCH_TEST_ASYNCAPI_DOCUMENT: output, FORKLAUNCH_TEST_ASYNCAPI_TRAITS: `${output}.traits.json` },
    encoding: 'utf8', timeout: 300000, maxBuffer: 16 * 1024 * 1024
  });
  assert.equal(run.status, 0, `${run.error || ''}\n${run.stdout}\n${run.stderr}`);
  for (const path of [output, `${output}.typed.json`]) {
    const { document, diagnostics } = await new Parser().parse(readFileSync(path, 'utf8'));
    const errors = diagnostics.filter(diagnostic => diagnostic.severity === 0);
    assert.deepEqual(errors, [], JSON.stringify(errors, null, 2));
    assert.ok(document, 'The official parser must produce a document');
  }
  const traitFixture = JSON.parse(readFileSync(`${output}.traits.json`, 'utf8'));
  const traitResult = await new Parser().parse(JSON.stringify(traitFixture.document));
  assert.deepEqual(traitResult.diagnostics.filter(d => d.severity === 0), []);
  assert.ok(traitResult.document);
  const merged = traitResult.document.json().components.messages.Request;
  for (const field of ['contentType', 'description', 'tags', 'x-settings']) {
    assert.deepEqual(traitFixture.message[field], merged[field], `trait merge: ${field}`);
  }
  console.log('PASS official AsyncAPI trait merging: local references, ordering, target precedence, arrays and nested null removal');
  const typed = JSON.parse(readFileSync(`${output}.typed.json`, 'utf8'));
  const validator = new Ajv({ strict: false });
  validator.addSchema(typed, 'events');
  const submit = validator.compile({ $ref: 'events#/channels/events/messages/submit/payload' });
  assert.equal(submit({ event: 'submit', payload: { id: 42 }, correlationId: 'request-1' }), true);
  assert.equal(submit({ event: 'submit', payload: { id: 'wrong' } }), false);
  assert.equal(submit({ event: 'created', payload: { id: 42 } }), false);
  assert.equal(submit({ event: 'submit' }), false);
  console.log('PASS official AsyncAPI 3.0 parser: grouped event channels, typed WebSocket envelopes, controls, schemas and escaped references');
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
