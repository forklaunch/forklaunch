// Build the generated shared image and probe its actual HTTP/MCP process.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {resolve} from 'node:path';
import {mcpRpc, probeMcp} from './rust_mcp_probe.mjs';

const [directory, ...requestedBinaries] = process.argv.slice(2);
assert.ok(directory, 'Generated registry application directory is required');
const tag = `forklaunch-registry-proof:${randomUUID()}`;
let container;
const docker = (...args) => execFileSync('docker', args, {encoding: 'utf8'}).trim();
try {
  const compose = JSON.parse(execFileSync('docker', ['compose', 'config', '--format', 'json',
    '--no-interpolate', '--no-env-resolution', '--no-path-resolution'], {
    cwd: resolve(directory), encoding: 'utf8',
  }));
  execFileSync('docker', ['build', '--progress=plain', '-f', process.env.FORKLAUNCH_RUST_CONTAINER_DOCKERFILE ?? 'Dockerfile.rust', '-t', tag, '.'], {
    cwd: resolve(directory), stdio: 'inherit', timeout: 30 * 60_000,
  });
  for (const binary of requestedBinaries.length ? requestedBinaries : ['notes', 'jobs']) {
    const service = Object.values(compose.services).find(service =>
      Array.isArray(service.command) && service.command[0] === `/app/bin/${binary}`);
    assert.ok(service, `Generated Compose command missing for ${binary}`);
    const environment = service.environment;
    const port = Number(environment.PORT);
    assert.ok(Number.isInteger(port) && port > 0 && port <= 65535);
    const httpEnvironment = ['HOST', 'PORT', 'PROTOCOL', 'VERSION', 'RUST_LOG',
      'PUBLIC_ORIGIN', 'DOCS_PATH', 'MCP_ENABLED', 'MCP_LISTENER'].flatMap(key =>
      environment[key] === undefined ? [] : ['--env', `${key}=${environment[key]}`]);
    container = docker('run', '--detach', '--label', 'forklaunch.test=registry-proof',
      '--publish', `127.0.0.1::${port}`, ...httpEnvironment,
      '--health-cmd', `curl --fail --silent http://127.0.0.1:${port}/health >/dev/null`,
      '--health-interval', '1s', '--health-timeout', '3s', '--health-retries', '30',
      tag, `/app/bin/${binary}`);
    const address = docker('port', container, `${port}/tcp`);
    assert.match(address, /^127\.0\.0\.1:\d+$/);
    const base = `http://${address}`;
    let ready = false;
    for (let attempt = 0; attempt < 200; attempt++) {
      assert.equal(docker('inspect', '--format', '{{.State.Running}}', container), 'true',
        `${binary} exited before readiness`);
      try {
        ready = (await fetch(`${base}/health`, {signal: AbortSignal.timeout(1000)})).ok;
      } catch {}
      if (ready && docker('inspect', '--format', '{{.State.Health.Status}}', container) === 'healthy') break;
      ready = false;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.ok(ready, `${binary} did not become healthy`);
    assert.equal((await (await fetch(`${base}/health`)).json()).status, 'ok');
    const openapi = await fetch(`${base}/openapi.json`);
    assert.equal(openapi.status, 200);
    assert.ok((await openapi.json()).paths['/health']);
    await probeMcp(base);
    assert.equal((await fetch(`${base}/health`, {
      headers: {origin: environment.PUBLIC_ORIGIN},
    })).status, 200, 'Generated local public origin must be accepted');
    if (binary === 'jobs') {
      const body = {resourceId: '00000000-0000-0000-0000-000000000001', revision: 1, message: 'untrusted-job'};
      const response = await fetch(`${base}/jobs`, {method: 'POST', headers: {'content-type': 'application/json', 'x-tenant-id': body.resourceId}, body: JSON.stringify(body)});
      assert.equal(response.status, 401, 'Docker producer must reject unverified identity before queue access');
      const result = await mcpRpc(base, 'tools/call', {name: 'enqueueWork', arguments: {body}});
      assert.equal(result.isError, true, 'Docker MCP must preserve producer authorization');
      assert.equal(docker('exec', container, 'test', '-x', '/app/bin/jobs-worker'), '');
    }
    assert.equal(docker('exec', container, 'id', '-u'), '65532');
    docker('stop', '--time', '15', container);
    assert.equal(docker('inspect', '--format', '{{.State.ExitCode}}', container), '0');
    docker('rm', container);
    container = undefined;
  }
  console.log('PASS generated shared Docker image, requested HTTP/MCP processes, worker producer policy when present, non-root processes and graceful shutdown');
} catch (error) {
  if (container) {
    console.error(docker('inspect', '--format', '{{json .State}}', container));
    console.error(docker('logs', container));
  }
  throw error;
} finally {
  if (container) docker('rm', '--force', container);
  try { docker('image', 'rm', tag); } catch {}
}
