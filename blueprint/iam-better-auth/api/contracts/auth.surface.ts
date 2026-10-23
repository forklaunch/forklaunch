// Better Auth owns these dynamic endpoints and its own typed client.
// This metadata deliberately does not claim generated Rust endpoint coverage.
export const betterAuthSurface = {
  id: 'better-auth',
  provider: 'better-auth',
  coverage: 'provider-owned',
  client: { package: 'better-auth/client', export: 'createAuthClient' },
  runtime: { basePath: 'runtime-resolved', openApi: true },
  callbacks: ['/test-callback', '/oauth-redirect']
} as const;
