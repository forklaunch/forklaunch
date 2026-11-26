// Build one application Dockerfile and start each Rust HTTP process it serves.
// Processes are discovered from docker-compose.yaml without interpolation: only
// non-secret HTTP settings are passed, so startup must not require a database.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';

const [dockerfile] = process.argv.slice(2);
assert.ok(dockerfile, 'Usage: node .github/scripts/container-smoke.mjs <Dockerfile>');
const docker = (...args) => execFileSync('docker', args, {encoding: 'utf8'}).trim();
const compose = JSON.parse(execFileSync('docker', ['compose', 'config', '--format', 'json',
  '--no-interpolate', '--no-env-resolution', '--no-path-resolution'], {encoding: 'utf8'}));
const services = Object.entries(compose.services ?? {});
const built = services.filter(([, service]) => service.build &&
  (service.build.dockerfile ?? 'Dockerfile') === dockerfile && service.image);
const images = new Set(built.map(([, service]) => service.image));
const tag = [...images][0] ?? `forklaunch-ci/${dockerfile.toLowerCase().replace(/[^a-z0-9]+/g, '-')}:local`;
execFileSync('docker', ['build', '--progress=plain', '-f', dockerfile, '-t', tag, '.'], {stdio: 'inherit'});
for (const image of images) if (image !== tag) docker('tag', tag, image);

const HTTP_SETTINGS = ['HOST', 'PORT', 'PROTOCOL', 'VERSION', 'RUST_LOG',
  'PUBLIC_ORIGIN', 'DOCS_PATH', 'MCP_ENABLED', 'MCP_LISTENER'];
const processes = services.filter(([, service]) => images.has(service.image) &&
  service.environment?.PORT !== undefined && Array.isArray(service.command) &&
  /^(\/app\/bin\/|\.\/target\/release\/)/.test(service.command[0]));

async function rpc(base, method, params = {}) {
  const response = await fetch(`${base}/mcp`, {method: 'POST', signal: AbortSignal.timeout(10_000),
    headers: {'content-type': 'application/json', accept: 'application/json, text/event-stream'},
    body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})});
  assert.equal(response.status, 200, await response.clone().text());
  const body = await response.json();
  assert.ok(!body.error, JSON.stringify(body.error));
  return body.result;
}

for (const [name, service] of processes) {
  const port = Number(service.environment.PORT);
  const settings = HTTP_SETTINGS.flatMap(key => service.environment[key] === undefined ? []
    : ['--env', `${key}=${service.environment[key]}`]);
  const container = docker('run', '--detach', '--publish', `127.0.0.1::${port}`,
    '--workdir', service.working_dir ?? '/app', ...settings, service.image, ...service.command);
  try {
    const base = `http://${docker('port', container, `${port}/tcp`).split('\n')[0]}`;
    let healthy = false;
    for (let attempt = 0; attempt < 120 && !healthy; attempt++) {
      assert.equal(docker('inspect', '--format', '{{.State.Running}}', container), 'true', `${name} exited during startup`);
      healthy = await fetch(`${base}/health`, {signal: AbortSignal.timeout(1000)}).then(r => r.ok, () => false);
      if (!healthy) await new Promise(resolve => setTimeout(resolve, 500));
    }
    assert.ok(healthy, `${name} did not report healthy`);
    assert.equal((await fetch(`${base}/openapi.json`)).status, 200, `${name} must serve OpenAPI`);
    const initialized = await rpc(base, 'initialize', {protocolVersion: '2025-11-25', capabilities: {},
      clientInfo: {name: 'forklaunch-ci', version: '1.0.0'}});
    assert.ok(initialized.capabilities?.tools, `${name} must advertise MCP tools`);
    const {tools} = await rpc(base, 'tools/list');
    assert.ok(tools.length > 0, `${name} must list MCP tools`);
    docker('stop', '--time', '15', container);
    assert.equal(docker('inspect', '--format', '{{.State.ExitCode}}', container), '0', `${name} must shut down cleanly`);
    console.log(`PASS ${name}: ${dockerfile} HTTP health, OpenAPI and MCP startup`);
  } catch (error) {
    console.error(docker('logs', container));
    throw error;
  } finally {
    docker('rm', '--force', container);
  }
}
if (!processes.length) console.log(`PASS ${dockerfile}: image built; it serves no Rust HTTP process`);
