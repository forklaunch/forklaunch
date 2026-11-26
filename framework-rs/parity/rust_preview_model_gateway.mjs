import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { spawn, execFileSync } from 'node:child_process';
import { actualHttpSources } from './rust_actual_http_fixture.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { load } = actualHttpSources(root);
const { createPreviewModelGatewayClient } = load(path.join(root, 'framework/core/src/http/previewModelGatewayClient.ts'));
const { createHmacToken } = load(path.join(root, 'framework/core/src/http/createHmacToken.ts'));
const now = 1_800_000_000_000;
const catalog = () => ({ models: [{ id: 'reviewed_.:AZ-09' }], preview: { expiresAt: now + 600000, maxCalls: 20, maxUnits: 100000 } });
const cases = [];
const add = (name, raw, options = {}) => cases.push({ name, raw: typeof raw === 'string' || Buffer.isBuffer(raw) ? raw : JSON.stringify(raw), kind: 'models', ...options });
add('valid', catalog());
add('utf8-bom', '\ufeff' + JSON.stringify(catalog()));
add('duplicate-known-key', JSON.stringify(catalog()).replace('\"maxCalls\":20', '\"maxCalls\":1,\"maxCalls\":20'));
for (const [field, values] of Object.entries({ expiresAt: [now, now - 1, now + 1, now + 900000, now + 900001, Number.MAX_SAFE_INTEGER, null, '1800000000001'], maxCalls: [0, 1, 21, 1.5, null, '1'], maxUnits: [0, 1, 100001, 1.5, null] })) {
  for (const value of values) { const raw = catalog(); raw.preview[field] = value; add(`${field}-${value}`, raw); }
}
for (const id of ['', 'a'.repeat(160), 'a'.repeat(161), 'é', 'a/b', 'a b', 'a\n']) { const raw = catalog(); raw.models[0].id = id; add(`id-${JSON.stringify(id)}`, raw); }
for (const raw of [null, [], {}, { ...catalog(), monthlyTokenBudget: 100 }, { ...catalog(), preview: { ...catalog().preview, remainingCalls: 1 } }, { ...catalog(), models: [] }, { ...catalog(), models: [{ id: 'a' }, { id: 'b' }] }, { ...catalog(), models: [{ id: 'a', baa: false }] }]) add('strict-shape', raw);
for (const literal of ['20.0', '2e1', '20.0000000000000001', '1e309']) add(`number-${literal}`, JSON.stringify(catalog()).replace('"maxCalls":20', `"maxCalls":${literal}`));
add('expired-missing', { models: [{ id: 'a' }], preview: { maxCalls: 1, maxUnits: 1 } });
add('malformed', 'private invalid JSON');
add('empty', '');
add('no-content', '', { status: 204 });
const raw = JSON.stringify(catalog());
for (const chunked of [false, true]) for (const bytes of [8192, 8193]) add(`raw-${bytes}-${chunked}`, raw.padEnd(bytes, ' '), { chunked });
for (const [status, retry] of [[429, '2.5'], [503, '0'], [400, '-1'], [502, 'Infinity']]) add(`refusal-${status}`, 'private gateway credential detail', { status, retry });
for (const created of [undefined, 0, 123]) add('chat', { id: 'reply', object: 'chat.completion', model: 'reviewed', choices: [], usage: { total_tokens: 1 }, extra: { retained: true }, ...(created === undefined ? {} : { created }) }, { kind: 'chat', body: { model: 'reviewed', messages: [{ role: 'user', content: 'Unicode 💬 request' }], temperature: 0.2 } });
const chatOptions = { kind: 'chat', body: { model: 'reviewed', messages: [] } };
add('chat-bom', '\ufeff{"id":"reply"}', chatOptions);
add('chat-invalid-utf8', Buffer.concat([Buffer.from('{"content":"'), Buffer.from([0xff]), Buffer.from('"}')]), chatOptions);
add('chat-invalid-json', 'private malformed body', chatOptions);
for (const literal of ['9007199254740993', '-9007199254740993', '18446744073709551615', '-9223372036854775808', '9007199254740995']) {
  add(`chat-number-${literal}`, `{"number":${literal},"nested":[${literal}]}`, chatOptions);
}
const nonces = new Set();
let queue = [], seen = [], failure;
const server = createServer(async (req, res) => {
  try {
    const item = queue.shift(); assert.ok(item, 'unexpected request/retry');
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const bytes = Buffer.concat(chunks);
    const route = item.kind === 'chat' ? '/models/chat/completions' : '/models';
    assert.equal(req.url, `/prefix/instance-gateway${route}`);
    assert.equal(req.method, item.kind === 'chat' ? 'POST' : 'GET');
    const match = /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(req.headers.authorization ?? '');
    assert.ok(match); assert.equal(match[1], 'preview-fixture'); assert.ok(!nonces.has(match[3])); nonces.add(match[3]);
    assert.equal(match[4], createHmacToken({ method: req.method, path: route, body: bytes.length ? bytes : undefined, timestamp: new Date(match[2]), nonce: match[3], secretKey: 'preview-only-fixture-secret' }));
    if (item.kind === 'chat') assert.deepEqual(JSON.parse(bytes), item.body); else assert.equal(bytes.length, 0);
    seen.push(item.name);
    res.statusCode = item.status ?? 200;
    if (item.retry) res.setHeader('retry-after', item.retry);
    res.setHeader('content-type', 'application/json');
    if (item.chunked) { res.write(item.raw.slice(0, 8000)); setImmediate(() => res.end(item.raw.slice(8000))); }
    else { res.setHeader('content-length', Buffer.byteLength(item.raw)); res.end(item.raw); }
  } catch (error) { failure = error; res.statusCode = 500; res.end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}/prefix`;
const options = { gatewayUrl: base, instanceId: 'preview-fixture', hmacKey: 'preview-only-fixture-secret' };
const originalNow = Date.now;
try {
  const client = createPreviewModelGatewayClient(options), expected = [];
  Date.now = () => now;
  for (const item of cases) {
    queue = [item];
    try { expected.push({ ok: item.kind === 'chat' ? await client.chat.completions.create(item.body) : await client.models() }); }
    catch (error) { expected.push({ error: true, status: error.status ?? null, retryAfterSeconds: error.retryAfterSeconds ?? null }); }
    assert.equal(queue.length, 0); if (failure) throw failure;
  }
  Date.now = originalNow;
  if (process.env.FORKLAUNCH_PREVIEW_GATEWAY_TS_ONLY === '1') {
    console.log(`Preview model gateway actual TypeScript: ${cases.length} cases PASS.`);
  } else {
  if (!process.env.FORKLAUNCH_PREVIEW_GATEWAY_BINARY) execFileSync('cargo', ['build', '--manifest-path', 'framework-rs/Cargo.toml', '-p', 'forklaunch-core', '--features', 'instance-gateway', '--example', 'preview_model_gateway'], { cwd: root, stdio: 'inherit', timeout: 180000 });
  const binary = process.env.FORKLAUNCH_PREVIEW_GATEWAY_BINARY ?? path.join(root, 'framework-rs/target/debug/examples/preview_model_gateway');
  const extras = [
    { name: 'default-above-preview-cap', kind: 'default', raw: raw.padEnd(8193, ' ') },
    { name: 'default-malformed-error-code', kind: 'default', raw: 'invalid private body' },
  ];
  queue = [...cases, ...extras]; seen = [];
  const actions = [...cases, ...extras].map(({ kind, body }) => ({ kind, body }));
  actions.push({ kind: 'invalid-limit', limit: 0 }, { kind: 'invalid-limit', limit: 1048577 });
  const child = spawn(binary, [], { cwd: root, stdio: ['pipe', 'pipe', 'inherit'] });
  let output = ''; child.stdout.on('data', chunk => output += chunk);
  child.stdin.end(JSON.stringify({ base, now, actions }));
  const exit = await new Promise(resolve => child.on('exit', resolve)); assert.equal(exit, 0);
  if (failure) throw failure; assert.equal(queue.length, 0); assert.equal(seen.length, cases.length + extras.length);
  const actual = JSON.parse(output);
  for (let index = 0; index < cases.length; index++) {
    const result = actual[index];
    if (result.ok && typeof result.ok.number === 'number') {
      const bits = Buffer.alloc(8); bits.writeDoubleBE(expected[index].ok.number);
      assert.equal(result.numberBits, bits.toString('hex'), cases[index].name);
      assert.equal(result.numberFloat, true); delete result.numberBits; delete result.numberFloat;
    }
    if (result.error) { assert.doesNotMatch(result.description, /private|credential|fixture-secret/); delete result.description; delete result.code; }
    assert.deepEqual(result, JSON.parse(JSON.stringify(expected[index])), cases[index].name);
  }
  assert.deepEqual(actual[cases.length], { ok: catalog() });
  assert.equal(actual[cases.length + 1].code, 'provider_transport_error');
  for (const result of actual.slice(-2)) assert.equal(result.code, 'provider_invalid_config');
  assert.equal(nonces.size, cases.length * 2 + extras.length);
  const unavailable = createServer();
  await new Promise(resolve => unavailable.listen(0, '127.0.0.1', resolve));
  const unavailableBase = `http://127.0.0.1:${unavailable.address().port}`;
  await new Promise(resolve => unavailable.close(resolve));
  let networkError;
  try { await createPreviewModelGatewayClient({ ...options, gatewayUrl: unavailableBase }).models(); }
  catch (error) { networkError = error; }
  assert.ok(networkError); assert.equal(networkError.status ?? null, null);
  const network = JSON.parse(execFileSync(binary, [], { cwd: root, input: JSON.stringify({ base: unavailableBase, now, actions: [{ kind: 'models' }] }), encoding: 'utf8', timeout: 20000 }))[0];
  assert.equal(network.error, true); assert.equal(network.status, null);
  assert.equal(network.code, 'provider_transport_error');
  assert.doesNotMatch(network.description, /fixture-secret|127\.0\.0\.1/);
  console.log(`Preview model gateway: ${cases.length} actual TS/native catalog/chat/error comparisons, raw8192/chunked bounds, HMAC bytes/path/fresh nonces, default-limit regressions and no retries PASS.`);
}
} finally { Date.now = originalNow; server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
