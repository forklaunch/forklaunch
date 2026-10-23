import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const cargo = (args, env = {}) => new Promise((resolve, reject) => {
  const child = spawn('cargo', args, {
    cwd: root,
    env: { ...Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("OTEL_"))), ...env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '', stderr = '';
  child.stdout.on('data', data => { stdout += data; });
  child.stderr.on('data', data => { stderr += data; });
  const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('OTLP fixture timed out')); }, 120000);
  child.on('error', error => { clearTimeout(timer); reject(error); });
  child.on('close', code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
});
const manifest = 'framework-rs/Cargo.toml';
const built = await cargo(['build', '--manifest-path', manifest, '-p', 'forklaunch-core', '--example', 'otlp_runtime', '--features', 'otlp']);
assert.equal(built.code, 0, built.stderr);
const requests = [];
const receiver = http.createServer((req, res) => {
  const chunks = [];
  req.on('data', chunk => chunks.push(chunk));
  req.on('end', () => {
    requests.push({ path: req.url, headers: req.headers, body: Buffer.concat(chunks) });
    res.setHeader('content-type', req.headers['content-type']);
    res.end(req.headers['content-type']?.includes('json') ? '{}' : Buffer.alloc(0));
  });
});
await new Promise(resolve => receiver.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${receiver.address().port}`;
const args = ['run', '--quiet', '--manifest-path', manifest, '-p', 'forklaunch-core', '--example', 'otlp_runtime', '--features', 'otlp'];
// Each fixture owns its environment and its global providers in a fresh process.
const env = {};
Object.assign(env, {
  OTEL_SDK_DISABLED: 'false',
  OTEL_EXPORTER_OTLP_ENDPOINT: base,
  OTEL_EXPORTER_OTLP_HEADERS: 'authorization=Bearer%20parity-secret',
  OTEL_SERVICE_NAME: 'configured-fixture',
  OTEL_RESOURCE_ATTRIBUTES: 'deployment.environment.name=parity',
  OTEL_APPLICATION_ID: 'fixture-app',
  OTEL_BSP_SCHEDULE_DELAY: '600000',
  OTEL_BLRP_SCHEDULE_DELAY: '600000',
  OTEL_METRIC_EXPORT_INTERVAL: '600000',
  OTEL_TRACES_EXPORTER: 'otlp', OTEL_METRICS_EXPORTER: 'otlp', OTEL_LOGS_EXPORTER: 'otlp',
  RUST_LOG: 'info',
});
try {
  for (const protocol of ['http/json', 'http/protobuf']) {
    requests.length = 0;
    const result = await cargo(args, { ...env, OTEL_EXPORTER_OTLP_PROTOCOL: protocol,
      OTEL_EXPORTER_OTLP_LOGS_ENDPOINT: `${base}/custom-logs` });
    assert.equal(result.code, 0, result.stderr);
    assert.ok(!result.stderr.includes('parity-secret'));
    const found = requests.filter(req => ['/v1/traces', '/v1/metrics', '/custom-logs'].includes(req.path));
    assert.deepEqual(new Set(found.map(req => req.path)), new Set(['/v1/traces', '/v1/metrics', '/custom-logs']));
    for (const req of found) {
      assert.equal(req.headers.authorization, 'Bearer parity-secret');
      assert.ok(req.body.length > 0);
      assert.ok(req.body.includes(Buffer.from('configured-fixture')));
      assert.ok(req.body.includes(Buffer.from('service.version')));
      assert.ok(req.body.includes(Buffer.from('1.2.3')));
      assert.ok(req.body.includes(Buffer.from('fixture-app')));
      assert.match(req.headers['content-type'], protocol === 'http/json' ? /application\/json/ : /application\/x-protobuf/);
    }
    if (protocol === 'http/json') {
      const trace = JSON.parse(found.find(req => req.path === '/v1/traces').body);
      const spans = trace.resourceSpans.flatMap(resource => resource.scopeSpans.flatMap(scope => scope.spans));
      assert.equal(spans.length, 1);
      assert.equal(spans[0].name, 'GET /items/{id}');
      assert.ok(spans[0].attributes.some(a => a.key === 'forklaunch.correlation_id' && a.value.stringValue === 'fixture-correlation'));
      const metric = JSON.parse(found.find(req => req.path === '/v1/metrics').body);
      const instruments = metric.resourceMetrics.flatMap(resource => resource.scopeMetrics.flatMap(scope => scope.metrics));
      assert.ok(instruments.some(m => m.name === 'http.server.request.count' && m.sum.dataPoints.some(p => String(p.asInt) === '1')));
      const logs = JSON.parse(found.find(req => req.path === '/custom-logs').body);
      const records = logs.resourceLogs.flatMap(resource => resource.scopeLogs.flatMap(scope => scope.logRecords));
      assert.ok(records.some(log => log.body.stringValue === 'fixture application log'));
    }
  }
  requests.length = 0;
  const disabled = await cargo(args, { ...env, OTEL_SDK_DISABLED: 'true' });
  assert.equal(disabled.code, 0, disabled.stderr);
  assert.equal(requests.length, 0);
  assert.match(disabled.stderr, /fixture application log/);
  const invalid = await cargo(args, { ...env, OTEL_EXPORTER_OTLP_PROTOCOL: 'unsupported-parity-secret' });
  assert.notEqual(invalid.code, 0);
  assert.ok(!invalid.stderr.includes('unsupported-parity-secret'));
  assert.equal(requests.length, 0);
  console.log('PASS: OTLP HTTP/JSON and protobuf traces, metrics, logs, headers, resources, opt-out and shutdown flush');
} finally {
  await new Promise(resolve => receiver.close(resolve));
}
