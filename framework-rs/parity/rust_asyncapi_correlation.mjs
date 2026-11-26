import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const {Parser} = require(process.env.ASYNCAPI_PARSER_PATH || '@asyncapi/parser');
const temporary = mkdtempSync(join(tmpdir(), 'forklaunch-correlation-'));
try {
  const imported = join(temporary, 'imported.json');
  const exported = join(temporary, 'exported.json');
  const run = spawnSync('cargo', ['test', '--quiet', '--manifest-path', join(root, 'framework-rs/Cargo.toml'), '-p', 'forklaunch-core','--features','build,asyncapi-import', '--all-features'], {
    cwd: root,
    env: {...process.env, FORKLAUNCH_TEST_ASYNCAPI_CORRELATION_DOCUMENT: imported, FORKLAUNCH_TEST_ASYNCAPI_CORRELATION_EXPORT: exported},
    encoding: 'utf8', timeout: 300000, maxBuffer: 16 * 1024 * 1024
  });
  assert.equal(run.status, 0, `${run.error || ''}\n${run.stdout}\n${run.stderr}`);
  for (const path of [imported, exported]) {
    const result = await new Parser().parse(readFileSync(path, 'utf8'));
    assert.deepEqual(result.diagnostics.filter(diagnostic => diagnostic.severity === 0), []);
    assert.ok(result.document);
    const document = result.document.json();
    if (path === imported) {
      assert.equal(document.components.messages.Request.correlationId.location, '$message.payload#/message');
      assert.equal(document.components.messages.Reply.correlationId.location, '$message.payload#/kind');
    } else {
      assert.equal(document.channels.events.messages.Created.correlationId.location, '$message.payload#/id');
    }
  }
  console.log('PASS AsyncAPI checked correlation expressions, official parser import/export and referenced trait precedence');
} finally {
  rmSync(temporary, {recursive: true, force: true});
}
