import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { randomInt } from 'node:crypto';
import { join } from 'node:path';

const [app, modules = 'modules'] = process.argv.slice(2);
assert.ok(app);
const port = await freePortPair();
const base = `http://127.0.0.1:${port}`;
const mcp = `http://127.0.0.1:${port + 2000}/mcp`;
const service = join(app, modules, 'legacy-api');
const env = {
  ...Object.fromEntries(
    Object.entries(process.env).filter(
      ([key]) =>
        !key.startsWith('OTEL_') &&
        !key.startsWith('MCP_') &&
        key !== 'FORKLAUNCH_MODE'
    )
  ),
  DOTENV_FILE_PATH: join(service, '.env.local'),
  HOST: '127.0.0.1',
  PORT: String(port),
  PROTOCOL: 'http',
  VERSION: 'v1',
  DOCS_PATH: '/docs',
  NODE_ENV: 'development',
  DB_NAME: 'fixture',
  DB_HOST: '127.0.0.1',
  DB_PORT: '9',
  DB_USER: 'fixture',
  DB_PASSWORD: 'fixture',
  DB_SSL: 'false',
  HMAC_SECRET_KEY: 'fixture-only-hmac-secret',
  ENCRYPTION_KEY: Buffer.alloc(32, 19).toString('base64'),
  OTEL_SERVICE_NAME: 'rust-first-typescript-fixture',
  OTEL_LEVEL: 'error',
  OTEL_EXPORTER_OTLP_ENDPOINT: 'http://127.0.0.1:9'
};
const child = spawn(process.execPath, ['--import', 'tsx', 'server.ts'], {
  cwd: service,
  env,
  stdio: ['ignore', 'pipe', 'pipe']
});
let output = '',
  closed = false;
child.stdout.on('data', (chunk) => (output += chunk));
child.stderr.on('data', (chunk) => (output += chunk));
const exited = new Promise((resolve) =>
  child.once('close', (code, signal) => {
    closed = true;
    resolve({ code, signal });
  })
);
let session,
  id = 0;
try {
  let response;
  for (let attempt = 0; attempt < 200; attempt++) {
    assert.ok(!closed, output);
    try {
      response = await fetch(`${base}/legacy-api`, {
        signal: AbortSignal.timeout(250)
      });
    } catch {}
    if (response?.status === 200) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.equal(response?.status, 200, output);
  const expected = await response.json();
  assert.equal(expected.message, 'LegacyApi');
  const initialized = await rpc('initialize', {
    protocolVersion: '2024-11-05',
    capabilities: {},
    clientInfo: { name: 'rust-first-fixture', version: '1.0.0' }
  });
  assert.ok(initialized.capabilities.tools);
  const { tools } = await rpc('tools/list');
  const tool = tools.find((tool) => tool.name === 'Legacy Api Get');
  assert.ok(tool, JSON.stringify(tools));
  const result = await rpc('tools/call', { name: tool.name, arguments: {} });
  assert.ok(!result.isError, JSON.stringify(result));
  assert.ok(
    result.content.some(
      (item) => item.type === 'text' && item.text.includes('LegacyApi')
    ),
    JSON.stringify(result)
  );
  console.log(
    'PASS unmodified generated TypeScript service HTTP startup, MCP initialization/discovery and real HTTP-backed tool invocation'
  );
} finally {
  if (!closed) child.kill('SIGTERM');
  const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
  try {
    const result = await exited;
    assert.ok(
      result.code === 0 || result.signal === 'SIGTERM',
      JSON.stringify(result) + output
    );
  } finally {
    clearTimeout(timer);
  }
}
async function rpc(method, params = {}) {
  const response = await fetch(mcp, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      accept: 'application/json, text/event-stream',
      ...(session ? { 'mcp-session-id': session } : {})
    },
    body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }),
    signal: AbortSignal.timeout(10000)
  });
  const text = await response.text();
  assert.equal(response.status, 200, text + output);
  session = response.headers.get('mcp-session-id') ?? session;
  const data = response.headers
    .get('content-type')
    ?.includes('text/event-stream')
    ? text
        .split('\n')
        .filter((line) => line.startsWith('data:'))
        .map((line) => JSON.parse(line.slice(5)))
        .find((item) => item.id === id)
    : JSON.parse(text);
  assert.ok(data && !data.error, text);
  return data.result;
}
async function freePortPair() {
  for (let attempt = 0; attempt < 100; attempt++) {
    const first = createServer(),
      second = createServer(),
      port = randomInt(20000, 40000);
    try {
      await new Promise((resolve, reject) => {
        first.once('error', reject);
        first.listen(port, '127.0.0.1', resolve);
      });
      await new Promise((resolve, reject) => {
        second.once('error', reject);
        second.listen(port + 2000, '127.0.0.1', resolve);
      });
      return port;
    } catch (error) {
      if (error.code !== 'EADDRINUSE') throw error;
    } finally {
      await Promise.all(
        [first, second].map(
          (server) => new Promise((resolve) => server.close(resolve))
        )
      );
    }
  }
  throw Error('No free HTTP/MCP port pair');
}
