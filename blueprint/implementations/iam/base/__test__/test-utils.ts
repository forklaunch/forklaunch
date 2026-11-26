/**
 * Shared vitest setup, named by the symlinked `vitest.config.ts`
 * (`setupFiles: ['__test__/test-utils.ts']`).
 *
 * The file was never created here, so vitest could not load the setup and
 * reported "no tests" instead of a failure — `schemaEquality.test.ts` had
 * silently never run. This package's entities carry compliance-annotated
 * columns, which require a registered field encryptor before any entity
 * module is imported.
 */
import {
  FieldEncryptor,
  registerEncryptor
} from '@forklaunch/core/persistence';

registerEncryptor(new FieldEncryptor('0'.repeat(64)));
