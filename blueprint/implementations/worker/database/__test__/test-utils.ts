/**
 * Shared vitest setup, referenced by the symlinked `vitest.config.ts`
 * (`setupFiles: ['__test__/test-utils.ts']`).
 *
 * The config is shared across every blueprint implementation package, but this
 * file was never created for the worker ones — so vitest failed to load the
 * suite and reported "no tests" rather than a failure. Every test in this
 * package had silently never run. The worker implementations register no
 * encryptor and need no fixture, so the setup is deliberately empty: what
 * matters is that it exists, and that removing the shared `setupFiles` entry
 * is NOT the fix, because six other packages do rely on it.
 */
export {};
