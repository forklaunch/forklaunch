import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, mkdir, writeFile, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const tsc = process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc');
const ts = require(require.resolve('typescript', { paths: [path.dirname(await realpath(tsc)), path.join(root, 'framework')] }));
const output = await mkdtemp(path.join(tmpdir(), 'fl-cac-parity-'));
try {
  for (const file of ['services/mockProcedureCodeProvider.service.ts', 'domain/mockProcedureCodes.ts']) {
    const source = await readFile(path.join(root, 'blueprint/implementations/cac/base', file), 'utf8');
    const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } });
    const destination = path.join(output, file.replace(/\.ts$/, '.js'));
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, compiled.outputText);
  }
  const { MockProcedureCodeProvider } = require(path.join(output, 'services/mockProcedureCodeProvider.service.js'));
  const provider = new MockProcedureCodeProvider({ debug() {} });
  const results = [];
  for (const code of ['PROC-001', 'PROC-002', 'PROC-003', 'unknown', 'proc-001', ' PROC-001', '99213', '']) {
    results.push(await provider.lookupProcedureCode({ code }) ?? null);
  }
  const actual = JSON.parse(execFileSync('cargo', ['run', '--quiet', '--manifest-path', 'blueprint-rs/Cargo.toml',
    '-p', 'forklaunch-cac-base', '--example', 'cac_provider_parity'], { cwd: root, encoding: 'utf8', timeout: 120000 }));
  assert.deepEqual(actual, { descriptor: provider.describe(), results });
  console.log('PASS actual TypeScript/Rust CAC provider descriptors, mock codes, exact lookup and unknown-code behavior');
} finally { await rm(output, { recursive: true, force: true }); }
