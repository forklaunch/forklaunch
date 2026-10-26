import { createRemoteJWKSet, decodeProtectedHeader, jwtVerify } from 'jose';
import { startTestJwtIssuer, surfaceFromTestClaims } from '../src/jwt';

describe('startTestJwtIssuer', () => {
  it('serves a JWKS that verifies the tokens it mints', async () => {
    const issuer = await startTestJwtIssuer();
    try {
      const response = await fetch(issuer.jwksUrl);
      expect(response.status).toBe(200);
      const body = (await response.json()) as { keys: Array<{ d?: string }> };
      expect(body.keys).toHaveLength(1);
      // Public material only.
      expect(body.keys[0].d).toBeUndefined();

      const token = await issuer.issue({
        sub: 'user-1',
        tenantId: 'org-1',
        roles: ['admin'],
        permissions: ['platform:read'],
        organizationId: 'org-1'
      });
      expect(decodeProtectedHeader(token)).toMatchObject({
        alg: 'RS256',
        kid: 'test-key'
      });
      const { payload } = await jwtVerify(
        token,
        createRemoteJWKSet(new URL(issuer.jwksUrl)),
        { issuer: 'forklaunch-test', audience: 'forklaunch-test' }
      );
      expect(payload).toMatchObject({
        sub: 'user-1',
        tenant_id: 'org-1',
        roles: ['admin'],
        permissions: ['platform:read'],
        organizationId: 'org-1'
      });
      expect(await surfaceFromTestClaims.surfacePermissions(payload)).toEqual(
        new Set(['platform:read'])
      );
      expect(await surfaceFromTestClaims.surfaceRoles(payload)).toEqual(
        new Set(['admin'])
      );
      expect(await issuer.bearer({ sub: 'user-1' })).toMatch(/^Bearer ey/);
    } finally {
      await issuer.close();
    }
  });

  it('mints expired tokens on request and 404s other paths', async () => {
    const issuer = await startTestJwtIssuer({ jwksPath: '/jwks.json' });
    try {
      expect(issuer.jwksUrl).toMatch(/\/jwks\.json$/);
      expect(
        (await fetch(issuer.jwksUrl.replace('jwks.json', 'x'))).status
      ).toBe(404);
      const token = await issuer.issue({ sub: 'user-1', expiresIn: -60 });
      await expect(
        jwtVerify(token, createRemoteJWKSet(new URL(issuer.jwksUrl)))
      ).rejects.toThrow(/exp/);
    } finally {
      await issuer.close();
    }
  });
});
