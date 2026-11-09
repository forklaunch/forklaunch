import { describe, expect, it } from 'vitest';
import {
  authorizationScopePath,
  getAuthorizationScope
} from '../domain/utils/authorizationScope.util';

describe('authorizationScopePath', () => {
  it('signs the full mounted path, including /user', () => {
    const scope = { userId: 'u/1', organizationId: 'org 1', sessionId: 's1' };
    expect(authorizationScopePath(scope, 'roles')).toBe(
      '/user/u%2F1/organizations/org%201/sessions/s1/surface-roles'
    );
    expect(authorizationScopePath(scope, 'permissions')).toBe(
      '/user/u%2F1/organizations/org%201/sessions/s1/surface-permissions'
    );
  });

  it('builds the scope only from a complete UUID payload', () => {
    const [u, o, sid] = [
      '00000000-0000-4000-8000-000000000001',
      '00000000-0000-4000-8000-000000000002',
      '00000000-0000-4000-8000-000000000003'
    ];
    const scope = getAuthorizationScope({
      sub: u,
      activeOrganizationId: o,
      sessionId: sid
    });
    expect(scope).toEqual({ userId: u, organizationId: o, sessionId: sid });
    expect(authorizationScopePath(scope!, 'roles')).toBe(
      `/user/${u}/organizations/${o}/sessions/${sid}/surface-roles`
    );
    expect(getAuthorizationScope({ sub: u })).toBeNull();
  });
});
