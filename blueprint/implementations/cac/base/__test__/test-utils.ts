/**
 * Shared vitest setup (referenced by vitest.config.ts setupFiles).
 * Compliance entities require a registered field encryptor before any
 * entity module is imported.
 */
import {
  FieldEncryptor,
  registerEncryptor
} from '@forklaunch/core/persistence';

registerEncryptor(new FieldEncryptor('0'.repeat(64)));
