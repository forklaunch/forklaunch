// The shared root vitest.config.ts wires `__test__/test-utils.ts` as this
// package's `setupFiles` entry (same convention as iam-base/billing-base) —
// vitest needs this exact path to exist. cac-base's actual test setup
// (dotenv, field-encryptor registration, the e2e harness) already lives in
// `./e2e/test-utils.ts`; its top-level side effects are harmless for the
// plain unit tests too (each of those also self-registers the encryptor
// inline, before importing any compliance entity), so re-exporting it here
// satisfies the convention without duplicating anything.
export * from './e2e/test-utils';
