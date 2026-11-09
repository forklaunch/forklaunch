// Compile actual public billing DTOs and their dependencies; no declaration stubs.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { realpathSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const compiler = realpathSync(process.env.TSC_BIN ?? path.join(root, 'framework/node_modules/.bin/tsc'));
const ts = require(require.resolve('typescript', { paths: [path.dirname(compiler)] }));
const dependencies = process.env.FORKLAUNCH_TEST_BILLING_TYPES_DEPENDENCIES
  ?? process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES
  ?? path.resolve(path.dirname(compiler), '../../..');
const modules = path.join(dependencies, 'node_modules');
const options = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  strict: true,
  noEmit: true,
  skipLibCheck: true,
  esModuleInterop: true,
  types: ['node'],
  typeRoots: [path.join(modules, '@types')],
  paths: {
    '@forklaunch/common': [path.join(root, 'framework/common/index.ts')],
    '@forklaunch/interfaces-billing/types': [path.join(root, 'blueprint/interfaces/billing/types/index.ts')],
    stripe: [path.join(modules, 'stripe')]
  }
};
const program = ts.createProgram([
  path.join(root, 'framework-rs/parity/fixtures/rust_billing_checkout_options.types.ts'),
  path.join(root, 'framework-rs/parity/fixtures/rust_billing_portal_options.types.ts'),
  path.join(root, 'framework-rs/parity/fixtures/rust_billing_recurring_options.types.ts')
], options);
const diagnostics = ts.getPreEmitDiagnostics(program);
if (diagnostics.length) {
  console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: file => file,
    getCurrentDirectory: () => root,
    getNewLine: () => '\n'
  }));
}
assert.equal(diagnostics.length, 0, 'Actual billing DTO type compilation must pass');
console.log('PASS actual billing DTO compilation: optional create/update options, required response evidence');
