import { readFileSync, mkdtempSync, writeFileSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
const source = readFileSync(new URL('../../src/typescript/websocket-codec.ts', import.meta.url), 'utf8');
const helper = source.split('// End transport admission.')[0]
  .replace('value: string): URL', 'value)');
const admit = new Function(`${helper}; return socketTransportUrl;`)();
for (const endpoint of ['ws://example.test/socket', 'ws://localhost.evil.test/socket', 'ws://0.0.0.0/socket',
  'wss://user:secret@example.test/socket', 'wss://example.test/socket#secret', 'http://localhost/socket']) {
  assert.throws(() => admit(endpoint));
}
for (const endpoint of ['wss://example.test/socket', 'ws://localhost:8000/socket', 'ws://127.0.0.1:8000/socket', 'ws://[::1]:8000/socket']) {
  assert.equal(admit(endpoint).toString(), endpoint);
}
console.log('PASS generated WebSocket transport rejects remote plaintext, credential URLs and downgrade schemes');
const sseSource = readFileSync(new URL('../../src/typescript/transport.ts', import.meta.url), 'utf8');
const streamHelper = sseSource.split('// End HTTP transport admission.')[0]
  .replace('value: string, allowPlaintextDevelopment: boolean = false): void', 'value, allowPlaintextDevelopment = false)');
const admitStream = new Function(`${streamHelper}; return requireSecureHttpTransport;`)();
for (const endpoint of ['http://example.test/events', 'http://localhost.evil.test/events', 'https://user:secret@example.test/events', 'https://example.test/events#secret']) {
  assert.throws(() => admitStream(endpoint));
}
for (const endpoint of ['https://example.test/events', 'http://127.0.0.1:8000/events', 'http://[::1]:8000/events']) {
  assert.doesNotThrow(() => admitStream(endpoint));
}
console.log('PASS generated event stream transport rejects remote plaintext and credential-bearing URLs');

assert.doesNotThrow(() => admitStream('http://notes:8000/v1/notes', true));
assert.throws(() => admitStream('ftp://notes:8000/v1/notes', true));
assert.throws(() => admitStream('http://user:secret@notes:8000/v1/notes', true));
console.log('PASS explicit HTTP development exception retains scheme and credential URL restrictions');

// Execute the generated, reviewed SDK rather than only its standalone helper.
const { stripTypeScriptTypes } = await import('node:module');
const generated = readFileSync(new URL('../../src/typescript/snapshots/catalog.typescript.snap', import.meta.url), 'utf8');
const directory = mkdtempSync(join(tmpdir(), 'forklaunch-transport-sdk-'));
process.on('exit', () => rmSync(directory, { recursive: true, force: true }));
const require = createRequire(import.meta.url);
const compiler = process.env.FORKLAUNCH_TYPESCRIPT_COMPILER ?? require.resolve('typescript/bin/tsc');
symlinkSync(process.env.FORKLAUNCH_SDK_NODE_MODULES ?? dirname(dirname(dirname(compiler))), join(directory, 'node_modules'), 'dir');
writeFileSync(join(directory, 'client.mjs'), stripTypeScriptTypes(generated, { mode: 'transform' }));
const { createForkLaunchSdk } = await import(pathToFileURL(join(directory, 'client.mjs')));
let calls = 0;
let lastInit;
let lastUrl;
const fetcher = async (_url, init) => {
  calls += 1;
  lastInit = init;
  lastUrl = String(_url);
  return new Response(JSON.stringify({ id: 'note', name: 'Note' }), { status: 200, headers: { 'content-type': 'application/json', etag: 'rev1', 'x-count': '7' } });
};
await assert.rejects(() => createForkLaunchSdk({ baseUrl: 'http://example.test', fetch: fetcher }).catalog.items.getItem({ params: { id: 'note' } }));
assert.equal(calls, 0);
await createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: fetcher }).catalog.items.getItem({ params: { id: 'note' }, redirect: 'follow' });
assert.equal(lastInit.redirect, 'manual');
await createForkLaunchSdk({ baseUrl: 'http://notes:8000', fetch: fetcher, allowPlaintextDevelopment: true }).catalog.items.getItem({ params: { id: 'note' } });
assert.equal(calls, 2);
console.log('PASS generated SDK rejects HTTP before fetch, forces no redirects, and supports named local exception');

await createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: fetcher }).catalog.items.getItem({
  params: { id: 'note' }, query: { includeDetails: true },
  headers: { 'x-request-id': 'r-1', 'x-revision': 3, 'x-filter': { active: true }, 'x-omitted': undefined }
});
assert.equal(new URL(lastUrl).searchParams.get('includeDetails'), 'true');
assert.equal(lastInit.headers.get('x-revision'), '3');
assert.equal(lastInit.headers.get('x-filter'), '{"active":true}');
assert.equal(lastInit.headers.has('x-omitted'), false);
console.log('PASS generated SDK serializes typed scalar/structured metadata and omits undefined headers');

for (const contentType of ['text/plain', 'text/html', 'text/csv']) {
  const sdk = createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: async () => new Response('<p>hello</p>', { status: 200, headers: { 'content-type': `${contentType}; charset=utf-8` } }) });
  const response = await sdk.catalog.items.getItem({ params: { id: 'note' } });
  assert.equal(response.status, 200);
  assert.equal(response.contentType, contentType);
  assert.equal(response.response, '<p>hello</p>');
}
await assert.rejects(() => createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: async () => new Response('bad', { status: 200, headers: { 'content-type': 'application/xml' } }) }).catalog.items.getItem({ params: { id: 'note' } }), /Undeclared response content type/);
console.log('PASS generated SDK discriminates same-status JSON/text/HTML and rejects undeclared media');

const pdf = await createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: async () => new Response(new Uint8Array([1, 2, 3]), { status: 200, headers: { 'content-type': 'application/pdf' } }) }).catalog.items.getItem({ params: { id: 'note' } });
assert.equal(pdf.contentType, 'application/pdf');
assert.deepEqual(pdf.response, new Uint8Array([1, 2, 3]));
console.log('PASS generated SDK selects CSV text and PDF bytes without raw serde/body conventions');

const redirect = await createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: async (_url, init) => {
  assert.equal(init.redirect, 'manual');
  return new Response(null, { status: 302, headers: { location: '/next' } });
} }).catalog.items.getItem({ params: { id: 'note' } });
assert.equal(redirect.status, 302);
assert.equal(redirect.response, undefined);
await createForkLaunchSdk({ baseUrl: 'https://example.test', fetch: async (_url, init) => {
  assert.equal(init.redirect, 'error');
  return new Response(JSON.stringify({ id: 'n', name: 'note' }), { status: 201, headers: { 'content-type': 'application/json' } });
} }).catalog.items.createItem({ body: { id: 'n', name: 'note' }, redirect: 'follow' });
console.log('PASS declared redirects are observed without following; other operations refuse redirects');

const headerResponse = await createForkLaunchSdk({ baseUrl:'https://example.test', fetch:fetcher }).catalog.items.getItem({ params:{id:'note'} });
assert.deepEqual(headerResponse.headers,{etag:'rev1','x-count':7});
await assert.rejects(()=>createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response('{}',{status:200,headers:{'content-type':'application/json'}})}).catalog.items.getItem({params:{id:'note'}}),/Missing required response header/);
console.log('PASS generated SDK decodes typed response headers and rejects required omissions');
