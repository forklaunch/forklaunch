import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {readFileSync, realpathSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const compiler = process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc');
const ts = require(require.resolve('typescript', {paths: [path.dirname(realpathSync(compiler))]}));
function compile(name) {
  return ts.transpileModule(readFileSync(path.join(root, `framework/testing/src/${name}.ts`), 'utf8'), {
    compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
  }).outputText;
}
const environmentSource = compile('environment'), tokenSource = compile('tokens');
const endpoints = {
  database: {host: 'database.test', ports: {5432: 35432, 3306: 33306, 27017: 37017, 1433: 31433}},
  redis: {host: 'redis.test', ports: {6379: 36379}},
  kafka: {host: 'kafka.test', ports: {9092: 39092}},
  s3: {host: 'storage.test', ports: {9000: 39000}},
};
const cases = [];
for (const databaseType of [undefined, 'postgres', 'postgresql', 'mysql', 'mariadb', 'mongodb', 'mongo', 'mssql', 'sqlite', 'libsql'])
for (const database of [undefined, endpoints.database])
for (let mask = 0; mask < 8; mask++)
for (const hmacSecret of [undefined, '', 'custom-test-secret'])
for (const customVars of [{}, {DB_NAME: 'overridden', HOST: '127.0.0.1', PORT: '0', HMAC_SECRET_KEY: '', S3_BUCKET: 'custom', EXTRA: 'é'}]) {
  cases.push({databaseType, database, hmacSecret, customVars,
    baseline: {KEPT: 'original', DB_HOST: 'previous-host', DB_PATH: 'previous-file', REDIS_URL: 'previous-redis', HOST: 'previous'},
    ...(mask & 1 ? {redis: endpoints.redis} : {}),
    ...(mask & 2 ? {kafka: endpoints.kafka} : {}),
    ...(mask & 4 ? {s3: endpoints.s3} : {}),
  });
}
for (const databaseType of ['postgres', 'mysql', 'mongodb', 'mssql', 'sqlite'])
  cases.push({databaseType, database: {host: 'missing', ports: {}}});
for (const kind of ['redis', 'kafka', 's3']) cases.push({[kind]: {host: 'missing', ports: {}}});
cases.push({});
cases.push({customVars: JSON.parse('{"__proto__":"literal","constructor":"data","toString":"data","EMPTY":""}')});
function reference(input) {
  const environment = Object.assign(Object.create(null), input.baseline), exported = {};
  vm.runInNewContext(environmentSource, {exports: exported, process: {env: environment}});
  const config = {...input};
  for (const kind of ['database', 'redis', 'kafka', 's3']) if (input[kind]) config[kind] = {
    getHost: () => input[kind].host,
    getMappedPort(port) {assert.ok(Object.hasOwn(input[kind].ports, port), 'missing mapped port'); return input[kind].ports[port];},
  };
  try { exported.setupTestEnvironment(config); } catch { return {error: true}; }
  const tokens = {};
  vm.runInNewContext(tokenSource, {exports: tokens});
  return {environment: JSON.parse(JSON.stringify(environment)), tokens: JSON.parse(JSON.stringify(tokens.TEST_TOKENS))};
}
const native = spawnSync('cargo', ['run', '--quiet', '--manifest-path', path.join(root, 'framework-rs/Cargo.toml'), '-p', 'forklaunch-testing', '--example', 'environment_parity'], {
  input: cases.map(value => JSON.stringify(value)).join('\n') + '\n', encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: 300000,
  env: {...process.env, CARGO_INCREMENTAL: '0'},
});
assert.equal(native.status, 0, native.stderr);
const results = native.stdout.trim().split('\n').map(line => JSON.parse(line));
assert.equal(results.length, cases.length);
for (const [index, input] of cases.entries()) assert.deepEqual(results[index], reference(input), JSON.stringify(input));
console.log(`PASS ${cases.length} actual TypeScript test environment/token comparisons plus native child-process environment isolation`);
