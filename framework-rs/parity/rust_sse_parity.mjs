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
const temporary = await mkdtemp(path.join(tmpdir(), 'fl-sse-parity-'));
let chunks;
async function fetcher() {
  let index = 0;
  return new Response(new ReadableStream({ pull(controller) {
    if (index === chunks.length) controller.close();
    else controller.enqueue(Uint8Array.from(chunks[index++]));
  } }), { headers: { 'content-type': 'text/event-stream' } });
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
  vm.runInNewContext(compiled, { exports, require(spec) { if (spec in imports) return imports[spec]; if (spec.startsWith('.')) return loadSync(path.resolve(path.dirname(file), spec.replace(/\.js$/, '')), imports, cache); return external(spec); }, fetch: fetcher, Buffer, URL, URLSearchParams, Response, FormData, Blob, File, TextDecoder, ReadableStream, TextEncoder, TextDecoder, structuredClone, crypto: globalThis.crypto, console });
  return exports;
}
async function load(file, imports = {}) {
  return loadSync(path.join(root, file), imports, new Map());
}
try {
  const runtime = await cliTypescriptSdkRuntime(root);
  await linkTypescriptPrefix(tsc, temporary);
  await writeFile(path.join(temporary, 'runtime.ts'), runtime);
  await writeFile(path.join(temporary, 'tsconfig.json'), JSON.stringify({ compilerOptions: cliSdkCompilerOptions, include: ['runtime.ts'] }));
  execFileSync(tsc, ['--project', path.join(temporary, 'tsconfig.json')], { encoding: 'utf8', timeout: 120000 });
  const { request } = require(path.join(temporary, 'compiled/runtime.js'));
  const common = { ...(await load('framework/common/src/safeStringify.ts')), ...(await load('framework/common/src/safeParse.ts')), ...(await load('framework/common/src/openApiCompliantPath.ts')) };
  const { getSdkPath } = await load('framework/universal-sdk/src/core/resolvePath.ts', { './regex': { generateStringFromRegex() { throw Error('unexpected regex'); } } });
  const specialTypes = await load('framework/universal-sdk/src/core/coerceSpecialTypes.ts');
  const referenced = await load('framework/universal-sdk/src/core/coerceReferenced.ts', { './coerceSpecialTypes': specialTypes });
  const responseSchemas = await load('framework/universal-sdk/src/core/responseSchema.ts', { './coerceSpecialTypes': specialTypes, './coerceReferenced': referenced });
  const { UniversalSdk } = await load('framework/universal-sdk/src/universalSdk.ts', { './core/responseSchema': responseSchemas, '@forklaunch/common': common, './core/coerceSpecialTypes': { coerceSpecialTypes: value => value }, './core/mapContentType': { mapContentType: value => value }, './core/openApi': { ...(await load('framework/universal-sdk/src/core/openApi.ts', { '@forklaunch/common': { safeStringify: JSON.stringify, openApiCompliantPath: value => value, safeParse: JSON.parse } })), refreshOpenApi() { throw Error('unexpected refresh'); } }, './core/resolvePath': { getSdkPath } });
  const universal = new UniversalSdk('http://127.0.0.1:9000', new (external('ajv').default ?? external('ajv'))({ strict: false }), {}, undefined);
  universal.registryOpenApiJson = { latest: { paths: { '/events': { get: {
    responses: { 200: { content: { 'text/event-stream': { schema: {} } } } }
  } } } } };
  // Common terminated frames supported by the actual TypeScript client. Additional
  // standards cases (multiline data, bare CR, whitespace and EOF) are native unit gates.
  const bodies = ['', ': heartbeat\n\n', 'data: {}\n\n', 'id: first\ndata: {"label":"水 🦀","active":false}\n\ndata: [0,null,"two"]\n\n', 'id: first\r\ndata: 42\r\n\r\nid:\r\ndata: null\r\n\r\n', '\uFEFFevent: update\nretry: 1000\nid: same\ndata: plain text\n\ndata:\n\n', 'unknown: ignored\n: keepalive\ndata: false\n\ndata: ""\n\n'];
  const cases = bodies.flatMap(body => {
    const bytes = Buffer.from(body);
    return [1,2,3,5,Math.max(1,bytes.length)].map(size => Array.from({ length: Math.ceil(bytes.length/size) }, (_, index) => [...bytes.subarray(index*size, (index+1)*size)]));
  });
  await writeFile(path.join(temporary, 'cases.json'), JSON.stringify(cases));
  execFileSync('cargo', ['test', '--quiet', '--manifest-path', 'framework-rs/Cargo.toml', '-p', 'forklaunch-core','--features','http', 'native_sse_transport_conformance'], { cwd: root, env: { ...process.env, FORKLAUNCH_TEST_SSE_INPUT: path.join(temporary, 'cases.json'), FORKLAUNCH_TEST_SSE_OUTPUT: path.join(temporary, 'events.json') }, encoding: 'utf8', timeout: 180000 });
  const native = JSON.parse(await readFile(path.join(temporary, 'events.json'), 'utf8'));
  for (let index = 0; index < cases.length; index++) {
    chunks = cases[index];
    const response = await universal.execute('/events', 'get', 'latest', {});
    const events = [];
    for await (const event of response.response) events.push(event);
    assert.deepEqual(JSON.parse(JSON.stringify(events)), native[index], `actual TypeScript SSE case ${index}`);
    const generated = await request({ baseUrl: 'http://127.0.0.1:9000', fetch: fetcher }, { service: 'streams', router: 'events', operation: 'watchEvents', method: 'GET', path: '/events', responseKinds: { '200:text/event-stream': 'sse' }, responseSchemas: { '200:text/event-stream': {} }, input: {} });
    const actual = [];
    for await (const event of generated.response) actual.push(event);
    assert.deepEqual(actual, native[index], `generated TypeScript SSE case ${index}`);
  }
  console.log(`PASS ${cases.length} actual TypeScript universal SDK/native codec/generated SDK SSE comparisons: chunked UTF-8, LF/CRLF, JSON/scalar/text data, IDs, resets, comments and empty streams`);
} finally { await rm(temporary, { recursive: true, force: true }); }
