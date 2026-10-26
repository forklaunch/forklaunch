import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { probeMcp, mcpRpc } from './rust_mcp_probe.mjs';
import { presenceSchema } from './rust_sdk_presence_schema.mjs';

const cases = JSON.parse(readFileSync(process.argv[5], 'utf8'));
const { schema, validator } = presenceSchema();
for (const item of cases) {
  const result = validator.parse(schema, item.value);
  assert.equal(result.ok, item.valid);
  if (result.ok) assert.deepEqual(JSON.parse(JSON.stringify(result.value)), item.normalized);
}
const service = spawn(process.argv[2], [], { env: { ...process.env, HOST: '127.0.0.1', PORT: '0', RUST_LOG: 'info', MCP_ENABLED: 'true', PROTOCOL: 'http', VERSION: 'v1', DOCS_PATH: '/docs' }, stdio: ['ignore', 'pipe', 'pipe'] });
const exited = once(service, 'exit'); exited.catch(() => {});
let errors = ''; service.stderr.on('data', chunk => errors += chunk);
const timer = setTimeout(() => service.kill('SIGKILL'), 60000);
let tsServer;
try {
  const rustBase = await new Promise((resolve, reject) => {
    let output = '';
    service.stdout.on('data', chunk => { output += chunk; const match = output.match(/listening on (http:\/\/[^\s]+)/); if (match) resolve(match[1]); });
    service.once('exit', () => reject(Error(`Presence service startup failed: ${errors}`)));
  });
  await probeMcp(rustBase, ['readPresence', 'writePresence']);
  for (const [index, item] of cases.entries()) {
    if (!item.valid || (index % 7 !== 0 && index !== cases.length - 1)) continue;
    const read = await mcpRpc(rustBase, 'tools/call', { name: 'readPresence', arguments: { params: { index: String(index) } } });
    assert.equal(read.isError, false, JSON.stringify(read));
    assert.deepEqual(JSON.parse(read.content[0].text), item.normalized);
    const echo = await mcpRpc(rustBase, 'tools/call', { name: 'writePresence', arguments: { body: item.value } });
    assert.equal(echo.isError, false, JSON.stringify(echo));
    assert.deepEqual(JSON.parse(echo.content[0].text), item.normalized);
  }
  tsServer = createServer(async (request, response) => {
    if (request.method === 'GET' && request.url.startsWith('/presence/')) {
      const item = cases[Number(request.url.slice('/presence/'.length))];
      response.writeHead(item ? 200 : 404, { 'content-type': 'application/json' });
      response.end(JSON.stringify(item?.value ?? {}));
    } else if (request.method === 'POST' && request.url === '/presence') {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const result = validator.parse(schema, JSON.parse(Buffer.concat(chunks).toString()));
      response.writeHead(result.ok ? 200 : 400, { 'content-type': result.ok ? 'application/json' : 'text/plain' });
      response.end(result.ok ? JSON.stringify(result.value) : 'Invalid presence input');
    } else { response.writeHead(404); response.end(); }
  });
  await new Promise(resolve => tsServer.listen(0, '127.0.0.1', resolve));
  const { verify } = await import(pathToFileURL(process.argv[4]));
  for (const base of [rustBase, `http://127.0.0.1:${tsServer.address().port}`]) {
    const values = await verify(base, cases);
    assert.equal(values.length, cases.filter(item => item.valid).length);
    for (const item of values) {
      assert.deepEqual(item.value, cases[item.index].normalized);
      assert.deepEqual(item.echo, cases[item.index].normalized);
    }
    const client = spawn(process.argv[3], [base, process.argv[5]], { stdio: ['ignore', 'pipe', 'pipe'] });
    let output = ''; client.stdout.on('data', chunk => output += chunk); client.stderr.on('data', chunk => output += chunk);
    const deadline = setTimeout(() => client.kill('SIGKILL'), 35000);
    try { const [code, signal] = await once(client, 'exit'); assert.equal(code, 0, `${signal} ${output}`); assert.match(output, /Rust SDK cases match actual TypeScript validation/); process.stdout.write(output); }
    finally { clearTimeout(deadline); }
  }
  service.kill('SIGTERM');
  const [code, signal] = await exited;
  assert.equal(code, 0, `${signal} ${errors}`);
  console.log(`PASS ${cases.length} actual validator cases: required/null/omitted fields, nested records, arrays and unions through both live SDKs, Rust/TypeScript HTTP, MCP and graceful shutdown`);
} finally {
  clearTimeout(timer);
  if (tsServer) { tsServer.closeAllConnections(); await new Promise(resolve => tsServer.close(resolve)); }
  if (service.exitCode === null && service.signalCode === null) { service.kill('SIGKILL'); await exited.catch(() => {}); }
}
