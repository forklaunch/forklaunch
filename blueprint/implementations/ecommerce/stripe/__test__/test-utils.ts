/**
 * Shared vitest setup, named by the symlinked `vitest.config.ts`
 * (`setupFiles: ['__test__/test-utils.ts']`).
 *
 * The file was never created here, so vitest could not load the setup and
 * reported "no tests" instead of a failure — `connectFee.test.ts` and
 * `resumePayment.test.ts` had silently never run. This package holds no
 * entities of its own, so it registers no encryptor; what matters is that
 * the file exists. Dropping the shared `setupFiles` entry is NOT the
 * alternative fix: the sibling packages genuinely depend on it.
 */
export {};
