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

  it('carries JWT verification settings through to the bound contract', () => {
    const policy = { auth: { scheme: 'jwt', allowedRoles: ['admin'] } } as const;
    const bound = bindRuntimeAuth(policy, {
      signatureKey: 'private',
      algorithms: ['HS512'],
      issuer: 'https://id.example.com',
      audience: ['api', 'admin-api']
    });
    expect(bound.auth.jwt).toEqual({
      signatureKey: 'private',
      algorithms: ['HS512'],
      issuer: 'https://id.example.com',
      audience: ['api', 'admin-api']
    });
    expect(
      bindRuntimeAuth(policy, {
        jwksPublicKeyUrl: 'https://identity.test/jwks',
        algorithms: ['RS256'],
        issuer: undefined
      }).auth.jwt
    ).toEqual({
      jwksPublicKeyUrl: 'https://identity.test/jwks',
      algorithms: ['RS256']
    });
    // The key is still required, and bad settings fail at bind time.
    for (const binding of [
      { algorithms: ['HS256'] },
      { signatureKey: 'private', algorithms: [] },
      { signatureKey: 'private', algorithms: 'HS256' },
      { signatureKey: 'private', algorithms: [''] },
      { signatureKey: 'private', issuer: '' },
      { signatureKey: 'private', audience: [] },
      { signatureKey: 'private', audience: [1] },
      { signatureKey: 'private', unknownOption: true }
    ]) {
      expect(() => bindRuntimeAuth(policy, binding as never)).toThrow(
        'Invalid runtime authentication binding'
      );
    }
  });

  it('carries HMAC replay settings through to the bound contract', () => {
    const policy = { auth: { scheme: 'hmac' } } as const;
    const nonceStore = { claim: async () => true };
    const bound = bindRuntimeAuth(policy, {
      secretKeys: { current: 'private' },
      maxSkewMs: 60_000,
      nonceStore
    });
    expect(bound.auth.hmac.maxSkewMs).toBe(60_000);
    expect(bound.auth.hmac.nonceStore).toBe(nonceStore);
    expect(bound.auth.hmac.secretKeys.current).toBe('private');

    const cache = { putRecordIfAbsent: async () => true };
    expect(
      bindRuntimeAuth(policy, {
        secretKeys: { current: 'private' },
        nonceStore: cache as never
      }).auth.hmac.nonceStore
    ).toBe(cache);
    expect(
      bindRuntimeAuth(policy, {
        secretKeys: { current: 'private' },
        maxSkewMs: undefined
      }).auth.hmac
    ).toEqual({ secretKeys: { current: 'private' } });

    for (const binding of [
      { maxSkewMs: 60_000 },
      { secretKeys: { current: 'private' }, maxSkewMs: 0 },
      { secretKeys: { current: 'private' }, maxSkewMs: -1 },
      { secretKeys: { current: 'private' }, maxSkewMs: Infinity },
      { secretKeys: { current: 'private' }, maxSkewMs: '300000' },
      { secretKeys: { current: 'private' }, nonceStore: {} },
      { secretKeys: { current: 'private' }, nonceStore: null },
      { secretKeys: { current: 'private' }, nonceStore: { peekRecord() {} } },
      { secretKeys: { current: 'private' }, unknownOption: true }
    ]) {
      expect(() => bindRuntimeAuth(policy, binding as never)).toThrow(
        'Invalid runtime authentication binding'
      );
    }
  });
});
