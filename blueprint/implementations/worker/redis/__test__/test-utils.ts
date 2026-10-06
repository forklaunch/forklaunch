/**
 * Shared vitest setup, named by the symlinked `vitest.config.ts`
 * (`setupFiles: ['__test__/test-utils.ts']`).
 *
 * The file was never created here, so vitest could not load the setup and
 * reported "no tests" instead of a failure — `schemaEquality.test.ts` had
 * silently never run. The worker implementations register no encryptor and
 * need no fixture, so the setup is deliberately empty.
 */
export {};
