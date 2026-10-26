import { describe, expect, it } from 'vitest';
import { bindRuntimeAuth } from './bindRuntimeAuth';

describe('portable authentication policy', () => {
  it('binds runtime keys while preserving portable roles and headers', () => {
    const policy = {
      auth: {
        scheme: 'jwt',
        headerName: 'X-Session',
        allowedRoles: ['system']
      },
      name: 'ExportData'
    } as const;
    const bound = bindRuntimeAuth(policy, {
      jwksPublicKeyUrl: 'https://identity.test/jwks'
    });
    expect(bound.auth.allowedRoles).toEqual(new Set(['system']));
    expect(bound.auth.headerName).toBe('X-Session');
    expect(bound.name).toBe('ExportData');
    expect(JSON.stringify(policy)).not.toContain('identity.test');
  });

  it('binds HMAC secrets and JWT resource decoding without putting them in policy', () => {
    const hmac = {
      auth: { scheme: 'hmac', headerName: 'X-Signature' }
    } as const;
    expect(
      bindRuntimeAuth(hmac, { secretKeys: { current: 'private' } }).auth.hmac
        .secretKeys.current
    ).toBe('private');
    expect(JSON.stringify(hmac)).not.toContain('private');
    const decodeResource = async () => ({ organizationId: 'organization' });
    const jwt = bindRuntimeAuth(
      { auth: { scheme: 'jwt', allowedPermissions: ['platform:read'] } },
      { signatureKey: 'private', decodeResource }
    );
    expect(jwt.auth.allowedPermissions).toEqual(new Set(['platform:read']));
    expect(jwt.auth.decodeResource).toBe(decodeResource);
    expect(jwt.auth.jwt).toEqual({ signatureKey: 'private' });
    expect(() => bindRuntimeAuth(hmac, { secretKeys: {} })).toThrow();
    expect(() =>
      bindRuntimeAuth(
        {
          auth: {
            scheme: 'jwt',
            allowedRoles: [],
            jwt: { signatureKey: 'hidden' }
          }
        } as never,
        { signatureKey: 'private' } as never
      )
    ).toThrow();
  });

  it('rejects credential bindings that override policy', () => {
    expect(() =>
      bindRuntimeAuth({ auth: { scheme: 'jwt', allowedRoles: ['admin'] } }, {
        jwksPublicKeyUrl: 'https://identity.test/jwks',
        allowedRoles: ['guest']
      } as never)
    ).toThrow('Invalid runtime authentication binding');
    expect(() =>
      bindRuntimeAuth(
        { auth: { scheme: 'jwt', allowedRoles: [] } },
        { signatureKey: '' }
      )
    ).toThrow();
  });
});
