import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    exclude: ['**/lib/**', '**/dist/**', '**/node_modules/**'],
    // Test files start their own containers (Postgres, Redis, Kafka, ...) in
    // beforeAll and stop them in afterAll. Creating a container can take tens
    // of seconds on a busy Docker daemon, more so when every file starts its
    // containers at once, so hooks get far longer than the 10s default.
    hookTimeout: 300_000,
    // A test's first request loads and wires the module, which can take a
    // while when every file does so at once.
    testTimeout: 60_000,
    teardownTimeout: 60_000
  }
});
