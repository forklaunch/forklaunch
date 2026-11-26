// Exercise emitted code against deliberately nonconforming remote responses.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtempSync, readFileSync, writeFileSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
const require = createRequire(import.meta.url);
const compiler = process.env.FORKLAUNCH_TYPESCRIPT_COMPILER ?? require.resolve('typescript/bin/tsc');
const dependencies = process.env.FORKLAUNCH_SDK_NODE_MODULES ?? dirname(dirname(dirname(compiler)));
const directory = mkdtempSync(join(tmpdir(), 'forklaunch-response-validation-'));
try {
  symlinkSync(dependencies, join(directory, 'node_modules'), 'dir');
  writeFileSync(join(directory, 'package.json'), '{"type":"module"}');
  const source = readFileSync(new URL('../../src/typescript/snapshots/catalog.typescript.snap', import.meta.url), 'utf8');
  writeFileSync(join(directory, 'client.ts'), source);
  execFileSync(process.execPath, [compiler, '--strict', '--skipLibCheck', '--target', 'ES2022', '--module', 'NodeNext', '--moduleResolution', 'NodeNext', '--lib', 'ES2022,DOM,DOM.Iterable', join(directory, 'client.ts')], { stdio: 'inherit' });
  const { createForkLaunchSdk } = await import(pathToFileURL(join(directory, 'client.js')));
  const sdk = (response, overrides = {}) => createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: async () => response, ...overrides }).catalog.items;
  const headers = { 'content-type': 'application/json', etag: 'revision', 'x-count': '2' };
  const get = (response, overrides) => sdk(response, overrides).getItem({ params: { id: 'one' }, query: {} });
  const json = (body, more = {}) => new Response(JSON.stringify(body), { status: 200, headers: { ...headers, ...more } });
  const valid = { id: 'one', name: 'First note' };
  assert.deepEqual((await get(json(valid))).response, valid);
  for (const body of [{ id: 1, name: 'valid' }, { id: 'one', name: '' }, { ...valid, unexpected: true }]) {
    await assert.rejects(get(json(body)), /declared schema/);
  }
  await assert.rejects(get(json(valid, { 'x-count': '-1' })), /response header/);
  await assert.rejects(get(json(valid, { 'x-count': '1.5' })), /response header/);
  await assert.rejects(get(new Response('{invalid', { headers })), SyntaxError);
  await assert.rejects(get(new Response('{}', { status: 418, headers })), /Undeclared response status/);
  await assert.rejects(get(new Response('{}', { status: 200, headers: { 'content-type': 'application/xml' } })), /content type/);
  assert.equal((await get(new Response('Unauthorized', { status: 401, headers: { 'content-type': 'text/plain' } }))).response, 'Unauthorized');
  await assert.rejects(get(json({ id: 42, name: '' }), { beforeRequest: meta => {
    meta.responseSchemas = {}; meta.responseKinds = {}; return meta;
  } }), /declared schema/);
  let cancelled = 0;
  const stream = text => new Response(new ReadableStream({
    start(controller) { controller.enqueue(new TextEncoder().encode(text)); },
    cancel() { cancelled++; }
  }), { headers: { 'content-type': 'text/event-stream' } });
  const good = (await sdk(stream('id: one\ndata: {"id":"one"}\n\n')).watchItems()).response[Symbol.asyncIterator]();
  assert.deepEqual((await good.next()).value, { data: { id: 'one' }, id: 'one' });
  await good.return();
  for (const wire of ['data: {"id":7}\n\n', 'data: not-json\n\n']) {
    const events = (await sdk(stream(wire)).watchItems()).response[Symbol.asyncIterator]();
    await assert.rejects(events.next());
  }
  const native = (await sdk(stream('id: revision\ndata: {"id":"native"}\n\n')).nativeEvents()).response[Symbol.asyncIterator]();
  assert.deepEqual((await native.next()).value, { data: { id: 'native' }, id: 'revision' });
  await native.return();
  const invalidNative = (await sdk(stream('data: {"id":""}\n\n')).nativeEvents()).response[Symbol.asyncIterator]();
  await assert.rejects(invalidNative.next(), /declared schema/);
  assert.equal(cancelled, 5, 'Success cancellation and invalid events release their stream readers');
  console.log('PASS generated HTTP/SSE schemas, header constraints, status/media rejection, hook isolation and stream cancellation');
} finally { rmSync(directory, { recursive: true, force: true }); }
