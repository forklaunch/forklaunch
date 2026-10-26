// Execute against a built package, including its public ESM and CommonJS exports.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHmac} from 'node:crypto';

const directory = resolve(process.argv[2]);
for (const api of [await import(pathToFileURL(`${directory}/lib/index.mjs`).href),
  createRequire(import.meta.url)(`${directory}/lib/index.js`)]) {
  const {universalSdk, forkLaunchQueryOptions, forkLaunchMutationOptions, ForkLaunchSdkHttpError, createHmacAuthorization, hmacSignedFetch} = api;
  const paths = {};
  for (const [name, kind, media] of [['json', 'json', 'application/json'],
    ['events', 'sse', 'text/event-stream'], ['parts', 'multipart', 'multipart/form-data']]) {
    paths[`/${name}`] = {get: {operationId: name, responses: {
      200: {description: 'OK', content: {[media]: {schema: {}}}},
    }, 'x-forklaunch-operation': {
      format: 'forklaunch.sdk-operation.v1', clientPath: [name],
      service: 'example', router: 'root', operation: name, method: 'GET', path: `/${name}`,
      requestKind: 'json', requestContentType: 'application/json', requestSchema: true,
      responseKinds: {[`200:${media}`]: kind}, responseSchemas: {[`200:${media}`]: true},
      responseHeaders: {},
    }}};
  }
  paths['/legacy'] = {get: {operationId: 'legacy', responses: {
    200: {description: 'OK', content: {'application/json': {schema: {type: 'object'}}}},
  }}};
  const registry = {openapi: '3.1.0', info: {title: 'Example', version: '1'}, paths};
  const calls = [];
  const requests = [];
  let streamSignal;
  const fetcher = async (url, init) => {
    calls.push(String(url));
    requests.push({url: String(url), init});
    const path = new URL(url).pathname;
    if (path === '/openapi-hash') return new Response('stable-registry');
    if (path === '/openapi') return Response.json(registry);
    if (path === '/json' || path === '/legacy') return Response.json({message: 'injected transport'});
    if (path === '/events') {
      streamSignal = init.signal;
      return new Response('data: {"message":"hello"}\n\n', {headers: {'content-type': 'text/event-stream'}});
    }
    if (path === '/parts') return new Response(
      '--boundary\r\nContent-Disposition: form-data; name="label"\r\n\r\nhello\r\n--boundary--\r\n',
      {headers: {'content-type': 'multipart/form-data; boundary=boundary'}});
    throw Error(`Unexpected transport request: ${url}`);
  };
  const options = {host: 'https://sdk.invalid', registryOptions: {url: 'https://sdk.invalid/openapi'}, fetch: fetcher};
  const sdk = await universalSdk(options);
  assert.deepEqual((await sdk.json({})).response, {message: 'injected transport'});
  assert.deepEqual((await sdk.legacy({})).response, {message: 'injected transport'});
  assert.equal(calls.filter(url => url.endsWith('/openapi')).length, 1, 'Unchanged registry must reuse its document');
  assert.equal(calls.filter(url => url.endsWith('/openapi-hash')).length, 3, 'Injected transport must handle registry refresh');
  const multipart = await sdk.parts({});
  assert.equal(multipart.response.get('label'), 'hello');
  const bounded = await universalSdk({...options, maxMultipartResponseBytes: 8, maxSseEventBytes: 8});
  await assert.rejects(() => bounded.parts({}), /exceeds byte limit/);
  const events = await bounded.events({});
  await assert.rejects(() => events.response.next(), /SSE event exceeds configured limit/);
  assert.equal(streamSignal.aborted, true, 'Bounded SSE failure must release its request');
  const invalid = await universalSdk({...options, maxMultipartResponseBytes: 0, maxSseEventBytes: 0});
  await assert.rejects(() => invalid.parts({}), /byte limit/);
  await assert.rejects(async () => (await invalid.events({})).response.next(), /SSE event limit must be positive/);
  const normal = await sdk.events({});
  assert.equal((await normal.response.next()).value.data.message, 'hello');
  await normal.response.return();
  assert.equal(streamSignal.aborted, true);
  let hooks = 0;
  const configured = await universalSdk({...options, registryOptions: {raw: registry},
    serviceBaseUrls: {example: 'https://service.invalid'},
    headers: {authorization: 'Bearer fixture-secret', 'x-default': 'configured'},
    beforeRequest: async request => {
      hooks++;
      assert.equal(request.operation, 'json');
      return {...request, input: {...request.input, headers: {...request.input.headers, 'x-hook': 'applied'}}};
    }});
  await configured.json({headers: {'x-default': 'request'}});
  const forwarded = requests.at(-1);
  assert.equal(forwarded.url, 'https://service.invalid/json');
  const headers = new Headers(forwarded.init.headers);
  assert.equal(headers.get('authorization'), 'Bearer fixture-secret');
  assert.equal(headers.get('x-default'), 'request');
  assert.equal(headers.get('x-hook'), 'applied');
  assert.equal(hooks, 1);
  const plaintext = {host: 'http://service.invalid', registryOptions: {raw: registry},
    fetch: fetcher, headers: {authorization: 'Bearer fixture-secret'}};
  const beforePlaintext = calls.length;
  await assert.rejects(async () => (await universalSdk(plaintext)).json({}), /TLS is required/);
  assert.equal(calls.length, beforePlaintext, 'Credentials must be rejected before transport');
  await (await universalSdk({...plaintext, allowPlaintextDevelopment: true})).json({});
  assert.equal(requests.at(-1).url, 'http://service.invalid/json');
  const versionedDocument = structuredClone(registry);
  const versionedOperation = versionedDocument.paths['/json'].get;
  const baseDescriptor = versionedOperation['x-forklaunch-operation'];
  delete versionedOperation['x-forklaunch-operation'];
  versionedOperation['x-forklaunch-operations'] = ['text', 'number'].map((name, index) => ({
    ...baseDescriptor, operation: `jsonVersion${index + 1}`, clientPath: ['versions', name],
    requestSchema: {type: index === 0 ? 'string' : 'number'},
    responseSchemas: {'200:application/json': {type: index === 0 ? 'string' : 'number'}},
    versionMetadata: {formatVersion: 1, branches: [{
      path: '/json', params: {}, query: {}, headers: {}, bodyKind: 'json',
      bodyMedia: 'application/json', bodyOptional: false, parserOptions: {jsonStrict: false},
      bodySchema: {type: index === 0 ? 'string' : 'number'},
      responses: {'200:application/json': {schema: {type: index === 0 ? 'string' : 'number'}, headers: {}}},
    }]},
  }));
  let versionedCalls = 0;
  const versionedSdk = await universalSdk({...options, registryOptions: {raw: versionedDocument},
    fetch: async (_url, init) => { versionedCalls++; return Response.json(JSON.parse(init.body)); }});
  assert.equal((await versionedSdk.versions.text({body: 'first'})).response, 'first');
  assert.equal((await versionedSdk.versions.number({body: 2})).response, 2);
  await assert.rejects(() => versionedSdk.versions.text({body: 2}));
  await assert.rejects(() => versionedSdk.versions.number({body: 'wrong'}));
  await assert.rejects(() => versionedSdk.json({body: 'ambiguous'}), /Sdk path not found/);
  await assert.rejects(() => versionedSdk.executeFetchCall('/json', {method: 'get'}), /requires a typed SDK operation/);
  assert.equal(versionedCalls, 2, 'Invalid/ambiguous version requests must never reach transport');
  for (const invalidDescriptors of [[], {}, [baseDescriptor, baseDescriptor]]) {
    const invalidRegistry = structuredClone(versionedDocument);
    invalidRegistry.paths['/json'].get['x-forklaunch-operations'] = invalidDescriptors;
    await assert.rejects(() => universalSdk({...options, registryOptions: {raw: invalidRegistry}}));
  }
  const conflictingRegistry = structuredClone(versionedDocument);
  conflictingRegistry.paths['/json'].get['x-forklaunch-operation'] = baseDescriptor;
  await assert.rejects(() => universalSdk({...options, registryOptions: {raw: conflictingRegistry}}), /Conflicting/);
  const extendedDocument = structuredClone(registry);
  const extendedMethods = ['PURGE', 'PROPFIND', 'M-SEARCH'];
  extendedDocument.paths['/extended'] = {'x-forklaunch-http-methods': Object.fromEntries(extendedMethods.map((method, index) => [method, {
    operationId: `extended${index}`, responses: {204: {description: 'OK'}},
    'x-forklaunch-operation': {...baseDescriptor, method, path: '/extended', clientPath: ['extended', `method${index}`],
      responseKinds: {'204:': 'empty'}, responseSchemas: {'204:': {type: 'null'}}},
  }]))};
  const sentMethods = [];
  const extendedSdk = await universalSdk({...options, registryOptions: {raw: extendedDocument},
    fetch: async (url, init) => { assert.equal(new URL(url).pathname, '/extended'); sentMethods.push(init.method); return new Response(null, {status: 204}); }});
  for (let index = 0; index < extendedMethods.length; index++) {
    assert.equal((await extendedSdk.extended[`method${index}`]({})).status, 204);
  }
  assert.deepEqual(sentMethods, extendedMethods);
  const badExtended = structuredClone(extendedDocument);
  delete badExtended.paths['/extended']['x-forklaunch-http-methods'].PURGE['x-forklaunch-operation'];
  await assert.rejects(() => universalSdk({...options, registryOptions: {raw: badExtended}}), /checked descriptor/);
  const controller = new AbortController();
  const query = forkLaunchQueryOptions(['example'], async input => {
    assert.equal(input.signal, controller.signal);
    return {status: 200, response: 'ok'};
  }, {});
  assert.deepEqual(await query.queryFn({signal: controller.signal}), {status: 200, response: 'ok'});
  // Per-request HMAC: same payload as core createHmacToken, full wire path, fresh nonce.
  const stamp = new Date('2026-10-22T00:00:00.000Z');
  const expected = (method, path, body, nonce) => createHmac('sha256', 'hmac-secret')
    .update(`${method}\n${path}\n${body ? body + '\n' : 'undefined'}${stamp.toISOString()}\n${nonce}`).digest('base64');
  assert.equal(await createHmacAuthorization({secretKey: 'hmac-secret', method: 'POST', path: '/payment-link', body: '{"a":1}', timestamp: stamp, nonce: 'n1'}),
    `HMAC keyId=default ts=${stamp.toISOString()} nonce=n1 signature=${expected('POST', '/payment-link', '{"a":1}', 'n1')}`);
  assert.ok((await createHmacAuthorization({secretKey: 'hmac-secret', keyId: 'other', method: 'GET', path: '/plan', timestamp: stamp, nonce: 'n2'}))
    .endsWith(expected('GET', '/plan', undefined, 'n2')));
  const signedRequests = [];
  const verifying = async (url, init) => {
    const header = new Headers(init.headers).get('authorization');
    const [, ts, nonce, signature] = /^HMAC keyId=default ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(header);
    const body = init.body === undefined ? undefined : new TextDecoder().decode(init.body);
    const path = new URL(url).pathname;
    assert.equal(signature, createHmac('sha256', 'hmac-secret').update(`${init.method}\n${path}\n${body ? body + '\n' : 'undefined'}${ts}\n${nonce}`).digest('base64'));
    signedRequests.push({nonce, path});
    return Response.json({message: 'signed'});
  };
  const signedFetch = hmacSignedFetch({secretKey: 'hmac-secret', fetch: verifying});
  await signedFetch('https://example.invalid/base/payment-link?expand=1', {method: 'POST', body: '{"amount":1}', headers: {authorization: 'stale'}});
  await signedFetch('https://example.invalid/base/payment-link?expand=1', {method: 'POST', body: '{"amount":1}'});
  await signedFetch('https://example.invalid/plan/a%2Fb', {method: 'GET'});
  assert.deepEqual(signedRequests.map(request => request.path), ['/base/payment-link', '/base/payment-link', '/plan/a%2Fb']);
  assert.equal(new Set(signedRequests.map(request => request.nonce)).size, 3);
  const sdkSigned = [];
  const signedSdk = await universalSdk({...options, fetch: async (url, init) => {
    const path = new URL(url).pathname;
    if (path === '/json') sdkSigned.push(new Headers(init.headers).get('authorization'));
    return fetcher(url, init);
  }, hmac: {secretKey: 'hmac-secret'}});
  await signedSdk.json();
  await signedSdk.json();
  assert.equal(sdkSigned.length, 2);
  assert.ok(sdkSigned.every(header => header?.startsWith('HMAC keyId=default ')));
  assert.notEqual(sdkSigned[0], sdkSigned[1]);
  const failure = {status: 404, response: 'missing'};
  await assert.rejects(() => forkLaunchMutationOptions(['example'], async () => failure).mutationFn({}),
    error => error instanceof ForkLaunchSdkHttpError && error.result === failure);
}
console.log('PASS public ESM/CJS SDK transport, registry refresh, portable limits, SSE cleanup and query/mutation helpers');
