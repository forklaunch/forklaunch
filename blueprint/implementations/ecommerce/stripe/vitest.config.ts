import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    exclude: ['**/lib/**', '**/dist/**', '**/node_modules/**'],
    setupFiles: ['__test__/test-utils.ts']
  }
});
