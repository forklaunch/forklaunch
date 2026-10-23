import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile, realpath, mkdtemp, writeFile, rm } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import { cliTypescriptSdkRuntime, linkTypescriptPrefix, cliSdkCompilerOptions } from './fixtures/cli_typescript_sdk_runtime.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const tsc = process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc');
const ts = require(require.resolve('typescript', { paths: [path.dirname(await realpath(tsc))] }));
const temporary = await mkdtemp(path.join(tmpdir(), 'fl-sdk-body-parity-'));
let captured;
async function fetcher(url, options) {
  captured = new Request(url, options);
  return new Response('{"ok":true}', { headers: { 'content-type': 'application/json' } });
}
// Relative imports execute their actual source unless stubbed; other packages
// come from the pinned TypeScript fixture prefix.
const external = createRequire(path.join(path.dirname(path.dirname(path.dirname(path.dirname(await realpath(tsc))))), 'package.json'));
function loadSync(absolute, imports, cache) {
  const file = existsSync(absolute) ? absolute : existsSync(absolute + '.ts') ? absolute + '.ts' : path.join(absolute, 'index.ts');
  if (cache.has(file)) return cache.get(file);
  const compiled = ts.transpileModule(readFileSync(file, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true } }).outputText;
  const exports = {};
  cache.set(file, exports);
  vm.runInNewContext(compiled, { exports, require(spec) { if (spec in imports) return imports[spec]; if (spec.startsWith('.')) return loadSync(path.resolve(path.dirname(file), spec.replace(/\.js$/, '')), imports, cache); return external(spec); }, fetch: fetcher, Buffer, URL, URLSearchParams, Response, FormData, Blob, File, Map, Set, Date, RegExp, ArrayBuffer, Uint8Array, DataView, Error, TextEncoder, TextDecoder, structuredClone, crypto: globalThis.crypto, console });
  return exports;
}
async function load(file, imports = {}) {
  return loadSync(path.join(root, file), imports, new Map());
}
async function describe(request) {
  const contentType = request.headers.get('content-type');
  if (contentType.startsWith('multipart/')) {
    const form = await request.formData();
    return Promise.all([...form].map(async ([name, value]) => typeof value === 'string' ? [name, value] : [name, value.name, value.type, [...new Uint8Array(await value.arrayBuffer())]]));
  }
  return [contentType, await request.text()];
}
try {
  const runtime = await cliTypescriptSdkRuntime(root);
  await linkTypescriptPrefix(tsc, temporary);
  await writeFile(path.join(temporary, 'runtime.ts'), runtime);
  await writeFile(path.join(temporary, 'tsconfig.json'), JSON.stringify({ compilerOptions: cliSdkCompilerOptions, include: ['runtime.ts'] }));
  execFileSync(tsc, ['--project', path.join(temporary, 'tsconfig.json')], { encoding: 'utf8', timeout: 120000 });
  const { request } = require(path.join(temporary, 'compiled/runtime.js'));
  const { safeStringify } = await load('framework/common/src/safeStringify.ts');
  const { openApiCompliantPath } = await load('framework/common/src/openApiCompliantPath.ts');
  const { getSdkPath } = await load('framework/universal-sdk/src/core/resolvePath.ts', { './regex': { generateStringFromRegex() { throw Error('unexpected regex'); } } });
  const specialTypes = await load('framework/universal-sdk/src/core/coerceSpecialTypes.ts');
  const referenced = await load('framework/universal-sdk/src/core/coerceReferenced.ts', { './coerceSpecialTypes': specialTypes });
  const dialect = await load('framework/universal-sdk/src/core/responseDialect.ts');
  const schemaAdapter = await load('framework/universal-sdk/src/core/ajvSchema.ts');
  const responseSchemas = await load('framework/universal-sdk/src/core/responseSchema.ts', { './coerceSpecialTypes': specialTypes, './coerceReferenced': referenced, './ajvSchema': schemaAdapter, './responseDialect': dialect });
  const uriFormat = await load('framework/universal-sdk/src/core/uriFormat.ts');
  const { UniversalSdk } = await load('framework/universal-sdk/src/universalSdk.ts', { './core/responseDialect': dialect, './core/uriFormat': uriFormat, './core/responseSchema': responseSchemas, '@forklaunch/common': { safeStringify, openApiCompliantPath, safeParse: JSON.parse }, './core/coerceSpecialTypes': { coerceSpecialTypes: value => value }, './core/mapContentType': { mapContentType: value => value }, './core/openApi': { ...(await load('framework/universal-sdk/src/core/openApi.ts', { '@forklaunch/common': { safeStringify: JSON.stringify, openApiCompliantPath: value => value, safeParse: JSON.parse } })), refreshOpenApi() { throw Error('unexpected refresh'); } }, './core/resolvePath': { getSdkPath } });
  const universal = new UniversalSdk('http://127.0.0.1:9000', new (external('ajv').default ?? external('ajv'))({ strict: false }), {}, undefined);
  universal.registryOpenApiJson = {
    latest: { paths: { '/upload': { post: {
      responses: { 200: { content: { 'application/json': { schema: {} } } } }
    } } } }
  };
  const forms = [{}, { label: '水 & +', empty: '', flag: false, zero: 0 }, { values: ['a,b', '水', ''], empty: [] }, { nested: { active: true, ids: ['x', 'y'] }, nullable: null, absent: undefined }, { line: 'a\nb\rc\r\nd', 'quoted"\r\nname': 'safe' }, { number: NaN, infinity: Infinity, bigint: 123n, collection: new Set(['a', 'b']), bytes: new Uint8Array([0,255]) }];
  let cases = 0;
  for (const kind of ['urlEncoded', 'multipart']) {
    for (const body of [...forms, ...(kind === 'multipart' ? [{ file: new File([new Uint8Array([0,255,1])], 'data.bin'), repeated: [new Blob([]), new File(['水'], 'unicode.txt', { type: 'text/plain' })] }] : [])]) {
      const typedBody = kind === 'multipart' ? { multipartForm: body } : { urlEncodedForm: body };
      await universal.execute('/upload', 'post', 'latest', { body: typedBody });
      const expected = await describe(captured);
      // The generated client refuses form values with no lossless wire form
      // (null, empty collections, non-finite numbers, bigint, Set, raw bytes outside
      // files; fb1e03d21) where the universal SDK still coerces or drops them.
      const lossy = (value) => value === null || (Array.isArray(value) && value.length === 0) || (typeof value === 'number' && !Number.isFinite(value)) || typeof value === 'bigint' || value instanceof Set || value instanceof Uint8Array;
      const rejected = await request({ baseUrl: 'http://127.0.0.1:9000', fetch: fetcher, headers: { 'CONTENT-TYPE': 'incorrect' }, beforeRequest: meta => ({ ...meta, input: { ...meta.input, headers: { 'Content-Type': 'incorrect' } } }) }, { service: 'uploads', router: 'upload', operation: 'upload', method: 'POST', path: '/upload', requestKind: kind, requestContentType: kind === 'multipart' ? 'multipart/form-data' : 'application/x-www-form-urlencoded', responseKinds: { '200:application/json': 'json' }, responseSchemas: { '200:application/json': { type: 'object' } }, input: { body } }).then(() => null, (error) => error);
      if (rejected) {
        assert.ok(Object.values(body).some(lossy), `${kind} generated client rejected a representable body: ${rejected.message}`);
        assert.match(rejected.message, /Empty form collection has no lossless wire representation|Unsupported form (scalar|value)/);
        cases++;
        continue;
      }
      assert.deepEqual(await describe(captured), expected, `${kind} actual TS universal SDK body`);
      cases++;
    }
  }
  for (const invalid of [null, [], 'text', 42]) {
    await assert.rejects(request({ baseUrl: 'http://127.0.0.1:9000', fetch: fetcher }, { service: 'uploads', router: 'upload', operation: 'upload', method: 'POST', path: '/upload', requestKind: 'multipart', input: { body: invalid } }), TypeError);
  }
  console.log(`PASS ${cases} actual TypeScript universal SDK/form comparisons: multipart files/repeats/Unicode/empty forms, URL-encoded JSON composites, special values and encoder-owned headers`);
} finally { await rm(temporary, { recursive: true, force: true }); }
