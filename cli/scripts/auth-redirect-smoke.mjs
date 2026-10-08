// Node 24+: node --experimental-strip-types cli/scripts/auth-redirect-smoke.mjs [IAM directory]
// Runs the actual registered callback bodies with synthetic sessions; no server/database/cloud access.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
const root = resolve(process.argv[2] ?? 'blueprint/iam-better-auth');
const utils = await import(pathToFileURL(resolve(root, 'domain/utils/authRedirect.util.ts')));
const source = readFileSync(resolve(root, 'server.ts'), 'utf8');
const handlersSource = source.slice(source.search(/app\.internal\.get\(['"]\/api\/auth\/test-callback['"]/), source.indexOf('//! registers the betterAuth middleware'));
function fixture(authOk = true) {
  const handlers = new Map();
  const calls = [];
  runInNewContext(stripTypeScriptTypes(handlersSource), {
    ...utils, URL, URLSearchParams, AbortSignal,
    app: { internal: { get: (path, fn) => handlers.set(path, fn) } },
    ci: { resolve: () => ['https://app.example'] }, tokens: { CORS_ORIGINS: 'origins' },
    betterAuth: { options: { baseURL: 'https://iam.example' } },
    fetch: async (url, options) => {
      calls.push({ url, options });
      return { ok: authOk, json: async () => url.endsWith('/token') ? { token: 'synthetic-token' } : { user: { email: 'qa@example.com' }, session: { token: 'synthetic-session' } } };
    }
  });
  const response = { code: 200, headers: {}, status(code) { this.code = code; return this; }, send(body) { this.body = body; return this; }, type(value) { this.contentType = value; return this; }, setHeader(key, value) { this.headers[key] = value; }, redirect(url) { this.redirected = url; } };
  return { handlers, calls, response };
}
const req = (query) => ({ query, protocol: 'https', headers: { host: 'attacker.example', cookie: 'synthetic-cookie' } });

test('only exact configured origins receive tokens', () => {
  for (const url of ['https://attacker.example/cb', 'https://app.example.attacker.example/cb', 'https://app.example:8443/cb', '//app.example/cb', 'javascript:alert(1)', 'https://user:pass@app.example/cb', 'https://app.example/cb#fragment', 'https://app.example\\@attacker.example', ' https://app.example/cb', ['https://app.example/cb']]) {
    assert.equal(utils.trustedAuthCallback(url, ['https://app.example']), null, String(url));
  }
  assert.equal(utils.trustedAuthCallback('https://app.example/cb?state=1', ['https://app.example']).origin, 'https://app.example');
  assert.equal(utils.trustedAuthCallback('https://app.example/cb', ['*', 'https://*.example']), null);
});
test('rejected callback never fetches or forwards session tokens', async () => {
  const f = fixture();
  await f.handlers.get('/api/auth/test-callback')(req({ callbackUrl: 'https://attacker.example' }), f.response);
  assert.equal(f.response.code, 400);
  assert.equal(f.calls.length, 0);
  assert.equal(f.response.redirected, undefined);
});
test('valid callback ignores hostile Host and forwards only to the configured browser origin', async () => {
  const f = fixture();
  await f.handlers.get('/api/auth/test-callback')(req({ callbackUrl: 'https://app.example/cb' }), f.response);
  assert.deepEqual(f.calls.map(x => x.url), ['https://iam.example/api/auth/token', 'https://iam.example/api/auth/get-session']);
  assert.ok(f.calls.every(call => call.options.redirect === 'error' && call.options.signal));
  const target = new URL(f.response.redirected);
  assert.equal(target.origin, 'https://app.example');
  assert.equal(new URLSearchParams(target.hash.slice(1)).get('token'), 'synthetic-token');
  assert.equal(f.response.headers['Cache-Control'], 'no-store');
  assert.equal(f.response.headers['Referrer-Policy'], 'no-referrer');
});
test('failed token exchange does not produce a success redirect', async () => {
  const f = fixture(false);
  await f.handlers.get('/api/auth/test-callback')(req({ callbackUrl: 'https://app.example/cb' }), f.response);
  assert.equal(f.response.code, 401);
  assert.equal(f.response.redirected, undefined);
});
test('OAuth entry rejects untrusted callback before returning executable HTML', () => {
  const f = fixture();
  f.handlers.get('/api/auth/oauth-redirect')(req({ callbackURL: 'https://attacker.example', provider: 'google' }), f.response);
  assert.equal(f.response.code, 400);
  assert.equal(f.response.contentType, undefined);
});
test('OAuth HTML cannot be escaped with a closing script tag', () => {
  const f = fixture();
  f.handlers.get('/api/auth/oauth-redirect')(req({ callbackURL: 'https://app.example/cb', provider: '</script><script>BAD()</script>' }), f.response);
  assert.equal(f.response.code, 200);
  assert.equal((f.response.body.match(/<script>/g) ?? []).length, 1);
  assert.equal((f.response.body.match(/<\/script>/g) ?? []).length, 1);
  assert.ok(!f.response.body.includes('attacker.example'));
  assert.ok(f.response.body.includes('https://iam.example/api/auth/test-callback'));
});
