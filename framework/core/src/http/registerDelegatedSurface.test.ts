import { describe, expect, it } from 'vitest';
import { registerDelegatedSurface } from './registerDelegatedSurface';

const surface = {
  id: 'auth',
  provider: 'better-auth',
  coverage: 'provider-owned',
  client: { package: 'better-auth/client', export: 'createAuthClient' },
  runtime: { basePath: 'runtime-resolved', openApi: true },
  callbacks: ['/callback']
} as const;

describe('delegated HTTP surface boundary', () => {
  it('retains the runtime provider mount and return value', () => {
    const mount = { basePath: '/auth', paths: '/auth/{*any}' };
    expect(
      registerDelegatedSurface(surface, mount, (received) => {
        expect(received).toBe(mount);
        return 42;
      })
    ).toBe(42);
  });
  it('rejects invalid mount scope before invoking registration', () => {
    let invoked = false;
    expect(() =>
      registerDelegatedSurface(
        surface,
        { basePath: '/auth', paths: '/unrelated' },
        () => {
          invoked = true;
        }
      )
    ).toThrow('Invalid delegated HTTP surface mount');
    expect(invoked).toBe(false);
  });
});
