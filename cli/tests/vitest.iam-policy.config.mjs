import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Run after installing blueprint dependencies:
// pnpm --dir blueprint exec vitest run --config ../cli/tests/vitest.iam-policy.config.mjs
// Use the blueprint's existing test dependencies; nothing is added to emitted apps.
const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(resolve(here, '../../blueprint/iam-better-auth/package.json'));
export default {
  root: resolve(here, '../..'),
  resolve: {
    alias: {
      '@forklaunch/core/persistence': require.resolve('@forklaunch/core/persistence'),
      '@mikro-orm/sqlite': require.resolve('@mikro-orm/sqlite'),
      vitest: resolve(dirname(require.resolve('vitest/package.json')), 'dist/index.js')
    }
  },
  test: {
    include: [
      'blueprint/iam-better-auth/__test__/authEncryptionPolicy.test.ts',
      'cli/tests/iam_auth_policy_transaction.test.ts'
    ],
    environment: 'node',
    maxWorkers: 1
  }
};
